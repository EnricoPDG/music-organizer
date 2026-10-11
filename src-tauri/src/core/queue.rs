//! The download queue (data-model.md, DownloadJob; constitution Principles III and IV).
//!
//! Pipeline per job: `queued → matching → downloading → tagging → finalizing → done`.
//! Every transition is saved, so the queue resumes after a crash. Each job succeeds or fails on
//! its own; files are written to a temp folder inside the music folder, tagged there, then
//! renamed into place, so the library never contains a partial file.

use crate::commands::library::download_target;
use crate::commands::settings::{self, AudioFormat};
use crate::core::matcher::{self, MatchQuery};
use crate::core::model::NormalizedTrack;
use crate::core::sanitize::{PathError, build_path, sanitize_component};
use crate::core::tagger::{TagSet, write_tags};
use crate::db::{Db, now};
use crate::error::{AppError, AppResult};
use crate::net::Net;
use crate::platforms::youtube::watch_url;
use crate::tools::{Tool, ToolError, ToolRunner};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

/// "≤ 5 automatic retries" (data-model.md).
const MAX_RETRIES: i64 = 5;
const FINAL_STATES: &str = "('done', 'skipped', 'failed', 'cancelled')";
const WAITING_FOR_NETWORK: &str = "Waiting for network…";

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct JobProgress {
    pub job_id: String,
    pub track_id: String,
    pub batch_id: String,
    pub state: String,
    pub progress: f64,
    pub error_kind: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BatchSummary {
    pub batch_id: String,
    pub total: u32,
    pub done: u32,
    pub failed: u32,
    pub skipped: u32,
    pub active: u32,
}

/// Where queue events go (Tauri events in the app, a recorder in tests).
pub trait QueueEvents: Send + Sync {
    fn progress(&self, items: Vec<JobProgress>);
    fn summary(&self, summary: BatchSummary);
    fn library_changed(&self);
}

#[derive(Debug, Clone)]
pub struct QueueConfig {
    /// First retry delay; doubles per attempt.
    pub retry_base: Duration,
    /// Random pause before each download, so YouTube sees human-paced requests.
    pub start_jitter: (Duration, Duration),
    /// Progress events are batched and sent at most this often (≤ 10 per second).
    pub flush_interval: Duration,
    /// How often to check whether the network is back for jobs waiting on it.
    pub connectivity_check: Duration,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            retry_base: Duration::from_secs(1),
            start_jitter: (Duration::from_secs(1), Duration::from_secs(3)),
            flush_interval: Duration::from_millis(100),
            connectivity_check: Duration::from_secs(10),
        }
    }
}

pub type ConnectivityProbe = Arc<dyn Fn() -> bool + Send + Sync>;

const INTENT_NONE: u8 = 0;
const INTENT_PAUSE: u8 = 1;
const INTENT_CANCEL: u8 = 2;

struct Running {
    cancel: CancellationToken,
    intent: Arc<AtomicU8>,
}

struct Inner {
    db: Db,
    tools: Arc<dyn ToolRunner>,
    net: Arc<Net>,
    cache_dir: PathBuf,
    events: Arc<dyn QueueEvents>,
    config: QueueConfig,
    running: Mutex<HashMap<String, Running>>,
    pending: Mutex<HashMap<String, JobProgress>>,
    wake: Notify,
    active: AtomicUsize,
    max_active_seen: AtomicUsize,
    started: AtomicBool,
    probe: Mutex<ConnectivityProbe>,
}

#[derive(Clone)]
pub struct Queue {
    inner: Arc<Inner>,
}

/// The temp folder for in-progress downloads: inside the music folder, so the final rename stays
/// on one volume and is atomic.
pub fn temp_dir(music_root: &Path) -> PathBuf {
    music_root.join(".music-organizer").join("tmp")
}

/// Values available to the naming pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct PatternVars {
    pub artist: String,
    pub album_artist: String,
    pub album: String,
    pub title: String,
    pub track: Option<u32>,
    pub year: Option<u32>,
}

/// Render the naming pattern into a safe, unused path under `root` (FR-022, research R10).
/// Collisions get " (2)", " (3)"… — existing files are never overwritten.
pub fn final_path(
    root: &Path,
    pattern: &str,
    v: &PatternVars,
    ext: &str,
) -> Result<PathBuf, PathError> {
    let rendered = render_pattern(pattern, v);
    let mut parts: Vec<&str> = rendered
        .split('/')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    let stem = parts.pop().unwrap_or("Untitled").to_string();
    let first = build_path(root, &parts, &stem, ext)?;
    if !first.exists() {
        return Ok(first);
    }
    for n in 2..1000 {
        let candidate = build_path(root, &parts, &format!("{stem} ({n})"), ext)?;
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(PathError::TooLong)
}

/// Replace `{artist}`, `{album_artist}`, `{album}`, `{title}`, `{year}`, `{track}`, `{track:02}`.
/// A component that renders empty around a missing track number ("{track:02} - {title}") drops
/// the dangling separator.
pub fn render_pattern(pattern: &str, v: &PatternVars) -> String {
    let album = if v.album.trim().is_empty() {
        "Singles".to_string()
    } else {
        v.album.clone()
    };
    let track = v.track.map(|n| n.to_string()).unwrap_or_default();
    let track02 = v.track.map(|n| format!("{n:02}")).unwrap_or_default();
    let year = v.year.map(|y| y.to_string()).unwrap_or_default();
    pattern
        .split('/')
        .map(|component| {
            // Values are sanitized individually so a "/" inside a title can't create folders.
            let s = component
                .replace("{album_artist}", &sanitize_value(&v.album_artist))
                .replace("{artist}", &sanitize_value(&v.artist))
                .replace("{album}", &sanitize_value(&album))
                .replace("{title}", &sanitize_value(&v.title))
                .replace("{track:02}", &track02)
                .replace("{track}", &track)
                .replace("{year}", &year);
            s.trim()
                .trim_start_matches(['-', ' ', '.'])
                .trim_end_matches(['-', ' '])
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn sanitize_value(s: &str) -> String {
    sanitize_component(s)
}

/// yt-dlp arguments to download `video_id` as `format` into `out_template` (research R4, R5).
/// M4A works without FFmpeg by taking YouTube's AAC stream as-is; Opus and MP3 need FFmpeg.
pub fn download_args(
    video_id: &str,
    format: AudioFormat,
    out_template: &str,
    has_ffmpeg: bool,
) -> Vec<String> {
    let mut args: Vec<String> = match (format, has_ffmpeg) {
        (AudioFormat::M4a, false) => vec!["-f", "ba[ext=m4a]"],
        (AudioFormat::M4a, true) => vec!["-f", "ba[ext=m4a]/ba", "-x", "--audio-format", "m4a"],
        (AudioFormat::Opus, _) => vec!["-f", "ba[acodec=opus]/ba", "-x", "--audio-format", "opus"],
        // MP3 re-encodes (the UI says so).
        (AudioFormat::Mp3, _) => vec![
            "-f",
            "ba",
            "-x",
            "--audio-format",
            "mp3",
            "--audio-quality",
            "0",
        ],
    }
    .into_iter()
    .map(String::from)
    .collect();
    args.extend(
        [
            "--no-playlist",
            "--no-part",
            "--newline",
            "--progress-template",
            "download:progress:%(progress._percent_str)s",
            "-o",
            out_template,
        ]
        .map(String::from),
    );
    args.push(watch_url(video_id));
    args
}

/// Parse yt-dlp progress lines ("progress: 45.3%") into 0–1.
pub fn parse_progress(line: &str) -> Option<f64> {
    let rest = line.trim().strip_prefix("progress:")?;
    let num: f64 = rest.trim().trim_end_matches('%').trim().parse().ok()?;
    Some((num / 100.0).clamp(0.0, 1.0))
}

/// Map yt-dlp's error text to a job error kind and a readable reason.
pub fn classify_download_error(stderr: &str) -> (&'static str, String) {
    let s = stderr.to_lowercase();
    let pick = |kind: &'static str, msg: &str| (kind, msg.to_string());
    if s.contains("no space left") {
        pick("disk_full", "The disk is full. Free some space and retry.")
    } else if s.contains("sign in to confirm your age")
        || s.contains("age-restricted")
        || s.contains("age restricted")
    {
        pick(
            "age_restricted",
            "This video is age-restricted on YouTube. Pick another source.",
        )
    } else if s.contains("in your country") || s.contains("geo restrict") {
        pick(
            "region_blocked",
            "This video is blocked in your region. Pick another source.",
        )
    } else if s.contains("429") || s.contains("too many requests") {
        pick(
            "rate_limited",
            "YouTube is limiting requests. The app will slow down and retry.",
        )
    } else if s.contains("video unavailable")
        || s.contains("has been removed")
        || s.contains("private video")
        || s.contains("not available")
    {
        pick(
            "unavailable",
            "This video is no longer available on YouTube. Pick another source.",
        )
    } else if s.contains("unable to download")
        || s.contains("urlopen error")
        || s.contains("timed out")
        || s.contains("connection")
        || s.contains("network is unreachable")
        || s.contains("name resolution")
        || s.contains("nodename nor servname")
    {
        pick("network", WAITING_FOR_NETWORK)
    } else {
        let last = stderr
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("Download failed");
        (
            "unknown",
            last.trim()
                .trim_start_matches("ERROR:")
                .trim()
                .chars()
                .take(300)
                .collect(),
        )
    }
}

struct JobError {
    kind: &'static str,
    message: String,
}

impl JobError {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    fn from_tool(e: ToolError) -> Self {
        match e {
            ToolError::Missing(t) => Self::new(
                "tool_missing",
                format!(
                    "{} is not installed. Open Settings → Tools to install it.",
                    t.display_name()
                ),
            ),
            ToolError::Failed { stderr, .. } => {
                let (kind, message) = classify_download_error(&stderr);
                Self::new(kind, message)
            }
            ToolError::Cancelled => Self::new("cancelled", "Cancelled"),
            ToolError::Parse(m) => Self::new("unknown", m),
        }
    }

    fn from_app(e: AppError) -> Self {
        let kind = match e.kind {
            crate::error::ErrorKind::DiskFull => "disk_full",
            crate::error::ErrorKind::Network => "network",
            crate::error::ErrorKind::RateLimited => "rate_limited",
            _ => "unknown",
        };
        Self::new(kind, e.message)
    }

    fn retryable(&self) -> bool {
        matches!(self.kind, "rate_limited" | "unknown")
    }
}

struct JobRow {
    id: String,
    batch_id: String,
    track_id: String,
    attempts: i64,
}

struct TrackRow {
    title: String,
    artists: Vec<String>,
    album: Option<String>,
    album_artist: Option<String>,
    track_number: Option<u32>,
    year: Option<u32>,
    duration_ms: Option<u64>,
    isrc: Option<String>,
    source: Option<(String, NormalizedTrack)>,
}

impl Queue {
    pub fn new(
        db: Db,
        tools: Arc<dyn ToolRunner>,
        net: Arc<Net>,
        cache_dir: PathBuf,
        events: Arc<dyn QueueEvents>,
        config: QueueConfig,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                db,
                tools,
                net,
                cache_dir,
                events,
                config,
                running: Mutex::new(HashMap::new()),
                pending: Mutex::new(HashMap::new()),
                wake: Notify::new(),
                active: AtomicUsize::new(0),
                max_active_seen: AtomicUsize::new(0),
                started: AtomicBool::new(false),
                probe: Mutex::new(Arc::new(default_probe)),
            }),
        }
    }

    pub fn set_connectivity_probe(&self, probe: ConnectivityProbe) {
        *self.inner.probe.lock().expect("probe lock") = probe;
    }

    /// Highest number of jobs that ran at the same time (for tests).
    pub fn max_observed_active(&self) -> usize {
        self.inner.max_active_seen.load(Ordering::SeqCst)
    }

    /// After a crash or quit: interrupted jobs go back to `queued` and leftover temp files are
    /// deleted (data-model.md, DownloadJob).
    pub async fn recover(&self) -> AppResult<()> {
        let root = self
            .inner
            .db
            .call(|c| {
                c.execute(
                    "UPDATE download_job SET state = 'queued', progress = 0, temp_path = NULL, updated_at = ?1
                     WHERE state IN ('matching', 'downloading', 'tagging', 'finalizing')",
                    [now()],
                )?;
                download_target(c)
            })
            .await?;
        if let Some(root) = root {
            let tmp = temp_dir(&root);
            if let Ok(entries) = std::fs::read_dir(&tmp) {
                for e in entries.flatten() {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        Ok(())
    }

    /// Start the dispatcher, the progress flusher and the connectivity watcher (idempotent).
    pub fn start(&self) {
        if self.inner.started.swap(true, Ordering::SeqCst) {
            self.inner.wake.notify_one();
            return;
        }
        let q = self.clone();
        tokio::spawn(async move { q.dispatch_loop().await });
        let q = self.clone();
        tokio::spawn(async move { q.flush_loop().await });
        let q = self.clone();
        tokio::spawn(async move { q.connectivity_loop().await });
    }

    /// Wake the dispatcher (new jobs were queued).
    pub fn poke(&self) {
        self.inner.wake.notify_one();
    }

    async fn dispatch_loop(self) {
        loop {
            loop {
                let max = self
                    .inner
                    .db
                    .call(|c| Ok(settings::load(c)?.max_parallel_downloads as usize))
                    .await
                    .unwrap_or(3);
                if self.inner.active.load(Ordering::SeqCst) >= max {
                    break;
                }
                match self.claim_next().await {
                    Ok(Some(job)) => {
                        let now_active = self.inner.active.fetch_add(1, Ordering::SeqCst) + 1;
                        self.inner
                            .max_active_seen
                            .fetch_max(now_active, Ordering::SeqCst);
                        let q = self.clone();
                        tokio::spawn(async move {
                            q.run_job(job).await;
                            q.inner.active.fetch_sub(1, Ordering::SeqCst);
                            q.inner.wake.notify_one();
                        });
                    }
                    Ok(None) => break,
                    Err(e) => {
                        tracing::error!(error = %e, "could not claim next job");
                        break;
                    }
                }
            }
            tokio::select! {
                _ = self.inner.wake.notified() => {}
                _ = tokio::time::sleep(Duration::from_millis(500)) => {}
            }
        }
    }

    async fn claim_next(&self) -> AppResult<Option<JobRow>> {
        let job = self
            .inner
            .db
            .call(|c| {
                let row = c
                    .query_row(
                        "SELECT j.id, j.batch_id, j.track_id, j.attempts, EXISTS (SELECT 1 FROM track_match m WHERE m.track_id = j.track_id)
                         FROM download_job j WHERE j.state = 'queued' ORDER BY j.rowid LIMIT 1",
                        [],
                        |r| Ok((JobRow { id: r.get(0)?, batch_id: r.get(1)?, track_id: r.get(2)?, attempts: r.get(3)? }, r.get::<_, bool>(4)?)),
                    )
                    .optional()?;
                if let Some((job, matched)) = &row {
                    let state = if *matched { "downloading" } else { "matching" };
                    c.execute(
                        "UPDATE download_job SET state = ?2, progress = 0, error_kind = NULL, error_message = NULL, updated_at = ?3 WHERE id = ?1",
                        params![job.id, state, now()],
                    )?;
                }
                Ok(row)
            })
            .await?;
        Ok(job.map(|(job, matched)| {
            self.report(
                &job,
                if matched { "downloading" } else { "matching" },
                0.0,
                None,
            );
            job
        }))
    }

    async fn run_job(&self, job: JobRow) {
        let cancel = CancellationToken::new();
        let intent = Arc::new(AtomicU8::new(INTENT_NONE));
        self.inner.running.lock().expect("running lock").insert(
            job.id.clone(),
            Running {
                cancel: cancel.clone(),
                intent: intent.clone(),
            },
        );

        let mut attempts = job.attempts;
        let result = loop {
            match self.pipeline(&job, &cancel).await {
                Ok(()) => break Ok(()),
                Err(e) if cancel.is_cancelled() => break Err(e),
                Err(e) if e.retryable() && attempts < MAX_RETRIES => {
                    attempts += 1;
                    let wait = self
                        .inner
                        .config
                        .retry_base
                        .saturating_mul(1 << (attempts - 1).min(10) as u32);
                    tracing::info!(job = %job.id, attempts, kind = e.kind, "retrying job");
                    let (id, n) = (job.id.clone(), attempts);
                    let _ = self
                        .inner
                        .db
                        .call(move |c| {
                            Ok(c.execute(
                                "UPDATE download_job SET attempts = ?2 WHERE id = ?1",
                                params![id, n],
                            )?)
                        })
                        .await;
                    tokio::select! {
                        _ = cancel.cancelled() => {}
                        _ = tokio::time::sleep(wait) => {}
                    }
                }
                Err(e) => break Err(e),
            }
        };

        self.inner
            .running
            .lock()
            .expect("running lock")
            .remove(&job.id);
        let (state, error) = match result {
            Ok(()) => ("done", None),
            Err(_) if intent.load(Ordering::SeqCst) == INTENT_PAUSE => ("paused", None),
            Err(_) if intent.load(Ordering::SeqCst) == INTENT_CANCEL => ("cancelled", None),
            // Network loss: pause and let the connectivity watcher resume it.
            Err(e) if e.kind == "network" => ("paused", Some(e)),
            Err(e) => ("failed", Some(e)),
        };
        if state != "done" {
            self.remove_temp_files(&job.id).await;
        }
        let progress = if state == "done" { 1.0 } else { 0.0 };
        let (kind, message) = error
            .map(|e| (Some(e.kind.to_string()), Some(e.message)))
            .unwrap_or((None, None));
        let (id, k, m) = (job.id.clone(), kind.clone(), message.clone());
        let saved = self
            .inner
            .db
            .call(move |c| {
                c.execute(
                    "UPDATE download_job SET state = ?2, progress = ?3, error_kind = ?4, error_message = ?5, temp_path = NULL, updated_at = ?6 WHERE id = ?1",
                    params![id, state, progress, k, m, now()],
                )?;
                if state == "failed" {
                    c.execute("UPDATE track SET status = 'failed', updated_at = ?2 WHERE id = (SELECT track_id FROM download_job WHERE id = ?1) AND status = 'pending'", params![id, now()])?;
                }
                Ok(())
            })
            .await;
        if let Err(e) = saved {
            tracing::error!(error = %e, job = %job.id, "could not save job state");
        }
        self.report(&job, state, progress, kind.zip(message));
        self.emit_summary(&job.batch_id).await;
        if state == "done" {
            self.inner.events.library_changed();
        }
    }

    async fn pipeline(&self, job: &JobRow, cancel: &CancellationToken) -> Result<(), JobError> {
        let track_id = job.track_id.clone();
        let (track, video, settings, root) = self
            .inner
            .db
            .call(move |c| {
                Ok((
                    load_track(c, &track_id)?,
                    load_match(c, &track_id)?,
                    settings::load(c)?,
                    download_target(c)?,
                ))
            })
            .await
            .map_err(JobError::from_app)?;
        let root = root
            .ok_or_else(|| JobError::new("unknown", "Choose a music folder in Settings first."))?;

        // 1. Matching (non-YouTube sources only).
        let video = match video {
            Some(v) => v,
            None => {
                self.set_state(job, "matching", 0.0).await;
                self.match_track(job, &track, cancel).await?
            }
        };

        // 2. Downloading into the temp folder.
        self.set_state(job, "downloading", 0.0).await;
        let (lo, hi) = self.inner.config.start_jitter;
        if hi > lo {
            let jitter = lo + (hi - lo).mul_f64(rand::random::<f64>());
            tokio::select! {
                _ = cancel.cancelled() => return Err(JobError::new("cancelled", "Cancelled")),
                _ = tokio::time::sleep(jitter) => {}
            }
        }
        let tmp = temp_dir(&root);
        std::fs::create_dir_all(&tmp).map_err(|e| JobError::from_app(e.into()))?;
        let template = tmp
            .join(format!("{}.%(ext)s", job.id))
            .to_string_lossy()
            .to_string();
        let has_ffmpeg = self.inner.tools.available(Tool::Ffmpeg);
        if settings.audio_format != AudioFormat::M4a && !has_ffmpeg {
            return Err(JobError::new(
                "tool_missing",
                "Opus and MP3 need FFmpeg. Install FFmpeg, or switch to M4A in Settings.",
            ));
        }
        let args = download_args(&video, settings.audio_format, &template, has_ffmpeg);
        let me = self.clone();
        let job_for_progress = JobRow {
            id: job.id.clone(),
            batch_id: job.batch_id.clone(),
            track_id: job.track_id.clone(),
            attempts: 0,
        };
        let mut on_line = move |line: &str| {
            if let Some(p) = parse_progress(line) {
                me.report(&job_for_progress, "downloading", p, None);
            }
        };
        self.inner
            .tools
            .run_streaming(Tool::YtDlp, &args, &mut on_line, cancel)
            .await
            .map_err(JobError::from_tool)?;
        let downloaded = find_output(&tmp, &job.id).ok_or_else(|| {
            JobError::new(
                "unknown",
                "The download finished but no audio file was produced.",
            )
        })?;

        // 3. Tagging, still in the temp folder.
        self.set_state(job, "tagging", 1.0).await;
        let (cover, artwork_hash) = self.fetch_artwork(&track).await;
        let source_id = track
            .source
            .as_ref()
            .map(|(p, t)| format!("{p}:{}", t.platform_track_id));
        let source_url = track
            .source
            .as_ref()
            .and_then(|(_, t)| t.url.as_ref().map(|u| u.to_string()));
        let tags = TagSet {
            title: track.title.clone(),
            artists: track.artists.clone(),
            album: track.album.clone(),
            album_artist: track
                .album_artist
                .clone()
                .or_else(|| track.artists.first().cloned()),
            track_number: track.track_number,
            year: track.year,
            cover,
            source_url,
            source_id,
        };
        let path_for_tags = downloaded.clone();
        tokio::task::spawn_blocking(move || write_tags(&path_for_tags, &tags))
            .await
            .map_err(|e| JobError::new("unknown", e.to_string()))?
            .map_err(|e| JobError::new("unknown", e.to_string()))?;
        if cancel.is_cancelled() {
            return Err(JobError::new("cancelled", "Cancelled"));
        }

        // 4. Finalizing: atomic rename into the library.
        self.set_state(job, "finalizing", 1.0).await;
        let ext = downloaded
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or(settings.audio_format.extension())
            .to_string();
        let vars = PatternVars {
            artist: track
                .artists
                .first()
                .cloned()
                .unwrap_or_else(|| "Unknown Artist".into()),
            album_artist: track
                .album_artist
                .clone()
                .or_else(|| track.artists.first().cloned())
                .unwrap_or_default(),
            album: track.album.clone().unwrap_or_default(),
            title: track.title.clone(),
            track: track.track_number,
            year: track.year,
        };
        let dest = final_path(&root, &settings.naming_pattern, &vars, &ext)
            .map_err(|e| JobError::new("unknown", e.to_string()))?;
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| JobError::from_app(e.into()))?;
        }
        std::fs::rename(&downloaded, &dest).map_err(|e| JobError::from_app(e.into()))?;

        let meta = std::fs::metadata(&dest).ok();
        let size = meta.as_ref().map(|m| m.len() as i64);
        let mtime = meta
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);
        let (track_id, dest_str) = (job.track_id.clone(), dest.to_string_lossy().to_string());
        self.inner
            .db
            .call(move |c| {
                c.execute(
                    "UPDATE track SET file_path = ?2, file_format = ?3, file_size = ?4, file_mtime = ?5, status = 'available',
                     artwork_hash = coalesce(?6, artwork_hash), updated_at = ?7 WHERE id = ?1",
                    params![track_id, dest_str, ext, size, mtime, artwork_hash, now()],
                )?;
                Ok(())
            })
            .await
            .map_err(JobError::from_app)?;
        Ok(())
    }

    async fn match_track(
        &self,
        job: &JobRow,
        track: &TrackRow,
        cancel: &CancellationToken,
    ) -> Result<String, JobError> {
        let query = MatchQuery {
            title: track.title.clone(),
            artists: track.artists.clone(),
            duration_ms: track.duration_ms,
            isrc: track.isrc.clone(),
        };
        let candidates = matcher::search(self.inner.tools.as_ref(), &query, cancel)
            .await
            .map_err(JobError::from_tool)?;
        let ranked = matcher::rank(&query, candidates);
        let track_id = job.track_id.clone();
        let saved = self
            .inner
            .db
            .call(move |c| matcher::save_auto_match(c, &track_id, &ranked))
            .await
            .map_err(JobError::from_app)?;
        saved.ok_or_else(|| {
            JobError::new(
                "not_found",
                "No matching song was found on YouTube. Pick a source in Matches.",
            )
        })
    }

    /// Download the cover image and keep a copy in the artwork cache. Failure isn't fatal.
    async fn fetch_artwork(&self, track: &TrackRow) -> (Option<Vec<u8>>, Option<String>) {
        let Some(url) = track
            .source
            .as_ref()
            .and_then(|(_, t)| t.artwork_url.clone())
        else {
            return (None, None);
        };
        let is_video_thumb = track
            .source
            .as_ref()
            .is_some_and(|(_, t)| t.youtube_video_id.is_some());
        let resp = self
            .inner
            .net
            .send("artwork", |c| c.get(url.as_str()))
            .await;
        let bytes = match resp {
            Ok(r) => match r.bytes().await {
                Ok(b) => b.to_vec(),
                Err(_) => return (None, None),
            },
            Err(e) => {
                tracing::debug!(error = %e, "artwork download failed");
                return (None, None);
            }
        };
        let bytes = if is_video_thumb {
            crop_square(&bytes).unwrap_or(bytes)
        } else {
            bytes
        };
        let hash = hex(&Sha256::digest(&bytes));
        let dir = self.inner.cache_dir.join("artwork");
        if std::fs::create_dir_all(&dir).is_ok() {
            let _ = std::fs::write(dir.join(&hash), &bytes);
        }
        (Some(bytes), Some(hash))
    }

    async fn set_state(&self, job: &JobRow, state: &'static str, progress: f64) {
        let id = job.id.clone();
        let _ = self
            .inner
            .db
            .call(move |c| {
                Ok(c.execute(
                    "UPDATE download_job SET state = ?2, progress = ?3, updated_at = ?4 WHERE id = ?1",
                    params![id, state, progress, now()],
                )?)
            })
            .await;
        self.report(job, state, progress, None);
    }

    fn report(&self, job: &JobRow, state: &str, progress: f64, error: Option<(String, String)>) {
        let (error_kind, error_message) = error
            .map(|(k, m)| (Some(k), Some(m)))
            .unwrap_or((None, None));
        self.inner.pending.lock().expect("pending lock").insert(
            job.id.clone(),
            JobProgress {
                job_id: job.id.clone(),
                track_id: job.track_id.clone(),
                batch_id: job.batch_id.clone(),
                state: state.to_string(),
                progress,
                error_kind,
                error_message,
            },
        );
    }

    async fn flush_loop(self) {
        loop {
            tokio::time::sleep(self.inner.config.flush_interval).await;
            let batch: Vec<JobProgress> = self
                .inner
                .pending
                .lock()
                .expect("pending lock")
                .drain()
                .map(|(_, v)| v)
                .collect();
            if !batch.is_empty() {
                self.inner.events.progress(batch);
            }
        }
    }

    async fn emit_summary(&self, batch_id: &str) {
        let b = batch_id.to_string();
        let summary = self
            .inner
            .db
            .call(move |c| {
                let s = c.query_row(
                    "SELECT count(*),
                            sum(state = 'done'), sum(state = 'failed'), sum(state = 'skipped'),
                            sum(state NOT IN ('done', 'skipped', 'failed', 'cancelled', 'paused'))
                     FROM download_job WHERE batch_id = ?1",
                    [&b],
                    |r| {
                        Ok(BatchSummary {
                            batch_id: b.clone(),
                            total: r.get::<_, i64>(0)? as u32,
                            done: r.get::<_, Option<i64>>(1)?.unwrap_or(0) as u32,
                            failed: r.get::<_, Option<i64>>(2)?.unwrap_or(0) as u32,
                            skipped: r.get::<_, Option<i64>>(3)?.unwrap_or(0) as u32,
                            active: r.get::<_, Option<i64>>(4)?.unwrap_or(0) as u32,
                        })
                    },
                )?;
                if s.active == 0 {
                    c.execute("UPDATE import_batch SET finished_at = coalesce(finished_at, ?2) WHERE id = ?1", params![b, now()])?;
                }
                Ok(s)
            })
            .await;
        if let Ok(s) = summary {
            self.inner.events.summary(s);
        }
    }

    async fn connectivity_loop(self) {
        loop {
            tokio::time::sleep(self.inner.config.connectivity_check).await;
            let waiting: i64 = self
                .inner
                .db
                .call(|c| Ok(c.query_row("SELECT count(*) FROM download_job WHERE state = 'paused' AND error_kind = 'network'", [], |r| r.get(0))?))
                .await
                .unwrap_or(0);
            if waiting == 0 {
                continue;
            }
            let probe = self.inner.probe.lock().expect("probe lock").clone();
            let online = tokio::task::spawn_blocking(move || probe())
                .await
                .unwrap_or(false);
            if online {
                let _ = self
                    .inner
                    .db
                    .call(|c| {
                        Ok(c.execute(
                            "UPDATE download_job SET state = 'queued', error_kind = NULL, error_message = NULL, updated_at = ?1
                             WHERE state = 'paused' AND error_kind = 'network'",
                            [now()],
                        )?)
                    })
                    .await;
                self.inner.wake.notify_one();
            }
        }
    }

    async fn remove_temp_files(&self, job_id: &str) {
        let Ok(Some(root)) = self.inner.db.call(|c| download_target(c)).await else {
            return;
        };
        if let Ok(entries) = std::fs::read_dir(temp_dir(&root)) {
            for e in entries.flatten() {
                if e.file_name().to_string_lossy().starts_with(job_id) {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
    }

    async fn jobs_matching(
        &self,
        job_ids: Option<Vec<String>>,
        batch_id: Option<String>,
        states: &'static str,
    ) -> AppResult<Vec<String>> {
        self.inner
            .db
            .call(move |c| {
                let sql = format!("SELECT id FROM download_job WHERE state IN {states} AND (?1 IS NULL OR batch_id = ?1)");
                let mut stmt = c.prepare(&sql)?;
                let all: Vec<String> = stmt.query_map([&batch_id], |r| r.get(0))?.collect::<Result<_, _>>()?;
                Ok(match job_ids {
                    Some(ids) => all.into_iter().filter(|id| ids.contains(id)).collect(),
                    None => all,
                })
            })
            .await
    }

    /// Pause jobs that haven't finished. Running jobs stop and keep nothing on disk.
    pub async fn pause(
        &self,
        job_ids: Option<Vec<String>>,
        batch_id: Option<String>,
    ) -> AppResult<usize> {
        let ids = self
            .jobs_matching(
                job_ids,
                batch_id,
                "('queued', 'matching', 'downloading', 'tagging', 'finalizing', 'awaiting_review')",
            )
            .await?;
        self.interrupt(&ids, INTENT_PAUSE, "paused").await
    }

    pub async fn resume(
        &self,
        job_ids: Option<Vec<String>>,
        batch_id: Option<String>,
    ) -> AppResult<usize> {
        let ids = self.jobs_matching(job_ids, batch_id, "('paused')").await?;
        self.set_many(&ids, "queued").await?;
        self.inner.wake.notify_one();
        Ok(ids.len())
    }

    pub async fn cancel(
        &self,
        job_ids: Option<Vec<String>>,
        batch_id: Option<String>,
    ) -> AppResult<usize> {
        let ids = self
            .jobs_matching(job_ids, batch_id, "('queued', 'matching', 'downloading', 'tagging', 'finalizing', 'awaiting_review', 'paused')")
            .await?;
        let n = self.interrupt(&ids, INTENT_CANCEL, "cancelled").await?;
        for id in &ids {
            self.remove_temp_files(id).await;
        }
        Ok(n)
    }

    /// Queue failed or cancelled jobs again, with a fresh retry budget.
    pub async fn retry(
        &self,
        job_ids: Option<Vec<String>>,
        batch_id: Option<String>,
    ) -> AppResult<usize> {
        let ids = self
            .jobs_matching(job_ids, batch_id, "('failed', 'cancelled')")
            .await?;
        let list = ids.clone();
        self.inner
            .db
            .call(move |c| {
                let tx = c.transaction()?;
                for id in &list {
                    tx.execute(
                        "UPDATE download_job SET state = 'queued', attempts = 0, progress = 0, error_kind = NULL, error_message = NULL, updated_at = ?2 WHERE id = ?1",
                        params![id, now()],
                    )?;
                    tx.execute(
                        "UPDATE track SET status = 'pending' WHERE status = 'failed' AND id = (SELECT track_id FROM download_job WHERE id = ?1)",
                        [id],
                    )?;
                }
                tx.commit()?;
                Ok(())
            })
            .await?;
        self.inner.wake.notify_one();
        Ok(ids.len())
    }

    async fn interrupt(
        &self,
        ids: &[String],
        intent: u8,
        idle_state: &'static str,
    ) -> AppResult<usize> {
        let mut idle = Vec::new();
        {
            let running = self.inner.running.lock().expect("running lock");
            for id in ids {
                match running.get(id) {
                    Some(r) => {
                        r.intent.store(intent, Ordering::SeqCst);
                        r.cancel.cancel();
                    }
                    None => idle.push(id.clone()),
                }
            }
        }
        self.set_many(&idle, idle_state).await?;
        Ok(ids.len())
    }

    async fn set_many(&self, ids: &[String], state: &'static str) -> AppResult<()> {
        let list = ids.to_vec();
        let reports: Vec<JobRow> = self
            .inner
            .db
            .call(move |c| {
                let tx = c.transaction()?;
                let mut out = Vec::new();
                for id in &list {
                    tx.execute(
                        &format!(
                            "UPDATE download_job SET state = ?2, error_kind = NULL, error_message = NULL, updated_at = ?3 WHERE id = ?1 AND state NOT IN {FINAL_STATES}"
                        ),
                        params![id, state, now()],
                    )?;
                    let row = tx.query_row("SELECT id, batch_id, track_id, attempts FROM download_job WHERE id = ?1", [id], |r| {
                        Ok(JobRow { id: r.get(0)?, batch_id: r.get(1)?, track_id: r.get(2)?, attempts: r.get(3)? })
                    })?;
                    out.push(row);
                }
                tx.commit()?;
                Ok(out)
            })
            .await?;
        let mut batches = Vec::new();
        for job in &reports {
            self.report(job, state, 0.0, None);
            if !batches.contains(&job.batch_id) {
                batches.push(job.batch_id.clone());
            }
        }
        for b in batches {
            self.emit_summary(&b).await;
        }
        Ok(())
    }
}

fn load_track(c: &Connection, id: &str) -> AppResult<TrackRow> {
    let row = c.query_row(
        "SELECT title, artists, album, album_artist, track_number, year, duration_ms, isrc FROM track WHERE id = ?1",
        [id],
        |r| {
            Ok(TrackRow {
                title: r.get(0)?,
                artists: serde_json::from_str(&r.get::<_, String>(1)?).unwrap_or_default(),
                album: r.get(2)?,
                album_artist: r.get(3)?,
                track_number: r.get(4)?,
                year: r.get(5)?,
                duration_ms: r.get::<_, Option<i64>>(6)?.map(|d| d as u64),
                isrc: r.get(7)?,
                source: None,
            })
        },
    )?;
    // Prefer the source with artwork (Spotify album art beats a video thumbnail).
    let mut stmt = c.prepare(
        "SELECT platform, raw_metadata FROM source_ref WHERE track_id = ?1 ORDER BY rowid",
    )?;
    let sources: Vec<(String, NormalizedTrack)> = stmt
        .query_map([id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .filter_map(|r| r.ok())
        .filter_map(|(p, raw)| serde_json::from_str(&raw).ok().map(|t| (p, t)))
        .collect();
    let source = sources
        .iter()
        .find(|(_, t)| t.artwork_url.is_some() && t.youtube_video_id.is_none())
        .or_else(|| sources.first())
        .cloned();
    Ok(TrackRow { source, ..row })
}

fn load_match(c: &Connection, track_id: &str) -> AppResult<Option<String>> {
    Ok(c.query_row(
        "SELECT youtube_video_id FROM track_match WHERE track_id = ?1",
        [track_id],
        |r| r.get(0),
    )
    .optional()?)
}

fn find_output(tmp: &Path, job_id: &str) -> Option<PathBuf> {
    std::fs::read_dir(tmp)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            name.starts_with(job_id) && !name.ends_with(".part") && !name.ends_with(".ytdl")
        })
}

/// Center-crop a video thumbnail to a square cover (research R5).
fn crop_square(bytes: &[u8]) -> Option<Vec<u8>> {
    let img = image::load_from_memory(bytes).ok()?;
    let side = img.width().min(img.height());
    let x = (img.width() - side) / 2;
    let y = (img.height() - side) / 2;
    let square = img.crop_imm(x, y, side, side);
    let mut out = std::io::Cursor::new(Vec::new());
    square
        .to_rgb8()
        .write_to(&mut out, image::ImageFormat::Jpeg)
        .ok()?;
    Some(out.into_inner())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Can we reach YouTube? Used to resume jobs paused for network loss.
fn default_probe() -> bool {
    use std::net::ToSocketAddrs;
    let Ok(mut addrs) = ("www.youtube.com", 443).to_socket_addrs() else {
        return false;
    };
    addrs
        .next()
        .is_some_and(|a| std::net::TcpStream::connect_timeout(&a, Duration::from_secs(3)).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_progress_lines() {
        assert!((parse_progress("progress: 45.3%").unwrap() - 0.453).abs() < 1e-9);
        assert_eq!(parse_progress("progress:100"), Some(1.0));
        assert_eq!(parse_progress("[download] Destination: x"), None);
    }

    #[test]
    fn classifies_errors() {
        assert_eq!(
            classify_download_error("ERROR: Sign in to confirm your age").0,
            "age_restricted"
        );
        assert_eq!(
            classify_download_error(
                "ERROR: The uploader has not made this video available in your country"
            )
            .0,
            "region_blocked"
        );
        assert_eq!(
            classify_download_error(
                "ERROR: unable to download video data: HTTP Error 429: Too Many Requests"
            )
            .0,
            "rate_limited"
        );
        assert_eq!(
            classify_download_error("OSError: [Errno 28] No space left on device").0,
            "disk_full"
        );
        assert_eq!(
            classify_download_error("ERROR: something odd happened").1,
            "something odd happened"
        );
    }

    #[test]
    fn download_args_keep_original_quality_for_m4a() {
        let a = download_args("dQw4w9WgXcQ", AudioFormat::M4a, "/tmp/j.%(ext)s", false);
        assert!(a.windows(2).any(|w| w[0] == "-f" && w[1] == "ba[ext=m4a]"));
        assert!(
            !a.contains(&"-x".to_string()),
            "no FFmpeg: no extraction step"
        );
        let b = download_args("dQw4w9WgXcQ", AudioFormat::Mp3, "/tmp/j.%(ext)s", true);
        assert!(
            b.windows(2)
                .any(|w| w[0] == "--audio-format" && w[1] == "mp3")
        );
        assert_eq!(
            a.last().unwrap(),
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        );
    }
}
