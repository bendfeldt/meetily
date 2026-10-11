# Plan: fix the 2026-10-11 ponytail review + audit findings

Fork-only plan (bendfeldt/meetily). Scope: findings 1–31 of the ultra review
and audit, **except 30** (API keys in the database; deferred by the owner).
Numbers below are the report's numbers.

## Facts this plan rests on (checked 2026-10-11)

- Every upstream finding still exists on `upstream/devtest` (24f4533). Its 10
  commits beyond `upstream/main` are logging hardening, parakeet download
  and a local-log feature; none touch the buggy lines. Several of the same
  files changed (`recording_commands.rs`, `whisper_engine.rs`, `api.rs`,
  `lib.rs`, `vad.rs`, `incremental_saver.rs`), so upstream fixes must be
  written against `devtest`, not against our `main`.
- Upstream rules (CONTRIBUTING.md): branch from `devtest`, PR into `devtest`,
  **one issue per PR** ("Fixes #N"), fill `.github/pull_request_template.md`.
- Upstream CI is manual-only (`workflow_dispatch`): no checks run on a PR.
  Our evidence is local Linux checks plus our own macOS CI (below).
- This session cannot open issues or PRs on `Zackriya-Solutions/meetily`:
  the repo cannot be attached next to the fork (same name), and the Claude
  GitHub App is only on `bendfeldt/meetily`. So: branches are pushed to the
  fork, and the issue + PR are opened either by you from a prepared text
  and compare link, or by a new session started with
  `Zackriya-Solutions/meetily` as its source (only works if the Claude app
  may act there).

## Two tracks

### Track F: fork-only code (our Teams/Obsidian features)

Branch `fix/teams-review` from `enhance/teams-auto-capture`, normal fork
flow (verify, commit, macOS CI, PR into the fork).

| Commit | Findings | Change |
|---|---|---|
| F1 `fix(teams): ask again after the 4 h cap` | 1 | `stop_finished(ok)` always returns to `Idle`; fix the test that polls during Stopping. |
| F2 `fix(export): resync the note after any retranscription` | 2 | Move `spawn_sync_if_exported` from `after_meeting.rs:322` into `start_retranscription`'s `Ok` branch (fork-marked line in an upstream file). |
| F3 `refactor(teams): drop leftovers` | 4, 5 | Delete `start_popup_recording_command`; restore upstream's `NO_AUDIO_FILE`/`NO_SPEECH` lines, keep only `ALREADY_IN_PROGRESS`. |
| F4 `refactor(teams): trim to what is used` | 6–11 | One settings path for the export folder (drop Option-click / `choose_folder`); insert summary Markdown as is (drop heading demotion); skip old-format summaries; create the pop-up panel at startup and notify once on failure (drop `UNAVAILABLE`, its command, the polling and the flags); warn when the export folder is missing; fixed temp file name. About −185 lines. |

F4 reverses choices made during step 5 review ("Unavailable" status, lazy
panel). That is what ultra asks for; confirm before I start F4.

### Track U: upstream code → PRs to `Zackriya-Solutions/meetily` `devtest`

Per PR: branch `fix/<topic>` from `upstream/devtest`, pushed to the fork;
write a failing test first where there is logic; Linux `cargo
check/test/clippy/fmt` on touched files, `tsc` + frontend tests when UI is
touched; independent verifier; macOS compile via a throwaway
`enhance/ci-<topic>` branch (fix commits + our workflow file) that is never
part of the PR. No fork files (`.claude/`, `AGENTS.md`, docs/plans) in these
branches. Each PR gets a prepared issue text and PR body (upstream
template) in `docs/plans/upstream/<topic>.md`.

Ordered by harm (silent loss of audio/transcript first):

| PR | Findings | Change (smallest fix) | Test |
|---|---|---|---|
| U1 `fix(audio): keep recording after transient stream errors` | 12, 27 | Remove the three `stop_recording()` calls in `RecordingState::report_error`; add a `recording-error` toast listener in `RecordingStateContext.tsx`'s mount-once effect. | Rust: 20 recoverable errors leave state recording. |
| U2 `fix(audio): bound checkpoint buffer, no FFmpeg panics` | 13 | Clear the buffer whether or not the checkpoint save succeeds; `?`/`map_err` instead of `expect`/`unwrap` in `encode.rs`. | Rust: a failing save still clears the buffer. |
| U3 `fix(recording): stop keeps the transcript tail and duration` | 14, 15, 25 | Guard re-entry with `IS_RECORDING_STOPPING.swap(true)`; on drain timeout keep the model loaded and emit a warning; delete `state.cleanup()` in `stop_streams_and_force_flush`. Three commits. | Rust: second stop returns early; duration survives stop. |
| U4 `fix(transcription): no model swap during a recording` | 16 | Refuse retranscribe/import while `IS_RECORDING`; refuse start while a retranscription/import runs. Also removes our fork's documented "batch vs. new recording" risk. | Rust: command returns the error while recording. |
| U5 `fix(whisper): only drop outputs that are entirely a filler phrase` | 17 | Match whole trimmed text (case/punctuation-insensitive), not `contains`. | Rust: the "round of applause" sentence survives; "[Applause]" is dropped. |
| U6 `fix(whisper): download to .part and rename when complete` | 18 | Write `ggml-<name>.bin.part`, rename after validation; startup ignores `.part`. | Rust: a `.part` file is not listed as Available. |
| U7 `fix(meetings): delete removes the meeting folder` | 19 | Delete rows, then the folder only if it is inside the recordings root and not used by another meeting (same rule as our `empty_recordings::discard_meeting`, written fresh upstream). Changes stored-data behaviour: covered by your request. | Rust: folder outside root is kept; inside is removed. |
| U8 `fix(search): snippet on character boundaries` | 20 | Snap start/end to char boundaries; find the match in the original text. | Rust: Danish text with "æ" no longer panics. |
| U9 `fix(openrouter): fetch models without blocking the UI` | 21 | `async fn` + async `reqwest::Client` + 5 s timeout, like `groq.rs`; drop reqwest `blocking` feature if unused. | `cargo check`; no network test. |
| U10 `fix(ui): transcription errors do not stop the UI` | 22, 23 | Register RecordingControls' listeners once with refs (as `RecordingStateContext` does); remove both `onRecordingStop(false)` calls. | Bun test: listener registered once across re-renders. |
| U11 `fix(onboarding): open the right System Settings pane` | 24 | Pass `{ preferencePane: 'Privacy_Microphone' }` / `'Privacy_ScreenCapture'`. | `tsc`. |
| U12 `fix(vad): close speech segments after 30 s` | 26 | Force-end a segment after 30 s of continuous speech, as `flush()` does. Links upstream issue #756. | Rust: 60 s of speech yields ≥ 2 segments. |
| U13 `fix(ui): show the real error text` | 28 | One `errorText(err)` helper (`String(err)` fallback) used at the 26 sites; toast on stop failure. | Bun test for the helper. |
| U14 `fix(security): remove save_transcript, https-only open_external_url` | 29 | Delete the unused command; accept only `https://`; Windows uses `explorer` not `cmd /C start`. | Rust: non-https rejected. |
| U15 `perf(retranscription): move samples instead of cloning` | 3 | `into_whisper_format(self)` moves mono samples. | Existing decoder tests. |

Dead-code cleanup (31), after the fixes and only with maintainer buy-in:
open **one tracking issue** first listing the ~19k lines and 26
dependencies, then split into PRs: C1 never-compiled files
(`lib_old_complex.rs`, `audio_v2/`, `core-old.rs`, …); C2 unused audio
modules + `nnnoiseless`; C3 parallel processor + unused transcription
commands; C4 unused summary/notification/API commands, FastAPI callers,
`tauri-plugin-log`; C5 unused UI files + 23 npm packages (lockfile via
`pnpm install`, never by hand). Each deletion is grep-checked across the
tree, including `invoke('…')` strings.

## How the fork gets the upstream fixes

Default: wait. `main` takes them by fast-forward when upstream releases
`devtest` to `main`; then rebase/merge our feature branches. Exception if you
want them sooner: cherry-pick specific U-commits into the feature branch,
knowing the sync will then need conflict resolution. Track F never touches
upstream files except F2's one marked line and F3's restore.

## Order of work

1. Track F: F1–F3 in one session (small), F4 after your OK.
2. Track U by table order, one PR per session (single-feature rule).
   After each: push branch, macOS CI branch green, write
   `docs/plans/upstream/<topic>.md` (issue + PR text + compare link
   `https://github.com/Zackriya-Solutions/meetily/compare/devtest...bendfeldt:meetily:fix/<topic>`).
3. Cleanup issue, then C1–C5 once maintainers agree.

## Not covered / risks

- No live Mac test for any of it; U1, U3, U4, U12 change recording
  behaviour that only a real long meeting proves.
- Upstream may reject or rework PRs; nothing in Track F depends on them.
- Finding 30 (API keys in SQLite) deliberately left out.
