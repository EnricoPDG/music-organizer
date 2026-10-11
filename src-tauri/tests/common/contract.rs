//! Shared adapter contract suite (contracts/platform-adapter.md, "Behavioral rules").
//!
//! Every adapter test module calls [`adapter_contract_suite!`] with fixture-backed cases:
//! 1. `parse_url` is pure and rejects other platforms' URLs.
//! 2. `fetch_collection` returns every page in order; unimportable items go to `unavailable`.
//! 3. Every track has a non-empty title and at least one artist.
//! 4. Same fixture in, identical output out (determinism → idempotent re-imports).
//! 5. Adapters never write: they only get `AdapterCtx`, which has no database or file access.
//! 6. All I/O goes through `ctx` (checked by counting requests on the fixture server or tools).
//! 7. HTTP 429 becomes `RateLimited` (checked per adapter, where the platform uses HTTP).

use music_organizer_lib::core::model::ResourceKind;
use music_organizer_lib::platforms::{AdapterCtx, PlatformAdapter};
use std::sync::Arc;
use url::Url;

pub struct CollectionCase {
    pub url: &'static str,
    pub kind: ResourceKind,
    pub expected_tracks: usize,
    pub expected_unavailable: usize,
    /// Expected first and last titles, to prove page order.
    pub first_title: &'static str,
    pub last_title: &'static str,
}

/// URLs from every supported platform. Each adapter must accept only its own.
pub const FOREIGN_URLS: &[(&str, &str)] = &[
    (
        "spotify",
        "https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M",
    ),
    (
        "youtube",
        "https://www.youtube.com/playlist?list=PLFgquLnL59alCl_2TQvOiD5Vgm1hCaGSI",
    ),
    ("soundcloud", "https://soundcloud.com/forss/sets/soulhack"),
    ("other", "https://example.com/playlist/1"),
];

pub fn check_parse_url(adapter: &dyn PlatformAdapter) {
    for (platform, url) in FOREIGN_URLS {
        let parsed = adapter.parse_url(&Url::parse(url).unwrap());
        if *platform == adapter.id() {
            let r =
                parsed.unwrap_or_else(|| panic!("{} must accept its own URL {url}", adapter.id()));
            assert_eq!(r.platform, adapter.id());
            // Pure: same input, same output.
            assert_eq!(Some(r), adapter.parse_url(&Url::parse(url).unwrap()));
        } else {
            assert!(
                parsed.is_none(),
                "{} accepted foreign URL {url}",
                adapter.id()
            );
        }
    }
}

/// `io_count` reports how many requests/tool calls went through `ctx` so far (rule 6).
pub async fn check_collection(
    adapter: Arc<dyn PlatformAdapter>,
    ctx: &AdapterCtx,
    case: &CollectionCase,
    io_count: &dyn Fn() -> usize,
) {
    let url = Url::parse(case.url).unwrap();
    let r = adapter
        .parse_url(&url)
        .unwrap_or_else(|| panic!("rejected {}", case.url));
    assert_eq!(r.kind, case.kind, "{}", case.url);

    let before = io_count();
    let a = adapter
        .fetch_collection(&r, ctx)
        .await
        .unwrap_or_else(|e| panic!("{}: {e:?}", case.url));
    assert!(
        io_count() > before,
        "rule 6: I/O must go through ctx ({})",
        case.url
    );

    // Rule 2: all pages, in order; unavailable items reported separately.
    assert_eq!(a.tracks.len(), case.expected_tracks, "{}", case.url);
    assert_eq!(
        a.unavailable.len(),
        case.expected_unavailable,
        "{}",
        case.url
    );
    assert_eq!(
        a.tracks.first().map(|t| t.title.as_str()),
        Some(case.first_title)
    );
    assert_eq!(
        a.tracks.last().map(|t| t.title.as_str()),
        Some(case.last_title)
    );

    // Rule 3: title and artist invariants.
    for t in &a.tracks {
        assert!(!t.title.trim().is_empty(), "empty title in {}", case.url);
        assert!(
            !t.artists.is_empty() && t.artists.iter().all(|x| !x.trim().is_empty()),
            "{t:?}"
        );
        assert_eq!(
            t.youtube_video_id.is_some(),
            adapter.capabilities().provides_audio,
            "{t:?}"
        );
    }

    // Rule 4: deterministic.
    let b = adapter.fetch_collection(&r, ctx).await.unwrap();
    assert_eq!(a, b, "rule 4: output must be deterministic");
}

/// Expands to `#[tokio::test]` functions for rules 1–4 and 6.
/// `$setup` is an async block returning `(Arc<dyn PlatformAdapter>, AdapterCtx, impl Fn() -> usize, Guard)`;
/// the guard keeps fixture servers alive for the test's duration.
#[macro_export]
macro_rules! adapter_contract_suite {
    ($setup:expr, cases: [$($case:expr),* $(,)?]) => {
        #[test]
        fn contract_parse_url_accepts_only_own_urls() {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let (adapter, _ctx, _io, _guard) = $setup.await;
                $crate::common::contract::check_parse_url(adapter.as_ref());
            });
        }

        #[tokio::test]
        async fn contract_collections() {
            let (adapter, ctx, io, _guard) = $setup.await;
            for case in [$($case),*] {
                $crate::common::contract::check_collection(adapter.clone(), &ctx, &case, &io).await;
            }
        }
    };
}
