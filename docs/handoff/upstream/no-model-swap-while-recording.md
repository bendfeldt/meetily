# no-model-swap-while-recording

## 1. Branch + commit
- Branch `fix/no-model-swap-while-recording` (based on upstream/devtest 24f4533), pushed to origin (bendfeldt/meetily).
- Commit `7ce1035` fix(audio): refuse to share the engine between recording and batch jobs
- Commit `1db9be2` fix(audio): claim batch flags atomically and show one refusal message (review follow-up)
- Commit `cf1a7ab` fix(audio): don't wait for the engine lock in batch commands (second follow-up)

### Second follow-up (cf1a7ab)
- The batch commands used to wait on the engine lifecycle lock with no time limit, so a stalled recording start would leave the dialog spinning. They now use `try_engine_lifecycle_lock()` (`common.rs`, a non-waiting `try_lock_owned`). If the lock is held they return right away with `ENGINE_LOCK_BUSY`: "The transcription engine is busy (a recording may be starting). Try again in a moment."
- Test `test_try_engine_lifecycle_lock_refuses_while_held`: failed to compile before the change, passes after. It holds the lock and asserts the try returns the busy error. It checks only the refusal half, not a successful try after release, because another test uses the same global lock in parallel and could make that half flaky.
- Checks: tests 33 passed, 0 failed, 1 ignored. `cargo check --all-targets` Finished. Clippy: no hits on changed lines.

### Review follow-up (1db9be2)
- Race closed: `start_retranscription_command` / `start_import_audio_command` now claim their in-progress flag synchronously, under the engine lifecycle lock, via a new `BatchFlagGuard::claim(flag, already_msg, engine_busy_reason(...))` in `common.rs`. This replaces the duplicated RetranscriptionGuard/ImportGuard. The guard is moved into the spawned task: `start_retranscription` / `start_import` now take `_guard: BatchFlagGuard` and are `pub(crate)`, since only the commands call them.
  - Recording start holds the same lock from its busy check until `finalize_recording_start()` sets IS_RECORDING. Either the batch claim runs first, and the recording's check sees the flag, or it runs after, and sees IS_RECORDING.
  - So no extra re-check before finalize is needed, and a refused recording returns before anything is set.
- One refusal message: the recording start no longer emits `transcription-error`. The Err is `"Recording cannot start: <reason>"`. `RecordingControls.tsx` (catch at ~L117, outside L159-183 and L248-332) shows that text in its existing alert, titled "Recording Not Started", instead of the generic device dialog. The sidebar start paths already `alert()` the error text.
- New test `audio::common::tests::test_batch_flag_guard_refuses_busy_engine_and_releases_on_drop`. It uses a test-local static flag, so it can't race the existing tests that reset the global flags. It checks three things: a claim on a busy engine is refused and leaves the flag clear; a free claim sets the flag and a second claim is refused; dropping the guard releases the flag.
- Checks:
  - `cargo test -p meetily --lib -- engine_busy batch_flag retranscription import common`: 32 passed, 0 failed, 1 ignored.
  - `cargo check --all-targets`: Finished.
  - clippy: the remaining hits in the touched files (import.rs:907, recording_commands.rs:64/175) are on lines this branch doesn't change. clippy still exits 101 on the existing `backend_config.rs` deny lint.
  - `pnpm exec tsc --noEmit`: exit 0.
  - rustfmt: diff-block counts per file are no higher than upstream.

## 2. What changed
- `frontend/src-tauri/src/audio/common.rs`: `BatchFlagGuard` (see follow-up) and the new pure `engine_busy_reason(recording, retranscribing, importing) -> Option<&'static str>` with user-facing reasons; new unit test.
- `audio/retranscription.rs` `start_retranscription_command`: returns the reason if a recording or an import is running.
- `audio/import.rs` `start_import_audio_command`: returns the reason if a recording or a retranscription is running.
- `audio/recording_commands.rs` both start paths (`start_recording_with_meeting_name`, `start_recording_with_devices_and_meeting`): under the engine lifecycle lock, after the "already recording" check, they refuse with `"Recording cannot start: <reason>"` while a retranscription or import runs. The first commit emitted `transcription-error` here; the follow-up removed it.
- UI: RetranscribeDialog and useImportAudio already show the command error in the dialog (`setError`). Messages avoid "already in progress" (useRecordingStart treats that as "recording live") and the device keywords that RecordingControls matches ("mic", "input", "output", "permission").

Test: `audio::common::tests::test_engine_busy_reason_blocks_any_other_engine_user`
- before fix: `cargo test -p meetily --lib -- engine_busy_reason` -> error[E0425] cannot find function `engine_busy_reason` (fails)
- after: `test result: ok. 2 passed` (with lifecycle-lock test); `-- engine_busy_reason retranscription import` -> `30 passed; 0 failed; 1 ignored`
- `cargo check -p meetily --all-targets`: exit 0 (only pre-existing warnings).
- clippy: no warnings on changed lines. `cargo clippy -p meetily --lib` exits 101 because of a pre-existing deny lint in `audio/capture/backend_config.rs` (`to_string` shadows Display), unrelated.
- rustfmt: the four files were already unformatted upstream; the number of rustfmt diff hunks per file is unchanged (new lines are rustfmt-clean).

## 3. Issue

**Title:** Retranscription/import during a live recording swaps or unloads the shared transcription model

**Description**
Live recording, retranscription and audio import all use the same global Whisper engine and the same global Parakeet engine, each holding a single loaded model. Nothing prevents them from running at the same time (the meeting-details page hard-codes `isRecording = false`, and the backend commands check only their own "in progress" flag), so one job can swap or unload the model under another.

**Steps to reproduce**
A:
1. Start a recording with model X.
2. While it records, open a past meeting, choose "Retranscribe" with a different model Y (or import an audio file with Y).
3. Keep talking.

B:
1. Start a retranscription (or import) of a long meeting.
2. While it runs, start a recording whose configured model differs from the one the batch job loaded.

**Expected**
The second job is refused with a clear message (or the jobs do not interfere).

**Actual**
A: the batch job calls `load_model(Y)`, which unloads X. Live chunks fail with "No model loaded" while Y loads, then the rest of the meeting is transcribed with Y.
B: the recording's engine loader sees a model that does not match the saved config and unloads it, so the retranscription/import errors out.

**Affected code (devtest 24f4533)**
- `frontend/src-tauri/src/audio/retranscription.rs:781` `start_retranscription_command` (checks only `RETRANSCRIPTION_IN_PROGRESS`); model load at L561 / L663
- `frontend/src-tauri/src/audio/import.rs:965` `start_import_audio_command`; model load at L786 / L832
- `frontend/src-tauri/src/audio/transcription/engine.rs:287` unloads a model that does not match the config on recording start
- `frontend/src-tauri/src/audio/recording_commands.rs:307` and `:510` recording start paths (no check for batch jobs)
- `frontend/src/app/meeting-details/page-content.tsx:61` `const isRecording = false;`

## 4. PR

**Title:** fix(audio): refuse to share the engine between recording and batch jobs

**Body:**

## Description
Live recording, retranscription and import share one global Whisper and one global Parakeet engine with a single loaded model. Running them at the same time swaps or unloads the model under the other job (live chunks fail with "No model loaded" and then use the other model, or the batch job errors when a recording starts).

This PR adds a small pure helper `engine_busy_reason(recording, retranscribing, importing)` and a `BatchFlagGuard` in `audio/common.rs`, and checks them at:
- `start_retranscription_command`: refuse while a recording or an import runs
- `start_import_audio_command`: refuse while a recording or a retranscription runs
- both recording start paths: refuse while a retranscription or import runs, returning `"Recording cannot start: <reason>"`. `RecordingControls` shows that text in its existing alert instead of the generic "check your audio device" dialog.
- The batch commands claim their in-progress flag (new `BatchFlagGuard`) under the existing engine lifecycle lock before spawning the job, so a check and a claim can't interleave with a recording start. They don't wait for that lock: if it is held (a recording is starting or a model is unloading), they return "The transcription engine is busy ... Try again in a moment." immediately.

The retranscribe and import dialogs already display the command's error string. Messages are user-facing, e.g. "Stop the recording first. It uses the same transcription model."

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
- [x] Unit tests added/updated (`audio::common::tests::test_engine_busy_reason_blocks_any_other_engine_user`, `test_batch_flag_guard_refuses_busy_engine_and_releases_on_drop`, `test_try_engine_lifecycle_lock_refuses_while_held`. Integration tests of the recording/batch lock contract are out of scope.)
- [ ] Manual testing performed (only Linux `cargo test`/`cargo check` and `pnpm exec tsc --noEmit` ran. No live recording, no retranscription in the running app, no macOS test.)
- [x] All tests pass (the retranscription/import/common unit tests that were run)

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
- The batch commands claim their in-progress flag under the engine lifecycle lock before spawning the job. Recording start holds that same lock until it marks itself as recording, so the busy check and the claim can't interleave.
- A refused recording shows a single alert with the reason (RecordingControls), or the existing `alert()` on the sidebar start paths.
- Tests: unit tests cover the busy-reason helper, the flag guard (refuses, claims, releases on drop) and the non-waiting lock refusing while held. Integration-level tests of the lock contract between a real recording start and a batch command are out of scope for this PR.

## 5. Risks / not done
- Race closed and single refusal message in 1db9be2. Since cf1a7ab a batch command no longer waits: if a recording is starting or a model is unloading, the user gets "busy, try again in a moment" and must click again.
- Other open PRs (fix/recording-controls-listeners, fix/error-text) also edit `RecordingControls.tsx`. This change stays in the start catch (~L114-124), but expect a small rebase if one of them lands first.
- Retranscription vs import are now also mutually exclusive. They share the same engine, so this is the same bug, but it goes slightly beyond the literal brief.
- No live app, recording or macOS test was run.
