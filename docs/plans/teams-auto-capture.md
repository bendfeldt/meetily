# Plan: Teams auto-capture → transcript → Obsidian Markdown

Status: proposed, not started. Fork-only feature (`bendfeldt/meetily`).

## Goal

When a Microsoft Teams meeting starts, meetily starts recording on its own.
When the meeting ends, it stops, finishes the transcript, generates the
summary, and writes one Markdown note per meeting into a local folder the
user picks (their Obsidian vault).

## Done when

1. Joining a Teams call or meeting starts a meetily recording within ~15 s,
   with a notification "Recording Teams meeting". No click needed.
2. Leaving the call stops the recording within ~30 s of Teams releasing the
   microphone. A short mic test or pre-join preview (under the start delay)
   records nothing.
3. After stop, the meeting is saved with its full transcript and (if
   auto-summary is on) a summary, through the same path as a manual stop.
4. A file `<vault folder>/YYYY-MM-DD HHmm - <title>.md` exists with YAML
   frontmatter, the summary and the timestamped transcript, and opens
   cleanly in Obsidian. Regenerating the summary updates that same file.
5. All of it can be switched off in Settings. Off by default.
6. `cargo check`, `cargo test` (new detector and export tests),
   `tsc --noEmit` and `.claude/scripts/frontend-tests.sh` pass. Detection is
   confirmed by hand on a real Teams call on each supported OS.

## What already exists (reuse, don't rebuild)

| Need | Existing code |
|---|---|
| "Which app is using audio" on macOS | `audio/system_detector.rs`: CoreAudio process list via `cidre` (`list_system_audio_using_apps`, `is_running_output`). Registered as Tauri commands in `audio/system_audio_commands.rs`, but the UI never calls them. |
| Start recording from outside the UI | `tray.rs` start handler emits an event the frontend start flow handles. |
| Stop + save | `tray.rs::stop_recording_handler` → Rust `stop_recording`; the frontend `useRecordingStop` flushes transcripts, saves the meeting to SQLite and navigates with `source=recording`. |
| Summary after recording | `meeting-details/page.tsx` auto-generates when `source=recording` and `api_get_auto_generate_setting` is on (default on). |
| Batch transcription from the saved audio | `audio/retranscription.rs::start_retranscription` (meeting id + folder, language, model, provider). |
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
  for new Teams, `com.microsoft.teams` for classic). Poll every 2 s on a
  background thread, which avoids a second CoreAudio listener. (The pinned
  `cidre` rev is assumed to expose `is_running_input` next to
  `is_running_output`; check that first.) Needs no new
  permission. CoreAudio process objects need macOS 14.2+; on older versions
  detection is off and Settings says so.
- **Windows:** enumerate WASAPI capture sessions
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
Idle --(Teams holds mic ≥ start_delay, default 10 s)--> Recording
Recording --(Teams released mic ≥ end_grace, default 30 s)--> Stopping --> Idle
```

- The start delay skips the pre-join device preview and quick mic tests.
  The first ~10 s of a meeting is not recorded. Mark this with
  `shortcut: start delay loses the first seconds, buffer from detection if it matters`.
- The end grace period covers Teams briefly dropping the mic during a device
  switch or reconnect.
- Never auto-start if a recording is already running (a manual one, or the
  user started early). Only auto-stop recordings the detector started, so a
  manual recording is never cut off.
- The detector is a pure state machine fed by `(now, teams_holds_mic)`
  samples. That makes it unit-testable with no audio hardware: start delay,
  end grace, flapping, and the manual-recording guard.

### 3. Start and stop through the existing paths

- **Start:** emit the same event the tray emits, with the meeting name
  `Teams meeting <local date time>`, plus a system notification
  ("Recording Teams meeting. Stop anytime from the tray."). Reusing the
  frontend start flow keeps device selection, the `STARTING` state and
  error toasts identical to a manual start.
- **Stop:** call the tray's stop path (`stop_recording_handler`), which the
  frontend already turns into flush → SQLite save → navigate with
  `source=recording` → auto-summary.
- **Consent:** recording other people may require telling them, depending on
  jurisdiction. The setting text says so. The notification shows on every
  auto-start, and off-by-default means the user opts in knowingly.

### 4. "When it ends, the transcription must happen"

Two modes, as one setting:
- **Live (default, no new code):** meetily already transcribes during the
  meeting. On stop it waits for the last chunks, flushes and saves, so the
  transcript is complete at meeting end.
- **After meeting (optional):** don't transcribe live. On stop, run
  `start_retranscription` on the saved recording. This uses less CPU and GPU
  during the call and allows a bigger model. It needs
  `RecordingPreferences.auto_save = true` (the default). It reuses the
  existing batch engine and its progress events.

Recommendation: ship Live first. Add the after-meeting mode only if CPU load
during calls or transcript quality is a real problem.

### 5. Obsidian Markdown export

A new Rust command `export_meeting_markdown(meeting_id) -> PathBuf`
(`src/obsidian_export.rs`, registered in `lib.rs`):

- **Settings:** export folder (picked with `tauri-plugin-dialog`, stored
  next to the recording preferences) and an "export automatically" switch.
  The folder can be the vault root or a subfolder like `Meetings/`.
- **When:** after the summary completes. If auto-summary is off or fails,
  export right after the meeting is saved, without a summary. A later
  regenerate exports again.
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
- Auto-record Teams meetings (off by default) and its consent note
- Transcription: Live / After meeting
- Obsidian export folder (Choose…, Open), Export automatically
- Status line: "Watching for Teams" / "Not supported on this OS version"

## Implementation order (one feature per session, per AGENTS.md)

1. **Markdown export, manual:** `export_meeting_markdown` plus an "Export to
   Obsidian" button on the meeting page, and the folder setting. Useful on
   its own and the easiest to verify.
2. **Auto-export:** after save and after summary completion.
3. **Detector on macOS or Windows** (whichever you use first): the state
   machine, tests and the platform probe, wired to start and stop.
4. **Detector on the other OS.**
5. **Optional:** after-meeting transcription mode.

Each step runs `/spec` → implement → checks → `/verify`, and ends with
`IMPLEMENTATION_PLAN.md` updated.

## Risks and open questions

- **Which OS do you run Teams on?** That decides step 3.
- **Pre-join preview length.** If Teams holds the mic on the pre-join screen
  longer than the start delay, a meeting you never join gets recorded and
  then stopped. The fallback is to discard recordings shorter than ~60 s
  with no speech.
- **Two calls back to back** inside the 30 s grace period become one
  recording. That's acceptable for now; mark it with a `shortcut:` comment.
- **Frontend-owned save.** Meeting save runs in the webview
  (`useRecordingStop`). It works while the window is hidden, because the
  webview keeps running, but it has to be tested with the window minimized
  or closed to the tray.
- **macOS < 14.2:** no detection, stated in Settings.
- **Upstream overlap:** none today. If upstream ships its own auto-detection,
  prefer theirs and keep only the Obsidian export.
