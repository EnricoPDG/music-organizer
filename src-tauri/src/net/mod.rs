//! Shared HTTP client with per-platform rate limiting and retries (research R12, constitution
//! Principle IV). Adapters only talk to the network through [`Net::send`].

pub mod oauth;

use crate::error::{AppError, ErrorKind};
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Longest backoff between attempts.
const MAX_BACKOFF: Duration = Duration::from_secs(30);
/// A `Retry-After` longer than this is returned to the caller instead of waited out here, so
/// the download queue can pause the job and keep the rest of the batch moving.
const MAX_INLINE_RETRY_AFTER: Duration = Duration::from_secs(60);
/// Default request rate per platform.
const DEFAULT_PER_SECOND: u32 = 5;
const USER_AGENT: &str = concat!("MusicOrganizer/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// First backoff; doubles on each retry, plus random jitter of up to one `base`.
    pub base: Duration,
    /// Total attempts including the first.
    pub max_attempts: u32,
    /// Per-request timeout.
    pub timeout: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            base: Duration::from_secs(1),
            max_attempts: 5,
            timeout: Duration::from_secs(30),
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum NetError {
    #[error("rate limited (retry after {retry_after:?})")]
    RateLimited { retry_after: Option<Duration> },
    #[error("HTTP {code}")]
    Status { code: u16, body: String },
    #[error("network error: {0}")]
    Network(String),
}

impl From<NetError> for AppError {
    fn from(e: NetError) -> Self {
        match e {
            NetError::RateLimited { .. } => AppError::new(
                ErrorKind::RateLimited,
                "The platform is limiting requests. The app will slow down and retry.",
            ),
            NetError::Network(detail) => {
                tracing::warn!(%detail, "network error");
                AppError::new(
                    ErrorKind::Network,
                    "Network problem. Check your connection and retry.",
                )
            }
            NetError::Status { code: 404, .. } => AppError::not_found("Not found on the platform."),
            NetError::Status { code, body } => {
                tracing::warn!(code, body = %truncate(&body, 500), "unexpected HTTP status");
                AppError::new(
                    ErrorKind::Platform,
                    format!("The platform returned an error (HTTP {code})."),
                )
            }
        }
    }
}

pub struct Net {
    client: Client,
    policy: RetryPolicy,
    limiters: Mutex<HashMap<String, Arc<DefaultDirectRateLimiter>>>,
    requests: AtomicUsize,
}

impl Net {
    pub fn new(policy: RetryPolicy) -> Self {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("HTTP client configuration is valid");
        Self {
            client,
            policy,
            limiters: Mutex::new(HashMap::new()),
            requests: AtomicUsize::new(0),
        }
    }

    /// Override the request rate for one platform.
    pub fn set_rate(&self, platform: &str, per_second: u32) {
        let quota = Quota::per_second(NonZeroU32::new(per_second.max(1)).expect("non-zero"));
        self.limiters
            .lock()
            .expect("limiter lock")
            .insert(platform.to_string(), Arc::new(RateLimiter::direct(quota)));
    }

    fn limiter(&self, platform: &str) -> Arc<DefaultDirectRateLimiter> {
        let mut map = self.limiters.lock().expect("limiter lock");
        map.entry(platform.to_string())
            .or_insert_with(|| {
                let quota =
                    Quota::per_second(NonZeroU32::new(DEFAULT_PER_SECOND).expect("non-zero"));
                Arc::new(RateLimiter::direct(quota))
            })
            .clone()
    }

    /// Requests sent so far, counting each retry (lets tests prove I/O went through here).
    pub fn request_count(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Send a request built by `build`, retrying network errors, 5xx and 429 responses.
    /// `build` is called once per attempt because a request can't be reused after sending.
    pub async fn send<F>(&self, platform: &str, build: F) -> Result<Response, NetError>
    where
        F: Fn(&Client) -> RequestBuilder,
    {
        let limiter = self.limiter(platform);
        let mut attempt = 1;
        loop {
            limiter.until_ready().await;
            let last = attempt >= self.policy.max_attempts;
            self.requests.fetch_add(1, Ordering::SeqCst);
            let result = build(&self.client)
                .timeout(self.policy.timeout)
                .send()
                .await;

            let wait = match result {
                Ok(resp) if resp.status().is_success() => return Ok(resp),
                Ok(resp) if resp.status() == StatusCode::TOO_MANY_REQUESTS => {
                    let retry_after = parse_retry_after(&resp);
                    tracing::info!(platform, ?retry_after, "rate limited");
                    if last || retry_after.is_some_and(|d| d > MAX_INLINE_RETRY_AFTER) {
                        return Err(NetError::RateLimited { retry_after });
                    }
                    retry_after.unwrap_or_else(|| self.backoff(attempt))
                }
                Ok(resp) if resp.status().is_server_error() && !last => self.backoff(attempt),
                Ok(resp) => {
                    let code = resp.status().as_u16();
                    let body = resp.text().await.unwrap_or_default();
                    return Err(NetError::Status {
                        code,
                        body: truncate(&body, 2000).to_string(),
                    });
                }
                Err(e) => {
                    // Strip the URL: query strings may carry identifiers we don't want in logs.
                    let e = e.without_url();
                    if last {
                        return Err(NetError::Network(e.to_string()));
                    }
                    tracing::debug!(platform, attempt, error = %e, "retrying after network error");
                    self.backoff(attempt)
                }
            };
            tokio::time::sleep(wait).await;
            attempt += 1;
        }
    }

    fn backoff(&self, attempt: u32) -> Duration {
        let exp = self.policy.base.saturating_mul(1 << (attempt - 1).min(16));
        let jitter = self.policy.base.mul_f64(rand::random::<f64>());
        (exp + jitter).min(MAX_BACKOFF)
    }
}

fn parse_retry_after(resp: &Response) -> Option<Duration> {
    let value = resp
        .headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?;
    value.trim().parse::<u64>().ok().map(Duration::from_secs)
}

fn truncate(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}
