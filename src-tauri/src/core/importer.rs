//! Turning a fetched collection into library rows (T053): playlist, tracks (deduplicated),
//! source references, direct matches for YouTube, and download jobs. Runs in one transaction so
//! a failed import leaves nothing half-written.

use crate::core::model::{
    NormalizedCollection, NormalizedTrack, ResourceKind, dedup_key, new_id, same_recording_duration,
};
use crate::db::now;
use crate::error::{AppError, AppResult};
use rusqlite::{Connection, OptionalExtension, Transaction, params};

#[derive(Debug, Clone, PartialEq)]
pub struct ImportOutcome {
    pub batch_id: String,
    pub playlist_id: Option<String>,
    /// Tracks new to the library.
    pub created: usize,
    /// Tracks already in the library (same source or same song).
    pub existing: usize,
    pub queued: usize,
    pub skipped: usize,
}

/// Persist `col` from `platform`. `provides_audio` is true when the platform is the audio source
/// (YouTube), so tracks get a direct match and skip the matching step.
pub fn persist_collection(
    conn: &mut Connection,
    platform: &str,
    provides_audio: bool,
    col: &NormalizedCollection,
    source_url: &str,
) -> AppResult<ImportOutcome> {
    let tx = conn.transaction()?;
    let ts = now();

    let playlist_id = if col.kind == ResourceKind::Track {
        None
    } else {
        Some(upsert_playlist(&tx, platform, col, source_url, &ts)?)
    };

    let batch_id = new_id();
    tx.execute(
        "INSERT INTO import_batch (id, playlist_id, source_url, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![batch_id, playlist_id, source_url, ts],
    )?;

    let mut out = ImportOutcome {
        batch_id: batch_id.clone(),
        playlist_id: playlist_id.clone(),
        created: 0,
        existing: 0,
        queued: 0,
        skipped: 0,
    };
    let mut track_ids = Vec::with_capacity(col.tracks.len());
    for t in &col.tracks {
        let (track_id, created) = find_or_create_track(&tx, platform, t, &ts)?;
        if created {
            out.created += 1
        } else {
            out.existing += 1
        }
        if provides_audio {
            if let Some(video) = &t.youtube_video_id {
                insert_direct_match(&tx, &track_id, video, &ts)?;
            }
        }
        // The same track twice in one collection gets one job.
        if !track_ids.contains(&track_id) {
            match queue_job(&tx, &batch_id, &track_id, &ts)? {
                JobOutcome::Queued => out.queued += 1,
                JobOutcome::Skipped => out.skipped += 1,
                JobOutcome::AlreadyActive => {}
            }
        }
        track_ids.push(track_id);
    }

    if let Some(pid) = &playlist_id {
        replace_entries(&tx, pid, &track_ids)?;
    }
    tx.commit()?;
    Ok(out)
}

fn upsert_playlist(
    tx: &Transaction,
    platform: &str,
    col: &NormalizedCollection,
    url: &str,
    ts: &str,
) -> AppResult<String> {
    let existing: Option<String> = tx
        .query_row(
            "SELECT id FROM playlist WHERE platform = ?1 AND platform_playlist_id = ?2",
            params![platform, col.platform_playlist_id],
            |r| r.get(0),
        )
        .optional()?;
    match existing {
        Some(id) => {
            tx.execute(
                "UPDATE playlist SET name = ?2, owner_name = ?3, url = ?4, fetch_mode = ?5, last_synced_at = ?6 WHERE id = ?1",
                params![id, col.name, col.owner_name, url, col.fetch_mode.as_str(), ts],
            )?;
            Ok(id)
        }
        None => {
            let id = new_id();
            tx.execute(
                "INSERT INTO playlist (id, kind, name, platform, platform_playlist_id, url, owner_name, fetch_mode, last_synced_at, created_at)
                 VALUES (?1, 'imported', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                params![id, col.name, platform, col.platform_playlist_id, url, col.owner_name, col.fetch_mode.as_str(), ts],
            )?;
            Ok(id)
        }
    }
}

/// Returns `(track_id, created)`. Looks up the source reference first, then the same song by
/// dedup key and duration (which also finds scanned files, FR-015b).
pub fn find_or_create_track(
    tx: &Transaction,
    platform: &str,
    t: &NormalizedTrack,
    ts: &str,
) -> AppResult<(String, bool)> {
    let by_source: Option<String> = tx
        .query_row(
            "SELECT track_id FROM source_ref WHERE platform = ?1 AND platform_track_id = ?2",
            params![platform, t.platform_track_id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = by_source {
        return Ok((id, false));
    }

    let primary = t.artists.first().map(String::as_str).unwrap_or_default();
    let key = dedup_key(primary, &t.title);
    let same_song = {
        let mut stmt = tx.prepare("SELECT id, duration_ms FROM track WHERE dedup_key = ?1")?;
        let rows = stmt.query_map([&key], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?))
        })?;
        let mut found = None;
        for row in rows {
            let (id, dur) = row?;
            if same_recording_duration(dur.map(|d| d as u64), t.duration_ms) {
                found = Some(id);
                break;
            }
        }
        found
    };

    let (track_id, created) = match same_song {
        Some(id) => (id, false),
        None => {
            let id = new_id();
            let artists = serde_json::to_string(&t.artists).map_err(AppError::internal)?;
            tx.execute(
                "INSERT INTO track (id, title, artists, album, album_artist, track_number, duration_ms, isrc, origin, status, dedup_key, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'downloaded', 'pending', ?9, ?10, ?10)",
                params![
                    id,
                    truncate_title(&t.title),
                    artists,
                    t.album,
                    t.artists.first(),
                    t.track_number.filter(|n| *n >= 1),
                    t.duration_ms.filter(|d| *d > 0).map(|d| d as i64),
                    t.isrc,
                    key,
                    ts
                ],
            )?;
            (id, true)
        }
    };

    let raw = serde_json::to_string(t).map_err(AppError::internal)?;
    tx.execute(
        "INSERT INTO source_ref (id, track_id, platform, platform_track_id, url, raw_metadata) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![new_id(), track_id, platform, t.platform_track_id, t.url.as_ref().map(|u| u.as_str()), raw],
    )?;
    Ok((track_id, created))
}

fn truncate_title(t: &str) -> String {
    t.chars().take(500).collect()
}

fn insert_direct_match(tx: &Transaction, track_id: &str, video: &str, ts: &str) -> AppResult<()> {
    // A user's own choice is never replaced (FR-007).
    tx.execute(
        "INSERT INTO track_match (track_id, youtube_video_id, confidence, confidence_level, method, signals, decided_at)
         VALUES (?1, ?2, 1.0, 'high', 'direct', '{}', ?3)
         ON CONFLICT(track_id) DO UPDATE SET youtube_video_id = excluded.youtube_video_id, decided_at = excluded.decided_at
         WHERE track_match.method NOT IN ('user_candidate', 'user_url')",
        params![track_id, video, ts],
    )?;
    Ok(())
}

enum JobOutcome {
    Queued,
    Skipped,
    AlreadyActive,
}

fn queue_job(tx: &Transaction, batch_id: &str, track_id: &str, ts: &str) -> AppResult<JobOutcome> {
    let active: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM download_job WHERE track_id = ?1 AND state NOT IN ('done', 'skipped', 'failed', 'cancelled'))",
        [track_id],
        |r| r.get(0),
    )?;
    if active {
        return Ok(JobOutcome::AlreadyActive);
    }
    let status: String =
        tx.query_row("SELECT status FROM track WHERE id = ?1", [track_id], |r| {
            r.get(0)
        })?;
    let (state, outcome) = if status == "available" {
        ("skipped", JobOutcome::Skipped)
    } else {
        ("queued", JobOutcome::Queued)
    };
    tx.execute(
        "INSERT INTO download_job (id, batch_id, track_id, state, progress, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![new_id(), batch_id, track_id, state, if state == "skipped" { 1.0 } else { 0.0 }, ts],
    )?;
    Ok(outcome)
}

/// Rewrite a playlist's entries in platform order.
fn replace_entries(tx: &Transaction, playlist_id: &str, track_ids: &[String]) -> AppResult<()> {
    tx.execute(
        "DELETE FROM playlist_entry WHERE playlist_id = ?1",
        [playlist_id],
    )?;
    let mut stmt = tx.prepare(
        "INSERT INTO playlist_entry (playlist_id, track_id, position) VALUES (?1, ?2, ?3)",
    )?;
    for (pos, id) in track_ids.iter().enumerate() {
        stmt.execute(params![playlist_id, id, pos as i64])?;
    }
    Ok(())
}
