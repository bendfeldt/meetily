# Plan: Teams meeting prompt → record → transcribe after → Obsidian note

Status: proposed, not started. Fork-only feature (`bendfeldt/meetily`).
Revision 5. Revisions 1-4 were reviewed four times by independent verifiers
against the upstream code, the Apple/Tauri/cidre sources, Ponytail ultra and
the loopkit skills; the condensed review log is at the end.

## Decisions (from the user)

- macOS only. No live Mac testing during development: macOS code is
  compiled and unit-tested by the fork's macOS GitHub Actions workflow;
  behaviour that only a real call can show is assumed to work when it relies
  on documented APIs or on paths that already work upstream.
- When Teams takes the microphone, a pop-up asks whether to record. It must
  be visible on top of everything, **including full-screen apps**.
- A recording started from the pop-up stops by itself 5 s after Teams lets go
  of the microphone. Manual recordings are never auto-stopped.
- Pop-up recordings are transcribed **after** the meeting when the configured
  transcription provider can batch-transcribe (local Whisper or Parakeet).
  With a cloud provider (Deepgram, ElevenLabs, Groq, OpenAI) they **fall
  back to live transcription**, as manual recordings do.
- Empty recordings (no audio, or no speech) are deleted, pop-up and manual.
- Obsidian export is **opt-in per meeting via an "Export to Obsidian"
  button**. Once a meeting has a note, the note is kept in sync, and it is
  **renamed** when the meeting title changes (links to the old name break;
  accepted).
- Timings: prompt ~1.5 s after Teams takes the mic; stop 5 s after release.

## Done when

1. With "Ask to record Teams meetings" on, Teams taking the microphone shows
   the pop-up within ~2 s on the display with the pointer, above every window
   including full-screen apps in their own Space.
2. Showing the pop-up and clicking its buttons does not switch Space,
   activate meetily, pull its main window forward, or take keyboard focus
   from Teams.
3. **Record** starts a recording named `Teams meeting <date time>`,
   record-only for batch-capable providers, live otherwise. **Not now** hides
   the pop-up until Teams releases the mic and takes it again. The pop-up
   also hides when Teams has released the mic for 5 s, when any recording
   starts, or when the feature is switched off. Works on any page and with
   the window hidden.
4. A pop-up recording stops within ~5 s of Teams releasing the microphone.
   Recordings started any other way are never stopped by the detector.
5. After a record-only pop-up recording stops, the meeting is saved and its
   audio is transcribed in one batch with the configured model and language.
   After a live pop-up recording stops, it is saved through the existing
   live save path. In both cases the meeting appears in the sidebar, and the
   meeting page opens if auto-summary is on.
6. Any recording whose audio or transcript is empty is deleted (DB rows and
   audio folder). A failed transcription is never treated as empty.
7. The meeting page has an **Export to Obsidian** button. The first click
   asks for the export folder if none is set, then writes
   `<folder>/YYYY-MM-DD HHmm - <title>.md`. After that the note is rewritten
   when the summary is generated or edited, and renamed when the title
   changes. Meetings never exported get no note.
8. Linux container: `cargo check`, `cargo test`, `cargo clippy` (no new
   warnings in changed code), `tsc --noEmit`,
   `.claude/scripts/frontend-tests.sh` pass. Fork macOS workflow
   (`fork-macos-rust-checks.yml`): green on the branch head. Live-only
   behaviour is listed under "Assumed, not tested" in `IMPLEMENTATION_PLAN.md`,
   never claimed as passed.

## Never touch

- `backend/` (archived), applied sqlx migrations, upstream release/CI files.
- The live transcription path itself.
- Recording file locations, audio permission handling, LLM credential
  handling.

## Stop if

- The macOS workflow cannot compile the meetily crate before any feature
  code lands (then fix CI first).
- `tauri-nspanel` cannot build against tauri 2.6.2 on the macOS runner.
- A change would need a schema migration. None is planned.

## Verification strategy

| What | Where | How |
|---|---|---|
| Pure logic: detector state machine, emptiness rule, error classification, Markdown rendering, file naming/renaming, settings load | Platform-neutral modules (no `cfg`) | `cargo test` in the Linux container **and** on the macOS runner |
| macOS glue: `cidre` probe, NSPanel setup, detector thread | `#[cfg(target_os = "macos")]` modules, kept thin (no logic beyond calling the pure modules) | Compiled, linted and test-built only by the macOS workflow; Linux `cargo check` skips them |
| Frontend wiring | `frontend/src` | `tsc --noEmit`, frontend tests, plus new Bun tests for the save-path branches |
| Teams holds the mic for the whole call (also muted) and its process bundle ids start with `com.microsoft.teams`; pop-up shows over full-screen Spaces without taking focus (documented AppKit panel behaviour) | Real Mac only | **Assumed.** The detector logs the matched bundle ids at debug level so the first real use produces evidence. Listed under "Assumed, not tested". |

## What exists upstream (verified)

| Need | Code | Notes |
|---|---|---|
| Per-process audio state | `cidre` rev `a9587fa` `core_audio::System::processes()`, `Process::{pid, bundle_id → cf::String, is_running_input}` | meetily uses only the output side today (`audio/system_detector.rs:358`). Apple forum reports: listeners for `IsRunningInput` are unreliable, so poll. |
| Start with bookkeeping | `lib.rs` private `start_recording_with_devices_and_meeting` (~`:364`): dispatches to `audio::recording_commands::start_recording_with_meeting_name` (`:307`, no devices) or `…_with_devices_and_meeting` (`:495`), then sets `RECORDING_FLAG`, updates the tray, shows the start notification | Crate-root private fns are callable from child modules. The detector calls this one. |
| Stop | `recording_commands.rs:676 stop_recording` → `Result<(), String>`; no-op `Ok` when not recording; a failed audio save is logged and the stop still succeeds (`:1030-1040`); emits `recording-stopped { folder_path: Option, meeting_name }` (`:1084`); `IS_RECORDING` clears at `:1051` | Tray stop then emits `recording-stop-complete`. |
| Live save | `RecordingPostProcessingProvider` (layout level, every page) listens to `recording-stop-complete` → `useRecordingStop` save → `api_save_transcript` → navigate after 2 s. `RecordingControls` (home page only) calls `onRecordingStop(true)` | Save only runs when `transcriptionComplete`. |
| Recovery store | `TranscriptContext.tsx:94-118` writes an unsaved IndexedDB entry on `recording-started`; `:145-150` writes `folderPath` on `recording-stopped` | `recordingService.onRecordingStarted(cb: () => void)` drops the payload. |
| Batch transcription | `retranscription::start_retranscription(...)` awaitable; supports `parakeet` and `localWhisper`/`whisper`; errors are strings: `"Retranscription already in progress"`, `"No audio file found in: {path}"`, `"No speech detected in audio file"` (`:297`); emits progress/complete/error events itself; unloads the engine after the job unless recording (`common.rs:20-45`) | |
| Transcription config | `api_get_transcript_config` (DB); `get_language_preference_internal()` (`lib.rs:443`, in-memory, set by `ConfigContext` on startup) | |
| Summary writes | `summary/service.rs process_transcript_background` (generation, may rename the meeting `:604-616`); `summary/commands.rs:95 api_save_meeting_summary` (user edits) | Two places to hook note sync. |
| Meeting data | `meetings(id, title, created_at, updated_at, folder_path)`; `transcripts.audio_end_time` | No meeting end time column. |
| Delete | `MeetingsRepository::delete_meeting` deletes rows only | Writer root: `get_default_recordings_folder()` (`recording_saver.rs:226`). |
| Folder picker | `tauri_plugin_dialog` registered in Rust (`lib.rs:468`); no `@tauri-apps/plugin-dialog` npm package | Use the Rust dialog API from a command; no new npm dependency. |
| Store | `tauri-plugin-store` file `recording_preferences.json` (`recording_preferences.rs`) | |

## Design

### 1. Detector (`audio/meeting_detector.rs`, pure) + probe (`audio/meeting_detector_macos.rs`, cfg macOS)

- **Probe** (macOS): every 0.5 s, `processes()`; for each, `bundle_id()`
  converted with `to_string()`; match `starts_with("com.microsoft.teams")`
  (new Teams and its helpers, classic Teams) and `is_running_input()`.
  Errors from a single process are skipped; a failing `processes()` call
  counts as "not held". Matched ids logged at debug level.
- **Inputs per tick:** `now`, `teams_holds_mic`, `recording_active` read
  from the single source of truth `crate::is_recording()` (not events).
- **Other inputs:** `record`, `dismiss`, `feature_off`, and results
  `start_ok`, `start_failed`, `stop_done`, `stop_failed`.
- **States and transitions** (`held`/`released` mean continuously for the
  given time; "any" means every state not listed earlier for that input):

  | State | Input | Next | Action |
  |---|---|---|---|
  | Idle | held ≥ 1.5 s, !recording_active | Prompting | show_prompt |
  | Idle | held ≥ 1.5 s, recording_active | Dismissed | – |
  | Prompting | record | Starting | hide_prompt, start |
  | Prompting | dismiss, or recording_active | Dismissed | hide_prompt |
  | Prompting | released ≥ 5 s | Idle | hide_prompt |
  | Starting | start_ok | Recording (if released ≥ 5 s meanwhile: Stopping + stop) | – |
  | Starting | start_failed | Dismissed | notify reason |
  | Recording | released ≥ 5 s | Stopping | stop |
  | Recording | !recording_active (user or fallback stopped it) | Dismissed | – |
  | Stopping | stop_done / stop_failed | Idle (Dismissed if held again) | notify on failure |
  | Dismissed | released ≥ 5 s | Idle | – |
  | any | feature_off | Idle | hide_prompt; a running pop-up recording continues as a normal recording (no auto-stop) |
  | any | record / dismiss outside Prompting | unchanged | – (late click) |

  Recording-state changes are ignored in Starting and Stopping. Constants:
  `START_DELAY 1.5 s`, `END_GRACE 5 s`, `POLL 0.5 s`;
  `shortcut: 5 s end grace splits a meeting on device switches, raise END_GRACE if it happens often`.
- Unit tests cover every row, plus flapping around the 5 s edge.
- macOS < 14: `processes()` errors or returns nothing; the Settings toggle
  shows "Requires macOS 14 or later" when the first probe fails. (Exact
  minor version not verifiable without the SDK header; the runtime check is
  the guard.)

### 2. Pop-up (`meeting_prompt_macos.rs` + page `app/meeting-prompt/page.tsx`)

- Window `meeting-prompt`, created hidden at startup with
  `WebviewWindowBuilder` (tauri 2.6.2): 360×110, `decorations(false)`,
  `transparent(true)`, `skip_taskbar(true)`, `focused(false)`,
  `accept_first_mouse(true)`.
- Converted with `tauri-nspanel` (macOS-only git dependency in
  `[target.'cfg(target_os = "macos")'.dependencies]`, pinned to a commit on
  its `v2` branch chosen when adding it; justification in the commit: it
  replaces unsafe Objective-C class swizzling we would otherwise own).
  Panel: style `nonactivatingPanel`; `becomesKeyOnlyIfNeeded = YES` (buttons
  work without taking key focus; no Esc shortcut); collection behaviour
  `canJoinAllSpaces | fullScreenAuxiliary | stationary | ignoresCycle`;
  level `statusBar`; `hidesOnDeactivate = NO`; show with
  `orderFrontRegardless` at the top-right of the screen with the pointer;
  hide with `orderOut`.
- Page: "Teams meeting detected", **Record** / **Not now**, accessible
  names. Buttons call `meeting_prompt_respond(record: bool)`.
- If the panel cannot be created, the detector does not start and Settings
  shows the logged reason. No notification substitute.

### 3. Start and stop of pop-up recordings

- **Mode:** `record_only = provider ∈ {localWhisper, whisper, parakeet}` from
  `api_get_transcript_config`, decided at Record time.
- **Start:** the detector calls the `lib.rs` start function (bookkeeping
  included) with preferred devices from `RecordingPreferences` and the name.
  When no device is preferred it passes the system defaults' names, so the
  dispatch always reaches `…_with_devices_and_meeting`. Nothing is focused or
  reloaded.
- **`record_only` plumbing:**
  - `lib.rs` start function and `recording_commands.rs:495` gain
    `record_only: bool`. Every existing caller passes `false`: `lib.rs:141`
    `start_recording`, the `start_recording_with_devices_and_meeting` Tauri
    command (its TS callers pass `recordOnly: false`), and
    `recording_commands.rs:491`.
  - `RecordingManager::start_recording` gains `send_to_transcription: bool`;
    `start_recording_with_meeting_name` (`:405`) passes `true`.
  - `AudioPipelineManager::start` (`pipeline.rs:984`) takes the flag and
    skips `transcription_sender.send` when false.
  - When `record_only`: no `start_transcription_task` (`:618`), and
    `auto_save` forced on.
  - A `static RECORD_ONLY: AtomicBool` beside `IS_RECORDING` holds the flag
    for the session; `recording-started` and `recording-stopped` payloads
    include it.
- **Stop:** the detector awaits `stop_recording`. Live mode: then emits
  `recording-stop-complete` (exactly as the tray does, without focusing), so
  the layout-level provider saves it on any page. Record-only: no
  `recording-stop-complete`.
- **Hand-off:** a Rust `app.listen("recording-stopped")` registered at
  startup enqueues `(folder_path, meeting_name)` for the worker when
  `record_only` is true. This covers the detector's stop and a manual
  (tray / Stop button) stop. `folder_path == None` → log and drop (nothing
  was recorded).
- **Frontend for record-only:**
  - `recordingService.onRecordingStarted` passes the payload through;
    `TranscriptContext` skips the recovery entry and the `recording-stopped`
    folder write when `record_only`.
  - `useRecordingStop` keeps `record_only` in a ref set from
    `recording-stopped` and **reset on every `recording-started`**; its save
    handler, when `record_only`, waits up to 2 s for the `recording-stopped`
    payload (as it already does for `folder_path`), then skips the save and
    only resets the UI. Bun test: a manual recording after a record-only one
    saves normally.

### 4. After stop

**Record-only worker** (`audio/after_meeting.rs`; one tokio task, FIFO
queue):
1. Save the meeting (`api_save_transcript`'s inner save, zero transcripts,
   folder path).
2. Wait while a live recording is running (`is_recording()`, poll 5 s), then
   await `start_retranscription` with provider/model from config and
   `get_language_preference_internal()` (`None` for Parakeet). Its own
   progress/complete events reach the UI; the meeting page's existing
   listeners show progress if open.
3. Classify (`pub const` prefixes in `retranscription.rs`, matched with
   `starts_with`): `ALREADY_IN_PROGRESS` → retry every 10 s, up to 30 min,
   then treat as failure; `NO_AUDIO_FILE` / `NO_SPEECH` → empty; `Ok` → empty
   check; other → failure (keep meeting and audio, notify "Transcription
   failed, retry from the meeting page").
4. Emit `meeting-ready { meeting_id }` or `meeting-discarded`. A new
   listener in `RecordingPostProcessingProvider` refetches the meeting list
   and, on `meeting-ready` with auto-summary on, navigates to the meeting
   page with `source=recording`.

Quitting during a job leaves a saved meeting without transcript; the
meeting page's Retranscribe button recovers it:
`shortcut: no automatic resume of interrupted batch jobs`.

**Manual and live pop-up recordings** (`useRecordingStop`): `useRecordingStop`
tracks `transcriptionErrorSeen` (a `transcription-error` listener inside the
hook, reset on start). Right after `api_save_transcript` succeeds and
**before** `markMeetingAsSaved`, `refetchMeetings`, `setCurrentMeeting`,
the summary-language step and analytics: unless an error was seen, await
`discard_meeting_if_empty(meeting_id)`. If `true`: mark the recovery entry
saved, toast "Recording had no speech, deleted", navigate home, return.

**Empty check**: Rust `discard_meeting_if_empty(meeting_id) -> bool` and
`discard_meeting(meeting_id)`, registered as Tauri commands. Empty = no
segments, or under 3 words total
(`shortcut: word-count emptiness, use VAD speech duration if real meetings get deleted`).
Delete rows via `MeetingsRepository::delete_meeting`, then `remove_dir_all`
`folder_path` only if its canonical path is inside canonical
`get_default_recordings_folder()`. Tests: no rows, filler only, one sentence,
folder outside root, symlink escaping root.

### 5. Obsidian export (opt-in per meeting)

- Button **Export to Obsidian** on the meeting page →
  `export_meeting_markdown(meeting_id)`. If no export folder is set, the
  command opens the Rust folder picker (`tauri_plugin_dialog`) first and
  stores the choice. After success the button reads "Open in Obsidian
  folder" (reveals the file).
- **Opt-in marker = the note exists.** The command finds a meeting's note by
  scanning `*.md` directly in the export folder for frontmatter
  `meetily_id: <id>` (first 1 KB):
  `shortcut: linear scan per sync, keep an id→file index if the folder holds thousands of notes`.
- **Sync** `sync_meeting_note_if_exported(meeting_id)`: if a note exists,
  rewrite it; if the title changed, write the new file
  `YYYY-MM-DD HHmm - <new title>.md` and delete the old one (temp file +
  rename, old file removed last). Called from the end of a successful
  `process_transcript_background` (after the rename at `:604-616`), from
  `api_save_meeting_summary`, and after retranscription. Errors are logged
  and notified once; they never fail the caller.
- **Content:** frontmatter `title`, `date` (from `created_at`),
  `created` (ISO), `duration_min` (max `transcripts.audio_end_time`, omitted
  if unknown), `source` (`teams` / `manual`, from the name prefix),
  `meetily_id`, `tags: [meeting]`. Body: `# <title>`, `## Summary` (the
  stored summary Markdown via the same accessor `api_get_summary` uses,
  headings demoted; omitted if none), `## Transcript` with `**[hh:mm:ss]**`
  lines. Pure `render_markdown` and `note_file_name` with tests (YAML
  escaping, empty summary, characters `* " \ / < > : | ? # ^ [ ]`, length
  cap, rename).

### 6. Settings and storage

- "Ask to record Teams meetings" (off by default) with the consent note.
- "Obsidian export folder" (shown, changeable; set on first export).
- Stored in a fork-owned `fork_settings.rs` using `tauri-plugin-store` with
  its own file `fork_settings.json`, commands `get_fork_settings` /
  `set_fork_settings`, registered in `lib.rs` next to the recording
  preference commands. `recording_preferences.rs` is not changed.

## Implementation order (one feature per session)

1. **CI gate:** `fork-macos-rust-checks.yml` green on unchanged upstream
   code (done when the first run passes; see `IMPLEMENTATION_PLAN.md`).
2. **Obsidian export:** fork settings, `export_meeting_markdown`, sync
   hooks, button. Pure tests on Linux.
3. **Empty check** on the manual save path, with Bun tests.
4. **Record-only plumbing + worker** (+ live fallback path), testable on
   Linux through the pure parts and a debug command that starts a
   record-only recording.
5. **Detector + pop-up + toggle** (pure state machine on Linux, macOS glue
   via CI).

Later, not planned: Windows, other meeting apps, merging split meetings,
pre-roll audio, automatic resume after quit.

## Implementation checklist (from the fourth review)

Each item is small but must not be forgotten:
- `tauri-nspanel` under the macOS target dependency table; pin a commit.
- CFString → `String` for bundle ids.
- `recordingService.onRecordingStarted` signature and the
  `RecordingStoppedPayload` TS type gain `record_only`.
- `TranscriptContext` skips both recovery writes for record-only.
- The `record_only` ref in `useRecordingStop` resets on every start.
- In-app Stop is home-page only; on other pages the tray stops recordings
  (unchanged upstream behaviour).
- `discard_meeting_if_empty`, `discard_meeting`, `export_meeting_markdown`,
  `get_fork_settings`, `set_fork_settings`, `meeting_prompt_respond`
  registered in `generate_handler!`.
- `meeting-ready` / `meeting-discarded` listener added to
  `RecordingPostProcessingProvider`.
- Batch jobs unload the engine afterwards (existing behaviour); the worker
  waits for live recordings to end before starting a job.

## Assumed, not tested (copy into `IMPLEMENTATION_PLAN.md` when built)

- Teams holds the microphone for the whole call, including while muted,
  from a process whose bundle id starts with `com.microsoft.teams`.
- The NSPanel shows over full-screen Spaces and leaves focus in Teams.
- `processes()` behaviour on macOS 14.0 vs later minors.

## Review log (condensed)

- **R1 → R2** (15 findings): tray start is a sessionStorage reload, auto-
  summary is a localStorage toggle, summary orchestration lives in the UI,
  pipeline warns without a receiver, Esc vs focus contradiction, Windows
  scope creep.
- **R2 → R3** (19): record-only meetings never saved, start hook home-page
  only, retranscription events racing, writer ignores `save_folder`,
  summary completion lost after unmount. Pop-up path moved into Rust.
- **R3 → R4** (13): stop returns no folder, silence is an error, recovery
  entry on every start, missing Stopping state, summary rename → two notes,
  settings wiped by the TS save.
- **R4 → R5** (35): no Mac available (verification strategy, cfg(macos)
  only compiled on macOS), cloud providers cannot batch, note name fixed
  before the summary, edited summaries not synced, incomplete state table,
  event-ordering races (now `is_recording()`), incomplete plumbing list,
  wrong pipeline function, payload-dropping TS listener, Option folder path,
  unbounded retry, unregistered commands, missing listener, no end-time
  column, missing npm dialog package, settings contradiction. User decisions:
  macOS CI + assume upstream-proven behaviour; live fallback for cloud
  providers; rename notes; export opt-in via a button.
