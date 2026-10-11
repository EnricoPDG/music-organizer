//! Spotify adapter (research R6). Signed in, it uses the Web API with PKCE. Since February 2026
//! the API returns playlist contents only for playlists the user owns or collaborates on, so
//! other public playlists are read from the public embed page (best-effort, FR-004b).

use super::common::split_artists;
use super::{
    AdapterCtx, AdapterError, Capabilities, ClientCredentials, CollectionSummary, OAuthConfig,
    PlatformAdapter, host_of,
};
use crate::core::model::{
    FetchMode, NormalizedCollection, NormalizedTrack, ResourceKind, ResourceRef, UnavailableItem,
};
use async_trait::async_trait;
use serde_json::Value;
use url::Url;

const ID: &str = "spotify";
const DEFAULT_API: &str = "https://api.spotify.com";
const DEFAULT_WEB: &str = "https://open.spotify.com";
const LIKED_ID: &str = "liked";

pub struct SpotifyAdapter {
    api_base: String,
    web_base: String,
}

impl Default for SpotifyAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl SpotifyAdapter {
    pub fn new() -> Self {
        Self::with_bases(DEFAULT_API, DEFAULT_WEB)
    }

    /// Point the Web API and the embed pages at other base URLs (tests).
    pub fn with_bases(api: &str, web: &str) -> Self {
        Self {
            api_base: api.trim_end_matches('/').into(),
            web_base: web.trim_end_matches('/').into(),
        }
    }

    async fn api_get(&self, ctx: &AdapterCtx, url: &str) -> Result<Value, AdapterError> {
        let token = ctx
            .token
            .as_ref()
            .ok_or(AdapterError::Private { can_sign_in: true })?;
        // Never send the token anywhere but the configured API host (paging URLs come from the response).
        if !url.starts_with(&self.api_base) {
            return Err(AdapterError::Platform("unexpected paging URL".into()));
        }
        let resp = ctx
            .http
            .send(ID, |c| c.get(url).bearer_auth(token.expose()))
            .await?;
        resp.json()
            .await
            .map_err(|e| AdapterError::Platform(e.to_string()))
    }

    /// Read every page of a paging object, starting from an already-fetched first page.
    async fn collect_pages(
        &self,
        ctx: &AdapterCtx,
        mut page: Value,
        album: Option<&Value>,
    ) -> Result<(Vec<NormalizedTrack>, Vec<UnavailableItem>), AdapterError> {
        let (mut tracks, mut unavailable) = (Vec::new(), Vec::new());
        loop {
            for entry in page["items"].as_array().into_iter().flatten() {
                // Playlist items wrap the track in `item` (Feb 2026) or `track` (older/saved
                // tracks); album track listings are the track itself.
                let t = if entry.get("item").is_some() {
                    &entry["item"]
                } else if entry.get("track").is_some() {
                    &entry["track"]
                } else {
                    entry
                };
                match api_track(t, album) {
                    Ok(t) => tracks.push(t),
                    Err(u) => unavailable.push(u),
                }
            }
            match page["next"].as_str() {
                Some(next) => page = self.api_get(ctx, next).await?,
                None => break,
            }
        }
        Ok((tracks, unavailable))
    }

    async fn embed(&self, ctx: &AdapterCtx, kind: &str, id: &str) -> Result<Value, AdapterError> {
        let url = format!("{}/embed/{kind}/{id}", self.web_base);
        let resp = ctx.http.send(ID, |c| c.get(&url)).await?;
        let html = resp
            .text()
            .await
            .map_err(|e| AdapterError::Network(e.to_string()))?;
        parse_embed_entity(&html).ok_or(AdapterError::PublicSourceChanged)
    }

    async fn embed_collection(
        &self,
        ctx: &AdapterCtx,
        r: &ResourceRef,
    ) -> Result<NormalizedCollection, AdapterError> {
        let kind = if r.kind == ResourceKind::Album {
            "album"
        } else {
            "playlist"
        };
        let e = self.embed(ctx, kind, &r.id).await?;
        let list = e["trackList"]
            .as_array()
            .ok_or(AdapterError::PublicSourceChanged)?;
        let (mut tracks, mut unavailable) = (Vec::new(), Vec::new());
        for item in list {
            match embed_track(item) {
                Ok(t) => tracks.push(t),
                Err(u) => unavailable.push(u),
            }
        }
        Ok(NormalizedCollection {
            platform_playlist_id: r.id.clone(),
            kind: r.kind,
            name: e["name"]
                .as_str()
                .or(e["title"].as_str())
                .unwrap_or("Spotify playlist")
                .into(),
            owner_name: e["subtitle"].as_str().map(str::to_string),
            artwork_url: embed_cover(&e),
            fetch_mode: FetchMode::PublicFallback,
            tracks,
            unavailable,
        })
    }
}

/// Extract `props.pageProps.state.data.entity` from the embed page's `__NEXT_DATA__` script.
fn parse_embed_entity(html: &str) -> Option<Value> {
    let marker = r#"<script id="__NEXT_DATA__" type="application/json">"#;
    let start = html.find(marker)? + marker.len();
    let end = start + html[start..].find("</script>")?;
    let data: Value = serde_json::from_str(&html[start..end]).ok()?;
    let entity = data.pointer("/props/pageProps/state/data/entity")?;
    entity.is_object().then(|| entity.clone())
}

fn embed_cover(e: &Value) -> Option<Url> {
    e["coverArt"]["sources"]
        .as_array()?
        .iter()
        .max_by_key(|s| s["width"].as_u64().unwrap_or(0))
        .and_then(|s| s["url"].as_str())
        .and_then(|u| Url::parse(u).ok())
}

fn embed_track(v: &Value) -> Result<NormalizedTrack, UnavailableItem> {
    let uri = v["uri"].as_str().unwrap_or_default();
    let title = v["title"]
        .as_str()
        .or(v["name"].as_str())
        .unwrap_or_default();
    let Some(id) = uri.strip_prefix("spotify:track:") else {
        let reason = if uri.starts_with("spotify:episode:") {
            "Podcast episodes can't be imported"
        } else {
            "Not a track"
        };
        return Err(UnavailableItem {
            title: Some(title.into()),
            reason: reason.into(),
        });
    };
    if v["isPlayable"].as_bool() == Some(false) {
        return Err(UnavailableItem {
            title: Some(title.into()),
            reason: "Not available on Spotify".into(),
        });
    }
    let artists = match v["artists"].as_array() {
        Some(a) => a
            .iter()
            .filter_map(|x| x["name"].as_str().map(str::to_string))
            .collect(),
        None => split_artists(v["subtitle"].as_str().unwrap_or_default()),
    };
    Ok(NormalizedTrack {
        platform_track_id: id.into(),
        url: Url::parse(&format!("https://open.spotify.com/track/{id}")).ok(),
        title: title.into(),
        artists,
        album: None,
        track_number: None,
        duration_ms: v["duration"].as_u64(),
        isrc: None,
        artwork_url: None,
        youtube_video_id: None,
    }
    .normalized())
}

/// Web API track object → track. `album` fills in album data for album track listings.
fn api_track(t: &Value, album: Option<&Value>) -> Result<NormalizedTrack, UnavailableItem> {
    if t.is_null() {
        return Err(UnavailableItem {
            title: None,
            reason: "This track was removed from Spotify".into(),
        });
    }
    let title = t["name"].as_str().unwrap_or_default();
    match t["type"].as_str() {
        Some("episode") => {
            return Err(UnavailableItem {
                title: Some(title.into()),
                reason: "Podcast episodes can't be imported".into(),
            });
        }
        Some("track") | None => {}
        Some(_) => {
            return Err(UnavailableItem {
                title: Some(title.into()),
                reason: "Not a track".into(),
            });
        }
    }
    if t["is_local"].as_bool() == Some(true) {
        return Err(UnavailableItem {
            title: Some(title.into()),
            reason: "Local files on Spotify can't be imported".into(),
        });
    }
    let Some(id) = t["id"].as_str() else {
        return Err(UnavailableItem {
            title: Some(title.into()),
            reason: "Not available on Spotify".into(),
        });
    };
    let album = album.unwrap_or(&t["album"]);
    let artwork = album["images"]
        .as_array()
        .and_then(|imgs| imgs.iter().max_by_key(|i| i["width"].as_u64().unwrap_or(0)))
        .and_then(|i| i["url"].as_str())
        .and_then(|u| Url::parse(u).ok());
    Ok(NormalizedTrack {
        platform_track_id: id.into(),
        url: t["external_urls"]["spotify"]
            .as_str()
            .and_then(|u| Url::parse(u).ok()),
        title: title.into(),
        artists: t["artists"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| a["name"].as_str().map(str::to_string))
            .collect(),
        album: album["name"]
            .as_str()
            .filter(|n| !n.is_empty())
            .map(str::to_string),
        track_number: t["track_number"].as_u64().map(|n| n as u32),
        duration_ms: t["duration_ms"].as_u64(),
        isrc: t["external_ids"]["isrc"].as_str().map(str::to_string),
        artwork_url: artwork,
        youtube_video_id: None,
    }
    .normalized())
}

fn is_spotify_id(s: &str) -> bool {
    s.len() == 22 && s.bytes().all(|b| b.is_ascii_alphanumeric())
}

#[async_trait]
impl PlatformAdapter for SpotifyAdapter {
    fn id(&self) -> &'static str {
        ID
    }

    fn display_name(&self) -> &'static str {
        "Spotify"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            public_links: true,
            sign_in: true,
            provides_audio: false,
        }
    }

    fn parse_url(&self, url: &Url) -> Option<ResourceRef> {
        if host_of(url) != "open.spotify.com" {
            return None;
        }
        let mut segs: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
        // Localized links: /intl-pt/playlist/…
        if segs.first().is_some_and(|s| s.starts_with("intl-")) {
            segs.remove(0);
        }
        let make = |kind, id: &str| {
            Some(ResourceRef {
                platform: ID,
                kind,
                id: id.into(),
                url: url.clone(),
            })
        };
        match segs.as_slice() {
            ["collection", "tracks"] => make(ResourceKind::UserLikes, LIKED_ID),
            ["playlist", id] if is_spotify_id(id) => make(ResourceKind::Playlist, id),
            ["album", id] if is_spotify_id(id) => make(ResourceKind::Album, id),
            ["track", id] if is_spotify_id(id) => make(ResourceKind::Track, id),
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
                    platform_playlist_id: r.id.clone(),
                    kind: ResourceKind::Track,
                    name: t.title.clone(),
                    owner_name: t.artists.first().cloned(),
                    artwork_url: t.artwork_url.clone(),
                    fetch_mode: if ctx.token.is_some() {
                        FetchMode::Official
                    } else {
                        FetchMode::PublicFallback
                    },
                    tracks: vec![t],
                    unavailable: vec![],
                })
            }
            ResourceKind::UserLikes => {
                let first = self
                    .api_get(ctx, &format!("{}/v1/me/tracks?limit=50", self.api_base))
                    .await?;
                let (tracks, unavailable) = self.collect_pages(ctx, first, None).await?;
                Ok(NormalizedCollection {
                    platform_playlist_id: LIKED_ID.into(),
                    kind: ResourceKind::UserLikes,
                    name: "Liked Songs".into(),
                    owner_name: None,
                    artwork_url: None,
                    fetch_mode: FetchMode::Official,
                    tracks,
                    unavailable,
                })
            }
            ResourceKind::Album if ctx.token.is_some() => {
                let a = self
                    .api_get(ctx, &format!("{}/v1/albums/{}", self.api_base, r.id))
                    .await?;
                let (tracks, unavailable) = self
                    .collect_pages(ctx, a["tracks"].clone(), Some(&a))
                    .await?;
                Ok(NormalizedCollection {
                    platform_playlist_id: r.id.clone(),
                    kind: ResourceKind::Album,
                    name: a["name"].as_str().unwrap_or("Album").into(),
                    owner_name: a["artists"][0]["name"].as_str().map(str::to_string),
                    artwork_url: tracks.first().and_then(|t| t.artwork_url.clone()),
                    fetch_mode: FetchMode::Official,
                    tracks,
                    unavailable,
                })
            }
            ResourceKind::Playlist if ctx.token.is_some() => {
                let p = self
                    .api_get(ctx, &format!("{}/v1/playlists/{}", self.api_base, r.id))
                    .await?;
                // Not owned by the user: metadata only, without `items`. Read the public page instead.
                if p.get("items").is_none() {
                    return self.embed_collection(ctx, r).await;
                }
                let (tracks, unavailable) =
                    self.collect_pages(ctx, p["items"].clone(), None).await?;
                Ok(NormalizedCollection {
                    platform_playlist_id: r.id.clone(),
                    kind: ResourceKind::Playlist,
                    name: p["name"].as_str().unwrap_or("Spotify playlist").into(),
                    owner_name: p["owner"]["display_name"].as_str().map(str::to_string),
                    artwork_url: p["images"][0]["url"]
                        .as_str()
                        .and_then(|u| Url::parse(u).ok()),
                    fetch_mode: FetchMode::Official,
                    tracks,
                    unavailable,
                })
            }
            _ => self.embed_collection(ctx, r).await,
        }
    }

    async fn fetch_track(
        &self,
        r: &ResourceRef,
        ctx: &AdapterCtx,
    ) -> Result<NormalizedTrack, AdapterError> {
        if ctx.token.is_some() {
            let t = self
                .api_get(ctx, &format!("{}/v1/tracks/{}", self.api_base, r.id))
                .await?;
            return api_track(&t, None).map_err(|u| AdapterError::Platform(u.reason));
        }
        let e = self.embed(ctx, "track", &r.id).await?;
        let mut item = e.clone();
        item["uri"] = Value::String(format!("spotify:track:{}", r.id));
        let mut t = embed_track(&item).map_err(|u| AdapterError::Platform(u.reason))?;
        t.artwork_url = embed_cover(&e);
        Ok(t)
    }

    async fn list_my_collections(
        &self,
        ctx: &AdapterCtx,
    ) -> Result<Vec<CollectionSummary>, AdapterError> {
        let mut out = vec![CollectionSummary {
            platform: ID.into(),
            kind: ResourceKind::UserLikes,
            id: LIKED_ID.into(),
            url: "https://open.spotify.com/collection/tracks".into(),
            name: "Liked Songs".into(),
            track_count: None,
            artwork_url: None,
        }];
        let mut page = self
            .api_get(ctx, &format!("{}/v1/me/playlists?limit=50", self.api_base))
            .await?;
        loop {
            for p in page["items"].as_array().into_iter().flatten() {
                let Some(id) = p["id"].as_str() else { continue };
                out.push(CollectionSummary {
                    platform: ID.into(),
                    kind: ResourceKind::Playlist,
                    id: id.into(),
                    url: format!("https://open.spotify.com/playlist/{id}"),
                    name: p["name"].as_str().unwrap_or("Untitled").into(),
                    track_count: p["items"]["total"]
                        .as_u64()
                        .or(p["tracks"]["total"].as_u64())
                        .map(|n| n as u32),
                    artwork_url: p["images"][0]["url"].as_str().map(str::to_string),
                });
            }
            match page["next"].as_str() {
                Some(next) => page = self.api_get(ctx, next).await?,
                None => break,
            }
        }
        Ok(out)
    }

    fn auth_config(&self, _client: &ClientCredentials) -> Option<OAuthConfig> {
        Some(OAuthConfig {
            authorize_url: Url::parse("https://accounts.spotify.com/authorize").expect("valid"),
            token_url: Url::parse("https://accounts.spotify.com/api/token").expect("valid"),
            scopes: [
                "playlist-read-private",
                "playlist-read-collaborative",
                "user-library-read",
            ]
            .map(String::from)
            .to_vec(),
            extra_params: vec![],
        })
    }
}
