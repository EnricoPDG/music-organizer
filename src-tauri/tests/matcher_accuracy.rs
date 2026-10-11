//! T038: matcher accuracy on the labelled set (SC-002: ≥ 90% matched to the correct recording).
//! The set is synthetic (see fixtures/ytdlp/match_set/generate.py): it proves the scorer handles
//! known traps; live accuracy is measured separately with the opt-in `live` tests.

use crate::common::fixture_str;
use music_organizer_lib::core::matcher::{Candidate, MatchQuery, rank};
use serde_json::Value;

#[test]
fn matches_at_least_90_percent_of_labelled_cases() {
    let cases: Vec<Value> =
        serde_json::from_str(&fixture_str("ytdlp/match_set/cases.json")).unwrap();
    assert!(
        cases.len() >= 200,
        "need ≥ 200 labelled cases, got {}",
        cases.len()
    );

    let mut failures = Vec::new();
    for case in &cases {
        let s = &case["source"];
        let query = MatchQuery {
            title: s["title"].as_str().unwrap().into(),
            artists: s["artists"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| a.as_str().unwrap().into())
                .collect(),
            duration_ms: s["duration_ms"].as_u64(),
            isrc: s["isrc"].as_str().map(str::to_string),
        };
        let cands: Vec<Candidate> = case["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Candidate::from_ytdlp)
            .collect();
        let ranked = rank(&query, cands);
        let chosen = ranked
            .first()
            .filter(|r| !r.rejected)
            .map(|r| r.candidate.video_id.clone());
        let ok = chosen.as_ref().is_some_and(|id| {
            case["acceptable"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a == id.as_str())
        });
        if !ok {
            let picked = ranked
                .first()
                .map(|r| format!("{} ({:.2})", r.candidate.title, r.score))
                .unwrap_or_default();
            failures.push(format!(
                "[{}] {} – {} → picked {picked}",
                case["scenario"], query.artists[0], query.title
            ));
        }
    }
    let accuracy = 1.0 - failures.len() as f64 / cases.len() as f64;
    println!(
        "matcher accuracy: {:.1}% ({} of {} cases)",
        accuracy * 100.0,
        cases.len() - failures.len(),
        cases.len()
    );
    assert!(
        accuracy >= 0.90,
        "accuracy {:.1}% < 90%. Failures:\n{}",
        accuracy * 100.0,
        failures.join("\n")
    );
}
