# stop-keeps-transcript-and-duration

## 1. Branch and commits

Branch `fix/stop-keeps-transcript-and-duration` (based on upstream/devtest 24f4533), pushed to origin (bendfeldt/meetily). There are 6 commits, all new commits (no amend, no force-push).

- 8940f3a fix(recording): ignore a second stop while one is draining (finding 15)
- 4924405 fix(recording): keep the model loaded when stop outlives transcription (finding 14)
- 98146fb fix(recording): keep recording duration through the stop tail (finding 25)
- 81635e1 fix(recording): report a duplicate stop as a distinct no-op error (review items 1 and 5)
- d40d989 fix(recording): exclude a trailing pause when stopping while paused (review item 2)
- b43c848 fix(recording): show the stop-time transcription timeout as a warning (review item 3)

## 2. What changed and tests

**Duplicate stop (8940f3a and 81635e1)**
- `frontend/src-tauri/src/audio/recording_commands.rs`:
  - `StoppingGuard::try_acquire(&'static AtomicBool)` swaps the flag to true and returns `None` if it was already set. Only the holder clears the flag, on Drop.
  - The guard is acquired at the top of `stop_recording`, right after the `IS_RECORDING` check.
  - A duplicate stop returns `Err(STOP_ALREADY_IN_PROGRESS)` (`pub const`, value `"Stop already in progress"`). It has no side effects: no model unload, no `IS_RECORDING=false`, no `recording-stopped`.
- `frontend/src-tauri/src/tray.rs`: both stop paths match that error and do nothing (no emit, no tray revert). There is no pre-check, so there is no check-then-act race.
- `frontend/src-tauri/src/lib.rs`: the `stop_recording` command passes that error through without touching `RECORDING_FLAG` or the tray.
- `frontend/src/components/RecordingControls.tsx`: the `stopRecordingAction` catch returns early on that message, so there is no `onRecordingStop` and no toast. The edit is at the top of the catch, outside the listener block.
- Test: `audio::recording_commands::tests::second_stop_cannot_acquire_stopping_guard_while_first_runs`.
  - It uses its own static `AtomicBool`, so it cannot race other tests on `IS_RECORDING_STOPPING`.
  - Before 8940f3a it failed to compile (E0599: no `try_acquire`); now it passes.
  - `stop_recording` itself needs a real `AppHandle`, and the crate has no tauri `test` feature or MockRuntime. That is why the test covers the guard rather than the whole command.

**Transcription timeout (4924405 and b43c848)**
- `recording_commands.rs`: when the 10-minute wait for the transcription task times out, the model is not unloaded. A warning is logged and a `transcription-error` is emitted with `{actionable: false, phase: "shutdown"}` and the message "Transcription is taking longer than expected. The saved transcript may be missing the end of the meeting."
- The model stays loaded until the next model load or unload, or until the app exits. This is intentional, so the detached worker can keep draining its backlog.
- `frontend/src/services/transcriptService.ts`: the TypeScript type for `phase` gains `'shutdown'`.
- `useModalState.ts`: phase `'shutdown'` is shown with `toast.warning`. Sonner 2 has `toast.warning`, and it is already used elsewhere in the app.
- `RecordingControls.tsx` transcription-error listener: one guard line at the top returns early for `'shutdown'`. It does not clear `isProcessing`, count an error or send analytics.
- No unit test: the path needs an `AppHandle` and a 10-minute timeout. `cargo check` and `tsc --noEmit` are clean, and no existing UI test covers these listeners.

**Duration (98146fb and d40d989)**
- `recording_manager.rs`: removed the `state.cleanup()` call from `stop_streams_and_force_flush`.
- `recording_state.rs`:
  - The new field `recording_end` is frozen once in `stop_recording()`. It is reset in `start_recording()`, in `cleanup()` and in `Default`.
  - The duration getters measure up to `recording_end`, so the transcription drain after stop does not count as recording time.
  - `stop_recording()` now adds an open pause to `total_pause_duration`, measured up to `recording_end`, before it clears `pause_start` and `is_paused`.
- Tests (none need audio hardware):
  - `recording_manager::tests::duration_survives_stop_streams_and_force_flush`: before the fix it panicked with "active duration must survive stop"; now it passes.
  - `recording_state::tests::stop_while_paused_excludes_trailing_pause`: before d40d989 it failed (active duration 0.23 s for 0.03 s recorded); now it passes.
  - `recording_state::tests::duration_read_later_equals_duration_at_stop`: passes.
  - `recording_state::tests::restart_after_stop_resets_recording_end`: passes. It already passed before d40d989, because 98146fb resets `recording_end` on start.
- I read the code to confirm that the microphone and system streams are stopped and released before the transcription wait, without the removed `cleanup()`:
  - `recording_manager.rs:346` calls `state.stop_recording()`, which clears the audio sender and the device references at `recording_state.rs:177` and `:180-181`.
  - `recording_manager.rs:349` calls `stream_manager.stop_streams()` (`stream.rs:430`). It takes both streams and calls `AudioStream::stop`, which pauses and drops the cpal stream or aborts the CoreAudio task (`stream.rs:320-345`).
  - `recording_manager.rs:355` calls `force_flush_and_stop` on the pipeline.
  - `cleanup()` never touched the streams. It only reset state fields: callback, stats, start time, pause totals, counters and the buffer pool. Nothing had to be restored.
  - `RecordingManager::drop` still runs `cleanup()` at the end of the stop tail.

**Checks**: `cargo test -p meetily --lib`: 257 passed, 0 failed, 2 ignored. `cargo clippy -p meetily --lib`: no warnings on changed lines (13 locations in these files, all on existing upstream lines). `pnpm exec tsc --noEmit`: clean. rustfmt: changed lines produce no rustfmt diff (the files are not formatted upstream).

## 3. Upstream issue

**ISSUE TITLE:** Stopping a recording can drop the end of the transcript and lose the recording duration

**ISSUE BODY:**

### Description
Problems in the stop path of the Tauri recording core:

1. A second stop while the first is still draining re-enters `stop_recording`. Shutdown can take minutes, and `IS_RECORDING` stays true the whole time. The second call finds no manager or task, unloads the model immediately, sets `IS_RECORDING=false` early and emits a second `recording-stopped`. The tray then emits a second `recording-stop-complete`. The first stop's transcription worker then skips every remaining chunk as "model not loaded".
2. If transcription takes longer than the 10-minute stop timeout, the model is unloaded anyway. The worker skips the rest of the backlog, while the UI reports a successful stop.
3. `stop_streams_and_force_flush` calls `state.cleanup()`. That resets `recording_start`, stats and `last_error` before `save_recording_only` and the meeting-end analytics read them. `metadata.json` duration falls back to the end of the last transcript segment, and analytics durations are None or 0.
4. `RecordingState::stop_recording` drops an open pause without adding it to the pause total. If you stop while paused, the trailing pause counts as recorded time.

### Steps to reproduce
1. Start a recording, talk for a while, then click Stop in the app. While the "processing transcripts" stage is running, click tray → Stop Recording.
2. Use CPU-only transcription with a large model and record a long meeting, so that more than 10 minutes of transcription backlog remains at stop. Then stop.
3. Record any meeting with a silent tail, stop it, and check `duration_seconds` in the meeting's `metadata.json`.

### Expected
1. The second stop is ignored, and transcription finishes normally.
2. No queued chunk is skipped because the model was unloaded, and the user is told if the transcript may be incomplete.
3. `duration_seconds` is the actual recording length excluding pauses, and analytics get real durations.

### Actual
1. The model is unloaded mid-drain, the end of the transcript is missing, and the stop events are duplicated.
2. The remaining chunks are silently dropped ("model not loaded").
3. The duration is the end of the last speech segment, and analytics get None or 0.

### Affected code (devtest 24f4533)
- `frontend/src-tauri/src/audio/recording_commands.rs:687`: the only guard is `IS_RECORDING`. The stopping flag is set later, at `:713`, without a re-entry check.
- `frontend/src-tauri/src/audio/recording_commands.rs:793`: the 10-minute timeout. After it, the model is unloaded at `:867` / `:893`, `IS_RECORDING=false` is set at `:1050` and `recording-stopped` is emitted at `:1085`.
- `frontend/src-tauri/src/tray.rs:94`, `:190`: `recording-stop-complete` is emitted after any `Ok` stop.
- `frontend/src-tauri/src/audio/transcription/worker.rs:239`: chunks are skipped when the model is not loaded.
- `frontend/src-tauri/src/audio/recording_manager.rs:361`: `self.state.cleanup()` runs before `save_recording_only`, which reads the duration at `:372`.
- `frontend/src-tauri/src/audio/recording_state.rs:159`: an open pause is cleared on stop without being added to the pause total.

## 4. Upstream PR

**PR TITLE:** fix(recording): keep transcript and duration intact through stop

**PR BODY:**

## Description
Fixes to the recording stop path:

- **Single-flight stop.**
  - `StoppingGuard` is acquired with an atomic swap at the top of `stop_recording`. A second stop while one is draining returns `Err(STOP_ALREADY_IN_PROGRESS)` with no side effects, and only the guard holder clears the flag.
  - The tray handlers and the `lib.rs` command treat that error as a no-op: no emit, no tray revert, no `RECORDING_FLAG` change, no notification. The UI stop handler returns early on it.
- **No unload on transcription timeout.**
  - If the 10-minute wait for the transcription task times out, the model is left loaded so the worker can keep draining. It stays loaded until the next model load or unload, or until the app exits. This is intentional.
  - A non-actionable `transcription-error` with `phase: "shutdown"` is emitted. `useModalState` shows it as a warning toast, and `RecordingControls` ignores that phase, so it doesn't clear processing state or count as an error.
- **Duration survives stop.**
  - The `state.cleanup()` call in `stop_streams_and_force_flush` is removed. Microphone and system streams are still stopped and released before the transcription wait:
    - `state.stop_recording()` clears the sender and device references.
    - `stream_manager.stop_streams()` pauses and drops each cpal stream, or aborts the CoreAudio task.
    - `cleanup()` itself never touched the streams, and `Drop` still runs it at the end of the stop tail.
  - `RecordingState` records `recording_end` on stop, so the transcription drain isn't counted as recording time.
  - An open pause is added to the pause total on stop, so stopping while paused doesn't count the trailing pause as recorded time.

Out of scope: if `stop_streams_and_force_flush` returns an error, `stop_recording` returns early with `IS_RECORDING` still true. That behavior was already in upstream and this PR does not change it.

## Related Issue
Fixes #<issue>

## Type of Change
- [x] Bug fix
- [ ] New feature
- [ ] Documentation update
- [ ] Performance improvement
- [ ] Code refactoring
- [ ] Other (please describe)

## Testing
- [x] Unit tests added/updated
  - `recording_commands::tests::second_stop_cannot_acquire_stopping_guard_while_first_runs`
  - `recording_manager::tests::duration_survives_stop_streams_and_force_flush`
  - `recording_state::tests::{stop_while_paused_excludes_trailing_pause, duration_read_later_equals_duration_at_stop, restart_after_stop_resets_recording_end}`
- [ ] Manual testing performed
- [x] All tests pass: `cargo test -p meetily --lib` on Linux, `cargo check --all-targets` and `pnpm exec tsc --noEmit`.

No live recording was tested, and nothing was tested on macOS or Windows. The 10-minute timeout path and the frontend listeners have no automated test.

## Documentation
- [ ] Documentation updated
- [x] No documentation needed

## Checklist
- [x] Code follows project style
- [x] Self-reviewed the code
- [x] Added comments for complex code
- [ ] Updated README if needed
- [x] Branch is up to date with devtest
- [x] No merge conflicts

## Screenshots (if applicable)
N/A

## Additional Notes
After a timeout, transcript updates the worker produces after the frontend has saved the meeting are still not persisted. The warning toast tells the user the transcript may be incomplete.

🤖 Generated with [Claude Code](https://claude.com/claude-code)

https://claude.ai/code/session_014xnK9q3eoz3HD4vE1Kr9Lh

## 5. Not done and risks

- The duplicate-stop fix is tested only at the guard level, because the tests have no `AppHandle` or MockRuntime.
- The timeout path and the frontend listener changes have no automated test.
- No live recording was tested, and nothing on macOS.
- After a timeout, transcript updates the worker produces after the frontend has saved the meeting are still not persisted. The model also stays in memory until the next load or until the app exits.
- When the UI stop handler gets "Stop already in progress", it returns early and leaves `isProcessing` true. This is the same behaviour as the existing "No recording in progress" early return. The running stop's `recording-stop-complete` or `recording-stopped` event drives the UI afterwards.
- Out of scope, and already in upstream: if `stop_streams_and_force_flush` errors, `IS_RECORDING` stays true.
- Flushed chunks now get the frozen stop time as their pipeline timestamp (`pipeline.rs:610`) instead of stop time plus a few ms. The difference is negligible.
