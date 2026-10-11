//! T040: download queue state machine (data-model.md, DownloadJob) and Principle IV.

use crate::common::fixture;
use crate::common::library::{Recorder, collection, env, queue, track, wait_batch, yt_track};
use music_organizer_lib::core::importer::persist_collection;
use music_organizer_lib::tools::fake::FakeToolRunner;
use music_organizer_lib::tools::{Tool, ToolError, ToolRunner};
use std::sync::Arc;
use std::time::Duration;

fn tools() -> FakeToolRunner {
    FakeToolRunner::new()
        .on(Tool::YtDlp, "ytsearch10:Bad Artist - Bad Song", fixture("ytdlp/search_bad.json"))
        .on(Tool::YtDlp, "ytsearch10:Nobody - Nothing", fixture("ytdlp/search_empty.json"))
        .on(Tool::YtDlp, "ytsearch10:", fixture("ytdlp/search_generic.json"))
        .on_fail(Tool::YtDlp, "watch?v=badVideo001", ToolError::Failed { code: Some(1), stderr: "ERROR: [youtube] badVideo001: Video unavailable. This video has been removed by the uploader".into() })
        .on_download("watch?v=", fixture("audio/sample.m4a"))
}

async fn import(
    env: &crate::common::library::Env,
    platform: &'static str,
    col: music_organizer_lib::core::model::NormalizedCollection,
) -> String {
    let provides_audio = platform == "youtube";
    env.db
        .call(move |c| persist_collection(c, platform, provides_audio, &col, "https://x"))
        .await
        .unwrap()
        .batch_id
}

#[tokio::test]
async fn happy_path_downloads_tags_and_files_every_track() {
    let env = env().await;
    let batch = import(
        &env,
        "spotify",
        collection(
            "P",
            vec![
                track("1", "Some Artist", "Some Song", 200),
                track("2", "Other Artist", "Other Song", 200),
            ],
        ),
    )
    .await;
    let events = Arc::new(Recorder::default());
    let q = queue(&env, Arc::new(tools()), events.clone());
    q.start();

    let states = wait_batch(&env.db, &batch).await;
    assert!(states.iter().all(|(_, s)| s == "done"), "{states:?}");

    let path = env
        .music
        .join("Some Artist")
        .join("Some Artist Album")
        .join("01 - Some Song.m4a");
    assert!(path.exists(), "expected {}", path.display());
    let tags = music_organizer_lib::core::tagger::read_tags(&path).unwrap();
    assert_eq!(tags.title.as_deref(), Some("Some Song"));
    let (status, fp): (String, String) = env
        .db
        .call(|c| {
            Ok(c.query_row(
                "SELECT status, file_path FROM track WHERE title = 'Some Song'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(status, "available");
    assert_eq!(std::path::PathBuf::from(fp), path);

    // Every observed state belongs to the documented pipeline, in order.
    let seen: Vec<String> = events
        .progress
        .lock()
        .unwrap()
        .iter()
        .flat_map(|(_, b)| b.iter().map(|p| p.state.clone()))
        .collect();
    let order = [
        "queued",
        "matching",
        "downloading",
        "tagging",
        "finalizing",
        "done",
    ];
    let mut last = 0;
    for s in &seen {
        let i = order
            .iter()
            .position(|o| o == s)
            .unwrap_or_else(|| panic!("unexpected state {s}"));
        assert!(i + 1 >= last, "state went backwards: {seen:?}");
        last = last.max(i);
    }
    assert!(events.summaries.lock().unwrap().last().unwrap().done == 2);
}

#[tokio::test]
async fn one_failure_does_not_stop_the_batch() {
    let env = env().await;
    let batch = import(
        &env,
        "spotify",
        collection(
            "P",
            vec![
                track("1", "Some Artist", "Some Song", 200),
                track("2", "Bad Artist", "Bad Song", 180),
                track("3", "Nobody", "Nothing", 200),
            ],
        ),
    )
    .await;
    let q = queue(&env, Arc::new(tools()), Arc::new(Recorder::default()));
    q.start();
    let states = wait_batch(&env.db, &batch).await;
    assert_eq!(
        states,
        vec![
            ("Bad Song".into(), "failed".into()),
            ("Nothing".into(), "failed".into()),
            ("Some Song".into(), "done".into()),
        ]
    );
    let reasons: Vec<(String, String)> = env.db.call(|c| {
        let mut s = c.prepare("SELECT error_kind, error_message FROM download_job WHERE state = 'failed' ORDER BY error_kind")?;
        Ok(s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?)
    }).await.unwrap();
    assert_eq!(reasons[0].0, "not_found", "{reasons:?}");
    assert_eq!(reasons[1].0, "unavailable", "{reasons:?}");
    assert!(
        reasons.iter().all(|(_, m)| !m.is_empty()),
        "readable reason (FR-012)"
    );
}

#[tokio::test]
async fn youtube_tracks_skip_matching() {
    let env = env().await;
    let batch = import(
        &env,
        "youtube",
        collection(
            "Y",
            vec![yt_track(
                "dQw4w9WgXcQ",
                "Rick Astley",
                "Never Gonna Give You Up",
            )],
        ),
    )
    .await;
    let fake = Arc::new(tools());
    let q = queue(
        &env,
        fake.clone() as Arc<dyn ToolRunner>,
        Arc::new(Recorder::default()),
    );
    q.start();
    assert_eq!(wait_batch(&env.db, &batch).await[0].1, "done");
    assert!(
        fake.calls()
            .iter()
            .all(|(_, a)| !a.join(" ").contains("ytsearch")),
        "no search for direct matches"
    );
}

#[tokio::test]
async fn concurrency_never_exceeds_the_setting() {
    let env = env().await;
    env.db
        .call(|c| {
            let s = music_organizer_lib::commands::settings::apply_patch(
                music_organizer_lib::commands::settings::load(c)?,
                music_organizer_lib::commands::settings::SettingsPatch {
                    max_parallel_downloads: Some(2),
                    ..Default::default()
                },
            )?;
            music_organizer_lib::commands::settings::save(c, &s)
        })
        .await
        .unwrap();
    let tracks = (0..8)
        .map(|i| yt_track(&format!("vid{i:08}"), "A", &format!("Song {i}")))
        .collect();
    let batch = import(&env, "youtube", collection("Y", tracks)).await;
    let events = Arc::new(Recorder::default());
    let q = queue(&env, Arc::new(tools()), events.clone());
    q.start();
    wait_batch(&env.db, &batch).await;
    assert!(
        q.max_observed_active() <= 2,
        "observed {}",
        q.max_observed_active()
    );
    assert!(q.max_observed_active() >= 1);
}

#[tokio::test]
async fn progress_events_are_throttled() {
    let env = env().await;
    let tracks = (0..10)
        .map(|i| yt_track(&format!("vid{i:08}"), "A", &format!("Song {i}")))
        .collect();
    let batch = import(&env, "youtube", collection("Y", tracks)).await;
    let events = Arc::new(Recorder::default());
    let q = queue(&env, Arc::new(tools()), events.clone());
    q.start();
    wait_batch(&env.db, &batch).await;
    let p = events.progress.lock().unwrap();
    // Flush interval is 100 ms in tests: consecutive batches are at least ~100 ms apart.
    for w in p.windows(2) {
        assert!(
            w[1].0.duration_since(w[0].0) >= Duration::from_millis(90),
            "events closer than the flush interval"
        );
    }
}

#[tokio::test]
async fn cancel_removes_temp_files_and_retry_requeues() {
    let env = env().await;
    let batch = import(
        &env,
        "spotify",
        collection("P", vec![track("1", "Some Artist", "Some Song", 200)]),
    )
    .await;
    let q = queue(&env, Arc::new(tools()), Arc::new(Recorder::default()));
    // Not started: cancel while queued.
    let n = q.cancel(None, Some(batch.clone())).await.unwrap();
    assert_eq!(n, 1);
    assert_eq!(wait_batch(&env.db, &batch).await[0].1, "cancelled");
    let tmp = env.music.join(".music-organizer").join("tmp");
    assert!(
        !tmp.exists() || std::fs::read_dir(&tmp).unwrap().next().is_none(),
        "no temp files left"
    );

    q.retry(None, Some(batch.clone())).await.unwrap();
    q.start();
    assert_eq!(wait_batch(&env.db, &batch).await[0].1, "done");
}

#[tokio::test]
async fn pause_and_resume() {
    let env = env().await;
    let batch = import(
        &env,
        "spotify",
        collection("P", vec![track("1", "Some Artist", "Some Song", 200)]),
    )
    .await;
    let q = queue(&env, Arc::new(tools()), Arc::new(Recorder::default()));
    q.pause(None, Some(batch.clone())).await.unwrap();
    q.start();
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(
        wait_batch(&env.db, &batch).await[0].1,
        "paused",
        "paused jobs don't start"
    );
    q.resume(None, Some(batch.clone())).await.unwrap();
    assert_eq!(wait_batch_done(&env, &batch).await, "done");
}

async fn wait_batch_done(env: &crate::common::library::Env, batch: &str) -> String {
    for _ in 0..200 {
        let s = wait_batch(&env.db, batch).await;
        if s[0].1 != "paused" {
            return s[0].1.clone();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    "paused".into()
}

#[tokio::test]
async fn startup_recovers_interrupted_jobs_and_deletes_partials() {
    let env = env().await;
    let batch = import(
        &env,
        "spotify",
        collection("P", vec![track("1", "Some Artist", "Some Song", 200)]),
    )
    .await;
    // Simulate a crash mid-download: job stuck in "downloading" with a partial file on disk.
    let tmp = env.music.join(".music-organizer").join("tmp");
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("crashed-job.m4a.part"), b"partial").unwrap();
    env.db
        .call(|c| {
            c.execute(
                "UPDATE download_job SET state = 'downloading', progress = 0.4",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();

    let q = queue(&env, Arc::new(tools()), Arc::new(Recorder::default()));
    q.recover().await.unwrap();
    assert!(
        std::fs::read_dir(&tmp).unwrap().next().is_none(),
        "leftover partial files are deleted"
    );
    q.start();
    assert_eq!(wait_batch(&env.db, &batch).await[0].1, "done");
}

#[tokio::test]
async fn network_loss_pauses_then_resumes_when_back() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let env = env().await;
    let batch = import(
        &env,
        "spotify",
        collection("P", vec![track("1", "Some Artist", "Some Song", 200)]),
    )
    .await;
    let offline = FakeToolRunner::new().on_fail(Tool::YtDlp, "ytsearch10:", ToolError::Failed {
        code: Some(1),
        stderr: "ERROR: Unable to download API page: <urlopen error [Errno 8] nodename nor servname provided>".into(),
    });
    // One flag drives both the tools (offline → online) and the connectivity probe.
    let online = Arc::new(AtomicBool::new(false));
    let runner = Arc::new(music_organizer_lib::tools::fake::SwitchRunner::new(
        Arc::new(offline),
        Arc::new(tools()),
        online.clone(),
    ));
    let q = queue(&env, runner, Arc::new(Recorder::default()));
    let probe = online.clone();
    q.set_connectivity_probe(Arc::new(move || probe.load(Ordering::SeqCst)));
    q.start();

    let states = wait_batch(&env.db, &batch).await;
    assert_eq!(states[0].1, "paused");
    let msg: String = env
        .db
        .call(|c| Ok(c.query_row("SELECT error_message FROM download_job", [], |r| r.get(0))?))
        .await
        .unwrap();
    assert!(msg.to_lowercase().contains("waiting for network"), "{msg}");

    // Connection is back: the watcher resumes the job.
    online.store(true, Ordering::SeqCst);
    assert_eq!(wait_batch_done(&env, &batch).await, "done");
}
