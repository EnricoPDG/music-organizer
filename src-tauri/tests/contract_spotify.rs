//! T035: Spotify adapter contract (fixtures: tests/fixtures/spotify). Covers the February 2026
//! Web API (`items.items[].item`), the metadata-only response for playlists the user doesn't own,
//! the public embed-page fallback, and rate limiting.

use crate::common::contract::{CollectionCase, check_collection};
use crate::common::{ctx, fixture_str, test_net};
use music_organizer_lib::core::model::{FetchMode, ResourceKind};
use music_organizer_lib::net::Net;
use music_organizer_lib::platforms::spotify::SpotifyAdapter;
use music_organizer_lib::platforms::{AdapterCtx, AdapterError, PlatformAdapter};
use music_organizer_lib::secrets::Secret;
use music_organizer_lib::tools::fake::FakeToolRunner;
use std::sync::Arc;
use url::Url;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const OWNED: &str = "https://open.spotify.com/playlist/6wnPbEqh4bIBGwdTcfDdMU";
const FOREIGN: &str = "https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M?si=abc";

/// One mock server plays both api.spotify.com and open.spotify.com (embed pages).
async fn server() -> MockServer {
    let s = MockServer::start().await;
    let base = s.uri();
    let body = |f: &str| {
        ResponseTemplate::new(200).set_body_string(fixture_str(f).replace("{BASE}", &base))
    };
    let html = |f: &str| {
        ResponseTemplate::new(200)
            .set_body_string(fixture_str(f))
            .insert_header("content-type", "text/html")
    };

    Mock::given(method("GET"))
        .and(path("/v1/playlists/6wnPbEqh4bIBGwdTcfDdMU/items"))
        .and(query_param("offset", "3"))
        .respond_with(body("spotify/playlist_owned_items_page2.json"))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/playlists/6wnPbEqh4bIBGwdTcfDdMU"))
        .and(header("authorization", "Bearer tok"))
        .respond_with(body("spotify/playlist_owned.json"))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/playlists/37i9dQZF1DXcBWIGoYBM5M"))
        .respond_with(body("spotify/playlist_foreign_meta.json"))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/tracks/0VjIjW4GlUZAMYd2vXMi3b"))
        .respond_with(body("spotify/track.json"))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/albums/4yP0hdKOZPNshxUOjY0cZj"))
        .respond_with(body("spotify/album.json"))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/me/tracks"))
        .respond_with(body("spotify/me_tracks.json"))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/me/playlists"))
        .respond_with(body("spotify/me_playlists.json"))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/embed/playlist/37i9dQZF1DXcBWIGoYBM5M"))
        .respond_with(html("spotify/embed_playlist.html"))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/embed/playlist/changedLayout000000000"))
        .respond_with(html("spotify/embed_changed.html"))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/playlists/ratelimited00000000000"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "120")
                .set_body_string(fixture_str("spotify/rate_limited.json")),
        )
        .mount(&s)
        .await;
    s
}

fn adapter(s: &MockServer) -> SpotifyAdapter {
    SpotifyAdapter::with_bases(&s.uri(), &s.uri())
}

fn signed_in(net: Arc<Net>) -> AdapterCtx {
    let mut c = ctx(net, Arc::new(FakeToolRunner::new()));
    c.token = Some(Secret::new("tok".into()));
    c
}

async fn setup() -> (
    Arc<dyn PlatformAdapter>,
    AdapterCtx,
    impl Fn() -> usize,
    MockServer,
) {
    let s = server().await;
    let net = test_net();
    let counter = net.clone();
    let ctx = ctx(net, Arc::new(FakeToolRunner::new()));
    (
        Arc::new(adapter(&s)) as Arc<dyn PlatformAdapter>,
        ctx,
        move || counter.request_count(),
        s,
    )
}

// Signed out: public links read the embed page.
crate::adapter_contract_suite!(
    setup(),
    cases: [CollectionCase {
        url: FOREIGN,
        kind: ResourceKind::Playlist,
        expected_tracks: 3,
        expected_unavailable: 1,
        first_title: "Blinding Lights",
        last_title: "Despacito",
    }]
);

#[tokio::test]
async fn contract_owned_playlist_signed_in_pages_through_items() {
    let s = server().await;
    let net = test_net();
    let counter = net.clone();
    let c = signed_in(net);
    let case = CollectionCase {
        url: OWNED,
        kind: ResourceKind::Playlist,
        expected_tracks: 5,
        expected_unavailable: 3,
        first_title: "Bohemian Rhapsody - Remastered 2011",
        last_title: "Billie Jean",
    };
    check_collection(Arc::new(adapter(&s)), &c, &case, &move || {
        counter.request_count()
    })
    .await;
}

#[tokio::test]
async fn owned_playlist_is_official_with_isrc_and_artwork() {
    let s = server().await;
    let a = adapter(&s);
    let r = a.parse_url(&Url::parse(OWNED).unwrap()).unwrap();
    let col = a
        .fetch_collection(&r, &signed_in(test_net()))
        .await
        .unwrap();
    assert_eq!(col.fetch_mode, FetchMode::Official);
    assert_eq!(col.owner_name.as_deref(), Some("Nathalia S."));
    let t = &col.tracks[0];
    assert_eq!(t.isrc.as_deref(), Some("GBUM71029604"));
    assert_eq!(
        t.album.as_deref(),
        Some("A Night At The Opera (2011 Remaster)")
    );
    assert_eq!(t.track_number, Some(11));
    assert!(
        t.artwork_url.as_ref().unwrap().as_str().ends_with("640"),
        "largest artwork"
    );
    let reasons: Vec<_> = col.unavailable.iter().map(|u| u.reason.as_str()).collect();
    assert!(reasons.iter().any(|r| r.contains("removed")), "{reasons:?}");
    assert!(reasons.iter().any(|r| r.contains("Podcast")), "{reasons:?}");
    assert!(
        reasons.iter().any(|r| r.contains("Local file")),
        "{reasons:?}"
    );
}

#[tokio::test]
async fn foreign_playlist_signed_in_falls_back_to_embed() {
    let s = server().await;
    let a = adapter(&s);
    let r = a.parse_url(&Url::parse(FOREIGN).unwrap()).unwrap();
    let col = a
        .fetch_collection(&r, &signed_in(test_net()))
        .await
        .unwrap();
    assert_eq!(col.fetch_mode, FetchMode::PublicFallback);
    assert_eq!(col.name, "Today's Top Hits");
    assert_eq!(
        col.tracks[2].artists,
        vec!["Luis Fonsi".to_string(), "Daddy Yankee".to_string()]
    );
    assert_eq!(col.tracks[0].duration_ms, Some(200_040));
}

#[tokio::test]
async fn changed_embed_page_reports_public_source_changed() {
    let s = server().await;
    let a = adapter(&s);
    let r = a
        .parse_url(&Url::parse("https://open.spotify.com/playlist/changedLayout000000000").unwrap())
        .unwrap();
    let err = a
        .fetch_collection(&r, &ctx(test_net(), Arc::new(FakeToolRunner::new())))
        .await
        .unwrap_err();
    assert!(matches!(err, AdapterError::PublicSourceChanged), "{err:?}");
}

#[tokio::test]
async fn long_rate_limit_is_returned_with_retry_after() {
    let s = server().await;
    let a = adapter(&s);
    let r = a
        .parse_url(&Url::parse("https://open.spotify.com/playlist/ratelimited00000000000").unwrap())
        .unwrap();
    let err = a
        .fetch_collection(&r, &signed_in(test_net()))
        .await
        .unwrap_err();
    match err {
        AdapterError::RateLimited { retry_after } => {
            assert_eq!(retry_after.unwrap().as_secs(), 120)
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn single_track_album_and_liked_songs() {
    let s = server().await;
    let a = adapter(&s);
    let c = signed_in(test_net());

    let r = a
        .parse_url(
            &Url::parse("https://open.spotify.com/intl-pt/track/0VjIjW4GlUZAMYd2vXMi3b").unwrap(),
        )
        .unwrap();
    let t = a.fetch_track(&r, &c).await.unwrap();
    assert_eq!(
        (t.title.as_str(), t.isrc.as_deref()),
        ("Blinding Lights", Some("USUG11904206"))
    );

    let r = a
        .parse_url(&Url::parse("https://open.spotify.com/album/4yP0hdKOZPNshxUOjY0cZj").unwrap())
        .unwrap();
    let album = a.fetch_collection(&r, &c).await.unwrap();
    assert_eq!(album.tracks.len(), 2);
    assert!(
        album
            .tracks
            .iter()
            .all(|t| t.album.as_deref() == Some("After Hours")),
        "album name filled in"
    );
    assert_eq!(album.tracks[1].track_number, Some(9));

    let r = a
        .parse_url(&Url::parse("https://open.spotify.com/collection/tracks").unwrap())
        .unwrap();
    assert_eq!(r.kind, ResourceKind::UserLikes);
    let liked = a.fetch_collection(&r, &c).await.unwrap();
    assert_eq!(liked.name, "Liked Songs");
    assert_eq!(liked.tracks.len(), 2);

    let mine = a.list_my_collections(&c).await.unwrap();
    assert_eq!(mine[0].kind, ResourceKind::UserLikes);
    assert_eq!(mine[1].name, "Road Trip Classics");
}

#[tokio::test]
async fn liked_songs_require_sign_in() {
    let s = server().await;
    let a = adapter(&s);
    let r = a
        .parse_url(&Url::parse("https://open.spotify.com/collection/tracks").unwrap())
        .unwrap();
    let err = a
        .fetch_collection(&r, &ctx(test_net(), Arc::new(FakeToolRunner::new())))
        .await
        .unwrap_err();
    assert!(matches!(err, AdapterError::Private { can_sign_in: true }));
}

#[test]
fn parse_url_forms() {
    let a = SpotifyAdapter::new();
    let p = |u: &str| a.parse_url(&Url::parse(u).unwrap()).map(|r| (r.kind, r.id));
    assert_eq!(
        p("https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M?si=1"),
        Some((ResourceKind::Playlist, "37i9dQZF1DXcBWIGoYBM5M".into()))
    );
    assert_eq!(
        p("https://open.spotify.com/intl-de/album/4yP0hdKOZPNshxUOjY0cZj"),
        Some((ResourceKind::Album, "4yP0hdKOZPNshxUOjY0cZj".into()))
    );
    assert_eq!(
        p("https://open.spotify.com/track/0VjIjW4GlUZAMYd2vXMi3b"),
        Some((ResourceKind::Track, "0VjIjW4GlUZAMYd2vXMi3b".into()))
    );
    assert_eq!(
        p("https://open.spotify.com/artist/1dfeR4HaWDbWqFHLkxsg1d"),
        None
    );
    assert_eq!(p("https://open.spotify.com/playlist/bad id"), None);
}

#[test]
fn auth_uses_pkce_without_client_secret() {
    use music_organizer_lib::platforms::ClientCredentials;
    let cfg = SpotifyAdapter::new()
        .auth_config(&ClientCredentials {
            client_id: "abc".into(),
            client_secret: None,
        })
        .unwrap();
    assert!(cfg.scopes.contains(&"playlist-read-private".to_string()));
    assert!(cfg.scopes.contains(&"user-library-read".to_string()));
}
