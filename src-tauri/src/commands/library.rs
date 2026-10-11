//! Library commands. Watched folders are needed from first run (onboarding, T033); browsing and
//! search arrive with US3.

use crate::core::model::new_id;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use specta::Type;
use std::path::PathBuf;
use tauri::State;

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WatchedFolder {
    pub id: String,
    pub path: String,
    pub is_download_target: bool,
    pub last_scan_at: Option<String>,
}

pub fn list_folders(conn: &Connection) -> AppResult<Vec<WatchedFolder>> {
    let mut stmt = conn.prepare(
        "SELECT id, path, is_download_target, last_scan_at FROM watched_folder ORDER BY is_download_target DESC, path",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(WatchedFolder {
                id: r.get(0)?,
                path: r.get(1)?,
                is_download_target: r.get(2)?,
                last_scan_at: r.get(3)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

/// The folder downloads go into, if one has been chosen.
pub fn download_target(conn: &Connection) -> AppResult<Option<PathBuf>> {
    Ok(conn
        .query_row(
            "SELECT path FROM watched_folder WHERE is_download_target = 1",
            [],
            |r| r.get::<_, String>(0),
        )
        .optional()?
        .map(PathBuf::from))
}

pub fn add_folder(
    conn: &mut Connection,
    path: &str,
    is_download_target: bool,
) -> AppResult<WatchedFolder> {
    let canonical = std::fs::canonicalize(path)
        .map_err(|_| AppError::invalid_input("That folder doesn't exist."))?;
    crate::os::ensure_readable_dir(&canonical)?;
    let path = canonical.to_string_lossy().to_string();

    let tx = conn.transaction()?;
    let existing: Option<String> = tx
        .query_row(
            "SELECT id FROM watched_folder WHERE path = ?1",
            [&path],
            |r| r.get(0),
        )
        .optional()?;
    if is_download_target {
        tx.execute(
            "UPDATE watched_folder SET is_download_target = 0 WHERE is_download_target = 1",
            [],
        )?;
    }
    let id = match existing {
        Some(id) => {
            if is_download_target {
                tx.execute(
                    "UPDATE watched_folder SET is_download_target = 1 WHERE id = ?1",
                    [&id],
                )?;
            }
            id
        }
        None => {
            let id = new_id();
            tx.execute(
                "INSERT INTO watched_folder (id, path, is_download_target) VALUES (?1, ?2, ?3)",
                params![id, path, is_download_target],
            )?;
            id
        }
    };
    tx.commit()?;
    list_folders(conn)?
        .into_iter()
        .find(|f| f.id == id)
        .ok_or_else(|| AppError::internal("folder vanished after insert"))
}

#[tauri::command]
#[specta::specta]
pub async fn list_watched_folders(state: State<'_, AppState>) -> AppResult<Vec<WatchedFolder>> {
    state.db.call(|c| list_folders(c)).await
}

#[tauri::command]
#[specta::specta]
pub async fn add_watched_folder(
    state: State<'_, AppState>,
    path: String,
    is_download_target: bool,
) -> AppResult<WatchedFolder> {
    state
        .db
        .call(move |c| add_folder(c, &path, is_download_target))
        .await
}
