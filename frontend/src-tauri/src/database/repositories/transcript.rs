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
        // Lowercasing can change byte lengths, so map each lowercased byte back
        // to the start of the original char it came from.
        let mut transcript_lower = String::with_capacity(transcript.len());
        let mut lower_to_orig = Vec::with_capacity(transcript.len() + 1);
        for (i, c) in transcript.char_indices() {
            let before = transcript_lower.len();
            transcript_lower.extend(c.to_lowercase());
            lower_to_orig.resize(lower_to_orig.len() + transcript_lower.len() - before, i);
        }
        lower_to_orig.push(transcript.len());
        let query_lower = query.to_lowercase();

        match transcript_lower.find(&query_lower) {
            Some(lower_index) => {
                let match_index = lower_to_orig[lower_index];
                let match_end = lower_to_orig[lower_index + query_lower.len()];
                let mut start_index = match_index.saturating_sub(100);
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

    #[test]
    fn match_context_handles_multibyte_text() {
        let transcript = format!("{}xhello world", "æ".repeat(60));
        let context = TranscriptsRepository::get_match_context(&transcript, "HELLO");
        assert!(context.starts_with("..."));
        assert!(context.contains("hello"));
    }

    #[test]
    fn match_context_ascii_unchanged() {
        let transcript = format!("{}Hello{}", "a".repeat(150), "b".repeat(150));
        let expected = format!("...{}Hello{}...", "a".repeat(100), "b".repeat(100));
        assert_eq!(
            TranscriptsRepository::get_match_context(&transcript, "hello"),
            expected
        );
    }
}
