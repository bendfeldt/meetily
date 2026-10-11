# Handoff: ultra review + audit fixes (2026-10-11)

Read this in the Claude desktop app (Claude Code on your Mac), in your local
clone of `bendfeldt/meetily`. Everything below was done in a cloud session
that could not run the app, open upstream issues/PRs, or test on a Mac.

```bash
git fetch origin
git checkout fix/teams-review     # this file lives here
```

## 1. Status at a glance

| Track | What | Where | State |
|---|---|---|---|
| Fork | Teams pop-up / Obsidian / empty-recording fixes (F1–F4) | `fix/teams-review` (on top of `enhance/teams-auto-capture`) | Done, reviewed, macOS CI green |
| Upstream | 15 bug fixes, one branch each | `fix/<topic>` on the fork, based on `upstream/devtest` 24f4533 | Pushed; see table in §3 |
| Upstream | Dead-code cleanup (finding 31) | not started | Needs maintainer agreement first (§5) |
| — | API keys in SQLite (finding 30) | — | Deliberately left out |

The plan these follow: `docs/plans/audit-fixes.md`.

## 2. Fork track: merge and test on your Mac

1. Open a PR in your fork: `fix/teams-review` → `enhance/teams-auto-capture`
   (your fork's `main` mirrors upstream, so fork features stay on
   `enhance/teams-auto-capture`).

   ```bash
   gh pr create -R bendfeldt/meetily --base enhance/teams-auto-capture \
     --head fix/teams-review \
     --title "fix(teams): ultra-review fixes for the Teams pop-up and Obsidian export" \
     --body-file docs/handoff/fork-pr-body.md
   ```
2. Build and run: `cd frontend && pnpm install --frozen-lockfile && pnpm run tauri:dev`.
3. Live checks only you can do (Settings → Recording → "Ask to record Teams
   meetings" on):
   - [ ] Join a Teams call: pop-up appears top-right within ~2 s, also over a
         full-screen app, and clicking it does not steal focus from Teams.
   - [ ] "Record" → recording starts; leave the call → it stops ~5 s later and
         the meeting is transcribed afterwards (`meeting-ready`).
   - [ ] "Not now" → no new pop-up until Teams has released the mic for 5 s.
   - [ ] Mute/unmute during the call does not stop the recording.
   - [ ] A recording with only silence is deleted (manual and pop-up).
   - [ ] Obsidian button exports a note; renaming the meeting renames the note;
         retranscribing updates it.
   - [ ] Log line `meeting_detector: Teams processes holding the microphone`
         shows which bundle id holds the mic (helps if detection misses).

## 3. Upstream track: open issues and PRs

Upstream rules (CONTRIBUTING.md): one issue per PR, PR into `devtest`,
"Fixes #N", fill `.github/pull_request_template.md`. Upstream CI is
manual-only, so our evidence is the Linux unit tests listed per PR.

Each `docs/handoff/upstream/<topic>.md` has the issue title/body and the PR
title/body. Paste this prompt into Claude Code on your Mac (it has your
`gh` login):

> For each file in docs/handoff/upstream/ (in the order of the table in
> docs/handoff/HANDOFF.md §3): create the upstream issue with
> `gh issue create -R Zackriya-Solutions/meetily` using the file's issue title
> and body, then create the PR with
> `gh pr create -R Zackriya-Solutions/meetily --base devtest --head bendfeldt:fix/<topic>`
> using the file's PR title and body with `Fixes #<issue>` replaced by the new
> issue number. Show me each issue and PR text before creating it. Stop and
> ask if a branch no longer merges cleanly into upstream devtest.

Recommended order (most harmful first; conflicts noted):

| # | Branch (`fix/…`) | Finding(s) | Fixes | Review |
|---|---|---|---|---|
| U1 | `audio-errors-keep-recording` | 12, 27 | Glitches no longer silently stop capture; `recording-error` toast | see note A |
| U2 | `checkpoint-buffer` | 13 | Bounded audio buffer on failed save; no FFmpeg panics/zombies | passed |
| U3 | `stop-keeps-transcript-and-duration` | 14, 15, 25 | Second stop ignored; timeout keeps model; duration kept (incl. paused) | passed (remaining notes in PR text) |
| U4 | `no-model-swap-while-recording` | 16 | Recording / retranscribe / import can't run together | passed |
| U5 | `whisper-filler-filter` | 17 | Filler filter no longer drops real speech | passed (pre-existing repetition rule noted) |
| U6 | `whisper-part-download` | 18 | Download to `.part`, installed model kept on failure | see note A |
| U7 | `delete-meeting-folder` | 19 | Deleting a meeting removes its folder (safely) | passed (3 rounds) |
| U8 | `search-snippet-utf8` | 20 | Search snippet can't crash on non-ASCII | see note A |
| U9 | `openrouter-async` | 21 | OpenRouter list no longer freezes the UI | passed |
| U10 | `recording-controls-listeners` | 22, 23 | Listener leak; chunk error no longer stops the UI | passed |
| U11 | `onboarding-settings-pane` | 24 | Onboarding "Open settings" works (Mac pane to confirm) | passed; Mac check needed |
| U12 | `vad-max-segment` | 26 | Speech segments split every 30 s (related #756) | see note A |
| U13 | `error-text` | 28 | Real error text instead of "Unknown error" | passed |
| U14 | `command-hardening` | 29 | Unused file-writer removed; https-only links | passed |
| U15 | `retranscription-memory` | 3 | No full copy of the recording in memory | passed |

Note A: STATUS_A_PLACEHOLDER

Several branches have more than one commit (review rounds); squash on merge.
The PR body for each is in `docs/handoff/upstream/<branch>.md`.

Merge conflicts between these branches (checked by merging all of them into
`upstream/devtest` in table order):

- `audio-errors-keep-recording` ↔ `stop-keeps-transcript-and-duration` (`recording_state.rs`)
- `delete-meeting-folder` ↔ `command-hardening` (`api.rs`)
- `whisper-part-download` ↔ `whisper-filler-filter` (`whisper_engine.rs`)

Open them all; whichever of a pair upstream merges second needs a rebase
onto the new devtest (ask Claude Code: "rebase fix/<topic> onto
upstream/devtest and resolve the conflict, then run cargo test"). The other
12 branches merged together cleanly: 280 Rust tests and `tsc` pass on the
combined tree (Linux).

## 4. When upstream merges these: fork follow-ups

- `fix/no-model-swap-while-recording` changes `start_retranscription`'s
  signature (it takes a guard) and refuses to start a recording while a
  retranscription/import runs. The fork's after-meeting worker
  (`after_meeting.rs`) calls `start_retranscription` directly, so after the
  next upstream sync it must claim the guard, and a new Teams recording would
  be refused while the previous meeting is still being batch-transcribed.
  Decide then: pause/cancel the batch when a pop-up recording starts, or
  queue the recording prompt.
- `fix/retranscription-memory` renames `to_whisper_format` →
  `into_whisper_format`; the fork does not call it directly.
- After each upstream release: `git fetch upstream main && git merge
  --ff-only upstream/main` on `main`, then merge `main` into
  `enhance/teams-auto-capture` and run `cargo test` + macOS CI.

## 5. Not done (next sessions)

- Finding 31, dead-code cleanup (~19k lines, 26 dependencies): open one
  tracking issue upstream first, then split into C1–C5 as in
  `docs/plans/audit-fixes.md`. Prompt for Claude Code:
  > Read docs/plans/audit-fixes.md (dead-code cleanup C1–C5). Draft the
  > upstream tracking issue listing what would be deleted and why, grep-check
  > each item for callers first, and show me the text before posting.
- Finding 30, API keys in plaintext SQLite: deliberately skipped.
- Nothing here has been run live; see each upstream file's "Risks" section.

## 6. Where things are

- Plan: `docs/plans/audit-fixes.md`; Teams design: `docs/plans/teams-auto-capture.md`
- Fork state/evidence gaps: `IMPLEMENTATION_PLAN.md`
- Upstream issue/PR texts: `docs/handoff/upstream/*.md`
