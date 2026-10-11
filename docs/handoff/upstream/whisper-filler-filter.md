# whisper-filler-filter: final report

## 1. Branch and commit
- Branch: `fix/whisper-filler-filter` (based on upstream/devtest 24f4533), pushed to origin (bendfeldt/meetily)
- Commits:
  - `0cbd71c` fix(whisper): only drop output that is entirely a filler phrase
  - `675052a` fix(whisper): drop output made only of filler phrases (review rework)

## 2. What changed
- `frontend/src-tauri/src/whisper_engine/whisper_engine.rs`, `is_meaningless_output`:
  - The text is normalized: lowercase, every non-alphanumeric char becomes a space, whitespace is collapsed.
  - Every whole-word occurrence of each filler phrase is then removed.
  - Output is meaningless only if at least one phrase was removed and nothing alphanumeric remains.
  - The few-unique-chars rule is unchanged.
- Five tests in the same file's `mod tests`:
  - `meaningless_filter_keeps_speech_that_mentions_a_filler_phrase`: the applause sentence is kept. It failed on devtest.
  - `meaningless_filter_drops_standalone_filler_phrases`: "[Applause]", "Thank you for watching.", "(Laughter)" and "♪ music playing ♪" are dropped.
  - `meaningless_filter_drops_output_made_only_of_several_fillers`: "Thanks for watching! Like and subscribe!" and "uh uh uh ah ah ah" are dropped. It failed against the first commit (exact equality).
  - `meaningless_filter_keeps_speech_around_fillers_and_non_latin_text`: "Thank you for watching, everyone", "Привет, как дела?" and "日本語のテスト" are kept.
  - `meaningless_filter_leaves_punctuation_only_output_alone`: "..." and "[ ]" are not meaningless, and `clean_repetitive_text` returns them unchanged. That matches devtest: no pattern matches, and the unique-chars rule needs more than 10 bytes.
- Checks:
  - `cargo test -p meetily --lib -- whisper_engine`: 25 passed, 0 failed.
  - `cargo check -p meetily --all-targets`: finished.
  - `cargo clippy -p meetily --lib`: no warnings in the changed lines.
  - `rustfmt --check`: no diffs in the changed lines. The rest of the file was already unformatted upstream and was left as is.

## 3. Upstream issue

**ISSUE TITLE:** Whisper filler filter drops whole transcript chunks that mention "applause", "laughter", etc.

**ISSUE BODY:**

### Description
`WhisperEngine::is_meaningless_output` treats a transcription as meaningless if the lowercased text *contains* any of "thank you for watching", "thanks for watching", "like and subscribe", "music playing", "applause", "laughter", "um um um", "uh uh uh" or "ah ah ah". When this returns true, `clean_repetitive_text` returns an empty string, so the whole chunk is discarded. Real speech that happens to contain one of these words is lost. This affects live transcription, retranscription and audio import, because they all use the same path.

### Steps to reproduce
1. Record, import or retranscribe audio of someone saying: "Let's give Sarah a round of applause, she closed the Q3 deal, and next we review the budget."
2. Look at the transcript.

### Expected
The sentence is transcribed. Only outputs that consist entirely of a hallucinated filler such as "[Applause]", "(Laughter)" or "Thank you for watching." are filtered.

### Actual
The whole chunk is missing from the transcript, with no user-visible indication.

### Affected code (devtest 24f4533)
- `frontend/src-tauri/src/whisper_engine/whisper_engine.rs:431` `is_meaningless_output` (the substring check is at L448)
- `frontend/src-tauri/src/whisper_engine/whisper_engine.rs:398` the caller in `clean_repetitive_text` empties the result

## 4. Pull request

**PR TITLE:** fix(whisper): only drop output that is entirely a filler phrase

**PR BODY:**

## Description
`is_meaningless_output` used a substring match for filler phrases. Because of that, any chunk containing "applause", "laughter", "thanks for watching", etc. was emptied, including real speech such as "Let's give Sarah a round of applause, she closed the Q3 deal...". The text is now normalized: lowercase, non-alphanumeric chars become spaces, whitespace is collapsed. Every whole-word occurrence of a filler phrase is then removed. Output counts as meaningless only if a phrase was removed and nothing alphanumeric remains. "[Applause]", "(Laughter)", "Thank you for watching.", "♪ music playing ♪" and multi-filler output such as "Thanks for watching! Like and subscribe!" are still filtered. Output that is only punctuation ("...", "[ ]") is returned unchanged, as before. The "few unique characters" rule is unchanged. 

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
- [x] Unit tests added/updated (5 `meaningless_filter_*` tests: speech mentioning a filler, standalone fillers, multi-filler output, speech around a filler and non-Latin text, punctuation-only output)
- [ ] Manual testing performed (none: no live recording or macOS test was done)
- [x] All tests pass (`cargo test -p meetily --lib -- whisper_engine`: 25 passed on Linux; `cargo check --all-targets` is clean)

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
Trade-off: a filler appended to real speech ("...the budget. Thanks for watching!") is now kept with the speech instead of dropping the whole chunk. The existing word and phrase repetition cleanup still runs on that text.

🤖 Generated with [Claude Code](https://claude.com/claude-code)

https://claude.ai/code/session_014xnK9q3eoz3HD4vE1Kr9Lh

## 5. Limits and risks
- Only Linux unit tests ran. There was no live recording, transcription or macOS test.
- Filler appended to real speech is now kept. This is the intended trade-off.
- "um um um um" (four times) leaves one "um" after phrase removal, so the filler rule does not drop it. The unchanged unique-chars rule still drops it, because it has 3 unique chars and is 11 bytes long.

## Note added by the coordinator (final review)
The pre-existing "few unique characters" rule in `is_meaningless_output`
(unchanged by this PR) still drops very repetitive short speech such as
"No no no no" or "はいはいはい". It exists on devtest today; mention it in the
PR as a known limit or a possible follow-up.
