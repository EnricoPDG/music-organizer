# Contract: PlatformAdapter

Every music platform plugs into the core through this contract (constitution Principle I, FR-003).
The core never matches on a platform name. It works only with the `PlatformAdapter` trait and the
normalized types below.

## Trait (Rust, `src-tauri/src/platforms/mod.rs`)

```rust
#[async_trait]
pub trait PlatformAdapter: Send + Sync {
    /// Stable lowercase id, e.g. "spotify". Used in SourceRef.platform.
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn capabilities(&self) -> Capabilities;

    /// Cheap, offline check: does this URL belong to this platform, and what does it point to?
    fn parse_url(&self, url: &Url) -> Option<ResourceRef>;

    /// Playlist/album/collection header + all tracks (adapter handles paging).
    async fn fetch_collection(&self, r: &ResourceRef, ctx: &AdapterCtx)
        -> Result<NormalizedCollection, AdapterError>;

    async fn fetch_track(&self, r: &ResourceRef, ctx: &AdapterCtx)
        -> Result<NormalizedTrack, AdapterError>;

    /// Only when capabilities().sign_in. Lists the signed-in user's collections,
    /// including the "liked" pseudo-collection.
    async fn list_my_collections(&self, ctx: &AdapterCtx)
        -> Result<Vec<CollectionSummary>, AdapterError>;

    /// Only when capabilities().sign_in. Builds the OAuth request for the user's own client app.
    fn auth_config(&self, client: &ClientCredentials) -> Option<OAuthConfig>;
}
```

`AdapterCtx` gives the adapter shared services so it never reaches into the core:

- an HTTP client with this platform's rate limiter and retry policy (research R12);
- the `ToolRunner` for yt-dlp;
- the access token for the signed-in account, if any;
- a cancellation token.

## Types

```rust
pub struct Capabilities {
    pub public_links: bool,      // import by URL without sign-in
    pub sign_in: bool,           // has OAuth flow
    pub provides_audio: bool,    // true only for YouTube: no matching step needed
}

pub enum ResourceKind { Track, Playlist, Album, UserLikes, Artist }
pub struct ResourceRef { pub platform: &'static str, pub kind: ResourceKind, pub id: String, pub url: Url }

pub struct NormalizedTrack {
    pub platform_track_id: String,
    pub url: Option<Url>,
    pub title: String,               // required, trimmed
    pub artists: Vec<String>,        // ≥ 1; "Unknown Artist" if the platform gives none
    pub album: Option<String>,
    pub track_number: Option<u32>,
    pub duration_ms: Option<u64>,
    pub isrc: Option<String>,
    pub artwork_url: Option<Url>,    // largest available
    pub youtube_video_id: Option<String>, // set only when provides_audio
}

pub struct NormalizedCollection {
    pub platform_playlist_id: String,
    pub kind: ResourceKind,
    pub name: String,
    pub owner_name: Option<String>,
    pub artwork_url: Option<Url>,
    pub fetch_mode: FetchMode,       // Official | PublicFallback (Spotify FR-004b)
    pub tracks: Vec<NormalizedTrack>,// in platform order
    pub unavailable: Vec<UnavailableItem>, // e.g. removed/local files/podcast episodes, with reason
}
```

## Errors

```rust
pub enum AdapterError {
    InvalidUrl,
    NotFound,
    Private { can_sign_in: bool },        // private playlist; tells UI whether sign-in helps
    AuthExpired,                          // → ConnectedAccount.status = expired
    RateLimited { retry_after: Option<Duration> },
    PublicSourceChanged,                  // fallback page format changed (FR-004b message)
    Network(String),
    Platform(String),                     // unexpected platform response; message is user-safe
}
```

Every error maps to a readable, localizable message in the UI. Error messages MUST NOT contain
tokens or client secrets (constitution Principle II).

## Behavioral rules (verified by contract tests)

Each adapter has a test module `tests/contract_<id>.rs` that runs the shared
`adapter_contract_suite!` macro against fixtures.

1. `parse_url` is pure and offline. It returns `None` for other platforms' URLs. It accepts the
   common URL forms: mobile and share links, `?si=` tracking parameters, `music.youtube.com`, and
   `on.soundcloud.com` short links (resolved later in `fetch_*`).
2. `fetch_collection` returns **all** tracks across pages, in platform order. Items that can't be
   imported go into `unavailable` and never cause an error for the whole collection.
3. Every `NormalizedTrack` has a non-empty `title` and at least one entry in `artists`.
4. Calling the same fixture twice gives identical output (determinism makes re-imports idempotent).
5. Adapters never write to disk or to the database. They only return data.
6. Network calls go only through `ctx.http` or `ctx.tools`, so rate limits and retries apply.
7. A 429 response becomes `RateLimited` with `retry_after` set when the platform provides it.

## Registered adapters (this feature)

| id | public_links | sign_in | provides_audio | Notes |
|----|--------------|---------|----------------|-------|
| `youtube` | ✓ (yt-dlp) | ✓ (YouTube Data API v3) | ✓ | Handles `youtube.com`, `youtu.be` and `music.youtube.com` |
| `spotify` | ✓ (official metadata; embed fallback for the track list) | ✓ (Web API PKCE) | ✗ | `fetch_mode` is reported to the UI |
| `soundcloud` | ✓ (yt-dlp) | ✓ (API, OAuth 2.1 PKCE) | ✗ | Sign-in requires the user's own app (Artist Pro) |

Adding a platform means writing a new module in `src-tauri/src/platforms/`, adding one line in
`platforms::registry()`, adding fixtures, and passing the contract suite. Nothing else changes.
