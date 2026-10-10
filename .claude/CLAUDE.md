@../AGENTS.md

# Project: meetily (fork)

Privacy-first meeting assistant: capture, transcribe and summarize meetings
locally. A Rust-first Tauri 2 desktop app: the Rust core owns audio,
transcription, storage and summary orchestration; a Next.js UI renders it
and talks to the core via Tauri commands and events. Architecture reference: the upstream root `CLAUDE.md`
(loaded alongside this file; parts of it, such as the "Current" branch
name, may be stale).

Stack: Rust 2021 edition (Tauri 2, tokio, cpal, whisper-rs, ONNX Runtime for
Parakeet, sqlx + SQLite, anyhow) is the core. UI: Next.js 14 + React 18 +
TypeScript, pnpm, Bun for UI tests. Most behavior changes belong in Rust.

Layout:

- `frontend/src-tauri/src/audio/` — capture, devices, pipeline (mixing + VAD),
  recording lifecycle.
- `frontend/src-tauri/src/{whisper_engine,parakeet_engine}/` — local STT and
  model downloads.
- `frontend/src-tauri/src/summary/` and provider dirs (`ollama`, `anthropic`,
  `groq`, `openai`, `openrouter`) — LLM summaries.
- `frontend/src-tauri/src/database/` + `frontend/src-tauri/migrations/` —
  SQLite persistence.
- `frontend/src/` — UI (`app/`, `components/`, `hooks/`, `contexts/`, `lib/`).
- `frontend/tests/` — Bun / node:test suites, run via
  `.claude/scripts/frontend-tests.sh`.
- `backend/` — archived FastAPI service. Do not use or extend.

## Conventions

- Match the existing code in the file you're editing. Read it before you write.
- One change, one purpose. No "while I was in there".
- Fork hygiene comes first: no edits to upstream-owned files unless that is
  the task (see AGENTS.md "Fork rules").
- New frontend-facing behavior goes through Tauri commands/events in the Rust
  core, registered in `lib.rs`.
- Rust errors use `anyhow::Result` internally and `Result<_, String>` at the
  command boundary; UI errors are user-friendly toasts, not raw traces.
- Shared state: `Arc<RwLock<T>>` across async tasks, `Arc<AtomicBool>` for
  flags. Don't hold a lock across `.await` or inside audio callbacks.
- Rust tests go in an inline `#[cfg(test)] mod tests` beside the code
  (`#[tokio::test]` for async), matching the existing modules.
- Audio naming: "microphone" and "system", not "input"/"output". Hot-path
  logging uses `perf_debug!` / `perf_trace!`.
- Use Tauri path APIs; never hardcode platform paths.
- Privacy is a product property: recordings, transcripts and API keys stay
  local. Never log or upload them.

## Claude-specific

- Slash commands live in `.claude/commands/`: `/spec`, `/verify`, `/polish`
  and `/next`. There is no installed `/loop` command or templates directory.
- The `verifier` subagent (`.claude/agents/verifier.md`) is dispatched by
  `/verify` and can be reused as an eval grader.
- `.claude/settings.json` wires a SessionStart hook that injects
  `using-loopkit`, and a PreCompact hook that appends extracted decisions to
  `.claude/claude-decisions.json` (gitignored; needs `jq`). It is a heuristic
  note, not a guaranteed memory store or a sanitizer.
- No format-on-save hook: upstream has no Prettier config, and reformatting
  upstream files would create sync conflicts.
- Cross-agent rules live in the imported `AGENTS.md` at repo root. Do not
  duplicate them here.

<!-- Keep under 300 lines. Prune weekly. Every paragraph is a tax on every turn. -->
