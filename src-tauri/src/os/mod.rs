//! OS-specific behavior lives here (constitution: OS-specific code isolated).

use crate::error::{AppError, AppResult};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_opener::OpenerExt;

/// Per-user directories for app data, cache and logs.
#[derive(Debug, Clone)]
pub struct AppDirs {
    pub data: PathBuf,
    pub cache: PathBuf,
    pub logs: PathBuf,
}

impl AppDirs {
    pub fn resolve<R: Runtime>(app: &AppHandle<R>) -> AppResult<Self> {
        let p = app.path();
        let dirs = Self {
            data: p.app_data_dir().map_err(AppError::internal)?,
            cache: p.app_cache_dir().map_err(AppError::internal)?,
            logs: p.app_log_dir().map_err(AppError::internal)?,
        };
        for d in [&dirs.data, &dirs.cache, &dirs.logs] {
            std::fs::create_dir_all(d)?;
        }
        Ok(dirs)
    }

    /// Directories under one root, for tests.
    pub fn under(root: &Path) -> AppResult<Self> {
        let dirs = Self {
            data: root.join("data"),
            cache: root.join("cache"),
            logs: root.join("logs"),
        };
        for d in [&dirs.data, &dirs.cache, &dirs.logs] {
            std::fs::create_dir_all(d)?;
        }
        Ok(dirs)
    }
}

/// Show the file in Finder / Explorer / the Linux file manager.
pub fn reveal_in_folder<R: Runtime>(app: &AppHandle<R>, path: &Path) -> AppResult<()> {
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(AppError::internal)
}

/// Open the file with the system's default application (e.g. the default music player).
pub fn open_with_default<R: Runtime>(app: &AppHandle<R>, path: &Path) -> AppResult<()> {
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(AppError::internal)
}

/// Check that a folder exists and can be listed ("must exist and be readable when added").
pub fn ensure_readable_dir(path: &Path) -> AppResult<()> {
    if !path.is_dir() {
        return Err(AppError::invalid_input("That folder doesn't exist."));
    }
    std::fs::read_dir(path).map_err(|_| {
        AppError::invalid_input("That folder can't be read. Check its permissions.")
    })?;
    Ok(())
}
