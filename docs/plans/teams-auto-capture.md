# Plan: Teams auto-capture → transcript → Obsidian Markdown

Status: proposed, not started. Fork-only feature (`bendfeldt/meetily`).
Decided: macOS first; transcription runs after the meeting, not live;
start after ~1.5 s, stop after 5 s.

## Goal

When a Microsoft Teams meeting starts on macOS, meetily starts recording
audio on its own, without live transcription. When the meeting ends, it
stops, transcribes the saved recording, generates the summary, and writes
one Markdown note per meeting into a local folder the user picks (their
Obsidian vault).

## Done when

1. Joining a Teams call or meeting starts a meetily recording within ~2 s
   (start delay 1.5 s, 0.5 s polling), with a notification "Recording Teams
   meeting". No click needed. Nothing is transcribed during the call.
2. Leaving the call stops the recording within ~5 s of Teams releasing the
   microphone.
3. An auto-recording shorter than the minimum length (default 60 s, e.g. a
   pre-join preview you backed out of) is deleted, not transcribed.
4. After stop, the saved audio is transcribed in one batch, the meeting gets
   its full transcript and (if auto-summary is on) a summary. This works
   with the window hidden or closed to the tray.
5. A file `<vault folder>/YYYY-MM-DD HHmm - <title>.md` exists with YAML
   frontmatter, the summary and the timestamped transcript, and opens
   cleanly in Obsidian. Regenerating the summary updates that same file.
6. All of it can be switched off in Settings. Off by default. Manual
   recordings keep today's live transcription.
7. `cargo check`, `cargo test` (new detector and export tests),
   `tsc --noEmit` and `.claude/scripts/frontend-tests.sh` pass. Detection is
   confirmed by hand on a real Teams call on macOS.

## What already exists (reuse, don't rebuild)

| Need | Existing code |
|---|---|
| "Which app is using audio" on macOS | `audio/system_detector.rs`: CoreAudio process list via `cidre` (`list_system_audio_using_apps`, `is_running_output`). Registered as Tauri commands in `audio/system_audio_commands.rs`, but the UI never calls them. |
| Start recording from outside the UI | `tray.rs` start handler emits an event the frontend start flow handles. |
| Stop + save | `tray.rs::stop_recording_handler` → Rust `stop_recording`; the frontend `useRecordingStop` flushes transcripts, saves the meeting to SQLite and navigates with `source=recording`. |
| Summary after recording | `meeting-details/page.tsx` auto-generates when `source=recording` and `api_get_auto_generate_setting` is on (default on). |
| Batch transcription from the saved audio | `audio/retranscription.rs::start_retranscription` (meeting id + folder, language, model, provider). It replaces the meeting's rows in `transcripts` itself. |
| Live transcription hook-up | `RecordingManager` creates the transcription channel; `audio/transcription/worker.rs::start_transcription_task` consumes it. Start currently always validates a model and starts this task. There is no record-only mode yet. |
| Summary from Rust | `summary/commands.rs::api_process_transcript` (Tauri command; the service underneath can be called directly). |
| Summary as Markdown | `summary_processes.result` carries a `markdown` field. The frontend has `blocksToMarkdownSafely` for BlockNote summaries. |
| Folder picker, settings store, notifications | `tauri-plugin-dialog`, `tauri-plugin-store`, `tauri-plugin-notification` are already dependencies. |

## Design

### 1. Detection signal: "Teams is holding the microphone"

When a call is live, Teams keeps an active microphone capture stream, even
while you are muted. When you leave, it releases it. That is the most
reliable signal that needs no new permissions. Rejected alternatives:
- Window titles: localized, change with Teams versions, and need Screen
  Recording/Accessibility permission on macOS.
- Calendar or Graph API: needs an online account, which breaks local-first.
- "Teams is playing audio": also true for notifications and video previews.

Per OS:
- **macOS:** extend `system_detector.rs` with an input-side twin of
  `list_system_audio_using_apps`. Use `process.is_running_input()` on
  `ca::System::processes()` and match the bundle id (`com.microsoft.teams2`
  for new Teams, `com.microsoft.teams` for classic). Poll every 0.5 s on a
  background thread, which avoids a second CoreAudio listener. (The pinned
  `cidre` rev is assumed to expose `is_running_input` next to
  `is_running_output`; check that first.) Needs no new
  permission. CoreAudio process objects need macOS 14.2+; on older versions
  detection is off and Settings says so.
- **Windows (later, not in this plan's scope):** enumerate WASAPI capture sessions
  (`IAudioSessionManager2::GetSessionEnumerator` on the default capture
  endpoint). Match `ms-teams.exe` (new) or `Teams.exe` (classic) by process
  id, using `sysinfo`, which is already a dependency, for the name. State
  must be `AudioSessionStateActive`. This needs the `windows` crate with
  `Win32_Media_Audio`. It is already in the build through `cpal`; add it
  as a direct Windows-only dependency and justify that in the commit.
- **Linux:** out of scope. Teams on Linux is a browser PWA, so the mic owner
  is the browser and can't be told apart from other tabs.

The app match list is a setting (`["Microsoft Teams"]` by default). That
makes Zoom or Webex a one-line config change later, with no new code.

### 2. Meeting state machine (Rust, one small module)

`audio/meeting_detector.rs`:

```
Idle --(Teams holds mic ≥ start_delay, default 1.5 s)--> Recording
Recording --(Teams released mic ≥ end_grace, default 5 s)--> Stopping --> Idle
```

- Polling is every 0.5 s. One CoreAudio process scan is cheap (a short
  list of objects), so this costs nothing noticeable.
- A 1.5 s start delay barely filters anything: opening the pre-join screen
  (Teams shows a mic level meter there) already starts a recording. That is
  fine because Teams usually keeps the mic open from pre-join into the
  meeting, so the recording simply includes the pre-join seconds. Backing
  out without joining leaves a short recording, which the minimum-length
  rule deletes (see section 4).
- A 5 s end grace period means a short mic drop (switching headset,
  reconnect) can split one meeting into two recordings and two notes.
  Accepted for now; mark it with
  `shortcut: 5 s end grace splits a meeting on device switches, merge notes if it happens often`.
- Both delays and the minimum length are settings with these defaults, so
  they can be tuned without code changes.
- Never auto-start if a recording is already running (a manual one, or the
  user started early). Only auto-stop recordings the detector started, so a
  manual recording is never cut off.
- The detector is a pure state machine fed by `(now, teams_holds_mic)`
  samples. That makes it unit-testable with no audio hardware: start delay,
  end grace, flapping around the 5 s edge, and the manual-recording guard.

### 3. Start and stop through the existing paths

- **Start:** emit the same event the tray emits, with the meeting name
  `Teams meeting <local date time>` and a `record_only` flag, plus a system
  notification ("Recording Teams meeting. Stop anytime from the tray.").
  Reusing the frontend start flow keeps device selection, the `STARTING`
  state and error toasts identical to a manual start.
- **Stop:** call the tray's stop path (`stop_recording_handler`). The
  frontend save creates the meeting row with its `folder_path` and no
  transcripts, and skips its "wait for live transcripts" step for
  record-only recordings. The after-meeting pipeline (section 4) takes over
  from there.
- **Consent:** recording other people may require telling them, depending on
  jurisdiction. The setting text says so. The notification shows on every
  auto-start, and off-by-default means the user opts in knowingly.

### 4. Transcription after the meeting

**Record-only mode (new, small):** a `record_only` flag on the start path.
When it is set:
- skip `validate_transcription_model_ready` at start, because no model is
  needed during the call;
- don't call `start_transcription_task`. The pipeline still mixes and saves
  audio; the transcription channel's receiver is dropped, so the pipeline
  stops sending to it (check that a closed channel is handled quietly, not
  logged every chunk);
- force `auto_save` on for this recording, because the audio file is the
  only input for transcription.

**After-meeting pipeline (Rust, `audio/after_meeting.rs`)**, run once the
meeting row exists:
1. If the recording is shorter than the minimum length (default 60 s),
   delete the meeting and its audio folder and stop. One line in the log,
   no note.
2. `start_retranscription(meeting_id, folder_path, language, model,
   provider)` with the user's configured transcription model. It writes the
   transcripts table itself and emits progress events the UI already shows.
3. If auto-summary is on, run the summary through the service behind
   `api_process_transcript` and wait for it to finish or fail.
4. Export the Markdown note (section 5), with or without a summary.

It runs in Rust, so it does not depend on the meeting page being open (the
current auto-summary only fires when the UI navigates with
`source=recording`). Only one batch job runs at a time (retranscription
already has a guard). If a second meeting ends while one is processing,
queue it, don't drop it. A plain `tokio` mpsc queue with one worker is
enough.

Failure handling: if transcription fails, keep the audio and the meeting,
show a notification "Transcription failed, retry from the meeting page",
and still export a note without a transcript, so the meeting is not lost
from the vault. Manual retry uses the existing Retranscribe dialog.

### 5. Obsidian Markdown export

A new Rust command `export_meeting_markdown(meeting_id) -> PathBuf`
(`src/obsidian_export.rs`, registered in `lib.rs`):

- **Settings:** export folder (picked with `tauri-plugin-dialog`, stored
  next to the recording preferences) and an "export automatically" switch.
  The folder can be the vault root or a subfolder like `Meetings/`.
- **When:** as the last step of the after-meeting pipeline, and again
  whenever the summary or transcript is regenerated.
- **File name:** `YYYY-MM-DD HHmm - <title>.md`. Strip the characters
  Obsidian and Windows reject (`* " \ / < > : | ? # ^ [ ]`) and cap the
  length.
- **Updates:** if the file exists and its frontmatter `meetily_id` matches,
  overwrite it. If it belongs to something else, add ` (2)`. Write to a temp
  file, then rename, so Obsidian never sees half a note.
- **Content:**

  ```markdown
  ---
  title: Weekly sync
  date: 2026-10-10
  start: 2026-10-10T14:00:12+02:00
  end: 2026-10-10T14:47:40+02:00
  duration_min: 47
  source: teams
  meetily_id: <meeting id>
  tags: [meeting]
  ---

  # Weekly sync

  ## Summary
  <summary markdown, headings demoted one level>

  ## Transcript
  - **[00:00:12]** Hello everyone…
  - **[00:00:20]** …
  ```

- **Privacy:** the note only goes to the folder the user picked. Audio is
  never copied into the vault. Nothing leaves the machine.
- **Pure function for tests:** `render_markdown(meeting, summary,
  transcripts) -> String` plus `safe_file_name(title, date) -> String`, both
  unit-tested (frontmatter escaping, empty summary, odd characters in the
  title).

### 6. Settings UI

One "Auto-capture" section in the existing Settings:
- Auto-record Teams meetings (off by default) and its consent note.
  Auto-recordings always transcribe after the meeting; manual ones stay live.
- Start delay (1.5 s), end grace (5 s), minimum length (60 s)
- Obsidian export folder (Choose…, Open), Export automatically
- Status line: "Watching for Teams" / "Not supported on this OS version"

## Implementation order (one feature per session, per AGENTS.md)

1. **Markdown export, manual:** `export_meeting_markdown` plus an "Export to
   Obsidian" button on the meeting page, and the folder setting. Useful on
   its own and the easiest to verify.
2. **Record-only mode + after-meeting pipeline:** testable with a manual
   "Record only" start before any detection exists. Includes the
   minimum-length delete and the auto-export.
3. **macOS Teams detector:** the state machine with tests, the CoreAudio
   input probe, wiring to start and stop, and the settings.

Later, not planned here: Windows detector, other apps (Zoom/Webex via the
match list), merging split meetings.

Each step runs `/spec` → implement → checks → `/verify`, and ends with
`IMPLEMENTATION_PLAN.md` updated.

## Risks and open questions

- **Short timings.** With a 1.5 s start and 5 s stop, the pre-join screen
  starts recordings and short mic drops split meetings. The minimum-length
  delete covers the first; the second is an accepted shortcut. Both are
  tunable in Settings.
- **Does Teams hold the mic from pre-join straight into the meeting?** If it
  briefly releases it on "Join now", the 5 s grace bridges that. Verify on a
  real call in step 3.
- **Two calls back to back** more than 5 s apart become two notes; closer
  than that, one. Acceptable.
- **Frontend-owned save.** The meeting row is still created by the webview
  (`useRecordingStop`). It works while the window is hidden, because the
  webview keeps running, but it has to be tested with the window minimized
  or closed to the tray. If that proves flaky, move the record-only save
  into Rust.
- **Pipeline coupling.** Dropping the transcription receiver must not make
  the audio pipeline error out or stop recording. Cover it with a test in
  step 2.
- **Processing time.** Batch transcription of a 1 h meeting takes minutes,
  depending on model and GPU (Metal on Apple Silicon). The note appears
  after that, not at meeting end.
- **macOS < 14.2:** no detection, stated in Settings.
- **Upstream overlap:** none today. If upstream ships its own auto-detection,
  prefer theirs and keep only the Obsidian export.
