# vad-max-segment handoff

## 1. Branch + commit
- Branch: `fix/vad-max-segment` (pushed to origin = bendfeldt/meetily), based on `upstream/devtest` (24f4533)
- Commits: `4f6f31c fix(audio): force-split VAD speech segments after 30 s`, `6f9751d fix(audio): document why VAD split keeps Silero buffer, log short end` (review follow-up)

## 2. What changed
`frontend/src-tauri/src/audio/vad.rs` (only file):
- Added `MAX_SEGMENT_MS = 30_000` and two small pure helpers: `unemitted_speech()` (slice `[emitted..end)` of the active utterance into a `SpeechSegment` with sample-exact timestamps) and `split_long_speech()` (returns the next 30 s piece once that much is still unemitted).
- `process_audio()`: after each 480-sample chunk, if in speech, force-close the next 30 s of `session.get_current_speech()` (confidence 0.8, same as the `flush()` force-end) and record it in a new `speech_emitted_samples` field. The VAD stays in speech.
- `flush()` force-end and the natural `SpeechEnd` now deliver only the not-yet-emitted remainder (start timestamp shifted by the emitted amount). The counter is reset on SpeechStart/SpeechEnd/flush. Updated the `#756` comment in `new()`.
- Why not trim Silero's buffer: verified unsafe on the real model with a scratch test (not committed). The sequence is SpeechStart at 450 ms, then `take_until(1050ms)`, then the next natural SpeechEnd panics: "Duration 450ms is outside of session audio range" (silero lib.rs:443). `take_until` moves `speech_start_ms`, but SpeechEnd uses the `start_ms` stored in `VadState::Speech`. This sequence is now documented in a comment next to the split.
- The follow-up commit replaces `unwrap_or_default` on the SpeechEnd remainder with an explicit branch that logs sample counts only. It uses `debug!`, not `warn!`, because this is a reachable non-bug case: Silero ends speech at last speech + post_speech_pad, which can fall before a split taken later in the redemption silence.

Test: `audio::vad::tests::test_continuous_speech_is_split_into_bounded_contiguous_segments`. It drives the split helpers chunk by chunk over 65 s of continuous audio (utterance starting at 450 ms) and asserts >= 2 forced live segments, every segment <= 30 s, contiguous timestamps, an end at 65 000 ms, and joined samples == the input (no drop, no duplicate).
- Before: with `split_long_speech` as a no-op (current behaviour) it fails: `expected >= 2 forced segments, got 0`.
- After: `cargo test -p meetily --lib -- audio::vad` gives 8 passed, 0 failed (also after the follow-up commit). `-- audio::` gives 95 passed, 0 failed, 2 ignored.
- Why not end to end with real Silero: synthetic audio can't hold the real model in speech. Over 65 s of continuous synthetic signal it reported only 450–1630 ms as speech, and the same held for the existing 5 s on / 5 s off fixture. So the brief's fallback (isolated pure helper) was used.
- `cargo check -p meetily --all-targets`: OK.
- `cargo clippy -p meetily --lib`: warnings in vad.rs are only on pre-existing upstream lines.
- `rustfmt --edition 2021 --check`: the file is unformatted upstream (35 hunks). My lines are rustfmt-clean, leaving 34 hunks, all upstream.

## 3. Upstream issue
**ISSUE TITLE:** VAD never closes a speech segment on continuous audio (webinar/podcast): no live transcript, unbounded memory, multi-hour Whisper clip

**ISSUE BODY:**

### Description
`ContinuousVadProcessor` only closes a speech segment when Silero sees a qualifying silence (`redemption_time`, 500 ms on the live path). Continuous audio, such as a webinar, podcast or video played through system audio, often never has a 500 ms gap. The segment then stays open for the whole recording. Nothing reaches Whisper while recording, both `current_speech` and Silero's internal session buffer grow at about 64 KB/s each (~128 KB/s, ~900 MB over 2 h), and at stop `flush()` hands Whisper one multi-hour clip.

### Steps to reproduce
1. Start a recording with system audio capture.
2. Play continuous speech or music-with-speech with no pauses of 500 ms or more (e.g. a podcast) for several minutes.
3. Watch the live transcript and the app's memory use.

### Expected
Speech is delivered in bounded segments (e.g. at most 30 s) while recording, so the live transcript updates and memory stays bounded.

### Actual
No live transcript appears until a pause or stop. Memory grows linearly, and Whisper gets one very long clip at stop.

### Affected code (devtest 24f4533)
- `frontend/src-tauri/src/audio/vad.rs:54`: `config.redemption_time`. Segments only end after a qualifying silence. The comment notes bounded delivery is tracked in #756.
- `frontend/src-tauri/src/audio/vad.rs:92-115`: `process_audio()` has no maximum segment length.
- `frontend/src-tauri/src/audio/vad.rs:237-309`: `process_chunk()` accumulates `current_speech` without bound and only emits on `VadTransition::SpeechEnd`.
- `frontend/src-tauri/src/audio/vad.rs:187-227`: `flush()` force-ends the whole open utterance at stop.

Related: #756

## 4. Pull request
**PR TITLE:** fix(audio): force-split VAD speech segments after 30 s

**PR BODY:**

## Description
On continuous audio with no 500 ms pause (webinars, podcasts via system audio), `ContinuousVadProcessor` kept one speech segment open for the entire recording: no live transcript, unbounded buffer growth, and one multi-hour clip sent to Whisper at stop.

`process_audio()` now force-closes the open utterance every `MAX_SEGMENT_MS` (30 s), the same way `flush()` force-ends speech (confidence 0.8), and keeps the VAD in speech for the audio that follows. The processor tracks how many samples of the current Silero utterance were already emitted (`speech_emitted_samples`). `flush()` and the natural `SpeechEnd` then deliver only the remainder, so segments are contiguous with no dropped or duplicated samples.

Silero's own session buffer is deliberately not trimmed. `VadSession::take_until` would leave Silero's internal `VadState::Speech { start_ms }` pointing at deleted audio, and the next natural `SpeechEnd` would panic in `get_speech()`.

## Related Issue
Fixes #<issue>
Related: #756

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

New test `test_continuous_speech_is_split_into_bounded_contiguous_segments` drives the split helpers over 65 s of continuous audio. It asserts at least 2 forced segments of 30 s or less, contiguous timestamps, and an exact sample round-trip. It fails against the old behaviour (0 forced segments) and passes now. Synthetic audio can't hold the real Silero model in speech for 60 s, so the helpers are tested in isolation. The existing VAD tests with the real model still pass.

Ran on Linux: `cargo test -p meetily --lib -- audio::` (95 passed, 2 ignored) and `cargo check -p meetily --all-targets`. No live recording was done and nothing was tested on macOS or Windows.

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
- Why Silero's buffer is not trimmed with `VadSession::take_until` after a split (silero-rs 26a6460): `take_until` updates `speech_start_ms` but not the `start_ms` held in the internal `VadState::Speech`, which the natural `SpeechEnd` passes to `get_speech()`. Verified on the real model: SpeechStart at 450 ms, `take_until(1050ms)`, then the next SpeechEnd panics with "Duration 450ms is outside of session audio range". The sequence is documented in a code comment.
- Small tail loss: `pipeline.rs` drops VAD segments shorter than 800 samples (50 ms at 16 kHz). If an utterance ends less than 50 ms after a forced split, that remainder is dropped on the live path. Merging it into the previous segment isn't possible because that segment has already been delivered.
- Not fixed, pre-existing: `resample_to_16k()` truncates `output_len` per call (`floor(len / ratio)`), so when the input chunk sizes aren't multiples of the ratio, samples are lost and VAD timestamps drift slightly behind wall-clock over long recordings. Out of scope here.
- No end-to-end test through `process_audio`/`flush` with the real model: synthetic input can't hold Silero in speech for 30 s or more (65 s of continuous synthetic signal gives speech only at 450–1630 ms). The splitting is tested through the helpers that `process_audio` and `flush` call.
- `current_speech` is cleared at each split, so the processor's own buffer is bounded to about 30 s. Silero's internal `session_audio` still grows until a natural `SpeechEnd`, because the crate has no safe way to trim it mid-utterance (see above). Fixing that needs a silero-rs change.
- A forced cut can land mid-word. Whisper sees a 30 s boundary instead of a pause boundary.

## 5. Gaps and risks
- Live path: a remainder under 800 samples (< 50 ms) right after a forced split is dropped by pipeline.rs (documented in the PR).
- Pre-existing resampler truncation drift is noted in the PR, not fixed.
- No end-to-end test with the real Silero model on long continuous speech (synthetic audio isn't classified as continuous speech). No live recording and no macOS/Windows run.
- Silero's internal buffer still grows (~64 KB/s) during uninterrupted speech. Only our `current_speech` copy and Whisper's input are now bounded. A full fix needs an upstream silero-rs API.
- Forced cuts can split words at the 30 s boundaries.
- Downstream effect not reviewed in depth: pipeline, import and retranscription now get more, shorter segments on long continuous audio (that is the intent). Nothing found relies on the confidence value 0.8.
- If a natural `SpeechEnd` lands after a forced split but inside the already-emitted range (end plus post-pad before the split point), no remainder is emitted. That is correct: there are no duplicates, and the trailing silence was already included in the forced piece.
