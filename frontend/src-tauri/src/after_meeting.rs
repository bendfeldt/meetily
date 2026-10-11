//! Fork-only (bendfeldt/meetily): record-only recordings that are transcribed
//! after the meeting.
//!
//! A pop-up (Teams) recording starts through `start_popup_recording`. When the
//! configured transcription provider can transcribe a saved file (local
//! Whisper or Parakeet), the recording runs record-only: audio is saved, live
//! transcription is skipped. When it stops, `on_recording_stopped` queues it
//! for one background worker that saves the meeting and transcribes the audio
//! in one batch. Whether a recording is empty is decided by the same VAD
//! evidence as manual recordings (`empty_recordings`), never by the batch
//! transcription: a batch that finds no audio or no speech for a recording
//! with VAD speech keeps the meeting and reports a failure. Cloud providers
//! cannot batch-transcribe, so for them the recording is a normal live one.
//!
//! Batch transcription is upstream's beta "retranscribe" engine; the pop-up
//! flow uses it regardless of the beta toggle (the user asked for
//! after-meeting transcription).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

use crate::audio::retranscription::{start_retranscription, ALREADY_IN_PROGRESS};
use crate::database::repositories::{
    setting::SettingsRepository, transcript::TranscriptsRepository,
};
use crate::empty_recordings::{self, Evidence};
use crate::state::AppState;

/// Meeting name of a pending record-only pop-up start; only the start with
/// that exact name consumes it, so a concurrent manual start never does.
static RECORD_ONLY_REQUESTED: Mutex<Option<String>> = Mutex::new(None);
/// Whether the current (or last) successfully started recording is
/// record-only. Only written when a recording has actually started.
static RECORD_ONLY_SESSION: AtomicBool = AtomicBool::new(false);
static QUEUE: OnceLock<UnboundedSender<Job>> = OnceLock::new();

const BUSY_RETRY: Duration = Duration::from_secs(10);
const BUSY_GIVE_UP: Duration = Duration::from_secs(30 * 60);
const RECORDING_POLL: Duration = Duration::from_secs(5);

struct Job {
    folder: String,
    meeting_name: String,
    evidence: Option<Evidence>,
}

/// Providers whose models can transcribe a saved recording in one batch.
pub fn can_batch(provider: &str) -> bool {
    matches!(provider, "localWhisper" | "whisper" | "parakeet")
}

fn requested() -> std::sync::MutexGuard<'static, Option<String>> {
    RECORD_ONLY_REQUESTED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Called by the device start path after its "already recording" check:
/// consumes the pending pop-up request if it is for this meeting.
pub fn take_record_only_request(meeting_name: Option<&str>) -> bool {
    let mut pending = requested();
    if pending.is_some() && pending.as_deref() == meeting_name {
        *pending = None;
        true
    } else {
        false
    }
}

/// Called by both start paths once the recording has actually started.
pub fn begin_session(record_only: bool) {
    RECORD_ONLY_SESSION.store(record_only, Ordering::SeqCst);
}

pub fn session_is_record_only() -> bool {
    RECORD_ONLY_SESSION.load(Ordering::SeqCst)
}

async fn transcript_provider_and_model<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<(String, String), String> {
    let pool = app.state::<AppState>().db_manager.pool().clone();
    match SettingsRepository::get_transcript_config(&pool).await {
        Ok(Some(config)) => Ok((config.provider, config.model)),
        // Same default as the live path (transcription/engine.rs).
        Ok(None) => Ok((
            "parakeet".to_string(),
            crate::config::DEFAULT_PARAKEET_MODEL.to_string(),
        )),
        Err(e) => Err(format!("could not read the transcription settings: {e}")),
    }
}

/// Start a pop-up recording with the preferred devices. Record-only when the
/// provider can batch-transcribe, live otherwise (also when the provider
/// cannot be read). Returns whether it is record-only.
pub async fn start_popup_recording<R: Runtime>(
    app: AppHandle<R>,
    meeting_name: String,
) -> Result<bool, String> {
    let record_only = match transcript_provider_and_model(&app).await {
        Ok((provider, _)) => can_batch(&provider),
        Err(e) => {
            log::warn!("after_meeting: {e}; starting a live recording");
            false
        }
    };
    let prefs = match crate::audio::recording_preferences::load_recording_preferences(&app).await {
        Ok(prefs) => prefs,
        Err(e) => {
            log::warn!(
                "after_meeting: recording preferences unavailable, using default devices: {e}"
            );
            Default::default()
        }
    };
    if record_only {
        *requested() = Some(meeting_name.clone());
    }
    let result = crate::start_recording(
        app,
        prefs.preferred_mic_device,
        prefs.preferred_system_device,
        Some(meeting_name),
    )
    .await;
    // A start that failed before reaching the start path must not leave the
    // request behind.
    *requested() = None;
    result.map(|_| record_only)
}

/// Lets the frontend stop handler skip its live-transcript save for a
/// record-only recording (the worker saves it).
#[tauri::command]
pub fn last_recording_was_record_only() -> bool {
    session_is_record_only()
}

fn emit_failure<R: Runtime>(app: &AppHandle<R>, meeting_id: Option<&str>, error: String) {
    log::warn!("after_meeting: {error}");
    let _ = app.emit(
        "meeting-transcription-failed",
        serde_json::json!({ "meeting_id": meeting_id, "error": error }),
    );
}

/// Called by `stop_recording` once the recording has stopped, right after
/// the VAD evidence for it was recorded.
pub fn on_recording_stopped<R: Runtime>(
    app: &AppHandle<R>,
    folder: Option<&str>,
    meeting_name: Option<&str>,
) {
    if !session_is_record_only() {
        return;
    }
    let Some(folder) = folder else {
        emit_failure(
            app,
            None,
            "the recording has no folder, so it cannot be transcribed".into(),
        );
        return;
    };
    let job = Job {
        folder: folder.to_string(),
        meeting_name: meeting_name.unwrap_or("Meeting").to_string(),
        evidence: empty_recordings::evidence_for(folder),
    };
    let queue = QUEUE.get_or_init(|| {
        let (tx, mut rx) = unbounded_channel::<Job>();
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(job) = rx.recv().await {
                process(&app, job).await;
            }
        });
        tx
    });
    if queue.send(job).is_err() {
        emit_failure(
            app,
            None,
            format!("the transcription worker stopped; audio kept in {folder}"),
        );
    }
}

enum Outcome {
    Transcribed,
    Busy,
    Failed(String),
}

fn classify(result: &anyhow::Result<()>) -> Outcome {
    match result {
        Ok(()) => Outcome::Transcribed,
        Err(e) => {
            let msg = e.to_string();
            // Anything else, including "no speech" from the batch, is a
            // failure: the VAD already heard speech, so the meeting is kept.
            if msg.starts_with(ALREADY_IN_PROGRESS) {
                Outcome::Busy
            } else {
                Outcome::Failed(format!("{msg} (the meeting and its audio were kept)"))
            }
        }
    }
}

async fn process<R: Runtime>(app: &AppHandle<R>, job: Job) {
    let pool = app.state::<AppState>().db_manager.pool().clone();
    let meeting_id = match TranscriptsRepository::save_transcript(
        &pool,
        &job.meeting_name,
        &[],
        Some(job.folder.clone()),
    )
    .await
    {
        Ok(id) => id,
        Err(e) => {
            emit_failure(
                app,
                None,
                format!(
                    "could not save the meeting ({e}); audio kept in {}",
                    job.folder
                ),
            );
            return;
        }
    };

    if job.evidence.is_some_and(|e| e.is_empty()) {
        let root = crate::audio::recording_preferences::get_default_recordings_folder();
        match empty_recordings::discard_meeting(&pool, &meeting_id, &root).await {
            Ok(true) => {
                let _ = app.emit(
                    "meeting-discarded",
                    serde_json::json!({ "meeting_id": meeting_id }),
                );
            }
            Ok(false) => emit_failure(
                app,
                Some(&meeting_id),
                "the empty recording could not be deleted".into(),
            ),
            Err(e) => emit_failure(
                app,
                Some(&meeting_id),
                format!("the empty recording could not be deleted: {e}"),
            ),
        }
        return;
    }
    let _ = app.emit(
        "meeting-saved",
        serde_json::json!({ "meeting_id": meeting_id }),
    );

    // Any running recording may load or unload the shared transcription
    // engine; batch only when nothing records.
    // shortcut: a recording started during the batch can still unload the engine under it (as with upstream's Retranscribe), queue live starts if that bites
    while crate::audio::recording_commands::is_recording().await {
        tokio::time::sleep(RECORDING_POLL).await;
    }

    let (provider, model) = match transcript_provider_and_model(app).await {
        Ok(config) => config,
        Err(e) => return emit_failure(app, Some(&meeting_id), e),
    };
    let language = if provider == "parakeet" {
        None
    } else {
        crate::get_language_preference_internal()
    };

    let started = Instant::now();
    let outcome = loop {
        let result = start_retranscription(
            app.clone(),
            meeting_id.clone(),
            job.folder.clone(),
            language.clone(),
            Some(model.clone()),
            Some(provider.clone()),
        )
        .await
        .map(|_| ());
        match classify(&result) {
            Outcome::Busy if started.elapsed() < BUSY_GIVE_UP => {
                tokio::time::sleep(BUSY_RETRY).await
            }
            Outcome::Busy => {
                break Outcome::Failed("another transcription kept running for 30 minutes".into())
            }
            other => break other,
        }
    };

    match outcome {
        Outcome::Transcribed => {
            let _ = app.emit(
                "meeting-ready",
                serde_json::json!({ "meeting_id": meeting_id }),
            );
        }
        Outcome::Failed(error) => emit_failure(app, Some(&meeting_id), error),
        Outcome::Busy => unreachable!("Busy is resolved in the loop"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_capable_providers() {
        assert!(can_batch("parakeet"));
        assert!(can_batch("localWhisper"));
        assert!(can_batch("whisper"));
        for cloud in ["deepgram", "elevenLabs", "groq", "openai"] {
            assert!(!can_batch(cloud));
        }
    }

    #[test]
    fn batch_errors_never_mean_empty() {
        let err = |m: String| -> anyhow::Result<()> { Err(anyhow::anyhow!(m)) };
        assert!(matches!(classify(&Ok(())), Outcome::Transcribed));
        assert!(matches!(
            classify(&err(ALREADY_IN_PROGRESS.into())),
            Outcome::Busy
        ));
        assert!(matches!(
            classify(&err("No speech detected in audio file".into())),
            Outcome::Failed(m) if m.ends_with("(the meeting and its audio were kept)")
        ));
        assert!(matches!(
            classify(&err("decoder exploded".into())),
            Outcome::Failed(_)
        ));
    }

    // The only test touching the request/session statics.
    #[test]
    fn requests_and_sessions() {
        *requested() = Some("Teams meeting 1".into());
        assert!(!take_record_only_request(Some("Manual meeting"))); // other start: untouched
        assert!(!take_record_only_request(None));
        assert!(take_record_only_request(Some("Teams meeting 1")));
        assert!(!take_record_only_request(Some("Teams meeting 1"))); // consumed once
        begin_session(true);
        assert!(session_is_record_only());
        begin_session(false); // the next (manual) recording
        assert!(!session_is_record_only());
    }
}
