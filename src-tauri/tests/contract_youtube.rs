//! T034: YouTube adapter contract (fixtures: tests/fixtures/youtube).

use crate::common::contract::CollectionCase;
use crate::common::{ctx, fixture, fixture_str, test_net};
use music_organizer_lib::core::model::ResourceKind;
use music_organizer_lib::platforms::youtube::YoutubeAdapter;
use music_organizer_lib::platforms::{AdapterCtx, AdapterError, PlatformAdapter};
use music_organizer_lib::secrets::Secret;
use music_organizer_lib::tools::fake::FakeToolRunner;
use music_organizer_lib::tools::{Tool, ToolError, ToolRunner};
use std::sync::Arc;
use url::Url;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PLAYLIST: &str = "https://www.youtube.com/playlist?list=PLFgquLnL59alCl_2TQvOiD5Vgm1hCaGSI";

fn tools() -> FakeToolRunner {
    FakeToolRunner::new()
        .on(Tool::YtDlp, "--flat-playlist https://www.youtube.com/playlist?list=PLFgquLnL59alCl_2TQvOiD5Vgm1hCaGSI", fixture("youtube/playlist.json"))
        .on(Tool::YtDlp, "watch?v=dQw4w9WgXcQ", fixture("youtube/video.json"))
        .on(Tool::YtDlp, "watch?v=lYBUbBu4W08", fixture("youtube/music_video.json"))
        .on_fail(Tool::YtDlp, "PLprivate", ToolError::Failed { code: Some(1), stderr: "ERROR: [youtube:tab] PLprivate: This playlist is private".into() })
}

async fn setup() -> (Arc<dyn PlatformAdapter>, AdapterCtx, impl Fn() -> usize, ()) {
    let tools = Arc::new(tools());
    let counter = tools.clone();
    let ctx = ctx(test_net(), tools as Arc<dyn ToolRunner>);
    (
        Arc::new(YoutubeAdapter::new()) as Arc<dyn PlatformAdapter>,
        ctx,
        move || counter.call_count(),
        (),
    )
}

crate::adapter_contract_suite!(
    setup(),
    cases: [
        CollectionCase {
            url: PLAYLIST,
            kind: ResourceKind::Playlist,
            expected_tracks: 5,
            expected_unavailable: 2,
            first_title: "Bohemian Rhapsody",
            last_title: "Billie Jean",
        },
        // A single video is a one-track collection.
        CollectionCase {
            url: "https://youtu.be/dQw4w9WgXcQ",
            kind: ResourceKind::Track,
            expected_tracks: 1,
            expected_unavailable: 0,
            first_title: "Never Gonna Give You Up",
            last_title: "Never Gonna Give You Up",
        },
    ]
);

fn parse(url: &str) -> Option<(ResourceKind, String)> {
    YoutubeAdapter::new()
        .parse_url(&Url::parse(url).unwrap())
        .map(|r| (r.kind, r.id))
}

#[test]
fn parses_common_url_forms() {
    let v = Some((ResourceKind::Track, "dQw4w9WgXcQ".to_string()));
    assert_eq!(
        parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ&si=abc"),
        v
    );
    assert_eq!(parse("https://m.youtube.com/watch?v=dQw4w9WgXcQ"), v);
    assert_eq!(parse("https://youtu.be/dQw4w9WgXcQ?si=xyz"), v);
    assert_eq!(parse("https://www.youtube.com/shorts/dQw4w9WgXcQ"), v);
    assert_eq!(parse("https://music.youtube.com/watch?v=dQw4w9WgXcQ"), v);
    // A video inside a playlist imports the playlist; radio mixes (RD…) import just the video.
    assert_eq!(
        parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=PLabc"),
        Some((ResourceKind::Playlist, "PLabc".into()))
    );
    assert_eq!(
        parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=RDdQw4w9WgXcQ"),
        v
    );
    assert_eq!(
        parse("https://music.youtube.com/playlist?list=OLAK5uy_abc"),
        Some((ResourceKind::Playlist, "OLAK5uy_abc".into()))
    );
    assert_eq!(
        parse("https://www.youtube.com/playlist?list=LL"),
        Some((ResourceKind::UserLikes, "LL".into()))
    );
    assert_eq!(parse("https://www.youtube.com/@queenofficial"), None);
    assert_eq!(parse("https://www.youtube.com/watch?v=short"), None);
}

#[tokio::test]
async fn tracks_carry_video_ids_and_clean_artist_title() {
    let (adapter, ctx, _, _) = setup().await;
    let r = adapter.parse_url(&Url::parse(PLAYLIST).unwrap()).unwrap();
    let c = adapter.fetch_collection(&r, &ctx).await.unwrap();
    assert_eq!(c.name, "Road Trip Classics");
    let t = &c.tracks[1];
    assert_eq!(
        (t.artists.as_slice(), t.title.as_str()),
        (&["Survivor".to_string()][..], "Eye Of The Tiger")
    );
    assert_eq!(t.youtube_video_id.as_deref(), Some("btPJPFnesV4"));
    assert_eq!(t.duration_ms, Some(246_000));
    // "- Topic" channels name the artist.
    assert_eq!(c.tracks[2].artists, vec!["Guns N' Roses".to_string()]);
    assert_eq!(c.unavailable[0].reason, "Private video");
}

#[tokio::test]
async fn music_youtube_metadata_is_used() {
    let (adapter, ctx, _, _) = setup().await;
    let r = adapter
        .parse_url(&Url::parse("https://music.youtube.com/watch?v=lYBUbBu4W08").unwrap())
        .unwrap();
    let t = adapter.fetch_track(&r, &ctx).await.unwrap();
    assert_eq!(t.title, "Blinding Lights");
    assert_eq!(t.artists, vec!["The Weeknd".to_string()]);
    assert_eq!(t.album.as_deref(), Some("After Hours"));
}

#[tokio::test]
async fn private_playlist_without_sign_in_suggests_signing_in() {
    let (adapter, ctx, _, _) = setup().await;
    let r = adapter
        .parse_url(&Url::parse("https://www.youtube.com/playlist?list=PLprivate").unwrap())
        .unwrap();
    let err = adapter.fetch_collection(&r, &ctx).await.unwrap_err();
    assert!(
        matches!(err, AdapterError::Private { can_sign_in: true }),
        "{err:?}"
    );
}

#[tokio::test]
async fn signed_in_liked_videos_use_data_api_with_paging() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/playlistItems"))
        .and(query_param("playlistId", "LL"))
        .and(query_param("pageToken", "PAGE2"))
        .and(header("authorization", "Bearer tok"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(fixture_str("youtube/api_liked_page2.json")),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/playlistItems"))
        .and(query_param("playlistId", "LL"))
        .and(header("authorization", "Bearer tok"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(fixture_str("youtube/api_liked_page1.json")),
        )
        .mount(&server)
        .await;

    let adapter = YoutubeAdapter::with_api_base(&server.uri());
    let mut c = ctx(test_net(), Arc::new(FakeToolRunner::new()));
    c.token = Some(Secret::new("tok".into()));
    let r = adapter
        .parse_url(&Url::parse("https://www.youtube.com/playlist?list=LL").unwrap())
        .unwrap();
    let col = adapter.fetch_collection(&r, &c).await.unwrap();
    let titles: Vec<_> = col.tracks.iter().map(|t| t.title.as_str()).collect();
    assert_eq!(
        titles,
        ["Bohemian Rhapsody", "Eye Of The Tiger", "Billie Jean"]
    );
    assert_eq!(col.name, "Liked videos");
}

#[tokio::test]
async fn liked_videos_require_sign_in() {
    let (adapter, ctx, _, _) = setup().await;
    let r = adapter
        .parse_url(&Url::parse("https://www.youtube.com/playlist?list=LL").unwrap())
        .unwrap();
    let err = adapter.fetch_collection(&r, &ctx).await.unwrap_err();
    assert!(matches!(err, AdapterError::Private { can_sign_in: true }));
}

#[tokio::test]
async fn lists_my_playlists_and_liked() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/playlists"))
        .and(query_param("mine", "true"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fixture_str("youtube/api_my_playlists.json")),
        )
        .mount(&server)
        .await;
    let adapter = YoutubeAdapter::with_api_base(&server.uri());
    let mut c = ctx(test_net(), Arc::new(FakeToolRunner::new()));
    c.token = Some(Secret::new("tok".into()));
    let mine = adapter.list_my_collections(&c).await.unwrap();
    assert_eq!(mine[0].kind, ResourceKind::UserLikes);
    assert_eq!(mine[1].name, "Gym");
    assert_eq!(mine[1].track_count, Some(12));
}
