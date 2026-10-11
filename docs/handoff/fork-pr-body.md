## Summary

Fixes from the ultra ponytail review of the Teams/Obsidian work (Track F in
`docs/plans/audit-fixes.md`):

- **4 h cap**: a call still running after the 4 h safety stop asks again
  instead of going quiet.
- **Obsidian sync** after any successful retranscription (meeting page
  Retranscribe too); a sync never erases a note's summary (summaries without
  Markdown fail with a clear message).
- **Leftovers removed**: step-4 debug command; two upstream lines in
  `retranscription.rs` restored (fewer sync conflicts).
- **Trims** (-169 lines): one way to change the export folder (Settings),
  summary Markdown inserted as is, fixed temp note name, pop-up panel created
  at startup (a failure notifies only users who enabled it), "Unavailable"
  status removed.

## Test plan

- [x] `cargo test -p meetily --lib -- meeting_detector after_meeting empty_recordings obsidian_export fork_settings retranscription` (Linux)
- [x] `tsc --noEmit`, frontend tests
- [x] macOS CI (`fork-macos-rust-checks.yml`) green
- [x] Independent reviews (several rounds) until no open findings
- [ ] Live on a Mac: see `docs/handoff/HANDOFF.md` §2

🤖 Generated with [Claude Code](https://claude.com/claude-code)

https://claude.ai/code/session_014xnK9q3eoz3HD4vE1Kr9Lh
