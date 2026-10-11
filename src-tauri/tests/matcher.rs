//! T037: matcher scoring signals and confidence levels (research R7).

use music_organizer_lib::core::matcher::{
    Candidate, ConfidenceLevel, MatchQuery, confidence_level, rank, score, search_args,
};

fn q(title: &str, artist: &str, secs: u64) -> MatchQuery {
    MatchQuery {
        title: title.into(),
        artists: vec![artist.into()],
        duration_ms: Some(secs * 1000),
        isrc: None,
    }
}

fn c(id: &str, title: &str, channel: &str, secs: u64) -> Candidate {
    Candidate {
        video_id: id.into(),
        title: title.into(),
        channel: channel.into(),
        duration_ms: Some(secs * 1000),
    }
}

#[test]
fn exact_topic_upload_is_high_confidence() {
    let s = score(
        &q("Billie Jean", "Michael Jackson", 294),
        &c("a", "Billie Jean", "Michael Jackson - Topic", 294),
    );
    assert!(s.score >= 0.85, "{s:?}");
    assert_eq!(confidence_level(s.score), ConfidenceLevel::High);
    assert!(s.signals.topic_channel);
}

#[test]
fn duration_bands() {
    let query = q("Song", "Artist", 200);
    let at = |secs| score(&query, &c("a", "Artist - Song", "Artist", secs));
    assert!(
        at(202).signals.duration > at(208).signals.duration,
        "≤ 3 s beats ≤ 10 s"
    );
    assert!(
        at(208).signals.duration > at(225).signals.duration,
        "≤ 10 s beats ≤ 30 s"
    );
    assert!(at(231).rejected, "> 30 s is rejected");
    assert!(!at(229).rejected);
}

#[test]
fn version_words_are_penalized_unless_requested() {
    let query = q("Bohemian Rhapsody", "Queen", 354);
    let studio = score(
        &query,
        &c(
            "a",
            "Queen - Bohemian Rhapsody (Official Video)",
            "Queen Official",
            355,
        ),
    );
    for trap in [
        "(Live at Wembley)",
        "(Cover)",
        "(Remix)",
        "(sped up)",
        "(8D Audio)",
        "(Karaoke Version)",
        "(Instrumental)",
    ] {
        let t = score(
            &query,
            &c(
                "b",
                &format!("Queen - Bohemian Rhapsody {trap}"),
                "Someone",
                355,
            ),
        );
        assert!(
            t.score < studio.score - 0.2,
            "{trap}: {} vs {}",
            t.score,
            studio.score
        );
        assert!(!t.signals.penalties.is_empty(), "{trap}");
    }
    // Asked for the remix: no penalty.
    let wants = q("Bohemian Rhapsody (Remix)", "Queen", 400);
    let rmx = score(
        &wants,
        &c(
            "c",
            "Queen - Bohemian Rhapsody (Remix)",
            "Remix Nation",
            401,
        ),
    );
    assert!(rmx.signals.penalties.is_empty(), "{rmx:?}");
}

#[test]
fn artist_found_in_title_or_channel() {
    let query = q("Creep", "Radiohead", 238);
    let in_title = score(&query, &c("a", "Radiohead - Creep", "random uploads", 238));
    let in_channel = score(&query, &c("b", "Creep", "Radiohead", 238));
    let missing = score(&query, &c("d", "Creep", "random uploads", 238));
    assert_eq!(in_title.signals.artist, 1.0);
    assert_eq!(in_channel.signals.artist, 1.0);
    assert!(missing.signals.artist < 0.7, "{missing:?}");
}

#[test]
fn title_similarity_ignores_feat_and_remaster_noise() {
    let s = score(
        &q(
            "Get Lucky (feat. Pharrell Williams) - Remastered",
            "Daft Punk",
            369,
        ),
        &c(
            "a",
            "Daft Punk - Get Lucky (Official Audio)",
            "Daft Punk",
            369,
        ),
    );
    assert!(s.signals.title > 0.95, "{s:?}");
}

#[test]
fn levels_follow_thresholds() {
    assert_eq!(confidence_level(0.85), ConfidenceLevel::High);
    assert_eq!(confidence_level(0.8499), ConfidenceLevel::Medium);
    assert_eq!(confidence_level(0.65), ConfidenceLevel::Medium);
    assert_eq!(confidence_level(0.6499), ConfidenceLevel::Low);
}

#[test]
fn rank_orders_by_score_and_puts_rejected_last() {
    let query = q("Song", "Artist", 200);
    let ranked = rank(
        &query,
        vec![
            c("long", "Artist - Song", "Artist", 300),
            c("cover", "Song (Cover)", "X", 200),
            c("best", "Song", "Artist - Topic", 200),
        ],
    );
    let ids: Vec<_> = ranked
        .iter()
        .map(|s| s.candidate.video_id.as_str())
        .collect();
    assert_eq!(ids, ["best", "cover", "long"]);
    assert!(ranked[2].rejected);
}

#[test]
fn signals_explain_the_score() {
    let s = score(
        &q("Song", "Artist", 200),
        &c("a", "Song (Live)", "Artist", 205),
    );
    let json = serde_json::to_value(&s.signals).unwrap();
    for key in [
        "title",
        "artist",
        "duration",
        "topicChannel",
        "penalties",
        "total",
    ] {
        assert!(json.get(key).is_some(), "missing {key} in {json}");
    }
    assert!((s.signals.total - s.score).abs() < 1e-9);
}

#[test]
fn search_query_uses_artists_and_title() {
    let args = search_args(&MatchQuery {
        title: "Despacito".into(),
        artists: vec!["Luis Fonsi".into(), "Daddy Yankee".into()],
        duration_ms: None,
        isrc: None,
    });
    assert_eq!(
        args.last().unwrap(),
        "ytsearch10:Luis Fonsi, Daddy Yankee - Despacito"
    );
    assert!(args.contains(&"--flat-playlist".to_string()));
}
