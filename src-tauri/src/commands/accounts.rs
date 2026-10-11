//! Connected accounts (FR-004, FR-004a/c). Tokens and client secrets live only in the OS
//! credential store; the database keeps a reference and the non-secret client ID.

use crate::error::{AppError, AppResult, ErrorKind};
use crate::net::oauth::{self, SIGN_IN_TIMEOUT, StoredTokens};
use crate::platforms::ClientCredentials;
use crate::secrets::Secret;
use crate::state::AppState;
use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountStatus {
    pub platform: String,
    pub platform_name: String,
    /// `connected`, `expired`, `error`, or `disconnected`.
    pub status: String,
    pub display_name: Option<String>,
    pub client_id: Option<String>,
    pub connected_at: Option<String>,
}

fn keyring_key(platform: &str) -> String {
    format!("account:{platform}")
}

pub async fn list(state: &AppState) -> AppResult<Vec<AccountStatus>> {
    let rows: Vec<(String, String, String, String, String)> = state
        .db
        .call(|c| {
            let mut s = c.prepare("SELECT platform, display_name, client_id, status, connected_at FROM connected_account")?;
            Ok(s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?.collect::<Result<_, _>>()?)
        })
        .await?;
    Ok(state
        .registry
        .all()
        .iter()
        .filter(|a| a.capabilities().sign_in)
        .map(|a| {
            let row = rows.iter().find(|r| r.0 == a.id());
            AccountStatus {
                platform: a.id().into(),
                platform_name: a.display_name().into(),
                status: row
                    .map(|r| r.3.clone())
                    .unwrap_or_else(|| "disconnected".into()),
                display_name: row.map(|r| r.1.clone()),
                client_id: row.map(|r| r.2.clone()),
                connected_at: row.map(|r| r.4.clone()),
            }
        })
        .collect())
}

async fn set_status(
    state: &AppState,
    app: Option<&AppHandle>,
    platform: &str,
    status: &'static str,
) {
    let p = platform.to_string();
    let _ = state
        .db
        .call(move |c| {
            Ok(c.execute(
                "UPDATE connected_account SET status = ?2 WHERE platform = ?1",
                params![p, status],
            )?)
        })
        .await;
    if let (Some(app), Ok(all)) = (app, list(state).await) {
        if let Some(s) = all.into_iter().find(|s| s.platform == platform) {
            let _ = app.emit("account://status", s);
        }
    }
}

/// A usable access token for `platform`, refreshing it if needed. `None` when not connected.
/// A failed refresh marks the account `expired` (the UI then asks to reconnect).
pub async fn token_for(
    state: &AppState,
    app: Option<&AppHandle>,
    platform: &str,
) -> AppResult<Option<Secret<String>>> {
    let p = platform.to_string();
    let row: Option<(String, String)> = state
        .db
        .call(move |c| {
            Ok(c.query_row("SELECT client_id, keyring_key FROM connected_account WHERE platform = ?1 AND status = 'connected'", [p], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?)
        })
        .await?;
    let Some((client_id, key)) = row else {
        return Ok(None);
    };
    let Some(stored) = StoredTokens::load(state.secrets.as_ref(), &key)? else {
        set_status(state, app, platform, "expired").await;
        return Ok(None);
    };
    if stored.is_fresh() {
        return Ok(Some(Secret::new(stored.access_token)));
    }
    let (Some(refresh_token), Some(adapter)) =
        (stored.refresh_token.clone(), state.registry.get(platform))
    else {
        set_status(state, app, platform, "expired").await;
        return Ok(None);
    };
    let client = ClientCredentials {
        client_id,
        client_secret: stored.client_secret.clone().map(Secret::new),
    };
    let Some(cfg) = adapter.auth_config(&client) else {
        return Ok(None);
    };
    match oauth::refresh(&state.net, &cfg, &client, &refresh_token).await {
        Ok(mut fresh) => {
            // Some platforms (SoundCloud) rotate refresh tokens; others keep the old one.
            if fresh.refresh_token.is_none() {
                fresh.refresh_token = Some(Secret::new(refresh_token));
            }
            StoredTokens::from_set(&fresh, client.client_secret.as_ref())
                .save(state.secrets.as_ref(), &key)?;
            Ok(Some(fresh.access_token))
        }
        Err(e) => {
            tracing::info!(platform, error = %e, "token refresh failed");
            set_status(state, app, platform, "expired").await;
            Ok(None)
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn list_accounts(state: State<'_, AppState>) -> AppResult<Vec<AccountStatus>> {
    list(&state).await
}

/// Open the platform's sign-in page in the browser and wait for the loopback redirect.
#[tauri::command]
#[specta::specta]
pub async fn connect_account(
    app: AppHandle,
    state: State<'_, AppState>,
    platform: String,
    client_id: String,
    client_secret: Option<String>,
) -> AppResult<AccountStatus> {
    let client_id = client_id.trim().to_string();
    if client_id.is_empty() {
        return Err(AppError::invalid_input(
            "Paste the client ID from your developer app.",
        ));
    }
    let adapter = state
        .registry
        .get(&platform)
        .cloned()
        .ok_or_else(|| AppError::invalid_input("Unknown platform."))?;
    let client = ClientCredentials {
        client_id: client_id.clone(),
        client_secret: client_secret
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .map(Secret::new),
    };
    let cfg = adapter
        .auth_config(&client)
        .ok_or_else(|| AppError::invalid_input("This platform doesn't support sign-in."))?;

    let pkce = oauth::pkce();
    let state_param = oauth::random_state();
    let (listener, port) = oauth::bind_loopback_on(oauth::REDIRECT_PORT)
        .await
        .map_err(|_| {
            AppError::new(
                ErrorKind::Conflict,
                "Another sign-in is already waiting. Finish or close it, then try again.",
            )
        })?;
    let redirect = oauth::redirect_uri(port);
    let url = oauth::authorize_url(
        &cfg,
        &client.client_id,
        &redirect,
        &state_param,
        &pkce.challenge,
    );
    app.opener()
        .open_url(url.as_str(), None::<&str>)
        .map_err(AppError::internal)?;

    let code = oauth::wait_for_code(listener, state_param, SIGN_IN_TIMEOUT).await?;
    let tokens =
        oauth::exchange_code(&state.net, &cfg, &client, &code, &pkce.verifier, &redirect).await?;
    let key = keyring_key(&platform);
    StoredTokens::from_set(&tokens, client.client_secret.as_ref())
        .save(state.secrets.as_ref(), &key)?;

    let (p, name, scopes) = (
        platform.clone(),
        format!("{} account", adapter.display_name()),
        serde_json::to_string(&cfg.scopes).unwrap_or_else(|_| "[]".into()),
    );
    state
        .db
        .call(move |c| {
            c.execute(
                "INSERT INTO connected_account (platform, display_name, client_id, keyring_key, scopes, status, connected_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'connected', ?6)
                 ON CONFLICT(platform) DO UPDATE SET display_name = excluded.display_name, client_id = excluded.client_id,
                   keyring_key = excluded.keyring_key, scopes = excluded.scopes, status = 'connected', connected_at = excluded.connected_at",
                params![p, name, client_id, key, scopes, crate::db::now()],
            )?;
            Ok(())
        })
        .await?;
    tracing::info!(%platform, "account connected");
    let status = list(&state)
        .await?
        .into_iter()
        .find(|s| s.platform == platform)
        .ok_or_else(|| AppError::internal("account vanished"))?;
    let _ = app.emit("account://status", &status);
    Ok(status)
}

/// Remove the account and its credentials from this computer (FR-004a).
#[tauri::command]
#[specta::specta]
pub async fn disconnect_account(
    app: AppHandle,
    state: State<'_, AppState>,
    platform: String,
) -> AppResult<()> {
    state.secrets.delete(&keyring_key(&platform))?;
    let p = platform.clone();
    state
        .db
        .call(move |c| Ok(c.execute("DELETE FROM connected_account WHERE platform = ?1", [p])?))
        .await?;
    if let Some(s) = list(&state)
        .await?
        .into_iter()
        .find(|s| s.platform == platform)
    {
        let _ = app.emit("account://status", s);
    }
    Ok(())
}

pub fn not_connected(platform_name: &str) -> AppError {
    AppError::new(
        ErrorKind::NotConnected,
        format!("Connect your {platform_name} account in Settings → Accounts first."),
    )
}
