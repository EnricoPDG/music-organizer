//! Helpers shared by adapters for turning platform metadata into `NormalizedTrack`s.

use regex::Regex;
use std::sync::LazyLock;

/// Bracketed video noise: "(Official Video)", "[Official HD Video]", "(Lyrics)", "(Visualizer)",
/// "(Official Video Remastered)", "(Audio)", "(HD)", "(4K)", "(M/V)".
static VIDEO_NOISE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\s*[(\[](?:[^)\]]*\b(?:official|video|lyrics?|visuali[sz]er|m/?v)\b[^)\]]*|\s*(?:audio|hd|hq|4k)\s*)[)\]]",
    )
    .expect("valid regex")
});

/// Channel suffixes that are not part of the artist name.
static CHANNEL_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(\s*-\s*topic|vevo|\s+official|official\s*$)$").expect("valid regex")
});

/// Remove video-only noise from a title, keeping meaningful version words (live, remix…).
pub fn clean_title(title: &str) -> String {
    VIDEO_NOISE.replace_all(title, "").trim().to_string()
}

/// Turn a channel or uploader name into an artist name ("Queen Official" → "Queen").
pub fn artist_from_channel(channel: &str) -> String {
    let mut name = channel.trim().to_string();
    // Strip repeatedly: "QueenVEVO - Topic" is unlikely, but cheap to handle.
    for _ in 0..2 {
        name = CHANNEL_SUFFIX.replace(&name, "").trim().to_string();
    }
    if name.is_empty() {
        channel.trim().to_string()
    } else {
        name
    }
}

/// Split "Artist - Title" (also "–" and "—"). Falls back to the channel as artist.
pub fn split_artist_title(raw_title: &str, channel: Option<&str>) -> (Vec<String>, String) {
    let cleaned = clean_title(raw_title);
    for sep in [" - ", " – ", " — "] {
        if let Some((artist, title)) = cleaned.split_once(sep) {
            let (artist, title) = (artist.trim(), title.trim());
            if !artist.is_empty() && !title.is_empty() {
                return (split_artists(artist), title.to_string());
            }
        }
    }
    let artists = channel
        .map(artist_from_channel)
        .filter(|a| !a.is_empty())
        .into_iter()
        .collect();
    (artists, cleaned)
}

/// "Luis Fonsi, Daddy Yankee" / "A & B" / "A x B" → separate artists.
pub fn split_artists(s: &str) -> Vec<String> {
    s.split([',', '&'])
        .flat_map(|p| p.split(" x "))
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Seconds (possibly fractional) → milliseconds.
pub fn secs_to_ms(secs: Option<f64>) -> Option<u64> {
    secs.filter(|s| *s > 0.0)
        .map(|s| (s * 1000.0).round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_video_noise_but_keeps_versions() {
        assert_eq!(
            clean_title("Bohemian Rhapsody (Official Video Remastered)"),
            "Bohemian Rhapsody"
        );
        assert_eq!(
            clean_title("Eye Of The Tiger (Official HD Video)"),
            "Eye Of The Tiger"
        );
        assert_eq!(clean_title("Song [Lyrics]"), "Song");
        assert_eq!(
            clean_title("Song (Live at Wembley)"),
            "Song (Live at Wembley)"
        );
        assert_eq!(clean_title("Song (Remix)"), "Song (Remix)");
    }

    #[test]
    fn splits_artist_and_title() {
        assert_eq!(
            split_artist_title(
                "Queen – Bohemian Rhapsody (Official Video)",
                Some("Queen Official")
            ),
            (vec!["Queen".into()], "Bohemian Rhapsody".into())
        );
        assert_eq!(
            split_artist_title("Billie Jean", Some("Michael Jackson - Topic")),
            (vec!["Michael Jackson".into()], "Billie Jean".into())
        );
        assert_eq!(artist_from_channel("SurvivorVEVO"), "Survivor");
    }
}
