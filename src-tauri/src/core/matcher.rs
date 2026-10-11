//! Matching non-YouTube tracks to YouTube sources (research R7). Deterministic and explainable:
//! every score comes with the [`Signals`] that produced it (constitution Principle IV).

use crate::core::model::{NormalizedTrack, norm};
use crate::platforms::common::{artist_from_channel, split_artist_title};
use crate::tools::{Tool, ToolError, ToolRunner};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

const W_TITLE: f64 = 0.45;
const W_ARTIST: f64 = 0.25;
const W_DURATION: f64 = 0.30;
const TOPIC_BOOST: f64 = 0.08;
const OFFICIAL_BOOST: f64 = 0.05;
const ISRC_BOOST: f64 = 0.15;
const PENALTY: f64 = 0.35;
/// Candidates whose duration differs by more than this are rejected.
const REJECT_DURATION_MS: u64 = 30_000;

/// Words that mark a different recording, unless the source title has them too.
const VERSION_WORDS: &[&str] = &[
    "live",
    "cover",
    "remix",
    "sped up",
    "slowed",
    "8d",
    "karaoke",
    "instrumental",
    "nightcore",
    "reverb",
    "acapella",
    "a cappella",
    "1 hour",
    "loop",
    "mashup",
    "tutorial",
    "reaction",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceLevel {
    High,
    Medium,
    Low,
}

impl ConfidenceLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            ConfidenceLevel::High => "high",
            ConfidenceLevel::Medium => "medium",
            ConfidenceLevel::Low => "low",
        }
    }
}

/// `high ≥ 0.85`, `medium ≥ 0.65`, `low` otherwise (data-model.md, Match).
pub fn confidence_level(score: f64) -> ConfidenceLevel {
    if score >= 0.85 {
        ConfidenceLevel::High
    } else if score >= 0.65 {
        ConfidenceLevel::Medium
    } else {
        ConfidenceLevel::Low
    }
}

/// What we know about the track we're looking for.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchQuery {
    pub title: String,
    pub artists: Vec<String>,
    pub duration_ms: Option<u64>,
    pub isrc: Option<String>,
}

impl From<&NormalizedTrack> for MatchQuery {
    fn from(t: &NormalizedTrack) -> Self {
        Self {
            title: t.title.clone(),
            artists: t.artists.clone(),
            duration_ms: t.duration_ms,
            isrc: t.isrc.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub video_id: String,
    pub title: String,
    pub channel: String,
    pub duration_ms: Option<u64>,
}

impl Candidate {
    /// One entry of `yt-dlp -J --flat-playlist ytsearch10:…`.
    pub fn from_ytdlp(v: &Value) -> Option<Self> {
        let id = v["id"].as_str()?;
        if id.len() != 11 {
            return None;
        }
        Some(Self {
            video_id: id.into(),
            title: v["title"].as_str()?.into(),
            channel: v["channel"]
                .as_str()
                .or(v["uploader"].as_str())
                .unwrap_or_default()
                .into(),
            duration_ms: v["duration"]
                .as_f64()
                .filter(|d| *d > 0.0)
                .map(|d| (d * 1000.0).round() as u64),
        })
    }
}

/// Score breakdown, stored as `Match.signals`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Signals {
    pub title: f64,
    pub artist: f64,
    pub duration: f64,
    pub duration_delta_ms: Option<i64>,
    pub topic_channel: bool,
    pub official_channel: bool,
    pub isrc_match: bool,
    pub penalties: Vec<String>,
    pub total: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scored {
    pub candidate: Candidate,
    pub score: f64,
    pub signals: Signals,
    /// Duration too far off to be the same recording.
    pub rejected: bool,
}

fn contains_phrase(haystack: &str, phrase: &str) -> bool {
    format!(" {haystack} ").contains(&format!(" {phrase} "))
}

fn title_similarity(want: &str, got: &str) -> f64 {
    if want.is_empty() || got.is_empty() {
        return 0.0;
    }
    if want == got {
        return 1.0;
    }
    let jw = strsim::jaro_winkler(want, got);
    // "song" inside "song extended edit" etc.: close, but not identical.
    let contained = want.len() >= 4 && (contains_phrase(got, want) || contains_phrase(want, got));
    if contained { jw.max(0.9) } else { jw }
}

pub fn score(q: &MatchQuery, c: &Candidate) -> Scored {
    let want_title = norm(&q.title);
    let want_artists: Vec<String> = q
        .artists
        .iter()
        .map(|a| norm(a))
        .filter(|a| !a.is_empty())
        .collect();
    let (cand_artists, cand_title) = split_artist_title(&c.title, Some(&c.channel));
    let got_title = norm(&cand_title);
    let full_title = norm(&c.title);
    let channel_artist = norm(&artist_from_channel(&c.channel));

    let title = title_similarity(&want_title, &got_title)
        .max(title_similarity(&want_title, &full_title) * 0.95);

    let artist_text = format!("{full_title} {channel_artist}");
    let artist = if want_artists
        .iter()
        .any(|a| contains_phrase(&artist_text, a))
    {
        1.0
    } else {
        want_artists
            .iter()
            .flat_map(|a| {
                cand_artists
                    .iter()
                    .map(|x| norm(x))
                    .chain([channel_artist.clone()])
                    .map(move |b| strsim::jaro_winkler(a, &b))
            })
            .fold(0.0, f64::max)
            * 0.7
    };

    let delta = match (q.duration_ms, c.duration_ms) {
        (Some(a), Some(b)) => Some(b as i64 - a as i64),
        _ => None,
    };
    let duration = match delta.map(i64::unsigned_abs) {
        None => 0.5,
        Some(d) if d <= 3_000 => 1.0,
        Some(d) if d <= 10_000 => 0.6,
        Some(d) if d <= REJECT_DURATION_MS => 0.2,
        Some(_) => 0.0,
    };
    let rejected = delta.is_some_and(|d| d.unsigned_abs() > REJECT_DURATION_MS);

    let channel_lower = c.channel.to_lowercase();
    let artist_is_channel = want_artists.contains(&channel_artist);
    let topic_channel = channel_lower.ends_with("- topic") && artist_is_channel;
    let official_channel = !topic_channel && (artist_is_channel || channel_lower.contains("vevo"));
    let isrc_match = q
        .isrc
        .as_ref()
        .is_some_and(|i| c.title.contains(i.as_str()));

    let penalties: Vec<String> = VERSION_WORDS
        .iter()
        .filter(|w| contains_phrase(&full_title, w) && !contains_phrase(&want_title, w))
        .map(|w| (*w).to_string())
        .collect();

    let mut total = W_TITLE * title + W_ARTIST * artist + W_DURATION * duration;
    if topic_channel {
        total += TOPIC_BOOST;
    }
    if official_channel {
        total += OFFICIAL_BOOST;
    }
    if isrc_match {
        total += ISRC_BOOST;
    }
    total -= PENALTY * penalties.len().min(2) as f64;
    let total = if rejected { 0.0 } else { total.clamp(0.0, 1.0) };

    Scored {
        candidate: c.clone(),
        score: total,
        signals: Signals {
            title,
            artist,
            duration,
            duration_delta_ms: delta,
            topic_channel,
            official_channel,
            isrc_match,
            penalties,
            total,
        },
        rejected,
    }
}

/// Best first; rejected candidates last. Ties keep the search engine's order.
pub fn rank(q: &MatchQuery, candidates: Vec<Candidate>) -> Vec<Scored> {
    let mut scored: Vec<Scored> = candidates.iter().map(|c| score(q, c)).collect();
    scored.sort_by(|a, b| {
        a.rejected
            .cmp(&b.rejected)
            .then(b.score.total_cmp(&a.score))
    });
    scored
}

pub fn search_query(q: &MatchQuery) -> String {
    format!("{} - {}", q.artists.join(", "), q.title)
}

pub fn search_args(q: &MatchQuery) -> Vec<String> {
    vec![
        "-J".into(),
        "--flat-playlist".into(),
        format!("ytsearch10:{}", search_query(q)),
    ]
}

/// Search YouTube for candidates with yt-dlp.
pub async fn search(
    tools: &dyn ToolRunner,
    q: &MatchQuery,
    cancel: &CancellationToken,
) -> Result<Vec<Candidate>, ToolError> {
    search_raw(tools, &search_query(q), cancel).await
}

/// Search with a custom query (match review, US2).
pub async fn search_raw(
    tools: &dyn ToolRunner,
    query: &str,
    cancel: &CancellationToken,
) -> Result<Vec<Candidate>, ToolError> {
    let args = vec![
        "-J".into(),
        "--flat-playlist".into(),
        format!("ytsearch10:{query}"),
    ];
    let v = tools.run_json(Tool::YtDlp, &args, cancel).await?;
    Ok(v["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Candidate::from_ytdlp)
        .collect())
}

/// How many ranked candidates are kept for review (data-model.md: at least 3 when available).
const KEEP_CANDIDATES: usize = 10;

/// Save the best candidate as an automatic match, with the ranked candidates for review.
/// A match the user chose (`user_candidate` / `user_url`) is never replaced (FR-007): its video
/// is returned instead. Returns `None` when no acceptable candidate exists.
pub fn save_auto_match(
    conn: &mut rusqlite::Connection,
    track_id: &str,
    ranked: &[Scored],
) -> crate::error::AppResult<Option<String>> {
    use rusqlite::{OptionalExtension, params};
    let tx = conn.transaction()?;
    let existing: Option<(String, String)> = tx
        .query_row(
            "SELECT youtube_video_id, method FROM track_match WHERE track_id = ?1",
            [track_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((video, method)) = &existing {
        if method.starts_with("user_") {
            return Ok(Some(video.clone()));
        }
    }
    let Some(best) = ranked.first().filter(|s| !s.rejected) else {
        return Ok(None);
    };
    let signals = serde_json::to_string(&best.signals).map_err(crate::error::AppError::internal)?;
    tx.execute(
        "INSERT INTO track_match (track_id, youtube_video_id, confidence, confidence_level, method, signals, decided_at)
         VALUES (?1, ?2, ?3, ?4, 'auto', ?5, ?6)
         ON CONFLICT(track_id) DO UPDATE SET youtube_video_id = excluded.youtube_video_id, confidence = excluded.confidence,
           confidence_level = excluded.confidence_level, method = 'auto', signals = excluded.signals, decided_at = excluded.decided_at",
        params![track_id, best.candidate.video_id, best.score, confidence_level(best.score).as_str(), signals, crate::db::now()],
    )?;
    tx.execute(
        "DELETE FROM match_candidate WHERE track_id = ?1",
        [track_id],
    )?;
    let mut stmt = tx.prepare(
        "INSERT OR IGNORE INTO match_candidate (track_id, youtube_video_id, title, channel, duration_ms, score, rank) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for (i, s) in ranked.iter().take(KEEP_CANDIDATES).enumerate() {
        let c = &s.candidate;
        stmt.execute(params![
            track_id,
            c.video_id,
            c.title,
            c.channel,
            c.duration_ms.map(|d| d as i64),
            s.score,
            (i + 1) as i64
        ])?;
    }
    drop(stmt);
    tx.commit()?;
    Ok(Some(best.candidate.video_id.clone()))
}
