# Plan: Teams auto-capture → transcript → Obsidian Markdown

Status: proposed, not started. Fork-only feature (`bendfeldt/meetily`).
Decided: macOS first; transcription runs after the meeting, not live;
detection after ~1.5 s asks before recording (pop-up), stop after 5 s.

## Goal

When a Microsoft Teams meeting starts on macOS, meetily shows a pop-up in
front of everything asking whether to record it. If the user says yes, it
records audio without live transcription. When the meeting ends, it
stops, transcribes the saved recording, generates the summary, and writes
one Markdown note per meeting into a local folder the user picks (their
Obsidian vault).

## Done when

1. Within ~2 s of Teams taking the microphone (start delay 1.5 s, 0.5 s
   polling), a small "Teams meeting detected — Record / Not now" pop-up
   appears on top of all windows, **including full-screen apps** (Teams or
   anything else in its own full-screen Space), on the display with the
   pointer. Showing it or clicking it never switches Space, never pulls
   meetily's main window forward, and never takes keyboard focus from
   Teams.
2. "Record" starts a record-only meeting recording at once. "Not now" (or
   Esc) closes the pop-up, and it stays quiet until Teams releases the mic
   and takes it again (the next meeting). If Teams releases the mic before
   the user answers, the pop-up closes on its own.
3. A recording started from the pop-up stops automatically within ~5 s of
   Teams releasing the microphone, exactly like a detected meeting end.
   Recordings started from the main window or tray are never auto-stopped.
   The user can still stop a pop-up recording early by hand.
4. An empty recording is deleted, whether it came from the pop-up or was
   started by hand: no audio frames, or no speech after transcription. The
   meeting rows and its audio folder are removed, no summary or note is
   made, and a notification says "Recording had no speech, deleted". A
   recording whose transcription *failed* is never deleted.
5. After stop, the saved audio is transcribed in one batch, the meeting gets
   its full transcript and (if auto-summary is on) a summary. This works
   with the window hidden or closed to the tray.
6. A file `<vault folder>/YYYY-MM-DD HHmm - <title>.md` exists with YAML
   frontmatter, the summary and the timestamped transcript, and opens
   cleanly in Obsidian. Regenerating the summary updates that same file.
7. All of it can be switched off in Settings. Off by default. Manual
   recordings keep today's live transcription.
8. `cargo check`, `cargo test` (new detector and export tests),
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
Idle --(Teams holds mic ≥ start_delay, 1.5 s)--> Prompting
Prompting --Record--> Recording (detector-owned)
Prompting --Not now / Esc--> Dismissed
Prompting --(Teams released mic ≥ end_grace)--> Idle      (pop-up closes)
Recording --(Teams released mic ≥ end_grace, 5 s)--> Stopping --> Idle
Recording --(user stops by hand)--> Dismissed
Dismissed --(Teams released mic ≥ end_grace)--> Idle
```

- Polling is every 0.5 s. One CoreAudio process scan is cheap (a short
  list of objects), so this costs nothing noticeable.
- The pop-up usually appears on the pre-join screen (Teams turns the mic on
  there for its level meter). That is the right moment to ask: the user
  decides before joining, and nothing is recorded until they click Record.
  Recordings that turn out empty (backed out of the pre-join screen,
  nobody spoke) are deleted after the meeting; see section 4.
- "Dismissed" is keyed to the mic session, not a timer: no second pop-up
  for the same meeting, even if the user stops a recording early.
- A 5 s end grace period means a short mic drop (switching headset,
  reconnect) ends the meeting and stops the recording, and the next mic
  grab prompts again. Accepted for now; mark it with
  `shortcut: 5 s end grace splits a meeting on device switches, raise end_grace if it happens often`.
- Never prompt while any recording is running (manual or detector-owned).
  Only a detector-owned recording is auto-stopped.
- Start delay and end grace are settings with these defaults.
- The detector is a pure state machine fed by `(now, teams_holds_mic)`
  samples and user events (`record`, `dismiss`, `manual_stop`), returning
  actions (`show_prompt`, `hide_prompt`, `start_recording`,
  `stop_recording`). That makes it unit-testable with no audio hardware or
  UI: start delay, end grace, flapping around the 5 s edge, dismiss for the
  rest of the session, mic released while prompting, and the
  manual-recording guard.

### 3. Start and stop through the existing paths

**The pop-up.** A second Tauri window, `meeting-prompt`, created once at
startup and hidden, with a tiny page (`frontend/src/app/meeting-prompt/`):
- Window: about 360×120 px, no decorations, transparent background,
  `skip_taskbar`, created with `focused(false)`, placed top-right of the
  display with the pointer.
- **On top of full-screen apps (required).** A plain Tauri window with
  `always_on_top` + `visible_on_all_workspaces` stays out of another app's
  full-screen Space, and clicking it activates meetily, which makes macOS
  jump to meetily's own Space. So the prompt is turned into a
  non-activating `NSPanel` right after creation, in
  `src/meeting_prompt_macos.rs` (~40 lines, `objc`, already a macOS
  dependency, same technique as the `tauri-nspanel` crate):
  - swap the window's class to an `NSPanel` subclass that returns `YES` from
    `canBecomeKeyWindow` (so the buttons and Esc work) and `NO` from
    `canBecomeMainWindow`;
  - style mask `NSWindowStyleMaskNonactivatingPanel`, so clicking it does
    not activate meetily or switch Space;
  - collection behavior `canJoinAllSpaces | fullScreenAuxiliary |
    stationary | ignoresCycle`, so it is present on every Space including
    full-screen ones and stays out of Cmd-` cycling;
  - window level `NSStatusWindowLevel` (above full-screen app windows, below
    system alerts), `hidesOnDeactivate = NO`;
  - show with `orderFrontRegardless`, never `makeKeyAndOrderFront`, so
    focus stays in Teams until the user clicks the pop-up.
  If the hand-rolled panel misbehaves, switch to the `tauri-nspanel` crate
  (git dependency, justified in the commit) instead of patching around it.
- Acceptance test on a real Mac, all must pass before step 3 is done:
  Teams full-screen in its own Space (pop-up visible, click Record, still
  in the Teams Space, Teams still has focus); Keynote/browser full-screen;
  normal desktop; second display; pop-up while typing in Teams chat (no
  keystrokes lost).
- Content: "Teams meeting detected", the time, and two buttons: **Record**
  (default) and **Not now**. Esc means Not now. Accessible names and
  keyboard focus order per `a11y-pass`. A short sound is optional and off
  by default.
- It answers with a Tauri command `meeting_prompt_respond(record: bool)`
  that feeds the state machine. Hide happens from Rust, so the pop-up also
  closes when the meeting ends first.
- Fallback only if the panel fails to build at startup (logged): a regular
  system notification, "Teams meeting detected, open meetily to record".
  No auto-record.

**Start (after Record):** emit the same event the tray emits, with the
meeting name `Teams meeting <local date time>` and a `record_only` flag.
Reusing the frontend start flow keeps device selection, the `STARTING`
state and error toasts identical to a manual start. The main window is not
brought to the front.

**Stop:** when the detector sees Teams release the mic for 5 s, it calls the
tray's stop path (`stop_recording_handler`), and shows a notification
"Teams meeting ended, transcribing…". The frontend save creates the
meeting row with its `folder_path` and no transcripts, and skips its "wait
for live transcripts" step for record-only recordings. The after-meeting
pipeline (section 4) takes over from there.

**Consent:** recording other people may require telling them, depending on
jurisdiction. The setting text says so; the pop-up is an explicit choice
for every meeting.

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
0. If the saved audio is missing or has no frames (recording stopped before
   any audio arrived), delete the meeting and its folder and stop.
1. `start_retranscription(meeting_id, folder_path, language, model,
   provider)` with the user's configured transcription model. It writes the
   transcripts table itself and emits progress events the UI already shows.
   **Empty check:** if transcription succeeded but produced no segments with
   non-whitespace text, the recording is empty: delete the meeting row
   (transcripts cascade) and its audio folder, notify "Teams meeting had no
   speech, recording deleted", and stop. Whisper sometimes "hears" filler
   in silence ("Thank you.", "you"), so the check also treats a transcript
   under ~3 words in total as empty. Mark it with
   `shortcut: word-count emptiness check, use VAD speech duration if real meetings get deleted`.
   A transcription *failure* is never treated as empty: the audio is kept.

**Empty manual recordings.** One shared Rust function,
`discard_meeting_if_empty(meeting_id)` (in `audio/after_meeting.rs`,
exposed as a Tauri command), applies the same rule to both paths:
- Pop-up path: called by the pipeline right after batch transcription.
- Manual path (live transcription): `useRecordingStop` calls the command
  after `api_save_transcript` succeeds and the meeting is marked saved in
  the IndexedDB recovery store, so the recovery prompt never offers the
  deleted meeting back. It skips the call if a `transcription-error` event
  was seen during this recording (then "no transcript" means failure, not
  silence). If the meeting is deleted, it navigates home instead of to the
  meeting page and shows the "no speech, deleted" toast.
- The function re-reads the transcripts from SQLite instead of trusting the
  caller, so both paths judge emptiness the same way.
2. If auto-summary is on, run the summary through the service behind
   `api_process_transcript` and wait for it to finish or fail.
3. Export the Markdown note (section 5), with or without a summary.

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
- Ask to record Teams meetings (off by default) and its consent note.
  Pop-up recordings always transcribe after the meeting and stop when
  Teams lets go of the mic; manual ones stay live and stop by hand.
- Start delay (1.5 s), end grace (5 s)
- Obsidian export folder (Choose…, Open), Export automatically
- Status line: "Watching for Teams" / "Not supported on this OS version"

## Implementation order (one feature per session, per AGENTS.md)

1. **Markdown export, manual:** `export_meeting_markdown` plus an "Export to
   Obsidian" button on the meeting page, and the folder setting. Useful on
   its own and the easiest to verify.
2. **Record-only mode + after-meeting pipeline:** testable with a manual
   "Record only" start before any detection exists. Includes the
   auto-export.
3. **macOS Teams detector + pop-up:** the state machine with tests, the
   CoreAudio input probe, the `meeting-prompt` window, auto-stop for
   pop-up recordings, and the settings.

Later, not planned here: Windows detector, other apps (Zoom/Webex via the
match list), merging split meetings.

Each step runs `/spec` → implement → checks → `/verify`, and ends with
`IMPLEMENTATION_PLAN.md` updated.

## Risks and open questions

- **Short timings.** With a 1.5 s start the pop-up appears on the pre-join
  screen, which is intended. With a 5 s stop, a short mic drop ends the
  recording and the user gets asked again. Accepted; tunable in Settings.
- **Audio before the click is lost.** Recording starts when the user clicks
  Record, not when the meeting was detected. Usually that is before
  joining, so nothing is missed. If it matters later, keep a few seconds of
  pre-roll from detection; mark with a `shortcut:` comment.
- **Pop-up over full-screen apps** relies on the NSPanel conversion
  (private-ish AppKit usage through `objc`; the app already enables
  `macos-private-api`). It can only be verified on a real Mac; the
  acceptance test in section 3 is the gate.
- **Does Teams hold the mic from pre-join straight into the meeting?** If it
  briefly releases it on "Join now", the 5 s grace bridges that. Verify on a
  real call in step 3.
- **Deleting the wrong thing.** Auto-delete runs only after a successful
  transcription (batch, or live with no transcription errors), and only when the transcript is
  empty or nearly so. The existing `MeetingsRepository::delete_meeting`
  removes only the DB rows, not audio. So the pipeline reads the meeting's
  `folder_path` first, deletes the rows, then removes that folder only if it
  sits inside the configured recordings folder (canonicalized path check).
  No path guessing. Unit-test the emptiness rule
  on fixtures (silence, Whisper filler, one real sentence).
- **Two calls back to back** more than 5 s apart get two pop-ups and two
  notes; closer than that, one. Acceptable.
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
