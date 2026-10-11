//! T042: atomic finalize — temp file → tag → rename; no partial file at the final path; never
//! overwrite (constitution Principle III).

use music_organizer_lib::core::queue::{final_path, temp_dir};
use std::path::Path;

#[test]
fn temp_dir_lives_inside_the_music_folder() {
    let root = Path::new("/music");
    assert_eq!(temp_dir(root), root.join(".music-organizer").join("tmp"));
}

#[test]
fn final_path_follows_pattern_and_sanitizes() {
    let dir = tempfile::tempdir().unwrap();
    let vars = music_organizer_lib::core::queue::PatternVars {
        artist: "AC/DC".into(),
        album_artist: "AC/DC".into(),
        album: "Back in Black".into(),
        title: "Hells Bells?".into(),
        track: Some(1),
        year: Some(1980),
    };
    let p = final_path(
        dir.path(),
        "{album_artist}/{album}/{track:02} - {title}",
        &vars,
        "m4a",
    )
    .unwrap();
    assert_eq!(
        p,
        dir.path()
            .join("ACDC")
            .join("Back in Black")
            .join("01 - Hells Bells.m4a")
    );
}

#[test]
fn missing_values_use_readable_fallbacks() {
    let dir = tempfile::tempdir().unwrap();
    let vars = music_organizer_lib::core::queue::PatternVars {
        artist: "Solo".into(),
        album_artist: "Solo".into(),
        album: String::new(),
        title: "Track".into(),
        track: None,
        year: None,
    };
    let p = final_path(
        dir.path(),
        "{album_artist}/{album}/{track:02} - {title}",
        &vars,
        "mp3",
    )
    .unwrap();
    // No album → "Singles"; no track number → the prefix is dropped cleanly.
    assert_eq!(p, dir.path().join("Solo").join("Singles").join("Track.mp3"));
}

#[test]
fn collisions_get_a_number_never_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let vars = music_organizer_lib::core::queue::PatternVars {
        artist: "A".into(),
        album_artist: "A".into(),
        album: "B".into(),
        title: "C".into(),
        track: None,
        year: None,
    };
    let first = final_path(dir.path(), "{artist}/{title}", &vars, "m4a").unwrap();
    std::fs::create_dir_all(first.parent().unwrap()).unwrap();
    std::fs::write(&first, b"existing").unwrap();
    let second = final_path(dir.path(), "{artist}/{title}", &vars, "m4a").unwrap();
    assert_eq!(second, dir.path().join("A").join("C (2).m4a"));
    assert_eq!(std::fs::read(&first).unwrap(), b"existing");
}
