use crate::api::{TranscriptSearchResult, TranscriptSegment};
use chrono::Utc;
use sqlx::{Connection, Error as SqlxError, SqlitePool};
use log::{error, info};
use uuid::Uuid;

pub struct TranscriptsRepository;

impl TranscriptsRepository {
    /// Saves a new meeting and its associated transcript segments.
    /// This function uses a transaction to ensure that either both the meeting
    /// and all its transcripts are saved, or none of them are.
    pub async fn save_transcript(
        pool: &SqlitePool,
        meeting_title: &str,
        transcripts: &[TranscriptSegment],
        folder_path: Option<String>,
    ) -> Result<String, SqlxError> {
        let meeting_id = format!("meeting-{}", Uuid::new_v4());

        let mut conn = pool.acquire().await?;
        let mut transaction = conn.begin().await?;

        let now = Utc::now();

        // 1. Create the new meeting
        let result = sqlx::query(
            "INSERT INTO meetings (id, title, created_at, updated_at, folder_path) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&meeting_id)
        .bind(meeting_title)
        .bind(now)
        .bind(now)
        .bind(&folder_path)
        .execute(&mut *transaction)
        .await;

        if let Err(e) = result {
            error!("Failed to create meeting with id: {}", meeting_id);
            transaction.rollback().await?;
            return Err(e);
        }

        info!("Successfully created meeting with id: {}", meeting_id);

        // 2. Save each transcript segment with audio timing fields
        for segment in transcripts {
            let transcript_id = format!("transcript-{}", Uuid::new_v4());
            let result = sqlx::query(
                "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, duration)
                 VALUES (?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&transcript_id)
            .bind(&meeting_id)
            .bind(&segment.text)
            .bind(&segment.timestamp)
            .bind(segment.audio_start_time)
            .bind(segment.audio_end_time)
            .bind(segment.duration)
            .execute(&mut *transaction)
            .await;

            if let Err(e) = result {
                error!("Failed to save transcript segment for meeting {}", meeting_id);
                transaction.rollback().await?;
                return Err(e);
            }
        }

        info!(
            "Successfully saved {} transcript segments for meeting {}",
            transcripts.len(),
            meeting_id
        );

        // Commit the transaction
        transaction.commit().await?;

        Ok(meeting_id)
    }

    /// Searches for a query string within the transcripts.
    /// It returns a list of matching transcripts with context.
    pub async fn search_transcripts(
        pool: &SqlitePool,
        query: &str,
    ) -> Result<Vec<TranscriptSearchResult>, SqlxError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let search_query = format!("%{}%", query.to_lowercase());

        let rows = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT m.id, m.title, t.transcript, t.timestamp
             FROM meetings m
             JOIN transcripts t ON m.id = t.meeting_id
             WHERE LOWER(t.transcript) LIKE ?",
        )
        .bind(&search_query)
        .fetch_all(pool)
        .await?;

        let results = rows
            .into_iter()
            .map(|(id, title, transcript, timestamp)| {
                let match_context = Self::get_match_context(&transcript, query);
                TranscriptSearchResult {
                    id,
                    title,
                    match_context,
                    timestamp,
                }
            })
            .collect();

        Ok(results)
    }

    /// Helper function to extract a snippet of text around the first match of a query.
    /// Note: the SQL filter uses SQLite LOWER(), which only folds ASCII, so
    /// non-ASCII case differences may not match at the query level.
    fn get_match_context(transcript: &str, query: &str) -> String {
        let transcript_lower = transcript.to_lowercase();
        let query_lower = query.to_lowercase();

        // Offsets found in the lowercased copy are only valid in the original when
        // lowercasing kept the byte length. Otherwise (e.g. 'İ' grows from 2 to 3 bytes)
        // scan the original's char boundaries, lowercasing a query-sized window at each.
        let found = if transcript_lower.len() == transcript.len() {
            transcript_lower
                .find(&query_lower)
                .map(|i| (i, i + query.len()))
        } else {
            let window_chars = query_lower.chars().count();
            transcript.char_indices().find_map(|(i, _)| {
                let end = transcript[i..]
                    .char_indices()
                    .nth(window_chars)
                    .map_or(transcript.len(), |(j, _)| i + j);
                transcript[i..end]
                    .to_lowercase()
                    .starts_with(&query_lower)
                    .then_some((i, end))
            })
        };

        match found {
            Some((match_index, match_end)) => {
                // Clamp and snap to the original's char boundaries so slicing never
                // lands inside a multi-byte char.
                let mut start_index = match_index.saturating_sub(100).min(transcript.len());
                while !transcript.is_char_boundary(start_index) {
                    start_index -= 1;
                }
                let mut end_index = (match_end + 100).min(transcript.len());
                while !transcript.is_char_boundary(end_index) {
                    end_index += 1;
                }

                let mut context = String::new();
                if start_index > 0 {
                    context.push_str("...");
                }
                context.push_str(&transcript[start_index..end_index]);
                if end_index < transcript.len() {
                    context.push_str("...");
                }
                context
            }
            None => transcript.chars().take(200).collect(), // Fallback to the start of the transcript
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(transcript: &str, query: &str) -> String {
        TranscriptsRepository::get_match_context(transcript, query)
    }

    #[test]
    fn match_context_handles_multibyte_text() {
        let transcript = format!("{}xhello world", "æ".repeat(60));
        let expected = format!("...{}xhello world", "æ".repeat(50));
        assert_eq!(ctx(&transcript, "HELLO"), expected);
    }

    #[test]
    fn match_context_handles_emoji_before_match() {
        let transcript = format!("{}xhello", "😀".repeat(30));
        let expected = format!("...{}xhello", "😀".repeat(25));
        assert_eq!(ctx(&transcript, "hello"), expected);
    }

    #[test]
    fn match_context_handles_cjk() {
        let transcript = format!("{}hello{}", "中".repeat(40), "文".repeat(40));
        let expected = format!("...{}hello{}...", "中".repeat(34), "文".repeat(34));
        assert_eq!(ctx(&transcript, "hello"), expected);
    }

    #[test]
    fn match_context_empty_query() {
        let transcript = "æ".repeat(60);
        let expected = format!("{}...", "æ".repeat(50));
        assert_eq!(ctx(&transcript, ""), expected);
    }

    #[test]
    fn match_context_not_found_falls_back() {
        assert_eq!(ctx(&"æ".repeat(250), "zz"), "æ".repeat(200));
    }

    #[test]
    fn match_context_match_at_start() {
        assert_eq!(ctx("Hello world", "hello"), "Hello world");
    }

    #[test]
    fn match_context_match_at_end() {
        let transcript = format!("{}Hello", "a".repeat(150));
        let expected = format!("...{}Hello", "a".repeat(100));
        assert_eq!(ctx(&transcript, "hello"), expected);
    }

    #[test]
    fn match_context_length_changing_lowercase() {
        // 'İ' is 2 bytes but lowercases to 3, so lowered offsets do not fit the original.
        let transcript = format!("{}hello", "İ".repeat(300));
        let expected = format!("...{}hello", "İ".repeat(50));
        assert_eq!(ctx(&transcript, "hello"), expected);
    }

    #[test]
    fn match_context_ascii_unchanged() {
        let transcript = format!("{}Hello{}", "a".repeat(150), "b".repeat(150));
        let expected = format!("...{}Hello{}...", "a".repeat(100), "b".repeat(100));
        assert_eq!(ctx(&transcript, "hello"), expected);
    }
}
