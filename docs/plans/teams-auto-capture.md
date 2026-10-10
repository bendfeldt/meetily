# Plan: Teams meeting prompt → record → transcribe after → Obsidian note

Status: proposed, not started. Fork-only feature (`bendfeldt/meetily`).
Revision 4: reviewed three times by independent verifiers against the upstream code, the Apple/Tauri/cidre sources
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
   the mic and takes it again. If Teams releases the mic for 5 s before the
   user answers, or a recording is started some other way, the pop-up
   closes by itself. This works whatever page meetily's window shows, and
   with the window hidden.
4. A pop-up recording stops within ~5 s of Teams releasing the microphone. A
   recording started any other way is never stopped by the detector.
5. After a pop-up recording stops, the meeting is saved, its audio is
   transcribed in one batch with the configured transcription model and
   language, the note is written, and the meeting appears in the sidebar.
   If auto-summary is on, the meeting page opens and summarizes as today.
6. Any recording (pop-up or manual) whose audio or transcript is empty is
   deleted: DB rows and its audio folder. A failed transcription is never
   treated as empty.
7. A note `<export folder>/YYYY-MM-DD HHmm - <title>.md` with YAML
   frontmatter, summary (when one exists) and timestamped transcript is
   written after transcription, and rewritten when a summary completes,
   even if the user has left the meeting page.
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
  version. Report; do not ship a pop-up that fails requirement 1, and do not
  substitute a notification.
- A change would need a schema migration. None is planned.

## What exists upstream (verified)

| Need | Code | Notes |
|---|---|---|
| Per-process audio state | `cidre` at the pinned rev `a9587fa`: `core_audio::System::processes()`, `Process::{pid, bundle_id, is_running_input, is_running_output}` (`cidre/src/core_audio/hardware.rs`) | Wraps `kAudioHardwarePropertyProcessObjectList` and `kAudioProcessPropertyIsRunningInput`. meetily only uses the output side today (`audio/system_detector.rs:358`). |
| Tray start | `tray.rs:105-112` sets `sessionStorage.autoStartRecording` and reloads `/`; `useRecordingStart.ts:214` picks it up and runs `checkModelReady` first | Not an event; focuses the main window. `useRecordingStart` is mounted only on the home page and its start function takes no name. **Not reused.** |
| Rust start | `recording_commands.rs:495 start_recording_with_devices_and_meeting(app, mic, system, meeting_name)` → validates the configured model (`transcription/engine.rs:55`), starts `RecordingManager`, calls `start_transcription_task` (`:618`), emits `recording-started` | The UI's `RecordingStateContext` follows `recording-started` / `recording-stopped` events, so a Rust-started recording shows up in the UI on any page. |
| Stop | `recording_commands.rs:676 stop_recording` → `Result<(), String>` (returns early with no event if nothing is recording), emits `recording-stopped { folder_path, meeting_name }` (`:1084`). Save runs only on `recording-stop-complete` (tray) or `RecordingControls` → `onRecordingStop(true)`; `useRecordingStop`'s `recording-stopped` listener only stores `folder_path`/name in `sessionStorage` | A failed audio save during stop (e.g. no checkpoints) is logged and the stop still succeeds (`:1030-1040`). `IS_RECORDING` clears only at the end of the stop tail (`:1051`). |
| Recovery store | `TranscriptContext.tsx:94-118` writes an unsaved IndexedDB entry on every `recording-started`; the home page offers unsaved entries for recovery | A record-only recording would show up as a 0-transcript recovery item unless skipped. |
| Save | `useRecordingStop`: waits for live transcripts, `api_save_transcript` (no length check), marks IndexedDB recovery entry saved, schedules navigation to `/meeting-details?source=recording` after 2 s | Save only runs when `transcriptionComplete` (`useRecordingStop.ts:236`); else the status goes to IDLE with no save. |
| Transcription config in Rust | `api::api_get_transcript_config` (provider + model, from the DB) and `get_language_preference_internal()` (`lib.rs:443`, static at `:125`) | Provider/model are persisted. The language is in memory, default `auto-translate`, and set by the UI when `ConfigContext` mounts (`ConfigContext.tsx:218-224`), which happens at app start because the main webview always loads. |
| Recordings root | `recording_saver.rs:226` always uses `get_default_recordings_folder()` | `RecordingPreferences.save_folder` is not used by the writer. |
| Auto-summary | `meeting-details/page.tsx` when `source=recording` and `useConfig().isAutoSummary` | `isAutoSummary` lives in `localStorage`, **default off**. Rust cannot read it. |
| Batch transcription | `retranscription::start_retranscription(app, meeting_id, folder_path, language, model, provider) -> anyhow::Result<RetranscriptionResult>` (async, awaitable); the `_command` wrapper spawns it and reports via events | Replaces the meeting's rows in `transcripts`. One job at a time: busy → `Err("Retranscription already in progress")`. No audio → `Err("No audio file found…")`; silence → `Err("No speech detected in audio file")` (`:297`), both before any DB write. Errors are plain strings. Parakeet gets `language = None` (`RetranscribeDialog.tsx:207`). |
| Summary rename | `summary/service.rs:604-616` renames the meeting from the summary's title | The note's file name must not follow the title. |
| Settings save | `set_recording_preferences` takes the whole struct from `RecordingSettings.tsx`; a deserialize error resets all preferences to defaults (`recording_preferences.rs:121-124`) | New fields there would be wiped by the TS save unless every TS copy is updated. |
| Meeting delete | `MeetingsRepository::delete_meeting` | Deletes DB rows only, not the audio folder. `meetings.folder_path` exists. |
| Summary completion | `summary/service.rs process_transcript_background` writes the result to `summary_processes` | The Rust place where every summary finishes, whether or not a page is open. |
| Window close | `lib.rs:604` hides the main window instead of closing | The webview keeps running in the tray, so frontend-owned flows work with the window hidden. |
| Pipeline → transcription | `pipeline.rs:877, 947` `transcription_sender.send(..)`, `warn!` on failure | With no receiver this would warn on every speech segment. |

## Design

Principle (Ponytail ultra): the pop-up path needs no page to be open, so it
lives in Rust and calls the existing Rust functions directly (start, stop,
save, batch transcription). The manual path stays as upstream wrote it and
only gains the empty check. Summaries stay where they are (frontend-started,
LLM config held by the UI); the only Rust hook there is the note export when
a summary finishes.

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
- **State machine**, pure. Inputs: `(now, holds_mic, recording_active)`
  samples, where `recording_active` mirrors the existing
  `recording-started` / `recording-stopped` events from any source; user
  events `record`, `dismiss`; results `start_ok`, `start_failed`,
  `stop_done`. Outputs are actions:

  ```
  Idle      --mic held ≥ 1.5 s, !recording_active-->   Prompting  [show_prompt]
  Idle      --mic held ≥ 1.5 s,  recording_active-->   Dismissed  (a manual recording owns this call)
  Prompting --record-->                                Starting   [hide_prompt, start]
  Prompting --dismiss, or recording_active turns true-> Dismissed [hide_prompt]
  Prompting --mic released ≥ 5 s-->                    Idle       [hide_prompt]
  Starting  --start_ok-->                              Recording
  Starting  --start_failed-->                          Dismissed  [notify reason]
  Recording --mic released ≥ 5 s-->                    Stopping   [stop]
  Recording --recording_active turns false-->          Dismissed  (user stopped it)
  Stopping  --stop_done-->                             Idle, or Dismissed if mic is held again
  Dismissed --mic released ≥ 5 s-->                    Idle
  ```

  The detector awaits its own start and stop calls, so it never needs to
  recognise its recording in an event; while `Starting`/`Stopping`, changes
  of `recording_active` are ignored. A Teams re-grab during `Stopping`
  cannot open a pop-up, which avoids "Recording already in progress".

  Constants, not settings: `START_DELAY = 1.5 s`, `END_GRACE = 5 s`,
  `POLL = 0.5 s`. A mic drop longer than 5 s (headset switch) ends the
  recording and the next grab prompts again:
  `shortcut: 5 s end grace splits a meeting on device switches, raise END_GRACE if it happens often`.
- Unit tests: start delay, end grace, flapping around 5 s, dismiss lasts the
  whole mic session, release while prompting, manual recording started
  during the 1.5 s hold and while prompting, start failure, user stops a
  pop-up recording early, mic re-grab while stopping.
- **macOS version:** process objects need macOS 14 (Sonoma) or later (the
  process properties first appear in the macOS 14 SDK). On older versions
  the toggle is disabled with a one-line explanation. Confirm the exact
  minor version against `AudioHardware.h` when building.
- Runs only while the "Ask to record Teams meetings" toggle is on (see
  section 6 for storage, default off).

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
- If the panel cannot be created at startup, the detector does not run and
  Settings shows "Pop-up unavailable" with the logged reason. No silent
  substitute (see Stop if).

### 3. Start and stop (Rust-owned for pop-up recordings)

- **Start:** after Record, `audio/meeting_detector.rs` calls
  `start_recording_with_devices_and_meeting` with the preferred devices from
  `RecordingPreferences` (`preferred_mic_device`, `preferred_system_device`,
  `None` = default), the name `Teams meeting <date time>`, and the new
  `record_only = true`. Model validation stays (the batch job needs the
  model). `Ok` feeds `start_ok`; an `Err` feeds `start_failed` and shows a
  notification with the reason. The UI picks up the recording through the
  existing `recording-started` event on whatever page is open; nothing is
  focused or reloaded.
- **`record_only` plumbing** (the real list of changes):
  - Only `start_recording_with_devices_and_meeting`
    (`recording_commands.rs:495`) gains `record_only: bool`; it is the one
    the detector calls. The existing callers (`lib.rs:364` wrapper and the
    other Rust callers) pass `false`. `start_recording_with_meeting_name`
    (`:307`, used by `start_recording` and the `(None, None)` branch of the
    `lib.rs:364` wrapper) is untouched: manual starts are never
    record-only. No Tauri command signature changes, so no TS caller
    changes.
  - When `record_only`: skip `start_transcription_task` at `:618`, and force
    `auto_save = true` instead of the preference read at `:562-569`.
  - `RecordingManager::start_recording` (`recording_manager.rs:228`) and the
    pipeline constructors (`pipeline.rs:715`, `:986`) get a
    `send_to_transcription: bool` so the VAD path skips the send instead of
    warning on every segment.
  - `recording-started` and `recording-stopped` payloads gain
    `record_only: bool` (additive JSON field). TS: `TranscriptContext`
    skips creating the IndexedDB recovery entry when it is true (nothing
    streams, so there is nothing to recover), and `useRecordingStop`'s
    `recording-stopped` listener stores it in `sessionStorage` next to
    `last_recording_folder_path`.
- **Stop:** on `[stop]` the detector awaits `stop_recording` (no
  `recording-stop-complete`, so no frontend save) and then feeds
  `stop_done`.
- **Hand-off to the worker, one place for every stop:** at startup Rust
  registers `app.listen("recording-stopped", …)`. When the payload has
  `record_only: true`, it enqueues `(folder_path, meeting_name)` for the
  after-meeting worker. This covers the detector's stop and a manual stop of
  a pop-up recording (Stop button or tray) alike.
- **Manual stop of a pop-up recording:** the Stop button / tray path still
  runs `useRecordingStop`'s save handler. It reads the `sessionStorage`
  `record_only` flag and, when set, skips the live-transcript wait and the
  save (the worker saves) and only resets the UI to idle.

### 4. After stop

**Pop-up recording: Rust worker (`audio/after_meeting.rs`).** One
`tokio::sync::Mutex`-guarded worker, jobs processed in order:
1. Save the meeting via the same function `api_save_transcript` uses, with
   zero transcripts and the folder path.
2. Await `retranscription::start_retranscription` (not the `_command`
   wrapper) with `api_get_transcript_config` provider/model and
   `get_language_preference_internal()` (`None` for Parakeet). Awaiting it
   directly removes the event-filtering and "Ok then fails later" problems.
3. Classify the result. The three error strings become `pub const`s in
   `retranscription.rs`, used where they are raised and where they are
   matched (a small additive edit; no error-type refactor):
   - `ALREADY_IN_PROGRESS` (the Retranscribe dialog is running): sleep 10 s
     and retry the same job, indefinitely; the dialog job always ends.
   - `NO_AUDIO_FILE` or `NO_SPEECH`: the recording is **empty** → delete
     (below).
   - `Ok`: run the empty check (below), which also catches Whisper filler.
   - Any other error is a **failure**: keep the meeting and audio, notify
     "Transcription failed, retry from the meeting page", export a note
     without transcript.
4. Export the note (section 5), unless deleted.
5. Emit `meeting-ready { meeting_id }` (or `meeting-discarded`). The layout-level
   `RecordingPostProcessingProvider` refreshes the sidebar and, if
   `isAutoSummary` is on, navigates to `/meeting-details?id=…&source=recording`
   so the existing auto-summary runs.

Engine sharing: pop-up recordings use no live engine, so back-to-back Teams
meetings never clash with the worker. A *manual* live recording started
while a batch job runs shares the engine; that is possible today with the
Retranscribe dialog too. The worker does not start a new job while a live
recording runs, and step 2 tests "manual recording during batch job" on a
real Mac before shipping.

**Manual recording (live)**, in `useRecordingStop` after the existing save
succeeds: unless a `transcription-error` was seen during this recording,
await `discard_meeting_if_empty(meeting_id)` **before** scheduling the
2 s navigation. If it returns `true`: toast "Recording had no speech,
deleted", navigate home, skip the meeting-page navigation. Otherwise export
the note and continue as today. If transcription never completed, today's
code saves nothing; that stays as is.

**Empty check**, Rust `discard_meeting_if_empty(meeting_id) -> bool`, used
by both paths, plus `discard_meeting(meeting_id)` for the worker's
known-empty cases. It re-reads transcripts from SQLite. Empty = no segments or
under 3 words in total (Whisper invents "Thank you." in silence):
`shortcut: word-count emptiness, use VAD speech duration if real meetings get deleted`.
If empty: read `meetings.folder_path`, delete rows with
`MeetingsRepository::delete_meeting`, then `remove_dir_all` the folder only
if its canonical path is inside canonical `get_default_recordings_folder()`
(the root the writer actually uses). File removal is new code; unit-tested
on: no rows, filler only, one real sentence, folder outside the root (kept),
symlink pointing outside the root (kept).

### 5. Obsidian Markdown export

- Rust command `export_meeting_markdown(meeting_id) -> PathBuf` in
  `src/obsidian_export.rs`. Pure helpers `render_markdown(..)` and
  `safe_file_name(..)` with unit tests (frontmatter escaping, empty summary,
  odd title characters `* " \ / < > : | ? # ^ [ ]`, length cap).
- Called by the after-meeting worker, by the manual path after the empty
  check, and from Rust at the end of a successful
  `process_transcript_background` (`summary/service.rs`), so a summary that
  finishes after the user left the page still updates the note. Export
  errors are logged and shown once as a notification; they never fail the
  summary.
- File name is fixed at the **first** export: `YYYY-MM-DD HHmm - <title>.md`.
  Later exports (e.g. after the summary renamed the meeting) find the
  existing note by its frontmatter `meetily_id` and overwrite it in place,
  keeping its name so Obsidian links stay valid; the new title goes into
  `title:` and the `#` heading. Lookup reads the first 1 KB of each `.md`
  directly in the export folder:
  `shortcut: linear scan of the export folder per export, keep an id→file index if the folder grows to thousands of notes`.
  A name clash with a different `meetily_id` gets ` (2)`. Temp file +
  rename.
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

Storage: both live in Rust because Rust reads them (detector, export), in
a new fork-owned `fork_settings.rs` with its own small JSON file next to
the recording preferences (same load/save mechanism), and its own
`get_fork_settings` / `set_fork_settings` commands. They are **not** added
to `RecordingPreferences`: its TS settings page saves the whole struct and
would wipe unknown fields, and keeping them separate keeps upstream files
untouched. No DB migration.

Auto-summary stays the existing toggle (default off). Without it, notes have
the transcript and no summary until the user generates one.

## Implementation order (one feature per session)

0. Spike (section 0). Throwaway; record findings in this file.
1. Markdown export + export folder setting + empty check, wired into the
   **manual** stop flow and the summary-completion hook. Useful on its own;
   no detector needed.
2. Record-only plumbing + after-meeting worker, testable with a temporary
   dev trigger before the detector exists. Real-Mac checks: record-only
   recording → note; manual stop of a record-only recording; manual live
   recording during a batch job; empty record-only recording deleted.
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
- **Language before the UI syncs.** The batch language comes from the
  in-memory preference the UI sets at startup. The worker only runs after a
  recording, by which time the main webview has loaded, so it is synced.
  Covered by the step 2 real-Mac check (set a non-default language, record,
  verify the batch used it).
- **Quitting mid-job.** Quitting meetily while the worker transcribes leaves
  a saved meeting with no transcript; the user can retranscribe from the
  meeting page: `shortcut: no resume of interrupted batch transcription after quit`.
- **Fork divergence.** This feature edits upstream files (`recording_commands.rs`,
  `recording_manager.rs`, `pipeline.rs`, `useRecordingStop.ts`,
  `summary/service.rs`, `lib.rs`, `recording_preferences.rs`). Keep each
  edit minimal and additive so upstream syncs conflict rarely; new logic goes
  in new files.
- **Audio before Record is lost.** Usually only pre-join.
- **Deleting the wrong thing**: only after a successful transcription, only
  under 3 words, folder removal only inside the recordings root.

## Review log

### Revision 4 (third independent verifier, 13 findings, confirmed)

`stop_recording` returns no folder path; a failed audio save does not fail
the stop; silence makes retranscription return `Err("No speech detected")`
(must count as empty); the IndexedDB recovery entry is written on every
`recording-started`; the save gate is `recording-stop-complete`, not
`recording-stopped`; `start_recording_with_meeting_name` was missing from
the plumbing list; `recording-started` carries no meeting identity; the
guard error is a plain string; the summary renames the meeting (two notes);
no `Stopping` state; `Idle` ignored recording state; wrong line for the
language preference and its UI-synced nature; new `RecordingPreferences`
fields would be wiped by the TS save. Fixed by: a Rust `recording-stopped`
listener as the single hand-off; error-string constants; skipping the
recovery entry for record-only; the detector awaiting its own start/stop;
a `Stopping` state and `recording_active` input; note file names fixed at
first export; separate fork settings file.

### Revision 3 (second independent verifier, 19 findings, confirmed)

Record-only meetings would never be saved (save gated on
`transcriptionComplete`); `useRecordingStart` is home-page-only and takes
no name; the Stop button, not `useRecordingStop`, calls `stop_recording`;
the `record_only` plumbing touches three entry points, `RecordingManager`
and both pipeline constructors; `RetranscribeDialog` picks its own model
and Parakeet needs `language = None`; the retranscription command returns
Ok before taking its guard and events need meeting-id filtering; stop
returns no file and fails with no checkpoints; "No audio file found" is an
error, not "empty"; the writer ignores `save_folder`; summary completion
has no hook once the page unmounts; the 2 s navigation races the delete;
`recordOnly` had nowhere to live; the notification fallback contradicted
Stop if; missing `start_failed` / user-stop events; 5 s close delay
unstated; engine sharing with a live recording; settings storage unstated.
Fixed by moving the pop-up path into Rust (awaiting
`start_retranscription` directly, model/language from Rust config), adding
the `record_only` event field, the summary-completion export hook in
`service.rs`, explicit storage, and the state-machine events above.

### Revision 2

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
