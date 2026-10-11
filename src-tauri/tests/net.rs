//! T012: retries, backoff, Retry-After and rate-limit errors (research R12), against a local mock server.

use music_organizer_lib::net::{Net, NetError, RetryPolicy};
use std::time::{Duration, Instant};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn fast_policy() -> RetryPolicy {
    RetryPolicy {
        base: Duration::from_millis(10),
        max_attempts: 5,
        timeout: Duration::from_millis(500),
    }
}

#[tokio::test]
async fn retries_server_errors_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/x"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/x"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;

    let net = Net::new(fast_policy());
    let url = format!("{}/x", server.uri());
    let resp = net.send("test", |c| c.get(&url)).await.unwrap();
    assert_eq!(resp.text().await.unwrap(), "ok");
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
}

#[tokio::test]
async fn gives_up_after_max_attempts() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let net = Net::new(fast_policy());
    let url = format!("{}/x", server.uri());
    let err = net.send("test", |c| c.get(&url)).await.unwrap_err();
    assert!(matches!(err, NetError::Status { code: 500, .. }), "{err:?}");
    assert_eq!(server.received_requests().await.unwrap().len(), 5);
}

#[tokio::test]
async fn does_not_retry_client_errors() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let net = Net::new(fast_policy());
    let url = format!("{}/x", server.uri());
    let err = net.send("test", |c| c.get(&url)).await.unwrap_err();
    assert!(matches!(err, NetError::Status { code: 404, .. }));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn honors_retry_after_on_429() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/x"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "1"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/x"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let net = Net::new(fast_policy());
    let url = format!("{}/x", server.uri());
    let start = Instant::now();
    net.send("test", |c| c.get(&url)).await.unwrap();
    assert!(
        start.elapsed() >= Duration::from_millis(950),
        "waited {:?}",
        start.elapsed()
    );
}

#[tokio::test]
async fn persistent_429_becomes_rate_limited_including_quota_exceeded() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(429)
                .set_body_string(r#"{"error":{"status":429,"reason":"QUOTA_EXCEEDED"}}"#),
        )
        .mount(&server)
        .await;

    let net = Net::new(fast_policy());
    let url = format!("{}/x", server.uri());
    let err = net.send("test", |c| c.get(&url)).await.unwrap_err();
    assert!(matches!(err, NetError::RateLimited { .. }), "{err:?}");
}

#[tokio::test]
async fn timeout_gives_network_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(5)))
        .mount(&server)
        .await;

    let policy = RetryPolicy {
        max_attempts: 1,
        timeout: Duration::from_millis(100),
        ..fast_policy()
    };
    let net = Net::new(policy);
    let url = format!("{}/x", server.uri());
    let err = net.send("test", |c| c.get(&url)).await.unwrap_err();
    assert!(matches!(err, NetError::Network(_)), "{err:?}");
}
