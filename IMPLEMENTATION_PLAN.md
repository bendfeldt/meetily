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
- Added `.github/workflows/fork-macos-rust-checks.yml` (cargo check, clippy
  report-only, cargo test on macos-latest). Step 1 of the plan is its first
  green run on unchanged upstream code.

## Evidence gaps

- `cargo check` / `cargo test` for `frontend/src-tauri` not yet run in a
  cloud session (heavy native build).
- `pnpm run lint` is unconfigured upstream (`next lint` prompts interactively).
