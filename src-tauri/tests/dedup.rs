//! T011: normalization and dedup key (data-model.md, Track.dedup_key).

use music_organizer_lib::core::model::{dedup_key, norm, same_recording_duration};

#[test]
fn norm_lowercases_strips_diacritics_and_punctuation() {
    assert_eq!(norm("Beyoncé — Halo!"), "beyonce halo");
    assert_eq!(norm("Sigur Rós"), "sigur ros");
    assert_eq!(norm("  Don't   Stop  "), "dont stop");
}

#[test]
fn norm_removes_featuring_clauses() {
    assert_eq!(norm("Song (feat. Someone)"), "song");
    assert_eq!(norm("Song [ft. A & B]"), "song");
    assert_eq!(norm("Song feat. Someone Else"), "song");
    assert_eq!(norm("Song featuring Someone"), "song");
}

#[test]
fn norm_removes_video_style_suffixes() {
    for s in [
        "Song (Official Video)",
        "Song [Official Music Video]",
        "Song (Official Audio)",
        "Song (Lyric Video)",
        "Song (Lyrics)",
        "Song (Visualizer)",
        "Song (HD)",
        "Song - Remastered 2011",
        "Song (Remastered)",
    ] {
        assert_eq!(norm(s), "song", "{s}");
    }
}

#[test]
fn norm_keeps_meaningful_version_words() {
    // Live/remix/acoustic are different recordings and must not be stripped.
    assert_eq!(norm("Song (Live)"), "song live");
    assert_eq!(norm("Song (Remix)"), "song remix");
}

#[test]
fn dedup_key_uses_primary_artist_and_title() {
    assert_eq!(
        dedup_key("Daft Punk", "One More Time (Official Video)"),
        "daft punk|one more time"
    );
    assert_eq!(
        dedup_key("Beyoncé", "Halo"),
        dedup_key("BEYONCE", "Halo (Official Audio)")
    );
}

#[test]
fn same_song_with_two_second_difference_matches() {
    assert!(same_recording_duration(Some(200_000), Some(202_000)));
    assert!(same_recording_duration(Some(200_000), Some(197_000)));
    assert!(!same_recording_duration(Some(200_000), Some(204_000)));
    // Unknown duration on either side: rely on artist + title only.
    assert!(same_recording_duration(None, Some(200_000)));
}
