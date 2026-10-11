//! Core domain types shared by the importer, matcher, queue and platform adapters
//! (contracts/platform-adapter.md, data-model.md).

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;
use url::Url;

/// Durations within this many milliseconds are treated as the same recording.
pub const SAME_RECORDING_TOLERANCE_MS: u64 = 3_000;

/// A new random, time-ordered id (UUIDv7) as stored in the database.
pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Track,
    Playlist,
    Album,
    UserLikes,
    Artist,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceRef {
    pub platform: &'static str,
    pub kind: ResourceKind,
    pub id: String,
    pub url: Url,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum FetchMode {
    Official,
    PublicFallback,
}

impl FetchMode {
    pub fn as_str(self) -> &'static str {
        match self {
            FetchMode::Official => "official",
            FetchMode::PublicFallback => "public_fallback",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedTrack {
    pub platform_track_id: String,
    pub url: Option<Url>,
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub track_number: Option<u32>,
    pub duration_ms: Option<u64>,
    pub isrc: Option<String>,
    pub artwork_url: Option<Url>,
    pub youtube_video_id: Option<String>,
}

impl NormalizedTrack {
    /// Enforce the contract invariants: trimmed non-empty title and at least one artist.
    pub fn normalized(mut self) -> Self {
        self.title = self.title.trim().to_string();
        if self.title.is_empty() {
            self.title = "Untitled".into();
        }
        self.artists = self
            .artists
            .into_iter()
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty())
            .collect();
        if self.artists.is_empty() {
            self.artists.push("Unknown Artist".into());
        }
        self.isrc = self.isrc.filter(|i| is_valid_isrc(i));
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnavailableItem {
    pub title: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedCollection {
    pub platform_playlist_id: String,
    pub kind: ResourceKind,
    pub name: String,
    pub owner_name: Option<String>,
    pub artwork_url: Option<Url>,
    pub fetch_mode: FetchMode,
    pub tracks: Vec<NormalizedTrack>,
    pub unavailable: Vec<UnavailableItem>,
}

static ISRC_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Z]{2}[A-Z0-9]{3}\d{7}$").expect("valid regex"));

/// `^[A-Z]{2}[A-Z0-9]{3}\d{7}$` (data-model.md, Track.isrc).
pub fn is_valid_isrc(s: &str) -> bool {
    ISRC_RE.is_match(s)
}

static FEAT_BRACKETED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)[(\[]\s*(feat\.?|ft\.?|featuring)\s[^)\]]*[)\]]").expect("valid regex")
});
static FEAT_TRAILING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\s(feat\.?|ft\.?|featuring)\s.*$").expect("valid regex"));
static NOISE_BRACKETED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)[(\[]\s*(official\s*(music\s*)?(video|audio|lyric\s*video|visuali[sz]er)|lyric\s*video|lyrics?|visuali[sz]er|audio|hd|hq|4k|remaster(ed)?(\s*\d{4})?|\d{4}\s*remaster(ed)?)\s*[)\]]",
    )
    .expect("valid regex")
});
static NOISE_DASHED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s[-–—]\s*(\d{4}\s*)?remaster(ed)?(\s*(version|\d{4}))?\s*$")
        .expect("valid regex")
});

/// Normalize a title or artist for comparison: lower-case, no diacritics, no punctuation,
/// no "feat." clauses and no "(Official Video)"-style suffixes.
pub fn norm(s: &str) -> String {
    let s = FEAT_BRACKETED.replace_all(s, " ");
    let s = FEAT_TRAILING.replace(&s, "");
    let s = NOISE_BRACKETED.replace_all(&s, " ");
    let s = NOISE_DASHED.replace(&s, "");
    let folded: String = s
        .nfkd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .filter_map(|c| match c {
            '\'' | '’' | '`' => None,
            c if c.is_alphanumeric() => Some(c),
            _ => Some(' '),
        })
        .collect();
    folded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Key used to find the same song across platforms and scanned files (FR-014, FR-015b).
/// Duration is compared separately with [`same_recording_duration`], so songs whose lengths
/// differ by a couple of seconds still share a key.
pub fn dedup_key(primary_artist: &str, title: &str) -> String {
    format!("{}|{}", norm(primary_artist), norm(title))
}

/// True when both durations are within ±3 s, or either is unknown.
pub fn same_recording_duration(a: Option<u64>, b: Option<u64>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.abs_diff(b) <= SAME_RECORDING_TOLERANCE_MS,
        _ => true,
    }
}
