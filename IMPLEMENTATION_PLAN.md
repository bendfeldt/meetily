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
- Step 3 done: empty manual recordings are deleted (`empty_recordings.rs`):
  only when the VAD reported no speech and no VAD error for that recording;
  folder removed only inside the recordings root and when not shared.
  Hooks: `RecordingManager::start_recording`, `pipeline.rs` (VAD segments
  and errors), `stop_recording`.
- Step 4 done: record-only pop-up recordings (`after_meeting.rs`).
  `start_popup_recording` (debug command `start_popup_recording_command`
  until step 5) starts with the preferred devices; record-only when the
  provider can batch (localWhisper/whisper/parakeet), live otherwise. No
  live transcription task; audio save forced. On stop, a FIFO worker saves
  the meeting, waits while any recording runs, batch-transcribes with the
  configured model/language, and emits meeting-ready / meeting-discarded /
  meeting-transcription-failed. Emptiness uses the step-3 VAD evidence;
  batch "no audio"/"no speech" keeps the meeting. Uses upstream's beta
  retranscribe engine regardless of the beta toggle.
  The record-only request is bound to the pop-up's meeting name, so a
  concurrent manual start cannot take it. Emptiness inherits step 3's
  documented VAD limits (bursts under 250 ms, the last <600 ms window).
- Next: step 5, Teams detector + pop-up + toggle.

## Evidence gaps

- Obsidian export not exercised in the running app (no desktop session):
  folder picker, button, toast, and sync after summary/title changes are
  covered only by unit tests of the pure parts.
- Empty-recording deletion not exercised with real audio: silence vs. speech
  detection relies on upstream's VAD, covered only by unit tests of the rule
  and the delete path.
- Record-only flow not exercised in the running app: start, batch
  transcription, and the worker events are covered only by unit tests of
  the pure parts. Known upstream-level risks kept: a recording started
  during a batch job can unload the engine under it; a failed
  `stop_recording` (stream stop error) leaves `IS_RECORDING` set, which
  also blocks the worker.
- Edits made to an exported note in Obsidian are overwritten on the next
  sync (accepted). Changing the export folder leaves old notes unsynced.

- `cargo check` / `cargo test` for `frontend/src-tauri` not yet run in a
  cloud session (heavy native build).
- `pnpm run lint` is unconfigured upstream (`next lint` prompts interactively).
