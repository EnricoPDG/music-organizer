//! T043: OAuth loopback flow with PKCE (research R3a, R6).

use music_organizer_lib::net::oauth::{
    OAuthError, StoredTokens, authorize_url, bind_loopback, exchange_code, pkce, wait_for_code,
};
use music_organizer_lib::platforms::{ClientCredentials, OAuthConfig};
use music_organizer_lib::secrets::{MemoryStore, Secret, SecretStore};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use url::Url;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn cfg(token_url: &str) -> OAuthConfig {
    OAuthConfig {
        authorize_url: Url::parse("https://accounts.example.com/authorize").unwrap(),
        token_url: Url::parse(token_url).unwrap(),
        scopes: vec!["read".into(), "library".into()],
        extra_params: vec![("access_type".into(), "offline".into())],
    }
}

#[test]
fn pkce_verifier_and_s256_challenge() {
    use base64::Engine;
    use sha2::Digest;
    let p = pkce();
    let v = p.verifier.expose();
    assert!((43..=128).contains(&v.len()));
    assert!(
        v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
    );
    let expected =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(v.as_bytes()));
    assert_eq!(p.challenge, expected);
    assert_ne!(pkce().verifier.expose(), v, "random per flow");
}

#[test]
fn authorize_url_has_pkce_state_and_scopes() {
    let u = authorize_url(
        &cfg("https://t.example.com/token"),
        "client123",
        "http://127.0.0.1:5555/callback",
        "st4te",
        "chal",
    );
    let q: std::collections::HashMap<_, _> = u.query_pairs().into_owned().collect();
    assert_eq!(q["response_type"], "code");
    assert_eq!(q["client_id"], "client123");
    assert_eq!(q["redirect_uri"], "http://127.0.0.1:5555/callback");
    assert_eq!(q["code_challenge_method"], "S256");
    assert_eq!(q["code_challenge"], "chal");
    assert_eq!(q["state"], "st4te");
    assert_eq!(q["scope"], "read library");
    assert_eq!(q["access_type"], "offline");
}

async fn hit(port: u16, path: &str) -> String {
    let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    s.write_all(format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let mut buf = String::new();
    let _ = s.read_to_string(&mut buf).await;
    buf
}

#[tokio::test]
async fn listener_binds_loopback_and_returns_code() {
    let (listener, port) = bind_loopback().await.unwrap();
    assert_eq!(listener.local_addr().unwrap().ip().to_string(), "127.0.0.1");
    assert_ne!(port, 0);
    let waiter = tokio::spawn(wait_for_code(
        listener,
        "good-state".into(),
        Duration::from_secs(5),
    ));
    // Browsers also ask for a favicon; it must be ignored.
    hit(port, "/favicon.ico").await;
    let page = hit(port, "/callback?code=abc123&state=good-state").await;
    assert!(page.contains("200"), "{page}");
    assert!(page.to_lowercase().contains("close this window"));
    assert_eq!(waiter.await.unwrap().unwrap().expose(), "abc123");
}

#[tokio::test]
async fn state_mismatch_is_rejected() {
    let (listener, port) = bind_loopback().await.unwrap();
    let waiter = tokio::spawn(wait_for_code(
        listener,
        "expected".into(),
        Duration::from_secs(5),
    ));
    hit(port, "/callback?code=abc&state=forged").await;
    assert!(matches!(
        waiter.await.unwrap(),
        Err(OAuthError::StateMismatch)
    ));
}

#[tokio::test]
async fn user_denial_is_reported() {
    let (listener, port) = bind_loopback().await.unwrap();
    let waiter = tokio::spawn(wait_for_code(listener, "s".into(), Duration::from_secs(5)));
    hit(port, "/callback?error=access_denied&state=s").await;
    assert!(matches!(waiter.await.unwrap(), Err(OAuthError::Denied)));
}

#[tokio::test]
async fn times_out() {
    let (listener, _port) = bind_loopback().await.unwrap();
    let r = wait_for_code(listener, "s".into(), Duration::from_millis(100)).await;
    assert!(matches!(r, Err(OAuthError::TimedOut)));
}

#[tokio::test]
async fn exchanges_code_with_verifier_and_stores_tokens_only_in_the_secret_store() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/token"))
        .and(body_string_contains("grant_type=authorization_code"))
        .and(body_string_contains("code=abc123"))
        .and(body_string_contains("code_verifier="))
        .and(body_string_contains("client_id=client123"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"access_token":"AT-1","refresh_token":"RT-1","expires_in":3600,"token_type":"Bearer","scope":"read"}"#,
        ))
        .mount(&server).await;
    let net = crate::common::test_net();
    let p = pkce();
    let client = ClientCredentials {
        client_id: "client123".into(),
        client_secret: None,
    };
    let tokens = exchange_code(
        &net,
        &cfg(&format!("{}/token", server.uri())),
        &client,
        &Secret::new("abc123".into()),
        &p.verifier,
        "http://127.0.0.1:1/callback",
    )
    .await
    .unwrap();
    assert_eq!(tokens.access_token.expose(), "AT-1");
    let body =
        String::from_utf8(server.received_requests().await.unwrap()[0].body.clone()).unwrap();
    assert!(
        !body.contains("client_secret"),
        "PKCE public client sends no secret"
    );

    let store = MemoryStore::default();
    StoredTokens::from_set(&tokens, None)
        .save(&store, "account:spotify")
        .unwrap();
    let raw = store.load("account:spotify").unwrap().unwrap();
    assert!(raw.expose().contains("AT-1"));
    let loaded = StoredTokens::load(&store, "account:spotify")
        .unwrap()
        .unwrap();
    assert_eq!(loaded.refresh_token.as_deref(), Some("RT-1"));
    // Debug output of the token set never shows values.
    assert!(!format!("{tokens:?}").contains("AT-1"));
}

#[tokio::test]
async fn confidential_clients_send_their_secret() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("client_secret=s3cr3t"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#"{"access_token":"A","expires_in":60}"#),
        )
        .mount(&server)
        .await;
    let client = ClientCredentials {
        client_id: "c".into(),
        client_secret: Some(Secret::new("s3cr3t".into())),
    };
    let t = exchange_code(
        &crate::common::test_net(),
        &cfg(&format!("{}/token", server.uri())),
        &client,
        &Secret::new("x".into()),
        &pkce().verifier,
        "http://127.0.0.1:1/callback",
    )
    .await;
    assert!(t.is_ok(), "{t:?}");
}
