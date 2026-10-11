//! Import commands (contracts/ipc.md, Import): preview a link, import it, list the signed-in
//! user's collections.

use crate::commands::accounts::{not_connected, token_for};
use crate::core::importer::persist_collection;
use crate::core::model::{FetchMode, ResourceKind};
use crate::error::{AppError, AppResult, ErrorKind};
use crate::platforms::{AdapterCtx, CollectionSummary, PlatformAdapter};
use crate::state::{AppState, CachedPreview};
use crate::tools::Tool;
use serde::Serialize;
use specta::Type;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, State};
use tokio_util::sync::CancellationToken;

/// Previews older than this are fetched again on import.
const PREVIEW_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PreviewTrack {
    pub title: String,
    pub artists: Vec<String>,
    pub duration_ms: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LinkPreview {
    pub url: String,
    pub platform: String,
    pub platform_name: String,
    pub kind: ResourceKind,
    pub name: String,
    pub owner: Option<String>,
    pub artwork_url: Option<String>,
    pub track_count: u32,
    pub unavailable_count: u32,
    /// `public_fallback` = best-effort public Spotify page (FR-004b).
    pub fetch_mode: FetchMode,
    pub signed_in: bool,
    /// First tracks, for the preview card.
    pub sample: Vec<PreviewTrack>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportStarted {
    pub batch_id: String,
    pub playlist_id: Option<String>,
    pub created: u32,
    pub existing: u32,
    pub queued: u32,
    pub skipped: u32,
}

async fn ctx_for(
    state: &AppState,
    app: &AppHandle,
    adapter: &Arc<dyn PlatformAdapter>,
) -> AppResult<AdapterCtx> {
    let token = if adapter.capabilities().sign_in {
        token_for(state, Some(app), adapter.id()).await?
    } else {
        None
    };
    Ok(AdapterCtx {
        http: state.net.clone(),
        tools: state.tools.clone(),
        token,
        cancel: CancellationToken::new(),
    })
}

fn require_tools(state: &AppState) -> AppResult<()> {
    if state.tools.available(Tool::YtDlp) {
        Ok(())
    } else {
        Err(AppError::new(
            ErrorKind::ToolMissing,
            "The download tools aren't installed yet. Open Settings → Tools to install them.",
        ))
    }
}

async fn fetch(
    state: &AppState,
    app: &AppHandle,
    url: &str,
) -> AppResult<(
    Arc<dyn PlatformAdapter>,
    bool,
    crate::core::model::NormalizedCollection,
)> {
    let (adapter, r) = state.registry.resolve(url)?;
    let ctx = ctx_for(state, app, &adapter).await?;
    if r.kind == ResourceKind::UserLikes && ctx.token.is_none() {
        return Err(not_connected(adapter.display_name()));
    }
    let signed_in = ctx.token.is_some();
    let col = adapter.fetch_collection(&r, &ctx).await?;
    Ok((adapter, signed_in, col))
}

#[tauri::command]
#[specta::specta]
pub async fn resolve_link(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
) -> AppResult<LinkPreview> {
    require_tools(&state)?;
    let (adapter, signed_in, col) = fetch(&state, &app, &url).await?;
    let preview = LinkPreview {
        url: url.clone(),
        platform: adapter.id().into(),
        platform_name: adapter.display_name().into(),
        kind: col.kind,
        name: col.name.clone(),
        owner: col.owner_name.clone(),
        artwork_url: col.artwork_url.as_ref().map(|u| u.to_string()),
        track_count: col.tracks.len() as u32,
        unavailable_count: col.unavailable.len() as u32,
        fetch_mode: col.fetch_mode,
        signed_in,
        sample: col
            .tracks
            .iter()
            .take(8)
            .map(|t| PreviewTrack {
                title: t.title.clone(),
                artists: t.artists.clone(),
                duration_ms: t.duration_ms.map(|d| d as u32),
            })
            .collect(),
    };
    state.previews.lock().expect("preview lock").insert(
        url,
        CachedPreview {
            platform: adapter.id(),
            collection: col,
            fetched_at: Instant::now(),
        },
    );
    Ok(preview)
}

#[tauri::command]
#[specta::specta]
pub async fn start_import(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
    auto_download: bool,
) -> AppResult<ImportStarted> {
    require_tools(&state)?;
    let cached = state
        .previews
        .lock()
        .expect("preview lock")
        .remove(&url)
        .filter(|p| p.fetched_at.elapsed() < PREVIEW_TTL);
    let (platform, col) = match cached {
        Some(p) => (p.platform, p.collection),
        None => {
            let (adapter, _, col) = fetch(&state, &app, &url).await?;
            (adapter.id(), col)
        }
    };
    let provides_audio = state
        .registry
        .get(platform)
        .is_some_and(|a| a.capabilities().provides_audio);
    let source = url.clone();
    let out = state
        .db
        .call(move |c| persist_collection(c, platform, provides_audio, &col, &source))
        .await?;
    if !auto_download {
        state.queue.pause(None, Some(out.batch_id.clone())).await?;
    }
    state.queue.poke();
    tracing::info!(
        platform,
        created = out.created,
        queued = out.queued,
        "import started"
    );
    Ok(ImportStarted {
        batch_id: out.batch_id,
        playlist_id: out.playlist_id,
        created: out.created as u32,
        existing: out.existing as u32,
        queued: out.queued as u32,
        skipped: out.skipped as u32,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn list_my_collections(
    app: AppHandle,
    state: State<'_, AppState>,
    platform: String,
) -> AppResult<Vec<CollectionSummary>> {
    let adapter = state
        .registry
        .get(&platform)
        .cloned()
        .ok_or_else(|| AppError::invalid_input("Unknown platform."))?;
    let ctx = ctx_for(&state, &app, &adapter).await?;
    if ctx.token.is_none() {
        return Err(not_connected(adapter.display_name()));
    }
    Ok(adapter.list_my_collections(&ctx).await?)
}
