# U2 checkpoint-buffer (finding 13)

Branch `fix/checkpoint-buffer` (commits 7d38773, fb8e136, 26d9419), based on upstream/devtest 24f4533.

Changes: `incremental_saver.rs` `add_chunk` always clears the 30 s buffer, then returns the save result (one ~30 s checkpoint lost on failure instead of unbounded growth); `encode.rs` `encode_single_audio` returns anyhow errors instead of panicking (spawn, stdin, wait, non-UTF-8 path) and, on any error after spawn, kills and waits the FFmpeg child so no zombie is left (fb8e136). Test `test_failed_checkpoint_clears_buffer` failed before, passes after; Test `test_encode_to_missing_dir_returns_err` asserts Err instead of panic; reaping verified by code reading plus a one-off local /proc check (1 zombie before fb8e136, 0 after); `cargo test -p meetily --lib -- audio::encode incremental_saver` 4 passed (3 runs); `cargo check --all-targets` OK.

## Issue

**Title:** Incremental audio saver grows memory without bound when checkpoint saves fail, and encoding panics if FFmpeg can't spawn

## Description
`IncrementalAudioSaver::add_chunk` only clears its 30 s checkpoint buffer after `save_checkpoint()` succeeds. If saving keeps failing (FFmpeg missing/broken, disk full, checkpoint dir removed), every later chunk is appended to the same buffer, copied into a new `Vec<f32>` and fails again, so memory and CPU grow for the rest of the recording (~2.7 GB of f32 samples after 4 h at 48 kHz, plus a full copy per chunk).

Separately, `encode_single_audio` uses `.expect("Failed to spawn FFmpeg process")`, `.expect("Failed to open stdin")`, `.wait_with_output().unwrap()` and `output_path.to_str().unwrap()`; any failure panics the audio accumulation task and no further audio is saved for that recording.

## Steps to reproduce
1. Start a recording.
2. Make checkpoint saving fail (fill the disk, delete `<meeting>/.checkpoints/` during recording, or make the FFmpeg binary non-executable).
3. Keep recording and watch memory.

## Expected
A failed checkpoint is reported, memory stays bounded at ~30 s of audio, and an FFmpeg spawn failure is returned as an error.

## Actual
The buffer is never cleared (unbounded growth, full re-copy per chunk); a spawn/wait failure panics the task.

## Affected code (devtest 24f4533)
- frontend/src-tauri/src/audio/incremental_saver.rs:69-72
- frontend/src-tauri/src/audio/encode.rs:57, 74, 76, 83

## PR

**Title:** fix(audio): bound checkpoint buffer when checkpoint save fails

## Description
- `IncrementalAudioSaver::add_chunk` clears the 30 s checkpoint buffer whether or not `save_checkpoint()` succeeded, and still returns the save error. Trade-off: on a failed save that one checkpoint (~30 s) of audio is dropped instead of retried forever.
- `encode_single_audio` no longer panics: spawn failure, missing stdin, `wait_with_output` failure and a non-UTF-8 output path return `anyhow` errors. On an error after spawn (no stdin, or the write fails because FFmpeg exited early, e.g. on a bad output path) the child is killed and waited, so a failing checkpoint every 30 s does not leave zombie FFmpeg processes.

**Trade-off:** when a checkpoint save fails, up to 30 s of audio is dropped instead of being retried. The merged `audio.mp4` is then shorter than the recording, and audio after the gap shifts earlier relative to the transcript timestamps. The caller (`recording_saver.rs`) only logs the error; nothing is shown to the user. Unbounded memory growth (and the eventual failure of the whole recording) seemed worse.

No other behaviour changes.

## Related Issue
Fixes #<issue>

## Type of Change
- [x] Bug fix

## Testing
- [x] Unit tests added: `test_failed_checkpoint_clears_buffer` and `test_encode_to_missing_dir_returns_err` (returns Err instead of panicking; skips if FFmpeg is missing). Reaping the FFmpeg child is verified by code reading plus a one-off local /proc check (1 zombie before, 0 after); that check is not in the test suite because other tests run FFmpeg in parallel and it could flake.
- [x] `cargo test -p meetily --lib -- audio::encode incremental_saver` (4 passed) and `cargo check -p meetily --all-targets` on Linux. No live recording, no macOS/Windows run.

## Documentation
- [x] No documentation needed

## Checklist
- [x] Code follows project style
- [x] Self-reviewed the code
- [x] Added comments for complex code

## Notes / risks
- `finalize()` keeps the old save-then-clear order; it runs once at the end, so no unbounded growth.
- A failed checkpoint drops up to 30 s of audio and shifts later audio earlier against transcript timestamps (see Description); only logged.
