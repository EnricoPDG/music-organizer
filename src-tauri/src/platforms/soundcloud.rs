//! SoundCloud adapter (research R6). Public sets and tracks use yt-dlp; signed-in access uses
//! the official API (OAuth 2.1 + PKCE with the user's own app, which needs Artist Pro).

use super::common::{secs_to_ms, split_artist_title};
use super::{
    AdapterCtx, AdapterError, Capabilities, ClientCredentials, CollectionSummary, OAuthConfig,
    PlatformAdapter, host_of,
};
use crate::core::model::{
    FetchMode, NormalizedCollection, NormalizedTrack, ResourceKind, ResourceRef, UnavailableItem,
};
use crate::tools::Tool;
use async_trait::async_trait;
use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;
use url::Url;

const ID: &str = "soundcloud";
const DEFAULT_API: &str = "https://api.soundcloud.com";
const LIKES_ID: &str = "you/likes";

/// First path segments that are SoundCloud pages, not user names.
const RESERVED_ROOTS: &[&str] = &[
    "discover",
    "stream",
    "search",
    "upload",
    "charts",
    "you",
    "settings",
    "messages",
    "notifications",
    "pages",
    "terms-of-use",
    "jobs",
    "imprint",
    "mobile",
    "pro",
    "artists",
    "people",
    "tags",
    "feed",
];
/// Second path segments under a user that are not tracks.
const RESERVED_USER_PAGES: &[&str] = &[
    "sets",
    "tracks",
    "albums",
    "reposts",
    "popular-tracks",
    "followers",
    "following",
    "comments",
    "likes",
];

static PREVIEW_SUFFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\s*[(\[]preview[)\]]\s*$").expect("valid regex"));

pub struct SoundcloudAdapter {
    api_base: String,
}

impl Default for SoundcloudAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl SoundcloudAdapter {
    pub fn new() -> Self {
        Self::with_api_base(DEFAULT_API)
    }

    pub fn with_api_base(base: &str) -> Self {
        Self {
            api_base: base.trim_end_matches('/').into(),
        }
    }

    async fn api_get(&self, ctx: &AdapterCtx, url: &str) -> Result<Value, AdapterError> {
        let token = ctx
            .token
            .as_ref()
            .ok_or(AdapterError::Private { can_sign_in: true })?;
        if !url.starts_with(&self.api_base) {
            return Err(AdapterError::Platform("unexpected paging URL".into()));
        }
        // SoundCloud uses the "OAuth" authorization scheme.
        let auth = format!("OAuth {}", token.expose());
        let resp = ctx
            .http
            .send(ID, |c| {
                c.get(url).header(reqwest::header::AUTHORIZATION, &auth)
            })
            .await?;
        resp.json()
            .await
            .map_err(|e| AdapterError::Platform(e.to_string()))
    }

    async fn api_pages(
        &self,
        ctx: &AdapterCtx,
        first_url: String,
    ) -> Result<Vec<Value>, AdapterError> {
        let mut out = Vec::new();
        let mut url = Some(first_url);
        while let Some(u) = url {
            let page = self.api_get(ctx, &u).await?;
            out.extend(page["collection"].as_array().cloned().unwrap_or_default());
            url = page["next_href"].as_str().map(str::to_string);
        }
        Ok(out)
    }
}

fn track_from_ytdlp(v: &Value) -> Result<NormalizedTrack, UnavailableItem> {
    if v.is_null() {
        return Err(UnavailableItem {
            title: None,
            reason: "Unavailable on SoundCloud".into(),
        });
    }
    let raw_title = v["title"].as_str().unwrap_or_default();
    let preview = v["availability"].as_str() == Some("premium_only");
    let raw_title = if preview {
        PREVIEW_SUFFIX.replace(raw_title, "").into_owned()
    } else {
        raw_title.to_string()
    };
    let (artists, title) = match v["artist"].as_str() {
        Some(a) if !a.is_empty() => (vec![a.to_string()], raw_title.clone()),
        _ => split_artist_title(&raw_title, v["uploader"].as_str()),
    };
    let id = v["id"]
        .as_str()
        .map(str::to_string)
        .or_else(|| v["id"].as_u64().map(|n| n.to_string()));
    let Some(id) = id else {
        return Err(UnavailableItem {
            title: Some(raw_title),
            reason: "Unavailable on SoundCloud".into(),
        });
    };
    Ok(NormalizedTrack {
        platform_track_id: id,
        url: v["webpage_url"].as_str().and_then(|u| Url::parse(u).ok()),
        title,
        artists,
        album: v["album"].as_str().map(str::to_string),
        track_number: None,
        // Go+ previews report ~30 s, which would mislead matching: treat as unknown.
        duration_ms: if preview {
            None
        } else {
            secs_to_ms(v["duration"].as_f64())
        },
        isrc: None,
        artwork_url: v["thumbnail"].as_str().and_then(|u| Url::parse(u).ok()),
        youtube_video_id: None,
    }
    .normalized())
}

fn track_from_api(v: &Value) -> Result<NormalizedTrack, UnavailableItem> {
    let raw_title = v["title"].as_str().unwrap_or_default();
    let Some(id) = v["id"].as_u64() else {
        return Err(UnavailableItem {
            title: Some(raw_title.into()),
            reason: "Unavailable on SoundCloud".into(),
        });
    };
    let meta = &v["publisher_metadata"];
    let (artists, title) = match meta["artist"].as_str() {
        Some(a) if !a.is_empty() => (vec![a.to_string()], raw_title.to_string()),
        _ => split_artist_title(raw_title, v["user"]["username"].as_str()),
    };
    Ok(NormalizedTrack {
        platform_track_id: id.to_string(),
        url: v["permalink_url"].as_str().and_then(|u| Url::parse(u).ok()),
        title,
        artists,
        album: meta["album_title"].as_str().map(str::to_string),
        track_number: None,
        duration_ms: v["duration"].as_u64(),
        isrc: meta["isrc"].as_str().map(str::to_string),
        // API artwork defaults to 100×100 ("-large"); ask for 500×500.
        artwork_url: v["artwork_url"]
            .as_str()
            .and_then(|u| Url::parse(&u.replace("-large.", "-t500x500.")).ok()),
        youtube_video_id: None,
    }
    .normalized())
}

fn page_url(r: &ResourceRef) -> String {
    if host_of(&r.url) == "on.soundcloud.com" {
        r.url.to_string()
    } else {
        format!("https://soundcloud.com/{}", r.id)
    }
}

#[async_trait]
impl PlatformAdapter for SoundcloudAdapter {
    fn id(&self) -> &'static str {
        ID
    }

    fn display_name(&self) -> &'static str {
        "SoundCloud"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            public_links: true,
            sign_in: true,
            provides_audio: false,
        }
    }

    fn parse_url(&self, url: &Url) -> Option<ResourceRef> {
        let host = host_of(url);
        let segs: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
        let make = |kind, id: String| {
            Some(ResourceRef {
                platform: ID,
                kind,
                id,
                url: url.clone(),
            })
        };
        if host == "on.soundcloud.com" {
            // Short links can point at a track or a set; yt-dlp resolves them.
            return match segs.as_slice() {
                [code] => make(ResourceKind::Track, (*code).to_string()),
                _ => None,
            };
        }
        if host != "soundcloud.com" {
            return None;
        }
        match segs.as_slice() {
            ["you", "likes"] => make(ResourceKind::UserLikes, LIKES_ID.into()),
            [user, "sets", set] if !RESERVED_ROOTS.contains(user) => {
                make(ResourceKind::Playlist, format!("{user}/sets/{set}"))
            }
            [user, "likes"] if !RESERVED_ROOTS.contains(user) => {
                make(ResourceKind::Playlist, format!("{user}/likes"))
            }
            [user, track]
                if !RESERVED_ROOTS.contains(user) && !RESERVED_USER_PAGES.contains(track) =>
            {
                make(ResourceKind::Track, format!("{user}/{track}"))
            }
            _ => None,
        }
    }

    async fn fetch_collection(
        &self,
        r: &ResourceRef,
        ctx: &AdapterCtx,
    ) -> Result<NormalizedCollection, AdapterError> {
        if r.kind == ResourceKind::UserLikes {
            let items = self
                .api_pages(
                    ctx,
                    format!(
                        "{}/me/likes/tracks?linked_partitioning=true&limit=200",
                        self.api_base
                    ),
                )
                .await?;
            let (mut tracks, mut unavailable) = (Vec::new(), Vec::new());
            for v in &items {
                match track_from_api(v) {
                    Ok(t) => tracks.push(t),
                    Err(u) => unavailable.push(u),
                }
            }
            return Ok(NormalizedCollection {
                platform_playlist_id: LIKES_ID.into(),
                kind: ResourceKind::UserLikes,
                name: "Liked tracks".into(),
                owner_name: None,
                artwork_url: None,
                fetch_mode: FetchMode::Official,
                tracks,
                unavailable,
            });
        }

        let args = vec!["-J".to_string(), page_url(r)];
        let v = ctx.tools.run_json(Tool::YtDlp, &args, &ctx.cancel).await?;
        let (entries, is_list) = match v["entries"].as_array() {
            Some(e) => (e.clone(), true),
            None => (vec![v.clone()], false),
        };
        let (mut tracks, mut unavailable) = (Vec::new(), Vec::new());
        for e in &entries {
            match track_from_ytdlp(e) {
                Ok(t) => tracks.push(t),
                Err(u) => unavailable.push(u),
            }
        }
        let name = if is_list {
            v["title"].as_str().unwrap_or("SoundCloud set").into()
        } else {
            tracks
                .first()
                .map(|t| t.title.clone())
                .unwrap_or_else(|| "SoundCloud track".into())
        };
        Ok(NormalizedCollection {
            platform_playlist_id: v["id"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| r.id.clone()),
            kind: if is_list {
                ResourceKind::Playlist
            } else {
                ResourceKind::Track
            },
            name,
            owner_name: v["uploader"].as_str().map(str::to_string),
            artwork_url: tracks.first().and_then(|t| t.artwork_url.clone()),
            fetch_mode: FetchMode::Official,
            tracks,
            unavailable,
        })
    }

    async fn fetch_track(
        &self,
        r: &ResourceRef,
        ctx: &AdapterCtx,
    ) -> Result<NormalizedTrack, AdapterError> {
        let mut c = self.fetch_collection(r, ctx).await?;
        if c.tracks.len() == 1 {
            Ok(c.tracks.remove(0))
        } else {
            Err(AdapterError::NotFound)
        }
    }

    async fn list_my_collections(
        &self,
        ctx: &AdapterCtx,
    ) -> Result<Vec<CollectionSummary>, AdapterError> {
        let mut out = vec![CollectionSummary {
            platform: ID.into(),
            kind: ResourceKind::UserLikes,
            id: LIKES_ID.into(),
            url: "https://soundcloud.com/you/likes".into(),
            name: "Liked tracks".into(),
            track_count: None,
            artwork_url: None,
        }];
        for p in self
            .api_pages(
                ctx,
                format!(
                    "{}/me/playlists?linked_partitioning=true&limit=50",
                    self.api_base
                ),
            )
            .await?
        {
            let (Some(url), Some(name)) = (p["permalink_url"].as_str(), p["title"].as_str()) else {
                continue;
            };
            out.push(CollectionSummary {
                platform: ID.into(),
                kind: ResourceKind::Playlist,
                id: p["id"].as_u64().map(|n| n.to_string()).unwrap_or_default(),
                url: url.into(),
                name: name.into(),
                track_count: p["track_count"].as_u64().map(|n| n as u32),
                artwork_url: p["artwork_url"].as_str().map(str::to_string),
            });
        }
        Ok(out)
    }

    fn auth_config(&self, _client: &ClientCredentials) -> Option<OAuthConfig> {
        Some(OAuthConfig {
            authorize_url: Url::parse("https://secure.soundcloud.com/authorize").expect("valid"),
            token_url: Url::parse("https://secure.soundcloud.com/oauth/token").expect("valid"),
            scopes: vec![],
            extra_params: vec![],
        })
    }
}
