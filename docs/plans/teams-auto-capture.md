# Plan: Teams meeting prompt → record → transcribe after → Obsidian note

Status: proposed, not started. Fork-only feature (`bendfeldt/meetily`).
Revision 2: reviewed against the upstream code, the Apple/Tauri/cidre sources
and Ponytail ultra; see "Review log" at the end.

## Decisions (from the user)

- macOS only.
- When Teams takes the microphone, a pop-up asks whether to record. It must be
  visible on top of everything, **including full-screen apps**.
- A recording started from the pop-up stops by itself 5 s after Teams lets go
  of the microphone. Manual recordings are never auto-stopped.
- Pop-up recordings are transcribed **after** the meeting, not live. Manual
  recordings keep today's live transcription.
- Empty recordings (no audio, or no speech) are deleted, pop-up and manual.
- One Markdown note per meeting, written to a local folder (Obsidian vault).
- Timings: prompt ~1.5 s after Teams takes the mic; stop 5 s after release.

## Done when

1. With "Ask to record Teams meetings" on, Teams taking the microphone shows
   the pop-up within ~2 s, on the display with the pointer, above every
   window including a full-screen Teams window in its own Space.
2. Showing the pop-up and clicking its buttons never switches Space, never
   activates meetily or pulls its main window forward, and never moves
   keyboard focus away from Teams (typing in Teams chat continues to work
   while the pop-up is up and after it closes).
3. **Record** starts a record-only recording named `Teams meeting <date
   time>`. **Not now** closes the pop-up; no new pop-up until Teams releases
   the mic and takes it again. If Teams releases the mic first, or a
   recording is started some other way, the pop-up closes by itself.
4. A pop-up recording stops within ~5 s of Teams releasing the microphone. A
   recording started any other way is never stopped by the detector.
5. After a pop-up recording stops, the meeting is saved, its audio is
   transcribed in one batch with the user's selected transcription model,
   and the meeting page opens (existing auto-summary then applies).
6. Any recording (pop-up or manual) whose audio or transcript is empty is
   deleted: DB rows and its audio folder. A failed transcription is never
   treated as empty.
7. A note `<export folder>/YYYY-MM-DD HHmm - <title>.md` with YAML
   frontmatter, summary (when one exists) and timestamped transcript is
   written after transcription, and rewritten when a summary completes.
8. `cargo check`, `cargo test`, `cargo clippy` (changed code),
   `pnpm exec tsc --noEmit` and `.claude/scripts/frontend-tests.sh` pass, and
   the real-Mac acceptance list in step 3 passes.

## Never touch

- `backend/` (archived), applied sqlx migrations, upstream release/CI files.
- The live-transcription path for manual recordings, apart from the
  empty-recording check after save.
- Recording file locations, audio permissions handling, LLM credential
  handling.

## Stop if

- The step 0 spike shows Teams does not hold the mic for the whole call
  (e.g. releases it when muted). The detection design is then wrong; re-plan.
- The NSPanel cannot be shown over a full-screen Space on the user's macOS
  version. Report; do not ship a pop-up that fails requirement 1.
- A change would need a schema migration. None is planned.

## What exists upstream (verified)

| Need | Code | Notes |
|---|---|---|
| Per-process audio state | `cidre` at the pinned rev `a9587fa`: `core_audio::System::processes()`, `Process::{pid, bundle_id, is_running_input, is_running_output}` (`cidre/src/core_audio/hardware.rs`) | Wraps `kAudioHardwarePropertyProcessObjectList` and `kAudioProcessPropertyIsRunningInput`. meetily only uses the output side today (`audio/system_detector.rs:358`). |
| Tray start | `tray.rs:105-112` sets `sessionStorage.autoStartRecording` and reloads `/`; `useRecordingStart.ts:214` picks it up and runs `checkModelReady` first | Not an event. It also focuses the main window. **Not reused**; see section 3. |
| Stop + save | Stop button → `useRecordingStop`: waits for live transcripts, `api_save_transcript` (no length check), marks IndexedDB recovery entry saved, navigates to `/meeting-details?source=recording` | Save is gated on `transcriptionComplete` (`useRecordingStop.ts:236`). |
| Auto-summary | `meeting-details/page.tsx` when `source=recording` and `useConfig().isAutoSummary` | `isAutoSummary` lives in `localStorage`, **default off**. Rust cannot read it. |
| Batch transcription | `start_retranscription_command(meeting_id, meeting_folder_path, language, model, provider)`, emits `retranscription-complete` / `retranscription-error`; used by `RetranscribeDialog.tsx` | Replaces the meeting's rows in `transcripts` itself. One job at a time (returns an error if busy). |
| Meeting delete | `MeetingsRepository::delete_meeting` | Deletes DB rows only, not the audio folder. `meetings.folder_path` exists. |
| Window close | `lib.rs:601` hides the main window instead of closing | The webview keeps running in the tray, so frontend-owned flows work with the window hidden. |
| Pipeline → transcription | `pipeline.rs:877, 947` `transcription_sender.send(..)`, `warn!` on failure | With no receiver this would warn on every speech segment. |

## Design

Principle (Ponytail ultra): reuse the frontend flows upstream already owns
(start, save, summary, retranscribe) and add only what is missing: a
detector, a pop-up, a record-only switch, an emptiness check, and the
Markdown writer. No new Rust orchestration of summaries.

### 0. Spike first (real Mac, ~30 min, throwaway)

A debug-only Tauri command that logs every 0.5 s, for each CoreAudio
process: `bundle_id`, `pid`, `is_running_input`. Run it through: Teams idle,
pre-join screen, joined, muted, unmuted, screen share, leave, a 1:1 call.
Record which bundle id(s) hold input (new Teams may capture from a helper
process, e.g. a `com.microsoft.teams2.*` WebView helper) and whether input
stays on while muted. This turns the plan's two assumptions into facts. If
they hold, the match rule is `bundle_id` starts with `com.microsoft.teams`.
If not, stop and re-plan.

### 1. Detector (Rust, `audio/meeting_detector.rs`)

- **Probe:** a function `teams_holds_microphone() -> bool` over
  `cidre::core_audio::System::processes()`, true when any process whose
  bundle id matches the spike's rule has `is_running_input()`. Polling every
  0.5 s, not a property listener: Apple forum reports say listeners for
  `kAudioProcessPropertyIsRunningInput` do not fire reliably, and the scan
  is a handful of objects.
- **State machine**, pure and fed by `(now, holds_mic)` samples plus events
  `record`, `dismiss`, `recording_started_elsewhere`; it returns actions:

  ```
  Idle      --mic held ≥ 1.5 s, no recording running--> Prompting   [show_prompt]
  Prompting --record-->                               Recording   [hide_prompt, start]
  Prompting --dismiss / recording started elsewhere--> Dismissed  [hide_prompt]
  Prompting --mic released ≥ 5 s-->                   Idle        [hide_prompt]
  Recording --mic released ≥ 5 s-->                   Idle        [stop]
  Recording --stopped by the user-->                  Dismissed
  Dismissed --mic released ≥ 5 s-->                   Idle
  ```

  Constants, not settings: `START_DELAY = 1.5 s`, `END_GRACE = 5 s`,
  `POLL = 0.5 s`. A mic drop longer than 5 s (headset switch) ends the
  recording and the next grab prompts again:
  `shortcut: 5 s end grace splits a meeting on device switches, raise END_GRACE if it happens often`.
- Unit tests: start delay, end grace, flapping around 5 s, dismiss lasts the
  whole mic session, release while prompting, recording started elsewhere
  while prompting, user stops a pop-up recording early.
- **macOS version:** process objects need macOS 14 (Sonoma) or later (the
  process properties first appear in the macOS 14 SDK). On older versions
  the toggle is disabled with a one-line explanation. Confirm the exact
  minor version against `AudioHardware.h` when building.
- Runs only while the "Ask to record Teams meetings" toggle is on (stored
  with the recording preferences, default off).

### 2. Pop-up (Rust + one small page)

- A second window `meeting-prompt`, created hidden at startup:
  360×110, no decorations, transparent, `skip_taskbar(true)`,
  `focused(false)`, `accept_first_mouse(true)` (first click hits the
  button), all verified on `WebviewWindowBuilder` in tauri 2.6.2.
- Converted to an `NSPanel` with the `tauri-nspanel` crate (git, pinned to a
  commit; it swizzles the Tauri `NSWindow` into an `NSPanel`). New
  dependency, justified in the commit: the same conversion by hand means
  registering an Objective-C subclass and swapping classes in unsafe code we
  would own. Panel settings:
  - style mask `nonactivatingPanel`: "the window is a panel … that does not
    activate the owning app" (AppKit docs), so clicking does not activate
    meetily or switch Space;
  - `becomesKeyOnlyIfNeeded = YES`: buttons work without the panel becoming
    key, so keyboard focus stays in Teams. No Esc shortcut (it would need key
    focus, which requirement 2 forbids);
  - collection behavior `canJoinAllSpaces` ("can appear in all spaces") +
    `fullScreenAuxiliary` ("displays on the same space as the full screen
    window") + `stationary` + `ignoresCycle`;
  - level `statusBar` (above full-screen app windows, below system alerts);
    `hidesOnDeactivate = NO`;
  - shown with `orderFrontRegardless`, positioned top-right of the screen
    containing the pointer; hidden with `orderOut`.
- Page `frontend/src/app/meeting-prompt/page.tsx`: "Teams meeting detected",
  **Record** and **Not now**, accessible names (`a11y-pass`). Buttons call
  `meeting_prompt_respond(record: bool)`.
- If the panel cannot be created at startup, log it and use a plain
  notification ("Teams meeting detected, open meetily to record") instead.

### 3. Start and stop (frontend-owned, like upstream)

- Rust emits `teams-recording-requested { meeting_name }` to the main
  window after Record, and `teams-meeting-ended` when the detector decides a
  pop-up recording should stop.
- `useRecordingStart` listens and runs its existing start function with the
  given name and `recordOnly: true` (same device selection, `STARTING` state,
  `checkModelReady` gate, error toasts). The model gate stays: batch
  transcription needs the model later. It does **not** focus or reload the
  main window (unlike the tray path).
- `useRecordingStop` listens for `teams-meeting-ended` and runs the same stop
  function as the Stop button.
- Rust start command gets one new parameter `record_only: bool`. When true,
  the transcription task is not started and the pipeline is told not to send
  to the transcription channel (a flag next to `transcription_sender` in
  `pipeline.rs`), so there is no per-segment warning. Audio is still saved;
  `auto_save` is forced on for this recording.
- The frontend remembers `recordOnly` for the current recording in its
  recording state context, so `useRecordingStop` knows to skip the
  "wait for live transcripts" step.
- Any recording start notifies the detector (`recording_started_elsewhere`),
  so an open pop-up closes and a late Record click is a no-op.

### 4. After stop

**Pop-up recording (record-only)** in `useRecordingStop`:
1. Save the meeting as today (zero transcripts) and mark the IndexedDB
   recovery entry saved.
2. `start_retranscription_command` with the user's selected transcription
   model/provider/language, as `RetranscribeDialog` does. If another
   retranscription is running, queue it in the frontend (a simple array; one
   job at a time).
3. On `retranscription-complete`: run the empty check (below). If kept,
   export the note, then navigate to `/meeting-details?source=recording` so
   the existing auto-summary runs when the user has it on.
4. On `retranscription-error`: keep meeting and audio, toast "Transcription
   failed, retry from the meeting page", export a note without transcript.

**Manual recording (live)**: after the existing save, run the empty check
unless a `transcription-error` event was seen during this recording; then
export the note; then navigate as today.

**Empty check**: Rust command `discard_meeting_if_empty(meeting_id) ->
bool`. It re-reads the transcripts from SQLite. Empty means no segments, or
under 3 words in total (Whisper invents "Thank you." in silence):
`shortcut: word-count emptiness, use VAD speech duration if real meetings get deleted`.
If empty: read `folder_path`, delete via `MeetingsRepository::delete_meeting`,
then remove the folder only if its canonical path is inside the configured
recordings folder. Returns `true` when deleted; the caller then toasts
"Recording had no speech, deleted" and goes home instead of to the meeting
page. Unit-tested on: no rows, filler only, one real sentence, folder
outside the recordings root (not removed).

No-audio case: if the stop result reports no audio file, skip
transcription and delete the same way.

### 5. Obsidian Markdown export

- Rust command `export_meeting_markdown(meeting_id) -> PathBuf` in
  `src/obsidian_export.rs`. Pure helpers `render_markdown(..)` and
  `safe_file_name(..)` with unit tests (frontmatter escaping, empty summary,
  odd title characters `* " \ / < > : | ? # ^ [ ]`, length cap).
- Called after save/transcription (section 4) and again from
  `useSummaryGeneration` when a summary reaches `completed`, so the note
  picks up the summary.
- File `YYYY-MM-DD HHmm - <title>.md`; overwrite only if its frontmatter
  `meetily_id` matches, else add ` (2)`. Temp file + rename.
- Frontmatter: `title`, `date`, `start`, `end`, `duration_min`, `source`
  (`teams` or `manual`), `meetily_id`, `tags: [meeting]`. Body: `# Title`,
  `## Summary` (headings demoted), `## Transcript` with `**[hh:mm:ss]**`
  lines.
- Export folder: one setting with a folder picker (`tauri-plugin-dialog`).
  Unset means no export. Audio never goes into the vault.

### 6. Settings (two items)

- "Ask to record Teams meetings" (off by default) with the consent note:
  recording other people may require telling them.
- "Obsidian export folder" (Choose…).

Auto-summary stays the existing toggle (default off). Without it, notes have
the transcript and no summary until the user generates one.

## Implementation order (one feature per session)

0. Spike (section 0). Throwaway; record findings in this file.
1. Markdown export + export folder setting + empty check, wired into the
   **manual** stop flow. Useful on its own; no detector needed.
2. Record-only mode + after-stop flow for record-only recordings, testable
   with a temporary dev trigger before the detector exists.
3. Detector + pop-up + start/stop wiring + toggle. Real-Mac acceptance:
   full-screen Teams in its own Space (pop-up visible; Record keeps you in
   that Space; typing in Teams chat keeps working); another full-screen app;
   normal desktop; second display; Not now; mic released while prompting;
   manual start while prompting; meeting end → stop → transcript → note.

Later, not in this plan: Windows, other meeting apps, merging split
meetings, pre-roll audio before the click.

## Risks

- **Teams behaviour assumptions** (mic held whole call, held while muted,
  which process holds it). Settled by the spike; "Stop if" covers failure.
- **Pop-up over full-screen** depends on AppKit panel behaviour; the real-Mac
  acceptance list is the gate. `tauri-nspanel` must build against tauri
  2.6.2; check before step 3.
- **Frontend-owned flow** needs the webview alive. It is: closing the window
  hides it (`lib.rs:601`). Quitting meetily mid-transcription leaves a saved
  meeting with no transcript; the user can retranscribe from the meeting
  page: `shortcut: no resume of interrupted batch transcription after quit`.
- **Audio before Record is lost.** Usually only pre-join.
- **Deleting the wrong thing**: only after a successful transcription, only
  under 3 words, folder removal only inside the recordings root.

## Review log (revision 2)

- **Upstream code (independent verifier, 15 findings, all confirmed):** tray
  start is a sessionStorage flag + reload, not an event; auto-summary is a
  localStorage toggle, default off, unreadable from Rust;
  `stop_recording_handler` is private and focuses the main window; the
  record-only flag had no path to the frontend; the pipeline warns on every
  send without a receiver; `process_transcript_background` reports no result
  and needs provider/model/keys from the frontend; the frontend gates every
  start on `checkModelReady`; the save has no length check and is skipped
  when transcription never completes; no transition for a manual start while
  prompting; Esc vs. no-focus contradiction; Windows design was scope creep;
  Teams mic behaviour unverified. All addressed above.
- **Official sources:** `cidre` rev `a9587fa` has `processes()`,
  `bundle_id()`, `is_running_input()`. Apple AppKit docs confirm
  `nonactivatingPanel` (panels only), `canJoinAllSpaces`,
  `fullScreenAuxiliary`, `statusBar` level. Process objects: macOS 14 SDK,
  not 14.2 as revision 1 said. Tauri 2.6.2 `WebviewWindowBuilder` has
  `focused`, `always_on_top`, `visible_on_all_workspaces`, `skip_taskbar`,
  `decorations`, `transparent`, `accept_first_mouse`, `ns_window`; it has no
  `focusable`.
- **Ponytail ultra cuts:** Rust after-meeting pipeline and summary
  orchestration (reuse frontend flows instead); tunable timings and app list
  (constants); status line; manual "Export to Obsidian" button (export is
  automatic); Windows design; skipping the model check for record-only (the
  model is needed afterwards anyway).
- **Loopkit:** added "Never touch" and "Stop if" (`spec-first`), a spike
  before building on unverified facts (`systematic-debugging`), and a fresh
  verifier pass (`adversarial-verify`, `self-eval-bias`).
