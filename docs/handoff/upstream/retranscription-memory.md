# retranscription-memory

## 1. Branch + commit
- Branch: `fix/retranscription-memory` (based on upstream/devtest 24f4533), pushed to origin (bendfeldt/meetily)
- Commit: `42b3068` fix(audio): move decoded samples into whisper conversion, not clone

## 2. What changed
- `frontend/src-tauri/src/audio/decoder.rs`: `to_whisper_format(&self)` / `to_whisper_format_with_progress(&self, ..)` replaced by consuming `into_whisper_format(self)` / `into_whisper_format_with_progress(self, ..)`. Mono samples are moved, not cloned (no copy at all when already 16 kHz). For multi-channel input the interleaved buffer is dropped right after the mono mix, before resampling. Existing tests updated to the new name; new test added.
- `frontend/src-tauri/src/audio/retranscription.rs`: call site renamed (decoded value already moved into `spawn_blocking`).
- `frontend/src-tauri/src/audio/import.rs`: import call site renamed. The `#[ignore]` integration test now reads `duration_seconds` before conversion, because it used `decoded` afterwards.
- No callers of the borrowing version remain (grep finds none), so it was removed.

Test: `audio::decoder::tests::test_into_whisper_format_mono_16k_reuses_buffer` checks that mono 16 kHz samples come back unchanged and in the **same allocation** (pointer equality). On the old code it FAILED: the pointer assertion fired with "samples were copied instead of moved". It passes now. The test shows the clone is gone. The memory saving itself comes from the code path: the clone is removed, and the stereo buffer is dropped before resampling. Peak memory was not measured.

Checks (Linux):
- `cargo test -p meetily --lib -- whisper_format`: 5 passed
- `cargo test -p meetily --lib -- audio::`: 95 passed, 0 failed, 2 ignored
- `cargo check -p meetily --all-targets`: Finished, no errors
- `cargo clippy -p meetily --lib`: no warnings on the changed lines
- `rustfmt --edition 2021 --check`: the files were already unformatted upstream. The lines I changed in decoder.rs are rustfmt-clean. The retranscription.rs call site (one word renamed) sits inside a block that upstream already left unformatted. I left that block as is.

## 3. Issue

**ISSUE TITLE:** Retranscription/import clones the entire decoded recording before resampling (peak memory ~5 GB for a 3 h meeting)

**ISSUE BODY:**

### Description
Retranscription and import decode the whole audio file into memory, then call `DecodedAudio::to_whisper_format(&self)`. For mono input that function does `self.samples.clone()` before normalising and resampling to 16 kHz. While the conversion runs, the full decoded buffer, a full copy of it and the 16 kHz output are all in memory at once. For multi-channel input the interleaved buffer also stays alive through the whole resampling step.

### Steps to reproduce
1. Record or import a long meeting (e.g. 3 h, 48 kHz mono).
2. Run Retranscribe on it, or import the file.
3. Watch the process memory during the "Converting audio format..." step.

### Expected
Peak memory during conversion is about the decoded buffer plus the 16 kHz output (~2.1 GB + ~0.7 GB for 3 h at 48 kHz).

### Actual
Peak memory is about decoded + clone + 16 kHz output: ~2.1 + 2.1 + 0.7 GB ≈ 5 GB, before the Whisper/Parakeet model is loaded. On machines with 8 GB this can lead to swapping or OOM on long recordings.

### Affected code (devtest 24f4533)
- `frontend/src-tauri/src/audio/decoder.rs:48-64`: `to_whisper_format(&self)` / `to_whisper_format_with_progress(&self, ..)`; `self.samples.clone()` at L63
- `frontend/src-tauri/src/audio/retranscription.rs:222-223`: `decoded.to_whisper_format()` (decoded is owned, moved into `spawn_blocking`)
- `frontend/src-tauri/src/audio/import.rs:414`: `decoded.to_whisper_format_with_progress(..)` (same pattern)

## 4. PR

**PR TITLE:** fix(audio): move decoded samples into whisper conversion, not clone

**PR BODY:**

## Description
`DecodedAudio::to_whisper_format(&self)` cloned the whole decoded buffer (`self.samples.clone()`) before resampling to 16 kHz. Both callers (retranscription and import) already own the decoded audio and move it into `spawn_blocking`, so the clone was pure overhead: about 2.1 GB extra for a 3 h 48 kHz mono recording.

This PR replaces it with consuming `into_whisper_format(self)` / `into_whisper_format_with_progress(self, ..)`:
- Mono input is moved, not cloned. When it is already 16 kHz, the same allocation is returned.
- Multi-channel input: the interleaved buffer is dropped right after the mono mix, before resampling.
- The borrowing version had no remaining callers and is removed.
- The output is unchanged. Decoding is still whole-file; streaming decode is out of scope.

## Related Issue
Fixes #<issue>

## Type of Change
- [x] Bug fix
- [ ] New feature
- [ ] Documentation update
- [x] Performance improvement
- [ ] Code refactoring
- [ ] Other (please describe)

## Testing
- [x] Unit tests added/updated
- [ ] Manual testing performed
- [x] All tests pass

New test `test_into_whisper_format_mono_16k_reuses_buffer` asserts the samples come back unchanged and in the same allocation. It failed on the old clone path and passes now. Existing `to_whisper_format` tests were switched to the new method and still pass. Ran on Linux: `cargo test -p meetily --lib -- audio::` (95 passed), `cargo check -p meetily --all-targets`, `cargo clippy` (no new warnings in changed lines). No live retranscription/import of a long recording was done, peak memory was not measured, and nothing was tested on macOS/Windows.

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
`to_whisper_format` / `to_whisper_format_with_progress` are renamed to `into_*` (they now take `self`). They are crate-internal; all in-repo callers are updated. The `#[ignore]` import integration test now reads `duration_seconds` before the conversion.

## 5. Risks / not done
- No live run on a long recording, and peak RSS was not measured. The saving follows from the code: the clone is removed and the stereo buffer is dropped early.
- The rename breaks any out-of-tree caller of `to_whisper_format` (none in repo).
- Peak memory is still about decoded + 16 kHz output, because decoding is whole-file. Streaming decode would be the next step (out of scope).
- The resampler (`resample_audio` / chunked path) still takes a slice and builds new output. That is unchanged.
