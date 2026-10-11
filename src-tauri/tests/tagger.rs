//! T041: write then read back every tag the app sets, for each output format.

use crate::common::fixture;
use music_organizer_lib::core::tagger::{TagSet, read_tags, write_tags};

fn tags() -> TagSet {
    TagSet {
        title: "Ação & Reação".into(),
        artists: vec!["Earth, Wind & Fire".into(), "Anitta".into()],
        album: Some("Live in São Paulo".into()),
        album_artist: Some("Earth, Wind & Fire".into()),
        track_number: Some(7),
        year: Some(2024),
        cover: Some(std::fs::read(fixture("audio/cover.png")).unwrap()),
        source_url: Some("https://open.spotify.com/track/0VjIjW4GlUZAMYd2vXMi3b".into()),
        source_id: Some("spotify:0VjIjW4GlUZAMYd2vXMi3b".into()),
    }
}

fn round_trip(name: &str) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(name);
    std::fs::copy(fixture(&format!("audio/{name}")), &path).unwrap();

    write_tags(&path, &tags()).unwrap_or_else(|e| panic!("{name}: {e}"));
    let read = read_tags(&path).unwrap_or_else(|e| panic!("{name}: {e}"));

    assert_eq!(read.title.as_deref(), Some("Ação & Reação"), "{name}");
    // Multiple artists survive, including one with a comma in its name.
    assert_eq!(
        read.artists,
        vec!["Earth, Wind & Fire".to_string(), "Anitta".to_string()],
        "{name}"
    );
    assert_eq!(read.album.as_deref(), Some("Live in São Paulo"), "{name}");
    assert_eq!(
        read.album_artist.as_deref(),
        Some("Earth, Wind & Fire"),
        "{name}"
    );
    assert_eq!(read.track_number, Some(7), "{name}");
    assert_eq!(read.year, Some(2024), "{name}");
    assert_eq!(
        read.cover.as_deref(),
        tags().cover.as_deref(),
        "{name}: cover art"
    );
    assert_eq!(
        read.source_url.as_deref(),
        tags().source_url.as_deref(),
        "{name}"
    );
    assert_eq!(
        read.source_id.as_deref(),
        tags().source_id.as_deref(),
        "{name}"
    );
}

#[test]
fn m4a_round_trip() {
    round_trip("sample.m4a");
}

#[test]
fn opus_round_trip() {
    round_trip("sample.opus");
}

#[test]
fn mp3_round_trip() {
    round_trip("sample.mp3");
}

#[test]
fn rewriting_replaces_previous_values_and_cover() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.m4a");
    std::fs::copy(fixture("audio/sample.m4a"), &path).unwrap();
    write_tags(&path, &tags()).unwrap();
    let mut second = tags();
    second.title = "New title".into();
    second.artists = vec!["Solo".into()];
    write_tags(&path, &second).unwrap();
    let read = read_tags(&path).unwrap();
    assert_eq!(read.title.as_deref(), Some("New title"));
    assert_eq!(read.artists, vec!["Solo".to_string()]);
    assert_eq!(
        read.cover_count, 1,
        "front cover is replaced, not duplicated"
    );
}

#[test]
fn reading_an_untagged_file_gives_empty_tags_and_duration() {
    let read = read_tags(&fixture("audio/sample.m4a")).unwrap();
    assert!(read.title.is_none());
    assert!(read.duration_ms.unwrap() > 500, "{:?}", read.duration_ms);
}
