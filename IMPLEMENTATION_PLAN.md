# Implementation plan

Tracked handoff between agent sessions for this fork (see AGENTS.md).
Keep it short: current state, next steps, evidence gaps. History lives in git.

## Current state

- `main` mirrors upstream `Zackriya-Solutions/meetily` at v0.4.1.
- Agentic workflow installed: loopkit skills, `/spec` `/verify` `/polish`
  `/next`, verifier subagent, SessionStart/PreCompact hooks, plus the
  vendored Ponytail skills (default coding mode, see AGENTS.md).
- No fork-specific runtime changes.

## Next steps

- Proposed: Teams meeting prompt → record → transcribe after → Obsidian
  note, see `docs/plans/teams-auto-capture.md` (revision 5). Decided: macOS
  only, verified by the fork macOS CI (no live Mac testing); live
  transcription fallback for cloud providers; notes renamed with the title;
  Obsidian export opt-in per meeting via a button.
- Step 1 done: `.github/workflows/fork-macos-rust-checks.yml` (cargo check,
  clippy report-only, cargo test on macos-latest) green on unchanged upstream
  code (run 38088365544).
- Step 2 done: opt-in Obsidian export. `fork_settings.rs` (own store file),
  `obsidian_export.rs` (render, file naming, find-by-id, rename, sync), an
  "Obsidian" button in the meeting transcript toolbar (Option-click picks
  another folder), sync hooks on summary generation/edit and title edit.
  Two independent reviews; accepted shortcuts are marked `shortcut:` in
  `obsidian_export.rs` (`/ponytail-debt` lists them).
- Next: step 3, empty-recording check on the manual save path.

## Evidence gaps

- Obsidian export not exercised in the running app (no desktop session):
  folder picker, button, toast, and sync after summary/title changes are
  covered only by unit tests of the pure parts.
- Edits made to an exported note in Obsidian are overwritten on the next
  sync (accepted). Changing the export folder leaves old notes unsynced.

- `cargo check` / `cargo test` for `frontend/src-tauri` not yet run in a
  cloud session (heavy native build).
- `pnpm run lint` is unconfigured upstream (`next lint` prompts interactively).
