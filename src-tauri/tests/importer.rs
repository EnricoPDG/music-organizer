//! T039: importer — collection → tracks (dedup) → matches → jobs (FR-014, SC-005).

use crate::common::library::{collection, env, track, yt_track};
use music_organizer_lib::core::importer::persist_collection;
use rusqlite::params;

async fn count(db: &music_organizer_lib::db::Db, sql: &'static str) -> i64 {
    db.call(move |c| Ok(c.query_row(sql, [], |r| r.get(0))?))
        .await
        .unwrap()
}

#[tokio::test]
async fn import_creates_playlist_tracks_sources_and_jobs() {
    let env = env().await;
    let col = collection(
        "PL1",
        vec![
            track("a", "Queen", "Bohemian Rhapsody", 354),
            track("b", "Survivor", "Eye of the Tiger", 245),
        ],
    );
    let out = env
        .db
        .call(move |c| persist_collection(c, "spotify", false, &col, "https://x"))
        .await
        .unwrap();

    assert_eq!(out.created, 2);
    assert!(out.playlist_id.is_some());
    assert_eq!(
        count(
            &env.db,
            "SELECT count(*) FROM track WHERE origin = 'downloaded' AND status = 'pending'"
        )
        .await,
        2
    );
    assert_eq!(
        count(
            &env.db,
            "SELECT count(*) FROM source_ref WHERE platform = 'spotify'"
        )
        .await,
        2
    );
    assert_eq!(
        count(&env.db, "SELECT count(*) FROM playlist_entry").await,
        2
    );
    assert_eq!(
        count(
            &env.db,
            "SELECT count(*) FROM download_job WHERE state = 'queued'"
        )
        .await,
        2
    );
    assert_eq!(
        count(&env.db, "SELECT count(*) FROM track_match").await,
        0,
        "non-YouTube tracks are matched later"
    );
}

#[tokio::test]
async fn reimport_is_idempotent() {
    let env = env().await;
    let col = collection("PL1", vec![track("a", "Queen", "Bohemian Rhapsody", 354)]);
    let c1 = col.clone();
    env.db
        .call(move |c| persist_collection(c, "spotify", false, &c1, "https://x"))
        .await
        .unwrap();
    // Simulate the first download finishing.
    env.db
        .call(|c| {
            c.execute(
                "UPDATE track SET status = 'available', file_path = '/m/a.m4a'",
                [],
            )?;
            c.execute("UPDATE download_job SET state = 'done'", [])?;
            Ok(())
        })
        .await
        .unwrap();

    let out = env
        .db
        .call(move |c| persist_collection(c, "spotify", false, &col, "https://x"))
        .await
        .unwrap();
    assert_eq!(out.created, 0);
    assert_eq!(out.existing, 1);
    assert_eq!(count(&env.db, "SELECT count(*) FROM track").await, 1);
    assert_eq!(count(&env.db, "SELECT count(*) FROM playlist").await, 1);
    assert_eq!(
        count(
            &env.db,
            "SELECT count(*) FROM download_job WHERE state = 'skipped'"
        )
        .await,
        1,
        "already in library"
    );
}

#[tokio::test]
async fn reimport_while_downloading_does_not_queue_twice() {
    let env = env().await;
    let col = collection("PL1", vec![track("a", "Queen", "Bohemian Rhapsody", 354)]);
    let c1 = col.clone();
    env.db
        .call(move |c| persist_collection(c, "spotify", false, &c1, "https://x"))
        .await
        .unwrap();
    env.db
        .call(move |c| persist_collection(c, "spotify", false, &col, "https://x"))
        .await
        .unwrap();
    assert_eq!(
        count(
            &env.db,
            "SELECT count(*) FROM download_job WHERE state = 'queued'"
        )
        .await,
        1
    );
}

#[tokio::test]
async fn same_song_from_two_platforms_is_stored_once() {
    let env = env().await;
    let spotify = collection("SP", vec![track("sp1", "Beyoncé", "Halo", 261)]);
    // SoundCloud copy: different id, slightly different duration and spelling.
    let sc = collection(
        "SC",
        vec![track("sc9", "BEYONCE", "Halo (Official Audio)", 263)],
    );
    env.db
        .call(move |c| persist_collection(c, "spotify", false, &spotify, "https://a"))
        .await
        .unwrap();
    env.db
        .call(move |c| persist_collection(c, "soundcloud", false, &sc, "https://b"))
        .await
        .unwrap();

    assert_eq!(count(&env.db, "SELECT count(*) FROM track").await, 1);
    assert_eq!(count(&env.db, "SELECT count(*) FROM source_ref").await, 2);
    assert_eq!(
        count(&env.db, "SELECT count(*) FROM playlist_entry").await,
        2,
        "one entry per playlist"
    );
}

#[tokio::test]
async fn different_recording_with_far_duration_is_a_new_track() {
    let env = env().await;
    let a = collection("A", vec![track("1", "Queen", "Bohemian Rhapsody", 354)]);
    let b = collection("B", vec![track("2", "Queen", "Bohemian Rhapsody", 420)]);
    env.db
        .call(move |c| persist_collection(c, "spotify", false, &a, "https://a"))
        .await
        .unwrap();
    env.db
        .call(move |c| persist_collection(c, "spotify", false, &b, "https://b"))
        .await
        .unwrap();
    assert_eq!(count(&env.db, "SELECT count(*) FROM track").await, 2);
}

#[tokio::test]
async fn youtube_tracks_get_a_direct_match() {
    let env = env().await;
    let col = collection(
        "YT",
        vec![yt_track(
            "dQw4w9WgXcQ",
            "Rick Astley",
            "Never Gonna Give You Up",
        )],
    );
    env.db
        .call(move |c| persist_collection(c, "youtube", true, &col, "https://y"))
        .await
        .unwrap();
    let (method, conf, level): (String, f64, String) = env
        .db
        .call(|c| {
            Ok(c.query_row(
                "SELECT method, confidence, confidence_level FROM track_match",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(
        (method.as_str(), conf, level.as_str()),
        ("direct", 1.0, "high")
    );
}

#[tokio::test]
async fn playlist_positions_follow_platform_order() {
    let env = env().await;
    let col = collection(
        "P",
        vec![
            track("x", "A", "Zeta", 100),
            track("y", "B", "Alpha", 100),
            track("z", "C", "Mid", 100),
        ],
    );
    env.db
        .call(move |c| persist_collection(c, "spotify", false, &col, "https://p"))
        .await
        .unwrap();
    let titles: Vec<String> = env
        .db
        .call(|c| {
            let mut s = c.prepare("SELECT t.title FROM playlist_entry e JOIN track t ON t.id = e.track_id ORDER BY e.position")?;
            Ok(s.query_map(params![], |r| r.get(0))?.collect::<Result<_, _>>()?)
        })
        .await
        .unwrap();
    assert_eq!(titles, ["Zeta", "Alpha", "Mid"]);
}
