//! Fork-only (bendfeldt/meetily): Teams meeting detection.
//!
//! Teams holds the microphone for a whole call (assumed; not testable without
//! a Mac, see IMPLEMENTATION_PLAN.md). A background loop polls every 0.5 s
//! whether a Teams process has an active microphone capture stream, feeds the
//! pure state machine below, and carries out its actions: show/hide the
//! "Record?" pop-up, start a pop-up recording, stop it when Teams lets go of
//! the microphone. Only recordings started from the pop-up are auto-stopped.

use std::time::{Duration, Instant};

pub const START_DELAY: Duration = Duration::from_millis(1500);
// shortcut: 5 s end grace splits a meeting on device switches, raise END_GRACE if it happens often
pub const END_GRACE: Duration = Duration::from_secs(5);
pub const POLL: Duration = Duration::from_millis(500);
/// Safety net: Teams has been reported to keep the microphone open after a
/// call, in which case the end grace never fires.
pub const MAX_RECORDING: Duration = Duration::from_secs(4 * 60 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    Prompting,
    Starting,
    Recording,
    Stopping,
    Dismissed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    ShowPrompt,
    HidePrompt,
    Start,
    Stop,
}

/// Pure detector state machine. Time and inputs are passed in so every
/// transition is unit-testable without audio hardware or UI.
#[derive(Debug)]
pub struct Detector {
    state: State,
    held_since: Option<Instant>,
    released_since: Option<Instant>,
    recording_since: Option<Instant>,
}

impl Default for Detector {
    fn default() -> Self {
        Self {
            state: State::Idle,
            held_since: None,
            released_since: None,
            recording_since: None,
        }
    }
}

impl Detector {
    pub fn state(&self) -> State {
        self.state
    }

    fn held_for(&self, now: Instant) -> Duration {
        self.held_since.map_or(Duration::ZERO, |t| now - t)
    }

    fn released_for(&self, now: Instant) -> Option<Duration> {
        self.released_since.map(|t| now - t)
    }

    fn released_long(&self, now: Instant) -> bool {
        self.released_for(now).is_some_and(|d| d >= END_GRACE)
    }

    /// One poll: does Teams hold the microphone, and is any recording active
    /// (from the recording core, not from events)?
    pub fn tick(&mut self, now: Instant, holds_mic: bool, recording_active: bool) -> Vec<Action> {
        if holds_mic {
            self.held_since.get_or_insert(now);
            self.released_since = None;
        } else {
            self.released_since.get_or_insert(now);
            self.held_since = None;
        }
        let held_long = self.held_for(now) >= START_DELAY;
        let released_long = self.released_long(now);
        match self.state {
            State::Idle if held_long && recording_active => {
                // A recording started another way owns this call.
                self.state = State::Dismissed;
                vec![]
            }
            State::Idle if held_long => {
                self.state = State::Prompting;
                vec![Action::ShowPrompt]
            }
            State::Prompting if recording_active => {
                self.state = State::Dismissed;
                vec![Action::HidePrompt]
            }
            State::Prompting if released_long => {
                self.state = State::Idle;
                vec![Action::HidePrompt]
            }
            State::Recording if !recording_active => {
                // Stopped by the user (or a fallback); never prompt again
                // for this call.
                self.state = State::Dismissed;
                vec![]
            }
            State::Recording
                if released_long
                    || self
                        .recording_since
                        .is_some_and(|t| now - t >= MAX_RECORDING) =>
            {
                self.state = State::Stopping;
                vec![Action::Stop]
            }
            State::Dismissed if released_long => {
                self.state = State::Idle;
                vec![]
            }
            // Starting and Stopping ignore polls until their result arrives.
            _ => vec![],
        }
    }

    /// The user clicked Record.
    pub fn record(&mut self) -> Vec<Action> {
        if self.state != State::Prompting {
            return vec![]; // late click
        }
        self.state = State::Starting;
        vec![Action::HidePrompt, Action::Start]
    }

    /// The user clicked Not now.
    pub fn dismiss(&mut self) -> Vec<Action> {
        if self.state != State::Prompting {
            return vec![];
        }
        self.state = State::Dismissed;
        vec![Action::HidePrompt]
    }

    pub fn start_finished(&mut self, ok: bool, now: Instant) -> Vec<Action> {
        if self.state != State::Starting {
            return vec![];
        }
        if !ok {
            self.state = State::Dismissed;
            return vec![];
        }
        if self.released_long(now) {
            // Teams left while the recording was starting.
            self.state = State::Stopping;
            return vec![Action::Stop];
        }
        self.state = State::Recording;
        self.recording_since = Some(now);
        vec![]
    }

    pub fn stop_finished(&mut self) {
        if self.state == State::Stopping {
            self.state = if self.held_since.is_some() {
                State::Dismissed
            } else {
                State::Idle
            };
        }
    }

    /// The feature was switched off. A running pop-up recording continues as
    /// a normal recording (no auto-stop).
    pub fn feature_off(&mut self) -> Vec<Action> {
        let prompting = self.state == State::Prompting;
        *self = Self::default();
        if prompting {
            vec![Action::HidePrompt]
        } else {
            vec![]
        }
    }
}

#[cfg(target_os = "macos")]
mod probe {
    use cidre::core_audio as ca;
    use std::sync::atomic::{AtomicBool, Ordering};

    static WARNED: AtomicBool = AtomicBool::new(false);

    /// Bundle ids of Teams processes with an active microphone stream. New
    /// Teams (com.microsoft.teams2), its helpers and classic Teams all start
    /// with this prefix.
    pub fn teams_mic_holders() -> Vec<String> {
        match ca::System::processes() {
            Ok(processes) => processes
                .iter()
                .filter(|p| p.is_running_input().unwrap_or(false))
                .filter_map(|p| p.bundle_id().ok().map(|id| id.to_string()))
                .filter(|id| id.starts_with("com.microsoft.teams"))
                .collect(),
            Err(e) => {
                // CoreAudio process objects need macOS 14+.
                if !WARNED.swap(true, Ordering::Relaxed) {
                    log::warn!("meeting_detector: cannot list audio processes ({e:?}); Teams detection is unavailable");
                }
                Vec::new()
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub use runner::start;

/// Clicks from the pop-up: Record (true) or Not now (false).
static UI: std::sync::OnceLock<tokio::sync::mpsc::UnboundedSender<bool>> =
    std::sync::OnceLock::new();

/// The pop-up's buttons. A no-op where the detector does not run.
#[tauri::command]
pub fn meeting_prompt_respond(record: bool) {
    if let Some(tx) = UI.get() {
        let _ = tx.send(record);
    }
}

#[cfg(target_os = "macos")]
mod runner {
    use super::*;
    use tauri::{AppHandle, Emitter, Runtime};
    use tauri_plugin_notification::NotificationExt;
    use tokio::sync::mpsc::unbounded_channel;

    fn notify<R: Runtime>(app: &AppHandle<R>, body: &str) {
        let _ = app
            .notification()
            .builder()
            .title("meetily")
            .body(body)
            .show();
    }

    async fn stop<R: Runtime>(app: &AppHandle<R>) -> bool {
        // Same as the tray's stop, without focusing the main window.
        let save_path = std::env::temp_dir().join("meetily-popup-stop.wav");
        let result = crate::audio::recording_commands::stop_recording(
            app.clone(),
            crate::audio::recording_commands::RecordingArgs {
                save_path: save_path.to_string_lossy().to_string(),
            },
        )
        .await;
        match result {
            Ok(()) => {
                let _ = app.emit("recording-stop-complete", true);
                true
            }
            Err(e) => {
                log::error!("meeting_detector: stopping the recording failed: {e}");
                notify(
                    app,
                    "Could not stop the Teams recording automatically. Stop it from the tray.",
                );
                false
            }
        }
    }

    async fn run<R: Runtime>(app: AppHandle<R>, actions: Vec<Action>, detector: &mut Detector) {
        let mut queue = actions;
        while !queue.is_empty() {
            let mut next = Vec::new();
            for action in queue {
                match action {
                    Action::ShowPrompt => crate::meeting_prompt::show(&app),
                    Action::HidePrompt => crate::meeting_prompt::hide(&app),
                    Action::Start => {
                        let name = format!(
                            "Teams meeting {}",
                            chrono::Local::now().format("%Y-%m-%d %H-%M")
                        );
                        let ok =
                            match crate::after_meeting::start_popup_recording(app.clone(), name)
                                .await
                            {
                                Ok(_) => true,
                                Err(e) => {
                                    log::warn!("meeting_detector: could not start recording: {e}");
                                    notify(&app, &format!("Could not start recording: {e}"));
                                    false
                                }
                            };
                        next.extend(detector.start_finished(ok, Instant::now()));
                    }
                    Action::Stop => {
                        stop(&app).await;
                        detector.stop_finished();
                    }
                }
            }
            queue = next;
        }
    }

    /// Start the detection loop (once, at app startup). It idles while the
    /// "Ask to record Teams meetings" setting is off.
    pub fn start<R: Runtime>(app: AppHandle<R>) {
        let (tx, mut rx) = unbounded_channel::<bool>();
        if UI.set(tx).is_err() {
            return;
        }
        tauri::async_runtime::spawn(async move {
            let mut detector = Detector::default();
            let mut interval = tokio::time::interval(POLL);
            let mut was_enabled = false;
            let mut last_holders: Vec<String> = Vec::new();
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let enabled = crate::fork_settings::load(&app).teams_prompt_enabled;
                        if !enabled {
                            if was_enabled {
                                let actions = detector.feature_off();
                                run(app.clone(), actions, &mut detector).await;
                            }
                            was_enabled = false;
                            continue;
                        }
                        was_enabled = true;
                        let holders = probe::teams_mic_holders();
                        if holders != last_holders {
                            log::info!("meeting_detector: Teams processes holding the microphone: {holders:?}");
                            last_holders = holders.clone();
                        }
                        let recording = crate::audio::recording_commands::is_recording().await;
                        let actions = detector.tick(Instant::now(), !holders.is_empty(), recording);
                        run(app.clone(), actions, &mut detector).await;
                    }
                    Some(record) = rx.recv() => {
                        let actions = if record { detector.record() } else { detector.dismiss() };
                        run(app.clone(), actions, &mut detector).await;
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Duration = Duration::from_millis(500);

    /// Feed `n` polls of the same input, half a second apart.
    fn polls(d: &mut Detector, now: &mut Instant, n: u32, held: bool, rec: bool) -> Vec<Action> {
        let mut out = Vec::new();
        for _ in 0..n {
            *now += T;
            out.extend(d.tick(*now, held, rec));
        }
        out
    }

    fn prompting() -> (Detector, Instant) {
        let mut d = Detector::default();
        let mut now = Instant::now();
        assert!(polls(&mut d, &mut now, 3, true, false).is_empty()); // 1.0 s held
        assert_eq!(
            polls(&mut d, &mut now, 1, true, false),
            vec![Action::ShowPrompt]
        ); // 1.5 s
        assert_eq!(d.state(), State::Prompting);
        (d, now)
    }

    fn recording() -> (Detector, Instant) {
        let (mut d, now) = prompting();
        assert_eq!(d.record(), vec![Action::HidePrompt, Action::Start]);
        assert!(d.start_finished(true, now).is_empty());
        assert_eq!(d.state(), State::Recording);
        (d, now)
    }

    #[test]
    fn prompts_after_start_delay_only_when_held_continuously() {
        let mut d = Detector::default();
        let mut now = Instant::now();
        polls(&mut d, &mut now, 2, true, false);
        polls(&mut d, &mut now, 1, false, false); // flap resets the hold
        assert!(polls(&mut d, &mut now, 3, true, false).is_empty());
        assert_eq!(
            polls(&mut d, &mut now, 1, true, false),
            vec![Action::ShowPrompt]
        );
    }

    #[test]
    fn manual_recording_owns_the_call() {
        let mut d = Detector::default();
        let mut now = Instant::now();
        assert!(polls(&mut d, &mut now, 4, true, true).is_empty());
        assert_eq!(d.state(), State::Dismissed);
        // And while prompting:
        let (mut d, mut now) = prompting();
        assert_eq!(
            polls(&mut d, &mut now, 1, true, true),
            vec![Action::HidePrompt]
        );
        assert_eq!(d.state(), State::Dismissed);
    }

    #[test]
    fn dismiss_lasts_until_teams_releases_the_mic() {
        let (mut d, mut now) = prompting();
        assert_eq!(d.dismiss(), vec![Action::HidePrompt]);
        assert!(polls(&mut d, &mut now, 20, true, false).is_empty());
        polls(&mut d, &mut now, 11, false, false); // first release poll + 5 s
        assert_eq!(d.state(), State::Idle);
        polls(&mut d, &mut now, 4, true, false);
        assert_eq!(d.state(), State::Prompting); // next meeting asks again
    }

    #[test]
    fn prompt_hides_when_teams_releases_for_end_grace() {
        let (mut d, mut now) = prompting();
        assert!(polls(&mut d, &mut now, 10, false, false).is_empty()); // 4.5 s after first release poll
        assert_eq!(
            polls(&mut d, &mut now, 1, false, false),
            vec![Action::HidePrompt]
        );
        assert_eq!(d.state(), State::Idle);
    }

    #[test]
    fn auto_stops_after_end_grace_and_survives_flapping() {
        let (mut d, mut now) = recording();
        polls(&mut d, &mut now, 10, false, true); // 4.5 s released
        polls(&mut d, &mut now, 1, true, true); // grabbed again: grace resets
        assert!(polls(&mut d, &mut now, 10, false, true).is_empty());
        assert_eq!(polls(&mut d, &mut now, 1, false, true), vec![Action::Stop]);
        assert_eq!(d.state(), State::Stopping);
        assert!(polls(&mut d, &mut now, 4, true, true).is_empty()); // re-grab while stopping: no prompt
        d.stop_finished();
        assert_eq!(d.state(), State::Dismissed);
    }

    #[test]
    fn user_stop_is_respected() {
        let (mut d, mut now) = recording();
        assert!(polls(&mut d, &mut now, 1, true, false).is_empty());
        assert_eq!(d.state(), State::Dismissed);
        assert!(polls(&mut d, &mut now, 20, true, false).is_empty()); // no new prompt this call
    }

    #[test]
    fn start_failure_and_release_during_start() {
        let (mut d, now) = prompting();
        d.record();
        assert!(d.start_finished(false, now).is_empty());
        assert_eq!(d.state(), State::Dismissed);

        let (mut d, mut now) = prompting();
        d.record();
        polls(&mut d, &mut now, 11, false, false); // Teams left while starting
        assert_eq!(d.start_finished(true, now), vec![Action::Stop]);
        assert_eq!(d.state(), State::Stopping);
        d.stop_finished();
        assert_eq!(d.state(), State::Idle);
    }

    #[test]
    fn stops_after_max_duration_even_if_teams_keeps_the_mic() {
        let (mut d, mut now) = recording();
        now += MAX_RECORDING - T;
        assert!(d.tick(now, true, true).is_empty());
        now += T;
        assert_eq!(d.tick(now, true, true), vec![Action::Stop]);
    }

    #[test]
    fn late_clicks_and_feature_off() {
        let mut d = Detector::default();
        assert!(d.record().is_empty());
        assert!(d.dismiss().is_empty());
        let (mut d, _) = prompting();
        assert_eq!(d.feature_off(), vec![Action::HidePrompt]);
        assert_eq!(d.state(), State::Idle);
        let (mut d, _) = recording();
        assert!(d.feature_off().is_empty()); // recording continues, no auto-stop
        assert_eq!(d.state(), State::Idle);
    }
}
