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
  note, see `docs/plans/teams-auto-capture.md` (revision 2, verified against
  upstream code, Apple/Tauri/cidre sources, Ponytail ultra, loopkit).
  Next: step 0 spike on a real Mac (which Teams process holds the mic, and
  whether it stays held while muted), then step 1.

## Evidence gaps

- `cargo check` / `cargo test` for `frontend/src-tauri` not yet run in a
  cloud session (heavy native build).
- `pnpm run lint` is unconfigured upstream (`next lint` prompts interactively).
