//! Shared application state managed by Tauri.

use crate::db::Db;
use crate::net::Net;
use crate::os::AppDirs;
use crate::platforms::Registry;
use crate::tools::ToolRunner;
use std::sync::Arc;

pub struct AppState {
    pub db: Db,
    pub net: Arc<Net>,
    pub tools: Arc<dyn ToolRunner>,
    pub registry: Arc<Registry>,
    pub dirs: AppDirs,
}
