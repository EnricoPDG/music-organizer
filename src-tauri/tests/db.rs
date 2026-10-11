//! T013: migrations, FTS sync and unique constraints (data-model.md).

use music_organizer_lib::db::Db;
use rusqlite::params;

fn open() -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("library.db")).unwrap();
    (dir, db)
}

fn insert_track(
    conn: &rusqlite::Connection,
    id: &str,
    title: &str,
    path: Option<&str>,
) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO track (id, title, artists, origin, status, dedup_key, created_at, updated_at)
         VALUES (?1, ?2, '[\"Artist\"]', 'downloaded', 'pending', ?3, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        params![id, title, format!("artist|{title}")],
    )?;
    if let Some(p) = path {
        conn.execute(
            "UPDATE track SET file_path = ?1 WHERE id = ?2",
            params![p, id],
        )?;
    }
    Ok(1)
}

#[tokio::test]
async fn migrations_apply_and_are_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("library.db");
    let v1 = Db::open(&file)
        .unwrap()
        .call(|c| Ok(c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?))
        .await
        .unwrap();
    let v2 = Db::open(&file)
        .unwrap()
        .call(|c| Ok(c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?))
        .await
        .unwrap();
    assert!(v1 >= 1);
    assert_eq!(v1, v2);
}

#[tokio::test]
async fn fts_stays_in_sync() {
    let (_d, db) = open();
    db.call(|c| {
        insert_track(c, "t1", "Bohemian Rhapsody", None)?;
        let hits: i64 = c.query_row(
            "SELECT count(*) FROM track_fts WHERE track_fts MATCH 'bohem*'",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(hits, 1);

        c.execute("UPDATE track SET title = 'Radio Ga Ga' WHERE id = 't1'", [])?;
        let old: i64 = c.query_row(
            "SELECT count(*) FROM track_fts WHERE track_fts MATCH 'bohem*'",
            [],
            |r| r.get(0),
        )?;
        let new: i64 = c.query_row(
            "SELECT count(*) FROM track_fts WHERE track_fts MATCH 'radio*'",
            [],
            |r| r.get(0),
        )?;
        assert_eq!((old, new), (0, 1));

        c.execute("DELETE FROM track WHERE id = 't1'", [])?;
        let left: i64 = c.query_row("SELECT count(*) FROM track_fts", [], |r| r.get(0))?;
        assert_eq!(left, 0);
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn unique_constraints_exist() {
    let (_d, db) = open();
    db.call(|c| {
        insert_track(c, "t1", "A", Some("/m/a.m4a"))?;
        assert!(insert_track(c, "t2", "B", Some("/m/a.m4a")).is_err(), "track.file_path unique");

        c.execute("INSERT INTO source_ref (id, track_id, platform, platform_track_id, raw_metadata) VALUES ('s1','t1','spotify','abc','{}')", [])?;
        assert!(c.execute("INSERT INTO source_ref (id, track_id, platform, platform_track_id, raw_metadata) VALUES ('s2','t1','spotify','abc','{}')", []).is_err());

        let pl = "INSERT INTO playlist (id, kind, name, platform, platform_playlist_id, created_at) VALUES (?1, 'imported', 'P', 'youtube', 'PL1', '2026-01-01T00:00:00Z')";
        c.execute(pl, ["p1"])?;
        assert!(c.execute(pl, ["p2"]).is_err());

        let wf = "INSERT INTO watched_folder (id, path, is_download_target) VALUES (?1, ?2, ?3)";
        c.execute(wf, params!["w1", "/music", true])?;
        assert!(c.execute(wf, params!["w2", "/music", false]).is_err(), "path unique");
        assert!(c.execute(wf, params!["w3", "/other", true]).is_err(), "only one download target");
        Ok(())
    }).await.unwrap();
}

#[tokio::test]
async fn check_constraints_reject_invalid_values() {
    let (_d, db) = open();
    db.call(|c| {
        assert!(
            insert_track(c, "t0", "", None).is_err(),
            "title 1–500 chars"
        );
        insert_track(c, "t1", "A", None)?;
        assert!(
            c.execute("UPDATE track SET status = 'weird' WHERE id = 't1'", [])
                .is_err()
        );
        assert!(
            c.execute("UPDATE track SET year = 999 WHERE id = 't1'", [])
                .is_err()
        );
        assert!(
            c.execute("UPDATE track SET track_number = 0 WHERE id = 't1'", [])
                .is_err()
        );
        assert!(
            c.execute("UPDATE track SET artists = '[]' WHERE id = 't1'", [])
                .is_err(),
            "≥ 1 artist"
        );
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn artist_and_album_views_exist() {
    let (_d, db) = open();
    db.call(|c| {
        insert_track(c, "t1", "A", None)?;
        c.execute(
            "UPDATE track SET album = 'Alb', album_artist = 'Artist' WHERE id = 't1'",
            [],
        )?;
        let artists: i64 = c.query_row("SELECT count(*) FROM artist_view", [], |r| r.get(0))?;
        let albums: i64 = c.query_row("SELECT count(*) FROM album_view", [], |r| r.get(0))?;
        assert_eq!((artists, albums), (1, 1));
        Ok(())
    })
    .await
    .unwrap();
}
