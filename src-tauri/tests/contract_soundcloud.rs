//! T036: SoundCloud adapter contract (fixtures: tests/fixtures/soundcloud).

use crate::common::contract::CollectionCase;
use crate::common::{ctx, fixture, fixture_str, test_net};
use music_organizer_lib::core::model::ResourceKind;
use music_organizer_lib::platforms::soundcloud::SoundcloudAdapter;
use music_organizer_lib::platforms::{AdapterCtx, PlatformAdapter};
use music_organizer_lib::secrets::Secret;
use music_organizer_lib::tools::fake::FakeToolRunner;
use music_organizer_lib::tools::{Tool, ToolRunner};
use std::sync::Arc;
use url::Url;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn setup() -> (Arc<dyn PlatformAdapter>, AdapterCtx, impl Fn() -> usize, ()) {
    let tools = Arc::new(
        FakeToolRunner::new()
            .on(
                Tool::YtDlp,
                "https://soundcloud.com/forss/sets/soulhack",
                fixture("soundcloud/set.json"),
            )
            .on(
                Tool::YtDlp,
                "https://on.soundcloud.com/AbC123",
                fixture("soundcloud/track.json"),
            )
            .on(
                Tool::YtDlp,
                "https://soundcloud.com/forss/flickermood",
                fixture("soundcloud/track.json"),
            ),
    );
    let counter = tools.clone();
    let ctx = ctx(test_net(), tools as Arc<dyn ToolRunner>);
    (
        Arc::new(SoundcloudAdapter::new()) as Arc<dyn PlatformAdapter>,
        ctx,
        move || counter.call_count(),
        (),
    )
}

crate::adapter_contract_suite!(
    setup(),
    cases: [
        CollectionCase {
            url: "https://soundcloud.com/forss/sets/soulhack",
            kind: ResourceKind::Playlist,
            expected_tracks: 4,
            expected_unavailable: 1,
            first_title: "Flickermood",
            last_title: "Pegasus",
        },
        // Short links resolve through yt-dlp; here it points at a single track.
        CollectionCase {
            url: "https://on.soundcloud.com/AbC123",
            kind: ResourceKind::Track,
            expected_tracks: 1,
            expected_unavailable: 0,
            first_title: "Flickermood",
            last_title: "Flickermood",
        },
    ]
);

#[tokio::test]
async fn splits_artist_from_title_and_drops_preview_duration() {
    let (a, c, _, _) = setup().await;
    let r = a
        .parse_url(&Url::parse("https://soundcloud.com/forss/sets/soulhack").unwrap())
        .unwrap();
    let col = a.fetch_collection(&r, &c).await.unwrap();
    assert_eq!(col.tracks[0].album.as_deref(), Some("Soulhack"));
    assert_eq!(col.tracks[1].artists, vec!["Forss".to_string()]);
    assert_eq!(col.tracks[1].title, "Journeyman");
    assert_eq!(col.tracks[0].duration_ms, Some(213_400));
    // Go+ previews report 30 s, which would break matching: the duration is treated as unknown.
    assert_eq!(col.tracks[3].duration_ms, None);
}

#[test]
fn parse_url_forms() {
    let a = SoundcloudAdapter::new();
    let p = |u: &str| a.parse_url(&Url::parse(u).unwrap()).map(|r| r.kind);
    assert_eq!(
        p("https://soundcloud.com/forss/sets/soulhack"),
        Some(ResourceKind::Playlist)
    );
    assert_eq!(
        p("https://m.soundcloud.com/forss/flickermood"),
        Some(ResourceKind::Track)
    );
    assert_eq!(
        p("https://soundcloud.com/forss/flickermood?in=forss/sets/soulhack"),
        Some(ResourceKind::Track)
    );
    assert_eq!(
        p("https://on.soundcloud.com/AbC123"),
        Some(ResourceKind::Track)
    );
    assert_eq!(
        p("https://soundcloud.com/you/likes"),
        Some(ResourceKind::UserLikes)
    );
    assert_eq!(p("https://soundcloud.com/forss"), None);
    assert_eq!(p("https://soundcloud.com/discover"), None);
}

#[tokio::test]
async fn signed_in_likes_and_playlists_use_api() {
    let s = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/me/likes/tracks"))
        .and(header("authorization", "OAuth tok"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(fixture_str("soundcloud/api_me_likes.json")),
        )
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/me/playlists"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture_str("soundcloud/api_me_playlists.json")),
        )
        .mount(&s)
        .await;
    let a = SoundcloudAdapter::with_api_base(&s.uri());
    let mut c = ctx(test_net(), Arc::new(FakeToolRunner::new()));
    c.token = Some(Secret::new("tok".into()));

    let r = a
        .parse_url(&Url::parse("https://soundcloud.com/you/likes").unwrap())
        .unwrap();
    let col = a.fetch_collection(&r, &c).await.unwrap();
    assert_eq!(col.tracks.len(), 2);
    assert_eq!(col.tracks[0].isrc.as_deref(), Some("USUM71900001"));
    assert_eq!(col.tracks[0].duration_ms, Some(245_000));

    let mine = a.list_my_collections(&c).await.unwrap();
    assert_eq!(mine.len(), 2);
    assert_eq!(mine[1].track_count, Some(14));
}
