//! Shared application state managed by Tauri.

use crate::core::model::NormalizedCollection;
use crate::core::queue::Queue;
use crate::db::Db;
use crate::net::Net;
use crate::os::AppDirs;
use crate::platforms::Registry;
use crate::secrets::SecretStore;
use crate::tools::ToolRunner;
use crate::tools::manager::ToolManager;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// A collection fetched for the preview card, reused by `start_import` so the platform isn't
/// asked twice.
pub struct CachedPreview {
    pub platform: &'static str,
    pub collection: NormalizedCollection,
    pub fetched_at: Instant,
}

pub struct AppState {
    pub db: Db,
    pub net: Arc<Net>,
    pub tools: Arc<dyn ToolRunner>,
    pub tool_manager: Arc<ToolManager>,
    pub registry: Arc<Registry>,
    pub secrets: Arc<dyn SecretStore>,
    pub queue: Queue,
    pub dirs: AppDirs,
    pub previews: Mutex<HashMap<String, CachedPreview>>,
}
