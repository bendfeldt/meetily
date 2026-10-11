# audio-errors-keep-recording

## 1. Branch and commit
- Branch `fix/audio-errors-keep-recording` (based on upstream/devtest 24f4533), pushed to origin (bendfeldt/meetily).
- Commit `1b9f886` fix(audio): keep recording on recoverable stream errors
- Commit `1f7c416` fix(audio): stop once when the audio pipeline is gone (review follow-up)

## 2. What changed
- `frontend/src-tauri/src/audio/recording_state.rs`: `report_error` no longer calls `stop_recording()` after 10 recoverable or 15 total errors. Non-recoverable errors still stop the state (decision below). `last_error` is set first; the warn log and the error callback fire only when the error kind changes (`std::mem::discriminant`), so a stream that fails on every callback cannot flood logs or the UI. Counting is unchanged. Added `#[cfg(test)] mod tests` with 2 tests.
- `frontend/src/services/recordingService.ts`: `onRecordingError(cb)`, which listens to `recording-error` (payload is a `&'static str` from `AudioError::user_message()`, i.e. a plain string), following the existing `onMic*` methods.
- `frontend/src/contexts/RecordingStateContext.tsx`: a listener in the mount-once effect (same cancelled/unlisten pattern) that shows `toast.error(message, { duration: 8000 })`, gated on `isRecordingRef` like mic-swap-failed.

Decision and evidence:
- Device disconnects are handled elsewhere. The device monitor emits `DeviceDisconnected` after 3x2s polls, and `spawn_device_event_processor` then runs `trigger_mic_fallback_to_default` / `do_mic_swap`. `do_mic_swap` aborts with "Recording stopped — aborting mic hot-swap" when `manager.is_recording()` is false. So the old count-based stop (cpal fires errors faster than the 6s threshold) actively blocked the hot-swap recovery, and `stop_recording` also clears `audio_sender`, so even a swapped mic would send nothing. Removing the stop lets the fallback recover.
- Non-recoverable errors (`ChannelClosed`, `PermissionDenied`, `InitializationFailed`, `ConfigurationError`, `SampleRateUnsupported`) mean the stream or pipeline is dead, so I kept the immediate stop. The callback still fires, so the UI now gets a toast. After the stop, `process_audio_data` returns early, so no repeats follow.
- The kind-change gate is needed because a failing stream can report an error on every audio callback. The dead-pipeline case is fixed in the second commit (see below).

Tests (fail before, pass after):
- `audio::recording_state::tests::recoverable_errors_do_not_stop_recording` (20x DeviceDisconnected leaves `is_recording()` true)
- `audio::recording_state::tests::repeated_error_notifies_once_per_kind` (first commit; replaced in the second commit, see below)
- Before: `test result: FAILED. 0 passed; 2 failed`. After: `test result: ok. 2 passed; 0 failed`.
- `cargo check -p meetily --all-targets`: Finished. `cargo clippy -p meetily --lib`: no warnings on the changed lines (the existing type_complexity warning on the `error_callback` field, L114, is upstream). `rustfmt --check`: the remaining diffs are only in upstream lines (1, 4, 194, 242, 251). `pnpm exec tsc --noEmit`: exit 0. `bun test` meeting-details-refresh (3 pass) and summary-generation (16 pass).

### Review follow-up (second commit)
- `frontend/src-tauri/src/audio/pipeline.rs`: new `send_error_kind()`. The pipeline channel is unbounded, so `send_audio_chunk` failing with "Failed to send audio chunk" means the pipeline task dropped its receiver. It now maps to the non-recoverable `ChannelClosed` (it used to be the recoverable `ProcessingFailed`), so a dead pipeline stops once and shows the toast instead of reporting forever. DeviceDisconnected and other cpal stream errors stay recoverable.
- `recording_state.rs`: the gate is described accurately: it notifies when the kind differs from the previous error, so A, B, A notifies 3 times. Every 100th recoverable error is also logged at warn, so a sustained failure stays visible. Edits are kept inside `report_error` and the test module.
- Tests: `pipeline::tests::dead_pipeline_send_error_stops_recording` (did not compile before the helper existed; the old mapping gave ProcessingFailed) and `recording_state::tests::notifies_only_when_error_kind_changes_and_counts_repeats` (A,A,A,A,A then B, A gives 3 callbacks; error and recoverable counts are 7). It replaces `repeated_error_notifies_once_per_kind`. Result: 4 passed.

## 3. Issue
ISSUE TITLE: Recording silently stops capturing after repeated audio stream errors, while the UI still shows recording

ISSUE BODY:
### Description
`RecordingState::report_error` counts audio stream errors and calls `stop_recording()` after 10 recoverable errors, on any non-recoverable error, and after 15 total errors. The counters are only reset at start, never during a session. `stop_recording()` stops capture and clears the pipeline sender, but the global `IS_RECORDING` flag in recording_commands.rs stays true, so the UI keeps showing a running recording. The only signal is a `recording-error` event, and nothing in `frontend/src` listens for it. It also makes the mic-disconnect fallback abort ("Recording stopped — aborting mic hot-swap"), because `do_mic_swap` checks `manager.is_recording()`.

### Steps to reproduce
1. Start a recording with a Bluetooth mic (e.g. AirPods).
2. Let the device drop or reconnect a few times during a long meeting, or produce >= 10 cpal stream errors in any other way.
3. Keep "recording", then stop.

### Expected
Recording continues: the device monitor falls back to the default mic, and the user is told about stream errors.

### Actual
Capture stops silently after the 10th recoverable or 15th total error. The timer and UI keep running, the rest of the meeting is missing from the audio and transcript, and no toast is shown.

### Affected code (devtest 24f4533)
- frontend/src-tauri/src/audio/recording_state.rs:266-297 (`report_error`, stops at :277, :282, :295)
- frontend/src-tauri/src/audio/recording_commands.rs:399-401 and :588-590 (emit `recording-error`, no listener)
- frontend/src-tauri/src/audio/recording_commands.rs:1342-1344 (`do_mic_swap` aborts when the state is stopped)
- frontend/src-tauri/src/audio/pipeline.rs:637-645 (send failure on a closed channel maps to recoverable `ProcessingFailed`, because the anyhow text "Failed to send audio chunk" never contains "channel closed")

## 4. Pull request
PR TITLE: fix(audio): keep recording on recoverable stream errors

PR BODY:
## Description
`RecordingState::report_error` stopped the session state after 10 recoverable or 15 total audio errors (counters never reset during a session). The UI kept showing a live recording while nothing was captured, and the mic hot-swap fallback aborted because the state was no longer recording.

- Recoverable errors now only count, log and notify. Disconnects are handled by the device monitor and the mic fallback.
- Non-recoverable errors (channel closed, permission denied, ...) still stop the state, because the stream is dead.
- A failed send to the pipeline channel (unbounded, so it fails only when the pipeline task is gone) is now reported as `ChannelClosed` instead of the recoverable `ProcessingFailed`, so a dead pipeline stops once with a toast instead of failing forever.
- The warn log and the error callback fire when the error kind differs from the previous error (A, B, A notifies three times), so a stream that fails on every callback cannot flood logs or events. Every 100th recoverable error is still logged.
- The UI now listens to `recording-error` (`recordingService.onRecordingError`) and shows the message as an error toast while recording.

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
- [x] Unit tests added/updated: `recording_state::tests::recoverable_errors_do_not_stop_recording`, `recording_state::tests::notifies_only_when_error_kind_changes_and_counts_repeats`, `pipeline::tests::dead_pipeline_send_error_stops_recording` (all fail on devtest, pass now)
- [ ] Manual testing performed: no live recording or macOS test; Linux unit tests and checks only
- [x] All tests pass: `cargo test -p meetily --lib -- recording_state`, `cargo check -p meetily --all-targets`, `pnpm exec tsc --noEmit`, related bun test files

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
Out of scope (pre-existing): a non-recoverable error still stops capture without updating the UI's recording indicator (the global `IS_RECORDING` stays true). The new toast at least tells the user. Unknown cpal stream errors stay recoverable (`StreamFailed`): they are logged when the kind changes and every 100th time, but they no longer end the recording.

## 5. Risks / not done
- No live recording test and no macOS run.
- The same error kind recurring later in a session with no other error in between (e.g. a second Bluetooth drop) does not toast again. The mic-device-switched toast from the fallback still informs the user.
- A non-recoverable error still stops capture while the UI shows recording (now with a toast). A full stop flow from the UI on fatal errors is out of scope.
- A cpal stream that fails forever with an unrecognised error stays "recording" with nothing captured. It is logged every 100th time and toasted once. Before, it stopped silently after 10.
- Rebase note: fix/stop-keeps-transcript-and-duration also edits recording_state.rs; my edits are limited to report_error and the test module at the end of the file.
