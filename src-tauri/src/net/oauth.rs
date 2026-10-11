//! OAuth 2 authorization code flow with PKCE and a loopback redirect (research R3a, R6).
//! The browser is sent to the platform; the platform redirects back to a temporary listener on
//! `127.0.0.1`. Tokens are kept only in the OS credential store.

use crate::net::{Net, NetError};
use crate::platforms::{ClientCredentials, OAuthConfig};
use crate::secrets::{Secret, SecretStore, SecretStoreError};
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

pub const CALLBACK_PATH: &str = "/callback";
pub const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// Fixed loopback port for sign-in. Some platforms require the redirect URI to match exactly,
/// so users register `http://127.0.0.1:43117/callback` in their developer app.
pub const REDIRECT_PORT: u16 = 43117;

#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
    #[error("sign-in timed out")]
    TimedOut,
    #[error("sign-in was cancelled in the browser")]
    Denied,
    #[error("the sign-in response didn't match this request")]
    StateMismatch,
    #[error("the platform rejected the sign-in: {0}")]
    Rejected(String),
    #[error("network error during sign-in: {0}")]
    Network(String),
}

impl From<OAuthError> for crate::error::AppError {
    fn from(e: OAuthError) -> Self {
        use crate::error::{AppError, ErrorKind};
        match e {
            OAuthError::TimedOut => AppError::new(
                ErrorKind::InvalidInput,
                "Sign-in timed out. Try connecting again.",
            ),
            OAuthError::Denied => AppError::new(
                ErrorKind::InvalidInput,
                "Sign-in was cancelled in the browser.",
            ),
            OAuthError::StateMismatch => AppError::new(
                ErrorKind::InvalidInput,
                "The sign-in response didn't match. Try again.",
            ),
            OAuthError::Rejected(detail) => {
                tracing::warn!(%detail, "token request rejected");
                AppError::new(
                    ErrorKind::AuthExpired,
                    "The platform rejected the sign-in. Check the client ID (and secret) and the redirect URI in your developer app.",
                )
            }
            OAuthError::Network(d) => AppError::from(NetError::Network(d)),
        }
    }
}

pub struct Pkce {
    pub verifier: Secret<String>,
    pub challenge: String,
}

fn b64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// A fresh PKCE verifier (64 random bytes → 86 URL-safe characters) and its S256 challenge.
pub fn pkce() -> Pkce {
    let bytes: [u8; 64] = rand::random();
    let verifier = b64url(&bytes);
    let challenge = b64url(&Sha256::digest(verifier.as_bytes()));
    Pkce {
        verifier: Secret::new(verifier),
        challenge,
    }
}

/// A random `state` value that ties the callback to this request.
pub fn random_state() -> String {
    b64url(&rand::random::<[u8; 24]>())
}

pub fn authorize_url(
    cfg: &OAuthConfig,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    challenge: &str,
) -> Url {
    let mut u = cfg.authorize_url.clone();
    {
        let mut q = u.query_pairs_mut();
        q.append_pair("response_type", "code")
            .append_pair("client_id", client_id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("code_challenge_method", "S256")
            .append_pair("code_challenge", challenge)
            .append_pair("state", state);
        if !cfg.scopes.is_empty() {
            q.append_pair("scope", &cfg.scopes.join(" "));
        }
        for (k, v) in &cfg.extra_params {
            q.append_pair(k, v);
        }
    }
    u
}

/// Listen on `127.0.0.1` with an OS-assigned port (tests).
pub async fn bind_loopback() -> std::io::Result<(TcpListener, u16)> {
    bind_loopback_on(0).await
}

/// Listen on `127.0.0.1:port` (`0` = any free port).
pub async fn bind_loopback_on(port: u16) -> std::io::Result<(TcpListener, u16)> {
    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    let port = listener.local_addr()?.port();
    Ok((listener, port))
}

pub fn redirect_uri(port: u16) -> String {
    format!("http://127.0.0.1:{port}{CALLBACK_PATH}")
}

const DONE_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Music Organizer</title>\
<body style=\"font-family:system-ui;padding:3rem;color:#16202e\"><h1>You're connected.</h1>\
<p>You can close this window and go back to Music Organizer.</p></body>";
const FAILED_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Music Organizer</title>\
<body style=\"font-family:system-ui;padding:3rem;color:#16202e\"><h1>Sign-in didn't complete.</h1>\
<p>You can close this window and try again in Music Organizer.</p></body>";

/// Wait for the platform's redirect and return the authorization code.
pub async fn wait_for_code(
    listener: TcpListener,
    expected_state: String,
    timeout: Duration,
) -> Result<Secret<String>, OAuthError> {
    tokio::time::timeout(timeout, async move {
        loop {
            let (mut sock, _) = listener.accept().await.map_err(|e| OAuthError::Network(e.to_string()))?;
            let mut buf = vec![0u8; 8192];
            let n = sock.read(&mut buf).await.unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]);
            let target = request.lines().next().and_then(|l| l.split_whitespace().nth(1)).unwrap_or("/");
            let Ok(url) = Url::parse(&format!("http://127.0.0.1{target}")) else { continue };
            if url.path() != CALLBACK_PATH {
                let _ = sock.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
                continue;
            }
            let get = |k: &str| url.query_pairs().find(|(key, _)| key == k).map(|(_, v)| v.into_owned());
            let result = if get("state").as_deref() != Some(expected_state.as_str()) {
                Err(OAuthError::StateMismatch)
            } else if get("error").is_some() {
                Err(OAuthError::Denied)
            } else {
                get("code").map(Secret::new).ok_or(OAuthError::Denied)
            };
            let page = if result.is_ok() { DONE_PAGE } else { FAILED_PAGE };
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
                page.len()
            );
            let _ = sock.write_all(resp.as_bytes()).await;
            let _ = sock.shutdown().await;
            return result;
        }
    })
    .await
    .map_err(|_| OAuthError::TimedOut)?
}

#[derive(Debug, Clone)]
pub struct TokenSet {
    pub access_token: Secret<String>,
    pub refresh_token: Option<Secret<String>>,
    /// Unix seconds.
    pub expires_at: Option<i64>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

async fn token_request(
    net: &Net,
    cfg: &OAuthConfig,
    client: &ClientCredentials,
    mut form: Vec<(&str, String)>,
) -> Result<TokenSet, OAuthError> {
    form.push(("client_id", client.client_id.clone()));
    if let Some(secret) = &client.client_secret {
        form.push(("client_secret", secret.expose().clone()));
    }
    let url = cfg.token_url.to_string();
    let resp = net
        .send("oauth", |c| {
            c.post(&url)
                .header(reqwest::header::ACCEPT, "application/json")
                .form(&form)
        })
        .await
        .map_err(|e| match e {
            NetError::Status { code, .. } => OAuthError::Rejected(format!("HTTP {code}")),
            other => OAuthError::Network(other.to_string()),
        })?;
    let t: TokenResponse = resp
        .json()
        .await
        .map_err(|e| OAuthError::Rejected(e.to_string()))?;
    Ok(TokenSet {
        access_token: Secret::new(t.access_token),
        refresh_token: t.refresh_token.map(Secret::new),
        expires_at: t.expires_in.map(|s| unix_now() + s),
    })
}

pub async fn exchange_code(
    net: &Net,
    cfg: &OAuthConfig,
    client: &ClientCredentials,
    code: &Secret<String>,
    verifier: &Secret<String>,
    redirect_uri: &str,
) -> Result<TokenSet, OAuthError> {
    let form = vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", code.expose().clone()),
        ("redirect_uri", redirect_uri.to_string()),
        ("code_verifier", verifier.expose().clone()),
    ];
    token_request(net, cfg, client, form).await
}

pub async fn refresh(
    net: &Net,
    cfg: &OAuthConfig,
    client: &ClientCredentials,
    refresh_token: &str,
) -> Result<TokenSet, OAuthError> {
    let form = vec![
        ("grant_type", "refresh_token".to_string()),
        ("refresh_token", refresh_token.to_string()),
    ];
    token_request(net, cfg, client, form).await
}

/// What's kept in the credential store for one account. Never logged or put in the database.
#[derive(Clone, Serialize, Deserialize)]
pub struct StoredTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
    pub client_secret: Option<String>,
}

impl std::fmt::Debug for StoredTokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("StoredTokens([REDACTED])")
    }
}

impl StoredTokens {
    pub fn from_set(t: &TokenSet, client_secret: Option<&Secret<String>>) -> Self {
        Self {
            access_token: t.access_token.expose().clone(),
            refresh_token: t.refresh_token.as_ref().map(|r| r.expose().clone()),
            expires_at: t.expires_at,
            client_secret: client_secret.map(|s| s.expose().clone()),
        }
    }

    /// Fresh for at least another minute.
    pub fn is_fresh(&self) -> bool {
        self.expires_at.is_none_or(|e| e > unix_now() + 60)
    }

    pub fn save(&self, store: &dyn SecretStore, key: &str) -> Result<(), SecretStoreError> {
        let json =
            serde_json::to_string(self).map_err(|e| SecretStoreError::Other(e.to_string()))?;
        store.store(key, &Secret::new(json))
    }

    pub fn load(store: &dyn SecretStore, key: &str) -> Result<Option<Self>, SecretStoreError> {
        match store.load(key)? {
            Some(raw) => Ok(serde_json::from_str(raw.expose()).ok()),
            None => Ok(None),
        }
    }
}
