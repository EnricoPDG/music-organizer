//! Platform adapters (contracts/platform-adapter.md, constitution Principle I). The core works
//! only with [`PlatformAdapter`] and the normalized types; it never matches on a platform name.

use crate::core::model::{NormalizedCollection, NormalizedTrack, ResourceKind, ResourceRef};
use crate::error::{AppError, ErrorKind};
use crate::net::{Net, NetError};
use crate::secrets::Secret;
use crate::tools::{ToolError, ToolRunner};
use async_trait::async_trait;
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    /// Import by URL without sign-in.
    pub public_links: bool,
    /// Has an OAuth flow.
    pub sign_in: bool,
    /// True only for YouTube: no matching step needed.
    pub provides_audio: bool,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CollectionSummary {
    pub platform: String,
    pub kind: ResourceKind,
    pub id: String,
    pub url: String,
    pub name: String,
    pub track_count: Option<u32>,
    pub artwork_url: Option<String>,
}

/// The user's own developer app for a platform (research R6, "bring your own app").
#[derive(Debug, Clone)]
pub struct ClientCredentials {
    pub client_id: String,
    pub client_secret: Option<Secret<String>>,
}

#[derive(Debug, Clone)]
pub struct OAuthConfig {
    pub authorize_url: Url,
    pub token_url: Url,
    pub scopes: Vec<String>,
    /// Extra query parameters for the authorize URL (e.g. `access_type=offline`).
    pub extra_params: Vec<(String, String)>,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum AdapterError {
    #[error("invalid URL")]
    InvalidUrl,
    #[error("not found")]
    NotFound,
    #[error("private")]
    Private { can_sign_in: bool },
    #[error("sign-in expired")]
    AuthExpired,
    #[error("rate limited")]
    RateLimited { retry_after: Option<Duration> },
    #[error("public source changed")]
    PublicSourceChanged,
    #[error("network: {0}")]
    Network(String),
    #[error("platform: {0}")]
    Platform(String),
}

impl From<NetError> for AdapterError {
    fn from(e: NetError) -> Self {
        match e {
            NetError::RateLimited { retry_after } => AdapterError::RateLimited { retry_after },
            NetError::Network(m) => AdapterError::Network(m),
            NetError::Status { code: 401, .. } => AdapterError::AuthExpired,
            NetError::Status { code: 403, .. } => AdapterError::Private { can_sign_in: true },
            NetError::Status { code: 404, .. } => AdapterError::NotFound,
            NetError::Status { code, .. } => AdapterError::Platform(format!("HTTP {code}")),
        }
    }
}

impl From<ToolError> for AdapterError {
    fn from(e: ToolError) -> Self {
        match e {
            ToolError::Failed { stderr, .. } => classify_ytdlp_error(&stderr),
            ToolError::Missing(t) => {
                AdapterError::Platform(format!("{} is not installed", t.display_name()))
            }
            other => AdapterError::Platform(other.to_string()),
        }
    }
}

/// Map yt-dlp's error text to a contract error.
pub fn classify_ytdlp_error(stderr: &str) -> AdapterError {
    let s = stderr.to_ascii_lowercase();
    if s.contains("private")
        || s.contains("sign in to confirm your age")
        || s.contains("members-only")
    {
        AdapterError::Private { can_sign_in: false }
    } else if s.contains("does not exist")
        || s.contains("not found")
        || s.contains("404")
        || s.contains("unavailable")
    {
        AdapterError::NotFound
    } else if s.contains("429") || s.contains("too many requests") {
        AdapterError::RateLimited { retry_after: None }
    } else {
        AdapterError::Platform(
            stderr
                .lines()
                .last()
                .unwrap_or("yt-dlp failed")
                .chars()
                .take(300)
                .collect(),
        )
    }
}

impl From<AdapterError> for AppError {
    fn from(e: AdapterError) -> Self {
        match e {
            AdapterError::InvalidUrl => {
                AppError::new(ErrorKind::InvalidUrl, "That link doesn't look right.")
            }
            AdapterError::NotFound => {
                AppError::not_found("Nothing was found at that link. It may have been deleted.")
            }
            AdapterError::Private { can_sign_in: true } => AppError::new(
                ErrorKind::Private,
                "This playlist is private. Connect your account in Settings → Accounts to import it.",
            ),
            AdapterError::Private { can_sign_in: false } => AppError::new(
                ErrorKind::Private,
                "This item is private or restricted and can't be imported.",
            ),
            AdapterError::AuthExpired => AppError::new(
                ErrorKind::AuthExpired,
                "Your sign-in has expired. Reconnect the account in Settings → Accounts.",
            ),
            AdapterError::RateLimited { .. } => AppError::new(
                ErrorKind::RateLimited,
                "The platform is limiting requests. Try again in a little while.",
            ),
            AdapterError::PublicSourceChanged => AppError::new(
                ErrorKind::PublicSourceChanged,
                "Spotify changed its public playlist page, so this link can't be read right now. \
                 Sign in to Spotify and copy the playlist into your own account to import it.",
            ),
            AdapterError::Network(detail) => AppError::from(NetError::Network(detail)),
            AdapterError::Platform(detail) => {
                tracing::warn!(%detail, "platform error");
                AppError::new(
                    ErrorKind::Platform,
                    "The platform returned an unexpected response.",
                )
            }
        }
    }
}

/// Shared services handed to adapters, so they never reach into the core.
#[derive(Clone)]
pub struct AdapterCtx {
    pub http: Arc<Net>,
    pub tools: Arc<dyn ToolRunner>,
    /// Access token for the signed-in account, if any.
    pub token: Option<Secret<String>>,
    pub cancel: CancellationToken,
}

#[async_trait]
pub trait PlatformAdapter: Send + Sync {
    /// Stable lowercase id, e.g. "spotify". Used in `SourceRef.platform`.
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn capabilities(&self) -> Capabilities;

    /// Cheap, offline check: does this URL belong to this platform, and what does it point to?
    fn parse_url(&self, url: &Url) -> Option<ResourceRef>;

    /// Collection header and all tracks (the adapter handles paging).
    async fn fetch_collection(
        &self,
        r: &ResourceRef,
        ctx: &AdapterCtx,
    ) -> Result<NormalizedCollection, AdapterError>;

    async fn fetch_track(
        &self,
        r: &ResourceRef,
        ctx: &AdapterCtx,
    ) -> Result<NormalizedTrack, AdapterError>;

    /// The signed-in user's collections, including the "liked" pseudo-collection.
    async fn list_my_collections(
        &self,
        _ctx: &AdapterCtx,
    ) -> Result<Vec<CollectionSummary>, AdapterError> {
        Ok(Vec::new())
    }

    /// OAuth request for the user's own client app. `None` when sign-in isn't supported.
    fn auth_config(&self, _client: &ClientCredentials) -> Option<OAuthConfig> {
        None
    }
}

/// Every registered adapter. Adding a platform = a new module plus one line here.
pub struct Registry {
    adapters: Vec<Arc<dyn PlatformAdapter>>,
}

impl Registry {
    pub fn new(adapters: Vec<Arc<dyn PlatformAdapter>>) -> Self {
        Self { adapters }
    }

    pub fn all(&self) -> &[Arc<dyn PlatformAdapter>] {
        &self.adapters
    }

    pub fn get(&self, id: &str) -> Option<&Arc<dyn PlatformAdapter>> {
        self.adapters.iter().find(|a| a.id() == id)
    }

    /// Find the adapter that recognizes `url`.
    pub fn resolve(&self, url: &str) -> Result<(Arc<dyn PlatformAdapter>, ResourceRef), AppError> {
        let parsed = parse_user_url(url)?;
        self.adapters
            .iter()
            .find_map(|a| a.parse_url(&parsed).map(|r| (a.clone(), r)))
            .ok_or_else(|| {
                AppError::new(
                    ErrorKind::UnsupportedPlatform,
                    "This link isn't from a supported platform (Spotify, YouTube or SoundCloud).",
                )
            })
    }
}

/// Parse a pasted link, accepting links without a scheme (e.g. "open.spotify.com/...").
pub fn parse_user_url(input: &str) -> Result<Url, AppError> {
    let trimmed = input.trim();
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let url = Url::parse(&with_scheme).map_err(|_| AppError::from(AdapterError::InvalidUrl))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(AdapterError::InvalidUrl.into());
    }
    Ok(url)
}

/// Lower-cased host without a leading "www." or "m.".
pub fn host_of(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    host.strip_prefix("www.")
        .or_else(|| host.strip_prefix("m."))
        .map(str::to_string)
        .unwrap_or(host)
}
