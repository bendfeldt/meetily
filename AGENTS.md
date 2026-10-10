# Contributor contract

meetily is a privacy-first meeting assistant built as a Tauri 2 desktop app.
It is Rust-first: the Rust core in `frontend/src-tauri` (~48k lines) owns audio
capture, mixing/VAD, Whisper/Parakeet transcription, SQLite storage and LLM
summary orchestration. A thinner Next.js/React UI in `frontend/src` (~31k
lines of TypeScript) renders state and calls the core through Tauri commands
and events. The `frontend/` directory name is historical: it holds both. The upstream
[CLAUDE.md](CLAUDE.md) describes the architecture (audio pipeline, Tauri
commands/events, Whisper/Parakeet, summaries) and is the reference for where
code lives.

This file is the shared agent guidance for **this fork**
(`bendfeldt/meetily`). `.claude/CLAUDE.md` imports it and contains only
Claude-specific context. Direct user instructions override repository defaults.

## Fork rules

This repository is a fork of `Zackriya-Solutions/meetily`. Keeping it cheap to
sync is a standing requirement.

- `main` mirrors upstream `main` and is updated only by fast-forward from the
  `upstream` remote (`git fetch upstream main && git merge --ff-only upstream/main`).
  Never commit directly to `main`; work on a branch and open a PR.
- Do not edit upstream-owned files (anything that exists in upstream,
  including the root `CLAUDE.md`, `.gitignore`, `package.json`) unless the
  task is that change. Every such edit becomes a merge conflict on the next
  sync. Fork-only material lives in `AGENTS.md`, `IMPLEMENTATION_PLAN.md`,
  `run.sh`, `.mcp.json` and `.claude/`.
- Branch names follow upstream: `fix/*`, `enhance/*`.
- A change meant for upstream follows [CONTRIBUTING.md](CONTRIBUTING.md):
  branch from upstream `devtest` and target `devtest` there, not this fork.

## Plan, act, verify

1. **Plan:** read the approved task/session goal (or `PROMPT.md` if present),
   `IMPLEMENTATION_PLAN.md` and `git log --oneline -20`. If state and history
   disagree, trust git. Confirm material scope/behavior choices before editing.
2. **Act:** implement exactly one agreed feature. A coordinated documentation
   refresh is one maintenance feature; do not add unrelated runtime fixes.
3. **Verify:** run the smallest applicable checks below and an independent
   adversarial review before claiming completion or committing. A failed
   review blocks both. In Claude use `/verify`; in other clients invoke the
   installed [adversarial checklist](.claude/skills/adversarial-verify/SKILL.md)
   and a separate verifier with the same goal and diff.

The tracked handoff is `IMPLEMENTATION_PLAN.md`. `PROMPT.md`, a JSON feature
ledger and `claude-progress.txt` are not required existing files. Read optional
`.claude/claude-decisions.json` only if an authorized hook/session has created
it. Do not create parallel state files just to satisfy an upstream example.

## Checks

Run from the repository root unless noted. Pick the ones the diff touches.

| Area | Command | Notes |
|---|---|---|
| Rust build | `cd frontend/src-tauri && cargo check` | Primary check for any core change. Slow first build (whisper.cpp, ONNX Runtime); needs cmake/llvm |
| Rust tests | `cd frontend/src-tauri && cargo test` | Unit tests live inline in `#[cfg(test)]` modules; fixtures in `tests/fixtures` |
| Rust lint | `cd frontend/src-tauri && cargo clippy` | Fix new warnings in code you changed; don't sweep unrelated ones |
| Rust format | `cd frontend/src-tauri && cargo fmt --check` | Format only files you changed (`rustfmt <file>`), not the whole crate |
| UI types | `cd frontend && pnpm exec tsc --noEmit` | When the diff touches `frontend/src` |
| UI tests | `.claude/scripts/frontend-tests.sh` | One Bun process per file + `node --test` for `.mjs`. A single `bun test` run leaks mocks between files and fails |
| UI deps | `cd frontend && pnpm install --frozen-lockfile` | Never regenerate `pnpm-lock.yaml` by hand |
| Full app | `cd frontend && pnpm run tauri:dev` | Needs a desktop session, audio devices and OS permissions; only when authorized |

A change to a Tauri command's signature or an emitted event payload touches
both sides: update the Rust type and its TypeScript caller/listener together
and run both the Rust and UI checks.

`pnpm run lint` (`next lint`) is not configured and prompts interactively; do
not rely on it. Native tests do not prove live recording, transcription or
summarization behavior. Record an unavailable live check instead of claiming
it passed.

## Single-feature rule

Keep one purpose per session and commit. Preserve the requested scope.
Ask and re-plan if evidence requires a different feature or behavior change.

## Clean-state contract

- Preserve unrelated user changes. Never revert them to make the tree clean.
- Update `IMPLEMENTATION_PLAN.md` with implemented work, next steps and evidence
  gaps; keep historical details in git.
- Stage explicit task files and use Conventional Commits:
  `type(scope): imperative description` (subject at most 72 characters),
  matching upstream history (`fix(audio): ...`, `feat(recording): ...`).
- Commit reviewed task changes when authorized; do not push to `main`,
  publish releases or change shared state without approval.
- Stop any `tauri dev` / Next.js dev server started by the session.

## Coding mode: Ponytail

Write code in [Ponytail](.claude/skills/ponytail/SKILL.md) full mode by
default: the smallest complete change, reuse before writing, no speculative
abstractions or dependencies, one small test where logic needs one, and a
closing line naming what was skipped and any risk. Upstream meetily already
uses its marker convention: a deliberate shortcut gets a comment
`shortcut: <the limit>, <when to upgrade>` (older code says `ponytail:`).
`/ponytail-debt` lists those markers. Ponytail never overrides the checks,
the verification step or the "Never" list here.

## Installed skills and commands

Skills are installed at `.claude/skills/<name>/SKILL.md`. Read matching skills
before acting; the [routing guide](.claude/skills/using-loopkit/SKILL.md) lists
them. Generic examples about web servers, SQL services, feature ledgers and
upstream release templates are conditional, not this repository's structure.
(meetily does have an app-managed SQLite database: sqlx migrations under
`frontend/src-tauri/migrations` run on app start; `migration-writer`,
`sql-review` and `schema-diff` apply there. Web-only skills such as
`authz-check` rarely apply: there is no server, but Tauri commands are the
trust boundary for `input-validation`.)

Installed Claude commands:

- `/spec`: write a goal contract without overwriting an existing one unless
  `--force` is explicitly requested.
- `/verify`: read-only adversarial diff review with a JSON verdict.
- `/polish`: scoped quality pass over the current diff.
- `/next`: optional ledger-based suggestions; requires a feature ledger first.
- `/ponytail [lite|full|ultra]`, `/ponytail-review` (diff review),
  `/ponytail-audit` (whole-repo review), `/ponytail-debt` (shortcut ledger),
  `/ponytail-help`, `/ponytail-gain`: vendored Ponytail skills; all but
  `/ponytail` are read-only reports.

There is no installed `/loop` command. Command Markdown is client guidance,
not an executable exit-code validator.

## Never

- Weaken, skip or delete tests to make a failing change pass.
- Mark work done or commit before verification.
- Change audio permissions handling, recording/data storage locations, LLM
  provider credentials handling or runtime behavior implicitly.
- Add a dependency (a crate in `Cargo.toml` or an npm package) without a
  concrete justification in the commit body.
- Use `unwrap()`/`expect()` on fallible paths in the audio or recording
  lifecycle, or block the async runtime from audio callbacks.
- Edit an already-applied sqlx migration.
- Run broad dependency upgrades for an unrelated feature.
- Reintroduce the archived `backend/` FastAPI service as a requirement.
- Read or expose secrets, API keys, recordings, transcripts or private machine
  configuration.

## Escalation and optional runner

When blocked, ask the human; use `hitl-escalate` only through approved channels.
Keep a sanitized block record when needed, and stop. `run.sh` does **not**
check `BLOCKED.md` or enforce the pre-commit review contract: it instructs the
executor to commit, then runs `/verify`, echoes failures and checks a
`STATUS: done` marker. Do not treat it as a safe unattended shipping harness.

The runner reads `CLAUDE_EXECUTOR_MODEL` and `CLAUDE_JUDGE_MODEL` only on its
default Claude CLI path; `LOOPKIT_CLI` bypasses those flags.
`CLAUDE_PLANNER_MODEL` is reserved but not read. See
[model routing](.claude/skills/model-routing/SKILL.md).
