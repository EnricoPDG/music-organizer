//! Settings (data-model.md, Settings) and the `get_settings` / `update_settings` commands.

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AudioFormat {
    M4a,
    Opus,
    Mp3,
}

impl AudioFormat {
    pub fn extension(self) -> &'static str {
        match self {
            AudioFormat::M4a => "m4a",
            AudioFormat::Opus => "opus",
            AudioFormat::Mp3 => "mp3",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolPaths {
    pub yt_dlp: Option<String>,
    pub deno: Option<String>,
    pub ffmpeg: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub audio_format: AudioFormat,
    pub naming_pattern: String,
    pub theme: Theme,
    /// `None` follows the OS setting.
    pub reduce_motion: Option<bool>,
    /// 1–5
    pub max_parallel_downloads: u8,
    pub review_low_confidence: bool,
    /// User overrides; `None` means the app-managed tool is used.
    pub tool_paths: ToolPaths,
}

pub const DEFAULT_NAMING_PATTERN: &str = "{artist} - {title}";
/// Default before 2.0.0 shipped; settings still holding it move to the new default on load.
const LEGACY_DEFAULT_NAMING_PATTERN: &str = "{album_artist}/{album}/{track:02} - {title}";

impl Default for Settings {
    fn default() -> Self {
        Self {
            audio_format: AudioFormat::M4a,
            naming_pattern: DEFAULT_NAMING_PATTERN.into(),
            theme: Theme::System,
            reduce_motion: None,
            max_parallel_downloads: 3,
            review_low_confidence: true,
            tool_paths: ToolPaths::default(),
        }
    }
}

/// Partial update; only the fields that are set change. `reduceMotion: null` keeps the current
/// value, so following the OS again is expressed with `followSystemMotion: true`.
#[derive(Debug, Clone, Default, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsPatch {
    #[specta(optional)]
    pub audio_format: Option<AudioFormat>,
    #[specta(optional)]
    pub naming_pattern: Option<String>,
    #[specta(optional)]
    pub theme: Option<Theme>,
    #[specta(optional)]
    pub reduce_motion: Option<bool>,
    #[specta(optional)]
    pub follow_system_motion: Option<bool>,
    #[specta(optional)]
    pub max_parallel_downloads: Option<u8>,
    #[specta(optional)]
    pub review_low_confidence: Option<bool>,
    #[specta(optional)]
    pub tool_paths: Option<ToolPaths>,
}

const KEY: &str = "app";

pub fn load(conn: &Connection) -> AppResult<Settings> {
    let raw: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [KEY], |r| {
            r.get(0)
        })
        .optional()?;
    // Unknown or outdated stored values fall back to defaults rather than failing startup.
    let mut s: Settings = raw
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    if s.naming_pattern == LEGACY_DEFAULT_NAMING_PATTERN {
        s.naming_pattern = DEFAULT_NAMING_PATTERN.into();
    }
    Ok(s)
}

pub fn apply_patch(mut s: Settings, p: SettingsPatch) -> AppResult<Settings> {
    if let Some(v) = p.audio_format {
        s.audio_format = v;
    }
    if let Some(v) = p.naming_pattern {
        if v.trim().is_empty() || !v.contains("{title}") {
            return Err(AppError::invalid_input(
                "The naming pattern must include {title}.",
            ));
        }
        s.naming_pattern = v.trim().to_string();
    }
    if let Some(v) = p.theme {
        s.theme = v;
    }
    if p.follow_system_motion == Some(true) {
        s.reduce_motion = None;
    } else if let Some(v) = p.reduce_motion {
        s.reduce_motion = Some(v);
    }
    if let Some(v) = p.max_parallel_downloads {
        if !(1..=5).contains(&v) {
            return Err(AppError::invalid_input(
                "Parallel downloads must be between 1 and 5.",
            ));
        }
        s.max_parallel_downloads = v;
    }
    if let Some(v) = p.review_low_confidence {
        s.review_low_confidence = v;
    }
    if let Some(v) = p.tool_paths {
        s.tool_paths = v;
    }
    Ok(s)
}

pub fn save(conn: &Connection, s: &Settings) -> AppResult<()> {
    let json = serde_json::to_string(s).map_err(AppError::internal)?;
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![KEY, json],
    )?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_settings(state: State<'_, AppState>) -> AppResult<Settings> {
    state.db.call(|c| load(c)).await
}

#[tauri::command]
#[specta::specta]
pub async fn update_settings(
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> AppResult<Settings> {
    state
        .db
        .call(move |c| {
            let next = apply_patch(load(c)?, patch)?;
            save(c, &next)?;
            Ok(next)
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::queue::{PatternVars, render_pattern};

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute(
            "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .unwrap();
        c
    }

    #[test]
    fn legacy_default_pattern_moves_to_artist_dash_title() {
        let c = conn();
        let old = Settings {
            naming_pattern: LEGACY_DEFAULT_NAMING_PATTERN.into(),
            ..Default::default()
        };
        save(&c, &old).unwrap();
        assert_eq!(load(&c).unwrap().naming_pattern, DEFAULT_NAMING_PATTERN);
    }

    #[test]
    fn custom_pattern_is_kept() {
        let c = conn();
        let s = Settings {
            naming_pattern: "{artist}/{title}".into(),
            ..Default::default()
        };
        save(&c, &s).unwrap();
        assert_eq!(load(&c).unwrap().naming_pattern, "{artist}/{title}");
    }

    #[test]
    fn default_pattern_is_a_single_file_name() {
        let vars = PatternVars {
            artist: "Daft Punk".into(),
            album_artist: "Daft Punk".into(),
            album: "Discovery".into(),
            title: "One More Time".into(),
            track: Some(1),
            year: Some(2001),
        };
        assert_eq!(
            render_pattern(DEFAULT_NAMING_PATTERN, &vars),
            "Daft Punk - One More Time"
        );
    }
}
