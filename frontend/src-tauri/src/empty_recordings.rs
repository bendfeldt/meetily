//! Fork-only (bendfeldt/meetily): delete recordings that captured no speech.
//!
//! "Empty" means the voice detector (VAD) reported no speech segment for the
//! whole recording and never errored. The VAD reads the same mixed audio that
//! is written to the recording file, and the decision does not depend on
//! transcription, so a failed or filtered transcription can never make a
//! recording with speech look empty. Noise that Whisper turns into filler
//! ("Thank you.") counts as speech here and is kept: the safe direction.
//!
//! A pipeline that errors or panics before finishing counts as a VAD error.
//!
//! The evidence is tied to the recording's folder and cleared when the next
//! recording starts; without matching evidence nothing is deleted.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use sqlx::SqlitePool;

use crate::audio::recording_preferences::get_default_recordings_folder;
use crate::database::repositories::meeting::MeetingsRepository;
use crate::state::AppState;

// shortcut: speech bursts under the VAD's 250 ms minimum and the last <600 ms window left in the mixer at stop are not counted, a recording whose only sound is that short counts as empty; count VAD speech frames if real recordings get deleted
static SPEECH_SEGMENTS: AtomicU64 = AtomicU64::new(0);
static VAD_ERRORS: AtomicU64 = AtomicU64::new(0);
/// Evidence of the last stopped recording, keyed by its folder.
static LAST_STOPPED: Mutex<Option<(String, Evidence)>> = Mutex::new(None);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Evidence {
    pub speech_segments: u64,
    pub vad_errors: u64,
}

impl Evidence {
    pub fn is_empty(&self) -> bool {
        self.speech_segments == 0 && self.vad_errors == 0
    }
}

fn last_stopped() -> MutexGuard<'static, Option<(String, Evidence)>> {
    LAST_STOPPED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Called when a recording starts, before its pipeline runs.
pub fn session_started() {
    *last_stopped() = None;
    SPEECH_SEGMENTS.store(0, Ordering::SeqCst);
    VAD_ERRORS.store(0, Ordering::SeqCst);
}

/// The VAD reported a speech segment (any length, sent or not).
pub fn speech_segment() {
    SPEECH_SEGMENTS.fetch_add(1, Ordering::Relaxed);
}

/// The VAD failed on some audio; that audio's content is unknown.
pub fn vad_error() {
    VAD_ERRORS.fetch_add(1, Ordering::Relaxed);
}

/// Called when the recording has fully stopped, with the folder it wrote to.
/// The pipeline (and its final VAD flush) has finished by then.
pub fn session_stopped(folder: Option<&str>) {
    let evidence = Evidence {
        speech_segments: SPEECH_SEGMENTS.load(Ordering::SeqCst),
        vad_errors: VAD_ERRORS.load(Ordering::SeqCst),
    };
    *last_stopped() = folder.map(|f| (f.to_string(), evidence));
}

fn evidence_for(folder: &str) -> Option<Evidence> {
    last_stopped()
        .as_ref()
        .filter(|(f, _)| f == folder)
        .map(|(_, e)| *e)
}

/// The folder to delete, only if it resolves (symlinks included) to a
/// directory strictly inside the recordings root.
fn folder_inside_root(folder: &Path, root: &Path) -> Option<PathBuf> {
    let folder = folder.canonicalize().ok()?;
    let root = root.canonicalize().ok()?;
    (folder != root && folder.starts_with(&root) && folder.is_dir()).then_some(folder)
}

async fn meeting_folder(pool: &SqlitePool, meeting_id: &str) -> sqlx::Result<Option<String>> {
    Ok(
        sqlx::query_scalar("SELECT folder_path FROM meetings WHERE id = ?")
            .bind(meeting_id)
            .fetch_optional(pool)
            .await?
            .flatten(),
    )
}

/// Delete the meeting's DB rows, and its recording folder if no other
/// meeting uses it and it sits inside `root`. Returns whether rows were
/// deleted.
pub async fn discard_meeting(
    pool: &SqlitePool,
    meeting_id: &str,
    root: &Path,
) -> anyhow::Result<bool> {
    let folder = meeting_folder(pool, meeting_id).await?;
    let shared: i64 = match &folder {
        Some(f) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM meetings WHERE folder_path = ? AND id != ?")
                .bind(f)
                .bind(meeting_id)
                .fetch_one(pool)
                .await?
        }
        None => 0,
    };
    if !MeetingsRepository::delete_meeting(pool, meeting_id).await? {
        return Ok(false);
    }
    log::info!("empty_recordings: deleted meeting {meeting_id} (no speech)");
    let Some(folder) = folder else {
        return Ok(true);
    };
    match folder_inside_root(Path::new(&folder), root) {
        Some(dir) if shared == 0 => {
            // shortcut: a failed folder removal is only logged, the UI still reports the recording as deleted
            if let Err(e) = std::fs::remove_dir_all(&dir) {
                log::warn!("empty_recordings: could not remove {}: {e}", dir.display());
            }
        }
        _ => log::warn!(
            "empty_recordings: kept folder {folder} (shared, missing or outside the recordings folder)"
        ),
    }
    Ok(true)
}

/// Delete the meeting if the recording that produced it was empty.
pub async fn discard_if_empty(pool: &SqlitePool, meeting_id: &str) -> anyhow::Result<bool> {
    let folder = meeting_folder(pool, meeting_id).await?;
    let Some(evidence) = folder.as_deref().and_then(evidence_for) else {
        return Ok(false); // no evidence for this recording: keep it
    };
    if !evidence.is_empty() {
        return Ok(false);
    }
    discard_meeting(pool, meeting_id, &get_default_recordings_folder()).await
}

#[tauri::command]
pub async fn discard_meeting_if_empty(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<bool, String> {
    discard_if_empty(state.db_manager.pool(), &meeting_id)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    fn evidence(speech_segments: u64, vad_errors: u64) -> Evidence {
        Evidence {
            speech_segments,
            vad_errors,
        }
    }

    #[test]
    fn emptiness_rule() {
        assert!(evidence(0, 0).is_empty()); // VAD heard nothing
        assert!(!evidence(1, 0).is_empty()); // any speech: keep
        assert!(!evidence(0, 1).is_empty()); // VAD failed, content unknown: keep
    }

    // The only test touching the session globals, so parallel tests can't race.
    #[tokio::test]
    async fn evidence_only_matches_the_stopped_recording() {
        session_started();
        speech_segment();
        session_stopped(Some("/rec/a"));
        assert_eq!(evidence_for("/rec/a"), Some(evidence(1, 0)));
        assert_eq!(evidence_for("/rec/b"), None);

        // A new recording clears the previous evidence immediately.
        session_started();
        assert_eq!(evidence_for("/rec/a"), None);

        session_stopped(Some("/rec/a"));
        assert_eq!(evidence_for("/rec/a"), Some(evidence(0, 0)));
        session_stopped(None);
        assert_eq!(evidence_for("/rec/a"), None);

        // Through discard_if_empty: speech keeps the meeting, silence deletes
        // its rows (the temp folder is outside the real recordings root, so
        // it is kept on disk).
        let pool = db().await;
        let base = tempfile::tempdir().unwrap();
        let folder = base.path().join("rec");
        std::fs::create_dir_all(&folder).unwrap();
        add_meeting(&pool, "m1", &folder).await;
        let folder_str = folder.to_string_lossy().to_string();

        session_started();
        speech_segment();
        session_stopped(Some(&folder_str));
        assert!(!discard_if_empty(&pool, "m1").await.unwrap());

        session_started();
        session_stopped(Some(&folder_str));
        assert!(discard_if_empty(&pool, "m1").await.unwrap());
        assert!(folder.exists());
        let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM meetings")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(left, 0);
    }

    #[test]
    fn only_folders_strictly_inside_the_root_are_removable() {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("recordings");
        let meeting = root.join("Meeting 1");
        let outside = base.path().join("elsewhere");
        std::fs::create_dir_all(&meeting).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        assert_eq!(
            folder_inside_root(&meeting, &root),
            Some(meeting.canonicalize().unwrap())
        );
        assert_eq!(folder_inside_root(&root, &root), None);
        assert_eq!(folder_inside_root(&outside, &root), None);
        assert_eq!(folder_inside_root(&root.join("missing"), &root), None);
        assert_eq!(
            folder_inside_root(&root.join("Meeting 1/../../elsewhere"), &root),
            None
        );

        #[cfg(unix)]
        {
            let link = root.join("link");
            std::os::unix::fs::symlink(&outside, &link).unwrap();
            assert_eq!(folder_inside_root(&link, &root), None);
        }
    }

    async fn db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    async fn add_meeting(pool: &SqlitePool, id: &str, folder: &Path) {
        sqlx::query(
            "INSERT INTO meetings (id, title, created_at, updated_at, folder_path) VALUES (?, 't', datetime('now'), datetime('now'), ?)",
        )
        .bind(id)
        .bind(folder.to_string_lossy().to_string())
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn discard_removes_rows_and_unshared_folder_only() {
        let pool = db().await;
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("recordings");
        let own = root.join("own");
        let shared = root.join("shared");
        std::fs::create_dir_all(&own).unwrap();
        std::fs::create_dir_all(&shared).unwrap();
        add_meeting(&pool, "m1", &own).await;
        add_meeting(&pool, "m2", &shared).await;
        add_meeting(&pool, "m3", &shared).await;

        assert!(discard_meeting(&pool, "m1", &root).await.unwrap());
        assert!(!own.exists());
        assert!(discard_meeting(&pool, "m2", &root).await.unwrap());
        assert!(shared.exists(), "folder still used by m3 must be kept");
        let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM meetings")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(left, 1);
        assert!(!discard_meeting(&pool, "missing", &root).await.unwrap());
    }

    #[tokio::test]
    async fn discard_if_empty_keeps_meetings_without_evidence() {
        let pool = db().await;
        let base = tempfile::tempdir().unwrap();
        let folder = base.path().join("rec");
        std::fs::create_dir_all(&folder).unwrap();
        add_meeting(&pool, "m1", &folder).await;
        // No recording ever stopped with this (unique temp) folder.
        assert!(!discard_if_empty(&pool, "m1").await.unwrap());
        assert!(folder.exists());
    }
}
