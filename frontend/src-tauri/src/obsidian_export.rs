//! Fork-only (bendfeldt/meetily): opt-in export of a meeting to a Markdown
//! note for Obsidian. A meeting is "exported" once its note exists in the
//! export folder (found by `meetily_id` in the frontmatter); from then on the
//! note is regenerated on summary/title changes and renamed with the title.
//! The note is generated output: edits made to it in Obsidian are overwritten
//! by the next sync.

use std::io::Read;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};
use serde_json::Value;
use sqlx::SqlitePool;
use tauri::{AppHandle, Runtime};

use crate::database::repositories::{
    meeting::MeetingsRepository, summary::SummaryProcessesRepository,
};
use crate::fork_settings;
use crate::state::AppState;

/// Title part of the file name, in bytes: keeps the whole name well under
/// the 255-byte limit of APFS/ext4 for any script.
const MAX_TITLE_BYTES: usize = 150;
const FRONTMATTER_SCAN_BYTES: u64 = 512;

/// Serializes note writes so a summary save and a summary completion for the
/// same meeting cannot interleave.
static WRITE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub struct Segment {
    pub start: Option<f64>,
    pub end: Option<f64>,
    pub text: String,
}

pub struct NoteInput {
    pub meeting_id: String,
    pub title: String,
    pub created_at: DateTime<Local>,
    pub summary_markdown: Option<String>,
    pub transcript: Vec<Segment>,
}

/// `YYYY-MM-DD HHmm - <title>.md`, with characters Obsidian, macOS and
/// Windows reject in file names removed.
pub fn note_file_name(created_at: DateTime<Local>, title: &str) -> String {
    let cleaned: String = title
        .chars()
        .filter(|c| !c.is_control() && !"*\"\\/<>:|?#^[]".contains(*c))
        .collect();
    let mut title = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    title = title.trim_matches('.').trim().to_string();
    if title.len() > MAX_TITLE_BYTES {
        let cut = (0..=MAX_TITLE_BYTES)
            .rev()
            .find(|i| title.is_char_boundary(*i))
            .unwrap_or(0);
        title.truncate(cut);
        title = title.trim_end().to_string();
    }
    if title.is_empty() {
        title = "Untitled".to_string();
    }
    format!("{} - {}.md", created_at.format("%Y-%m-%d %H%M"), title)
}

fn yaml_string(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' | '\r' | '\t' => out.push(' '),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn timestamp(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    format!(
        "{:02}:{:02}:{:02}",
        total / 3600,
        total % 3600 / 60,
        total % 60
    )
}

/// Markdown of a stored summary (`summary_processes.result`), from its
/// `markdown` field.
// shortcut: summaries without `markdown` (legacy sections, BlockNote-only) fail the export, convert them if old meetings need exporting
pub fn summary_markdown(summary: &Value) -> Option<String> {
    let markdown = summary.get("markdown").and_then(Value::as_str)?;
    // Same cleaning as the meeting page: strips model reasoning envelopes.
    let cleaned = crate::summary::processor::clean_llm_markdown_detailed(markdown).markdown;
    let markdown = cleaned.trim();
    (!markdown.is_empty()).then(|| markdown.to_string())
}

pub fn render_markdown(note: &NoteInput) -> String {
    // meetily_id first: find_note only reads the start of each file.
    let mut out = format!("---\nmeetily_id: {}\n", yaml_string(&note.meeting_id));
    out.push_str(&format!("title: {}\n", yaml_string(&note.title)));
    out.push_str(&format!("date: {}\n", note.created_at.format("%Y-%m-%d")));
    out.push_str(&format!("created: {}\n", note.created_at.to_rfc3339()));
    let duration = note
        .transcript
        .iter()
        .filter_map(|s| s.end.or(s.start))
        .fold(None, |max: Option<f64>, t| {
            Some(max.map_or(t, |m| m.max(t)))
        });
    if let Some(seconds) = duration.filter(|s| *s > 0.0) {
        out.push_str(&format!(
            "duration_min: {}\n",
            (seconds / 60.0).ceil() as u64
        ));
    }
    out.push_str("tags: [meeting]\n---\n\n");
    let heading: String = note
        .title
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    out.push_str(&format!("# {}\n", heading.trim()));
    if let Some(summary) = &note.summary_markdown {
        out.push_str(&format!("\n## Summary\n\n{summary}\n"));
    }
    out.push_str("\n## Transcript\n\n");
    for segment in &note.transcript {
        let text = segment.text.trim();
        if text.is_empty() {
            continue;
        }
        match segment.start {
            Some(seconds) => out.push_str(&format!("- **[{}]** {text}\n", timestamp(seconds))),
            None => out.push_str(&format!("- {text}\n")),
        }
    }
    out
}

/// The note directly in `dir` whose frontmatter carries this meeting id.
// shortcut: an unreadable note (e.g. iCloud placeholder) counts as missing and can get a duplicate, surface read errors if that happens
// shortcut: linear scan of the export folder per sync, keep an id→file index if it holds thousands of notes
pub fn find_note(dir: &Path, meeting_id: &str) -> Option<PathBuf> {
    let needle = format!("meetily_id: {}", yaml_string(meeting_id));
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|path| {
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                return false;
            }
            let mut head = Vec::new();
            if std::fs::File::open(path)
                .and_then(|f| f.take(FRONTMATTER_SCAN_BYTES).read_to_end(&mut head))
                .is_err()
            {
                return false;
            }
            String::from_utf8_lossy(&head)
                .lines()
                .any(|line| line.trim_end() == needle)
        })
}

/// True when both paths name the same file on disk (e.g. `b.md` and `B.md`
/// on a case-insensitive file system).
fn same_file(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (std::fs::metadata(a), std::fs::metadata(b)) {
            (Ok(x), Ok(y)) => x.dev() == y.dev() && x.ino() == y.ino(),
            _ => false,
        }
    }
    // shortcut: path equality off Unix, so a case-only rename on Windows gets a " (2)" note; macOS is the target platform
    #[cfg(not(unix))]
    {
        a == b
    }
}

/// Write `content` as `file_name` in `dir` (temp file + rename). A different
/// note already using the name gets a ` (n)` suffix; a previous note for the
/// same meeting under another name is removed afterwards (title rename).
pub fn write_note(
    dir: &Path,
    existing: Option<&Path>,
    file_name: &str,
    content: &str,
) -> std::io::Result<PathBuf> {
    let stem = file_name.trim_end_matches(".md");
    let mut target = dir.join(file_name);
    let mut n = 2;
    // On case-insensitive file systems (macOS default) a case-only rename
    // finds the meeting's own note; that is not a clash.
    while target.exists() && !existing.is_some_and(|old| same_file(old, &target)) {
        target = dir.join(format!("{stem} ({n}).md"));
        n += 1;
    }
    // WRITE_LOCK serializes writes and the app is single-instance.
    let temp = dir.join(".meetily-note.tmp");
    let written = std::fs::write(&temp, content).and_then(|_| std::fs::rename(&temp, &target));
    if let Err(e) = written {
        let _ = std::fs::remove_file(&temp);
        return Err(e);
    }
    if let Some(old) = existing.filter(|old| !same_file(old, &target)) {
        // shortcut: a failed removal leaves two notes for one meeting (logged), retry removal on next sync if it shows up
        if let Err(e) = std::fs::remove_file(old) {
            log::warn!(
                "obsidian_export: could not remove old note {}: {e}",
                old.display()
            );
        }
    }
    Ok(target)
}

async fn load_note(pool: &SqlitePool, meeting_id: &str) -> anyhow::Result<NoteInput> {
    let meeting = MeetingsRepository::get_meeting(pool, meeting_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Meeting {meeting_id} not found"))?;
    let created_at = DateTime::parse_from_rfc3339(&meeting.created_at)?.with_timezone(&Local);
    // Same query the meeting page uses for its summary.
    let stored = SummaryProcessesRepository::get_summary_data_for_meeting(pool, meeting_id)
        .await?
        .and_then(|process| process.result);
    // An unreadable summary, or one without Markdown (legacy sections, or an
    // editor save whose Markdown conversion failed), fails the export rather
    // than erasing the summary from an existing note.
    let summary_markdown = match stored {
        Some(result) => Some(
            summary_markdown(&serde_json::from_str::<Value>(&result)?).ok_or_else(|| {
                anyhow::anyhow!("the summary has no Markdown version; re-save or regenerate it")
            })?,
        ),
        None => None,
    };
    let mut transcript: Vec<Segment> = meeting
        .transcripts
        .into_iter()
        .map(|t| Segment {
            start: t.audio_start_time,
            end: t.audio_end_time,
            text: t.text,
        })
        .collect();
    // Untimed segments keep their order after the timed ones.
    transcript.sort_by(|a, b| {
        a.start
            .unwrap_or(f64::INFINITY)
            .total_cmp(&b.start.unwrap_or(f64::INFINITY))
    });
    Ok(NoteInput {
        meeting_id: meeting.id,
        title: meeting.title,
        created_at,
        summary_markdown,
        transcript,
    })
}

/// Write the meeting's note. With `only_if_exported`, do nothing unless a
/// note already exists. The DB snapshot and the write happen under one lock,
/// so concurrent syncs cannot write stale content last.
async fn export_to(
    dir: &Path,
    pool: &SqlitePool,
    meeting_id: &str,
    only_if_exported: bool,
) -> anyhow::Result<Option<PathBuf>> {
    let _guard = WRITE_LOCK.lock().await;
    let existing = find_note(dir, meeting_id);
    if only_if_exported && existing.is_none() {
        return Ok(None);
    }
    let note = load_note(pool, meeting_id).await?;
    let file_name = note_file_name(note.created_at, &note.title);
    let content = render_markdown(&note);
    Ok(Some(write_note(
        dir,
        existing.as_deref(),
        &file_name,
        &content,
    )?))
}

/// Regenerate (and rename if needed) the meeting's note, only if it was
/// exported before. Errors are logged; callers never fail because of the note.
pub fn spawn_sync_if_exported<R: Runtime>(app: AppHandle<R>, pool: SqlitePool, meeting_id: String) {
    tauri::async_runtime::spawn(async move {
        let Some(dir) = fork_settings::load(&app).obsidian_export_folder else {
            return;
        };
        if !dir.is_dir() {
            log::warn!("obsidian_export: export folder is missing, notes are not synced");
            return;
        }
        if let Err(e) = export_to(&dir, &pool, &meeting_id, true).await {
            log::warn!("obsidian_export: sync failed for {meeting_id}: {e}");
        }
    });
}

async fn pick_folder<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("Choose the folder for meeting notes (e.g. in your Obsidian vault)")
        .pick_folder(move |path| {
            let _ = tx.send(path);
        });
    rx.await
        .ok()
        .flatten()
        .and_then(|path| path.into_path().ok())
}

/// Settings: choose the export folder. Returns the new folder, or `None` if
/// the picker was cancelled.
#[tauri::command]
pub async fn choose_obsidian_export_folder<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Option<String>, String> {
    Ok(choose_folder(&app)
        .await?
        .map(|dir| dir.to_string_lossy().into_owned()))
}

/// Pick a folder and store it. Reloads the settings after the picker so a
/// change made while it was open is kept.
async fn choose_folder<R: Runtime>(app: &AppHandle<R>) -> Result<Option<PathBuf>, String> {
    let Some(dir) = pick_folder(app).await else {
        return Ok(None);
    };
    let mut settings = fork_settings::load(app);
    settings.obsidian_export_folder = Some(dir.clone());
    fork_settings::save(app, &settings).map_err(|e| e.to_string())?;
    Ok(Some(dir))
}

/// Export a meeting note. Asks for the export folder when none is set (it is
/// changed in Settings). Returns the note path, or `None` if the user
/// cancelled the folder picker.
#[tauri::command]
pub async fn export_meeting_markdown<R: Runtime>(
    app: AppHandle<R>,
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<Option<String>, String> {
    let dir = match fork_settings::load(&app).obsidian_export_folder {
        Some(dir) => dir,
        None => match choose_folder(&app).await? {
            Some(dir) => dir,
            None => return Ok(None),
        },
    };
    export_to(&dir, state.db_manager.pool(), &meeting_id, false)
        .await
        .map(|path| path.map(|p| p.to_string_lossy().into_owned()))
        .map_err(|e| format!("Export to Obsidian failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;

    fn when() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 10, 10, 14, 5, 0).unwrap()
    }

    fn seg(start: Option<f64>, end: Option<f64>, text: &str) -> Segment {
        Segment {
            start,
            end,
            text: text.into(),
        }
    }

    fn note(title: &str) -> NoteInput {
        NoteInput {
            meeting_id: "m-1".into(),
            title: title.into(),
            created_at: when(),
            summary_markdown: None,
            transcript: vec![
                seg(Some(12.0), Some(15.0), "Hello"),
                seg(Some(125.5), Some(190.0), "Bye"),
                seg(None, None, "  "),
            ],
        }
    }

    #[test]
    fn file_name_strips_forbidden_characters_and_caps_bytes() {
        assert_eq!(
            note_file_name(when(), "Q3: plan/review? [draft] #1"),
            "2026-10-10 1405 - Q3 planreview draft 1.md"
        );
        assert_eq!(
            note_file_name(when(), " ... "),
            "2026-10-10 1405 - Untitled.md"
        );
        let emoji = note_file_name(when(), &"😀".repeat(100));
        assert!(emoji.len() < 255);
        assert!(emoji.ends_with("😀.md"));
    }

    #[test]
    fn renders_frontmatter_summary_and_transcript() {
        let mut n = note("Weekly \"sync\"\r\nnext");
        n.summary_markdown = summary_markdown(
            &json!({"markdown": "# Notes\n#tag text\n###### Deep\n- a\n~~~\n# not a heading\n~~~"}),
        );
        let md = render_markdown(&n);
        assert!(md.starts_with(
            "---\nmeetily_id: \"m-1\"\ntitle: \"Weekly \\\"sync\\\"  next\"\ndate: 2026-10-10\n"
        ));
        assert!(md.contains("duration_min: 4\ntags: [meeting]\n---\n"));
        assert!(md.contains(
            "## Summary\n\n# Notes\n#tag text\n###### Deep\n- a\n~~~\n# not a heading\n~~~\n"
        ));
        assert!(md.contains("- **[00:00:12]** Hello\n- **[00:02:05]** Bye\n"));
        assert!(!md.contains("- \n"));
    }

    #[test]
    fn titles_with_newlines_and_reasoning_envelopes() {
        assert!(render_markdown(&note("a\nb")).contains("\n# a b\n"));
        let think = summary_markdown(&json!({"markdown": "<think>secret</think>\n# Visible"}));
        assert_eq!(think.as_deref(), Some("# Visible"));
    }

    #[test]
    fn no_summary_section_without_summary() {
        let md = render_markdown(&note("T"));
        assert!(!md.contains("## Summary"));
        assert_eq!(summary_markdown(&json!({"markdown": "  "})), None);
    }

    #[test]
    fn legacy_summaries_are_skipped() {
        let legacy = json!({"_section_order": ["A"], "A": {"blocks": [{"content": "x"}]}});
        assert_eq!(summary_markdown(&legacy), None);
    }

    #[test]
    fn write_find_rename_and_clash() {
        let dir = tempfile::tempdir().unwrap();
        let first = write_note(dir.path(), None, "a.md", &render_markdown(&note("a"))).unwrap();
        assert_eq!(find_note(dir.path(), "m-1"), Some(first.clone()));
        assert_eq!(find_note(dir.path(), "m-2"), None);

        // Rename: new name written, old note removed.
        let renamed = write_note(
            dir.path(),
            Some(&first),
            "b.md",
            "---\nmeetily_id: \"m-1\"\r\n---\n",
        )
        .unwrap();
        assert!(!first.exists());
        assert_eq!(find_note(dir.path(), "m-1"), Some(renamed.clone()));

        // Case-only rename: one note afterwards, whatever the file system's case rules.
        let upper = write_note(
            dir.path(),
            Some(&renamed),
            "B.md",
            "---\nmeetily_id: \"m-1\"\n",
        )
        .unwrap();
        assert_eq!(upper, dir.path().join("B.md"));
        let notes = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case("b.md"))
            .count();
        assert_eq!(notes, 1);

        // Name taken by another note: suffix, other note untouched.
        std::fs::write(dir.path().join("c.md"), "other").unwrap();
        let clash = write_note(dir.path(), None, "c.md", "mine").unwrap();
        assert_eq!(clash, dir.path().join("c (2).md"));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("c.md")).unwrap(),
            "other"
        );

        // No temp files left behind.
        assert!(std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .all(|e| !e.file_name().to_string_lossy().ends_with(".tmp")));
    }

    #[test]
    fn find_note_survives_multibyte_text_at_the_scan_boundary() {
        let dir = tempfile::tempdir().unwrap();
        let body = format!(
            "---\nmeetily_id: \"m-9\"\ntitle: \"{}\"\n---\n",
            "ä".repeat(600)
        );
        std::fs::write(dir.path().join("x.md"), body).unwrap();
        assert!(find_note(dir.path(), "m-9").is_some());
    }

    #[test]
    fn failed_write_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("taken.md")).unwrap(); // rename onto a directory fails
        let existing = dir.path().join("taken.md");
        assert!(write_note(dir.path(), Some(&existing), "taken.md", "x").is_err());
        assert!(std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .all(|e| !e.file_name().to_string_lossy().ends_with(".tmp")));
    }
}
