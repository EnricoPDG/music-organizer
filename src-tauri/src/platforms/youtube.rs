//! YouTube adapter. Public links use yt-dlp; signed-in access uses the YouTube Data API v3
//! (research R6). YouTube is the audio source, so tracks carry `youtube_video_id`.

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
use serde_json::Value;
use url::Url;

const ID: &str = "youtube";
const DEFAULT_API: &str = "https://www.googleapis.com/youtube/v3";
const LIKED: &str = "LL";

pub struct YoutubeAdapter {
    api_base: String,
}

impl Default for YoutubeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl YoutubeAdapter {
    pub fn new() -> Self {
        Self::with_api_base(DEFAULT_API)
    }

    /// Point the Data API at another base URL (tests).
    pub fn with_api_base(base: &str) -> Self {
        Self {
            api_base: base.trim_end_matches('/').to_string(),
        }
    }
}

fn is_video_id(s: &str) -> bool {
    s.len() == 11
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn is_playlist_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub fn watch_url(video_id: &str) -> String {
    format!("https://www.youtube.com/watch?v={video_id}")
}

fn playlist_url(id: &str) -> String {
    format!("https://www.youtube.com/playlist?list={id}")
}

fn best_thumbnail(v: &Value) -> Option<Url> {
    if let Some(u) = v["thumbnail"].as_str() {
        return Url::parse(u).ok();
    }
    v["thumbnails"]
        .as_array()?
        .iter()
        .max_by_key(|t| t["width"].as_u64().unwrap_or(0))
        .and_then(|t| t["url"].as_str())
        .and_then(|u| Url::parse(u).ok())
}

/// One yt-dlp entry (flat or full) → track, or why it can't be imported.
fn track_from_ytdlp(v: &Value) -> Result<NormalizedTrack, UnavailableItem> {
    let id = v["id"].as_str().unwrap_or_default();
    let raw_title = v["title"].as_str().unwrap_or_default();
    match raw_title {
        "[Private video]" => {
            return Err(UnavailableItem {
                title: None,
                reason: "Private video".into(),
            });
        }
        "[Deleted video]" => {
            return Err(UnavailableItem {
                title: None,
                reason: "Deleted video".into(),
            });
        }
        _ if !is_video_id(id) => {
            return Err(UnavailableItem {
                title: Some(raw_title.into()),
                reason: "Not a video".into(),
            });
        }
        _ => {}
    }
    let channel = v["channel"].as_str().or(v["uploader"].as_str());
    // YouTube Music metadata, when present, is better than parsing the video title.
    let (artists, title) = match (v["artists"].as_array(), v["track"].as_str()) {
        (Some(a), Some(track)) if !a.is_empty() => (
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect(),
            track.to_string(),
        ),
        _ => split_artist_title(raw_title, channel),
    };
    Ok(NormalizedTrack {
        platform_track_id: id.into(),
        url: Url::parse(&watch_url(id)).ok(),
        title,
        artists,
        album: v["album"].as_str().map(str::to_string),
        track_number: None,
        duration_ms: secs_to_ms(v["duration"].as_f64()),
        isrc: None,
        artwork_url: best_thumbnail(v),
        youtube_video_id: Some(id.into()),
    }
    .normalized())
}

fn track_from_api_item(v: &Value) -> Result<NormalizedTrack, UnavailableItem> {
    let s = &v["snippet"];
    let id = s["resourceId"]["videoId"].as_str().unwrap_or_default();
    let raw_title = s["title"].as_str().unwrap_or_default();
    if raw_title == "Private video" || raw_title == "Deleted video" || !is_video_id(id) {
        return Err(UnavailableItem {
            title: None,
            reason: raw_title.to_string(),
        });
    }
    let (artists, title) = split_artist_title(raw_title, s["videoOwnerChannelTitle"].as_str());
    Ok(NormalizedTrack {
        platform_track_id: id.into(),
        url: Url::parse(&watch_url(id)).ok(),
        title,
        artists,
        album: None,
        track_number: None,
        duration_ms: None,
        isrc: None,
        artwork_url: s["thumbnails"]["high"]["url"]
            .as_str()
            .and_then(|u| Url::parse(u).ok()),
        youtube_video_id: Some(id.into()),
    }
    .normalized())
}

impl YoutubeAdapter {
    async fn api_get(
        &self,
        ctx: &AdapterCtx,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<Value, AdapterError> {
        let token = ctx
            .token
            .as_ref()
            .ok_or(AdapterError::Private { can_sign_in: true })?;
        let url = format!("{}/{path}", self.api_base);
        let resp = ctx
            .http
            .send(ID, |c| c.get(&url).query(query).bearer_auth(token.expose()))
            .await?;
        resp.json()
            .await
            .map_err(|e| AdapterError::Platform(e.to_string()))
    }

    async fn api_playlist(
        &self,
        ctx: &AdapterCtx,
        id: &str,
    ) -> Result<NormalizedCollection, AdapterError> {
        let mut tracks = Vec::new();
        let mut unavailable = Vec::new();
        let mut page: Option<String> = None;
        loop {
            let mut q = vec![
                ("part", "snippet"),
                ("playlistId", id),
                ("maxResults", "50"),
            ];
            if let Some(p) = &page {
                q.push(("pageToken", p.as_str()));
            }
            let v = self.api_get(ctx, "playlistItems", &q).await?;
            for item in v["items"].as_array().into_iter().flatten() {
                match track_from_api_item(item) {
                    Ok(t) => tracks.push(t),
                    Err(u) => unavailable.push(u),
                }
            }
            match v["nextPageToken"].as_str() {
                Some(next) => page = Some(next.to_string()),
                None => break,
            }
        }
        let (name, owner_name) = if id == LIKED {
            ("Liked videos".to_string(), None)
        } else {
            // Best effort: the items call already succeeded, so a missing title isn't fatal.
            let meta = self
                .api_get(ctx, "playlists", &[("part", "snippet"), ("id", id)])
                .await
                .ok();
            let snippet = meta.as_ref().map(|m| &m["items"][0]["snippet"]);
            (
                snippet
                    .and_then(|s| s["title"].as_str())
                    .unwrap_or("YouTube playlist")
                    .to_string(),
                snippet
                    .and_then(|s| s["channelTitle"].as_str())
                    .map(str::to_string),
            )
        };
        Ok(NormalizedCollection {
            platform_playlist_id: id.into(),
            kind: if id == LIKED {
                ResourceKind::UserLikes
            } else {
                ResourceKind::Playlist
            },
            name,
            owner_name,
            artwork_url: tracks.first().and_then(|t| t.artwork_url.clone()),
            fetch_mode: FetchMode::Official,
            tracks,
            unavailable,
        })
    }
}

#[async_trait]
impl PlatformAdapter for YoutubeAdapter {
    fn id(&self) -> &'static str {
        ID
    }

    fn display_name(&self) -> &'static str {
        "YouTube"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            public_links: true,
            sign_in: true,
            provides_audio: true,
        }
    }

    fn parse_url(&self, url: &Url) -> Option<ResourceRef> {
        let host = host_of(url);
        let query = |k: &str| {
            url.query_pairs()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.into_owned())
        };
        let make = |kind, id: String| {
            Some(ResourceRef {
                platform: ID,
                kind,
                id,
                url: url.clone(),
            })
        };
        let mut segs = url
            .path_segments()
            .into_iter()
            .flatten()
            .filter(|s| !s.is_empty());

        if host == "youtu.be" {
            let id = segs.next()?;
            return is_video_id(id)
                .then(|| make(ResourceKind::Track, id.into()))
                .flatten();
        }
        if host != "youtube.com" && host != "music.youtube.com" {
            return None;
        }
        match segs.next()? {
            "playlist" => {
                let list = query("list").filter(|l| is_playlist_id(l))?;
                let kind = if list == LIKED {
                    ResourceKind::UserLikes
                } else {
                    ResourceKind::Playlist
                };
                make(kind, list)
            }
            "watch" => {
                let v = query("v").filter(|v| is_video_id(v));
                // Radio mixes ("RD…") are endless generated lists: import just the video.
                match query("list").filter(|l| is_playlist_id(l) && !l.starts_with("RD")) {
                    Some(list) => make(ResourceKind::Playlist, list),
                    None => make(ResourceKind::Track, v?),
                }
            }
            "shorts" | "live" => {
                let id = segs.next()?;
                is_video_id(id)
                    .then(|| make(ResourceKind::Track, id.into()))
                    .flatten()
            }
            _ => None,
        }
    }

    async fn fetch_collection(
        &self,
        r: &ResourceRef,
        ctx: &AdapterCtx,
    ) -> Result<NormalizedCollection, AdapterError> {
        match r.kind {
            ResourceKind::Track => {
                let t = self.fetch_track(r, ctx).await?;
                Ok(NormalizedCollection {
                    platform_playlist_id: t.platform_track_id.clone(),
                    kind: ResourceKind::Track,
                    name: t.title.clone(),
                    owner_name: t.artists.first().cloned(),
                    artwork_url: t.artwork_url.clone(),
                    fetch_mode: FetchMode::Official,
                    tracks: vec![t],
                    unavailable: vec![],
                })
            }
            // Signed in: the Data API also covers private playlists and Liked videos.
            ResourceKind::UserLikes => self.api_playlist(ctx, &r.id).await,
            _ if ctx.token.is_some() => self.api_playlist(ctx, &r.id).await,
            _ => {
                let args = vec![
                    "-J".to_string(),
                    "--flat-playlist".to_string(),
                    playlist_url(&r.id),
                ];
                let v = ctx
                    .tools
                    .run_json(Tool::YtDlp, &args, &ctx.cancel)
                    .await
                    .map_err(|e| match AdapterError::from(e) {
                        AdapterError::Private { .. } => AdapterError::Private { can_sign_in: true },
                        other => other,
                    })?;
                let mut tracks = Vec::new();
                let mut unavailable = Vec::new();
                for e in v["entries"].as_array().into_iter().flatten() {
                    match track_from_ytdlp(e) {
                        Ok(t) => tracks.push(t),
                        Err(u) => unavailable.push(u),
                    }
                }
                Ok(NormalizedCollection {
                    platform_playlist_id: r.id.clone(),
                    kind: ResourceKind::Playlist,
                    name: v["title"]
                        .as_str()
                        .unwrap_or("YouTube playlist")
                        .to_string(),
                    owner_name: v["channel"]
                        .as_str()
                        .or(v["uploader"].as_str())
                        .map(str::to_string),
                    artwork_url: best_thumbnail(&v),
                    fetch_mode: FetchMode::Official,
                    tracks,
                    unavailable,
                })
            }
        }
    }

    async fn fetch_track(
        &self,
        r: &ResourceRef,
        ctx: &AdapterCtx,
    ) -> Result<NormalizedTrack, AdapterError> {
        let args = vec![
            "-J".into(),
            "--no-playlist".into(),
            "--skip-download".into(),
            watch_url(&r.id),
        ];
        let v = ctx.tools.run_json(Tool::YtDlp, &args, &ctx.cancel).await?;
        track_from_ytdlp(&v).map_err(|u| AdapterError::Platform(u.reason))
    }

    async fn list_my_collections(
        &self,
        ctx: &AdapterCtx,
    ) -> Result<Vec<CollectionSummary>, AdapterError> {
        let v = self
            .api_get(
                ctx,
                "playlists",
                &[
                    ("part", "snippet,contentDetails"),
                    ("mine", "true"),
                    ("maxResults", "50"),
                ],
            )
            .await?;
        let mut out = vec![CollectionSummary {
            platform: ID.into(),
            kind: ResourceKind::UserLikes,
            id: LIKED.into(),
            url: playlist_url(LIKED),
            name: "Liked videos".into(),
            track_count: None,
            artwork_url: None,
        }];
        for p in v["items"].as_array().into_iter().flatten() {
            let Some(id) = p["id"].as_str() else { continue };
            out.push(CollectionSummary {
                platform: ID.into(),
                kind: ResourceKind::Playlist,
                id: id.into(),
                url: playlist_url(id),
                name: p["snippet"]["title"].as_str().unwrap_or("Untitled").into(),
                track_count: p["contentDetails"]["itemCount"].as_u64().map(|n| n as u32),
                artwork_url: p["snippet"]["thumbnails"]["high"]["url"]
                    .as_str()
                    .map(str::to_string),
            });
        }
        Ok(out)
    }

    fn auth_config(&self, _client: &ClientCredentials) -> Option<OAuthConfig> {
        Some(OAuthConfig {
            authorize_url: Url::parse("https://accounts.google.com/o/oauth2/v2/auth")
                .expect("valid"),
            token_url: Url::parse("https://oauth2.googleapis.com/token").expect("valid"),
            scopes: vec!["https://www.googleapis.com/auth/youtube.readonly".into()],
            // Offline access returns a refresh token; consent forces it on reconnect.
            extra_params: vec![
                ("access_type".into(), "offline".into()),
                ("prompt".into(), "consent".into()),
            ],
        })
    }
}
