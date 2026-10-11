//! Builders for library-level tests: a temp music folder, a database, collections and a queue.

use music_organizer_lib::commands::library::add_folder;
use music_organizer_lib::core::model::{
    FetchMode, NormalizedCollection, NormalizedTrack, ResourceKind,
};
use music_organizer_lib::core::queue::{
    BatchSummary, JobProgress, Queue, QueueConfig, QueueEvents,
};
use music_organizer_lib::db::Db;
use music_organizer_lib::net::{Net, RetryPolicy};
use music_organizer_lib::tools::ToolRunner;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct Env {
    pub _dir: tempfile::TempDir,
    pub music: PathBuf,
    pub cache: PathBuf,
    pub db: Db,
}

pub async fn env() -> Env {
    let dir = tempfile::tempdir().unwrap();
    let music = dir.path().join("Music");
    let cache = dir.path().join("cache");
    std::fs::create_dir_all(&music).unwrap();
    std::fs::create_dir_all(&cache).unwrap();
    let db = Db::open(&dir.path().join("library.db")).unwrap();
    let m = music.to_string_lossy().to_string();
    db.call(move |c| add_folder(c, &m, true)).await.unwrap();
    // Canonical path (macOS /var → /private/var) so comparisons match what the app stores.
    let music = std::fs::canonicalize(&music).unwrap();
    Env {
        _dir: dir,
        music,
        cache,
        db,
    }
}

pub fn track(id: &str, artist: &str, title: &str, secs: u64) -> NormalizedTrack {
    NormalizedTrack {
        platform_track_id: id.into(),
        url: None,
        title: title.into(),
        artists: vec![artist.into()],
        album: Some(format!("{artist} Album")),
        track_number: Some(1),
        duration_ms: Some(secs * 1000),
        isrc: None,
        artwork_url: None,
        youtube_video_id: None,
    }
}

pub fn yt_track(video_id: &str, artist: &str, title: &str) -> NormalizedTrack {
    NormalizedTrack {
        youtube_video_id: Some(video_id.into()),
        ..track(video_id, artist, title, 200)
    }
}

pub fn collection(id: &str, tracks: Vec<NormalizedTrack>) -> NormalizedCollection {
    NormalizedCollection {
        platform_playlist_id: id.into(),
        kind: ResourceKind::Playlist,
        name: format!("Playlist {id}"),
        owner_name: Some("Owner".into()),
        artwork_url: None,
        fetch_mode: FetchMode::Official,
        tracks,
        unavailable: vec![],
    }
}

#[derive(Default)]
pub struct Recorder {
    pub progress: Mutex<Vec<(std::time::Instant, Vec<JobProgress>)>>,
    pub summaries: Mutex<Vec<BatchSummary>>,
}

impl QueueEvents for Recorder {
    fn progress(&self, items: Vec<JobProgress>) {
        self.progress
            .lock()
            .unwrap()
            .push((std::time::Instant::now(), items));
    }
    fn summary(&self, s: BatchSummary) {
        self.summaries.lock().unwrap().push(s);
    }
    fn library_changed(&self) {}
}

pub fn fast_config() -> QueueConfig {
    QueueConfig {
        retry_base: Duration::from_millis(5),
        start_jitter: (Duration::ZERO, Duration::ZERO),
        flush_interval: Duration::from_millis(100),
        connectivity_check: Duration::from_millis(50),
    }
}

pub fn queue(env: &Env, tools: Arc<dyn ToolRunner>, events: Arc<Recorder>) -> Queue {
    let net = Arc::new(Net::new(RetryPolicy {
        base: Duration::from_millis(5),
        max_attempts: 1,
        timeout: Duration::from_secs(2),
    }));
    Queue::new(
        env.db.clone(),
        tools,
        net,
        env.cache.clone(),
        events,
        fast_config(),
    )
}

/// Wait until every job in the batch reached a final state.
pub async fn wait_batch(db: &Db, batch_id: &str) -> Vec<(String, String)> {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let b = batch_id.to_string();
        let states: Vec<(String, String)> = db
            .call(move |c| {
                let mut s = c.prepare("SELECT t.title, j.state FROM download_job j JOIN track t ON t.id = j.track_id WHERE j.batch_id = ?1 ORDER BY t.title")?;
                let rows = s.query_map([b], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await
            .unwrap();
        if states.iter().all(|(_, s)| {
            ["done", "skipped", "failed", "cancelled", "paused"].contains(&s.as_str())
        }) {
            return states;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "batch did not finish: {states:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
