//! Download queue commands (contracts/ipc.md, Downloads).

use crate::error::AppResult;
use crate::state::AppState;
use rusqlite::params;
use serde::Serialize;
use specta::Type;
use tauri::State;

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct JobView {
    pub job_id: String,
    pub batch_id: String,
    pub track_id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub artwork_url: Option<String>,
    pub state: String,
    pub progress: f64,
    pub error_kind: Option<String>,
    pub error_message: Option<String>,
    pub confidence_level: Option<String>,
    pub file_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BatchView {
    pub batch_id: String,
    pub name: String,
    pub platform: Option<String>,
    pub created_at: String,
    pub finished_at: Option<String>,
    pub total: u32,
    pub done: u32,
    pub failed: u32,
    pub skipped: u32,
    pub active: u32,
}

#[tauri::command]
#[specta::specta]
pub async fn list_batches(state: State<'_, AppState>) -> AppResult<Vec<BatchView>> {
    state
        .db
        .call(|c| {
            let mut s = c.prepare(
                "SELECT b.id, coalesce(p.name, (SELECT t.title FROM download_job j JOIN track t ON t.id = j.track_id WHERE j.batch_id = b.id LIMIT 1), b.source_url),
                        p.platform, b.created_at, b.finished_at,
                        count(j.id), coalesce(sum(j.state = 'done'), 0), coalesce(sum(j.state = 'failed'), 0), coalesce(sum(j.state = 'skipped'), 0),
                        coalesce(sum(j.state NOT IN ('done', 'skipped', 'failed', 'cancelled', 'paused')), 0)
                 FROM import_batch b LEFT JOIN playlist p ON p.id = b.playlist_id LEFT JOIN download_job j ON j.batch_id = b.id
                 GROUP BY b.id ORDER BY b.created_at DESC, b.rowid DESC LIMIT 50",
            )?;
            let rows = s
                .query_map([], |r| {
                    Ok(BatchView {
                        batch_id: r.get(0)?,
                        name: r.get(1)?,
                        platform: r.get(2)?,
                        created_at: r.get(3)?,
                        finished_at: r.get(4)?,
                        total: r.get::<_, i64>(5)? as u32,
                        done: r.get::<_, i64>(6)? as u32,
                        failed: r.get::<_, i64>(7)? as u32,
                        skipped: r.get::<_, i64>(8)? as u32,
                        active: r.get::<_, i64>(9)? as u32,
                    })
                })?
                .collect::<Result<_, _>>()?;
            Ok(rows)
        })
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn list_jobs(
    state: State<'_, AppState>,
    batch_id: Option<String>,
) -> AppResult<Vec<JobView>> {
    state
        .db
        .call(move |c| {
            let mut s = c.prepare(
                "SELECT j.id, j.batch_id, j.track_id, t.title, t.artists, t.album,
                        (SELECT json_extract(sr.raw_metadata, '$.artwork_url') FROM source_ref sr WHERE sr.track_id = t.id
                          ORDER BY json_extract(sr.raw_metadata, '$.youtube_video_id') IS NOT NULL LIMIT 1),
                        j.state, j.progress, j.error_kind, j.error_message, m.confidence_level, t.file_path
                 FROM download_job j JOIN track t ON t.id = j.track_id LEFT JOIN track_match m ON m.track_id = t.id
                 WHERE (?1 IS NULL OR j.batch_id = ?1)
                 ORDER BY j.rowid",
            )?;
            let rows = s
                .query_map(params![batch_id], |r| {
                    Ok(JobView {
                        job_id: r.get(0)?,
                        batch_id: r.get(1)?,
                        track_id: r.get(2)?,
                        title: r.get(3)?,
                        artists: serde_json::from_str(&r.get::<_, String>(4)?).unwrap_or_default(),
                        album: r.get(5)?,
                        artwork_url: r.get(6)?,
                        state: r.get(7)?,
                        progress: r.get(8)?,
                        error_kind: r.get(9)?,
                        error_message: r.get(10)?,
                        confidence_level: r.get(11)?,
                        file_path: r.get(12)?,
                    })
                })?
                .collect::<Result<_, _>>()?;
            Ok(rows)
        })
        .await
}

#[tauri::command]
#[specta::specta]
pub async fn pause_jobs(
    state: State<'_, AppState>,
    job_ids: Option<Vec<String>>,
    batch_id: Option<String>,
) -> AppResult<u32> {
    Ok(state.queue.pause(job_ids, batch_id).await? as u32)
}

#[tauri::command]
#[specta::specta]
pub async fn resume_jobs(
    state: State<'_, AppState>,
    job_ids: Option<Vec<String>>,
    batch_id: Option<String>,
) -> AppResult<u32> {
    Ok(state.queue.resume(job_ids, batch_id).await? as u32)
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_jobs(
    state: State<'_, AppState>,
    job_ids: Option<Vec<String>>,
    batch_id: Option<String>,
) -> AppResult<u32> {
    Ok(state.queue.cancel(job_ids, batch_id).await? as u32)
}

#[tauri::command]
#[specta::specta]
pub async fn retry_jobs(
    state: State<'_, AppState>,
    job_ids: Option<Vec<String>>,
    batch_id: Option<String>,
) -> AppResult<u32> {
    Ok(state.queue.retry(job_ids, batch_id).await? as u32)
}
