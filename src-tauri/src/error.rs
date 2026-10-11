//! The single error type returned by every IPC command (contracts/ipc.md, Conventions).
//! `message` is shown to the user as-is, so it must be readable and never contain secrets.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    InvalidUrl,
    UnsupportedPlatform,
    Private,
    PublicSourceChanged,
    NotFound,
    RateLimited,
    Network,
    AuthExpired,
    NotConnected,
    CredentialStoreUnavailable,
    ToolMissing,
    DiskFull,
    InvalidInput,
    Conflict,
    Platform,
    Internal,
}

#[derive(Debug, Clone, Serialize, specta::Type, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[error("{message}")]
pub struct AppError {
    pub kind: ErrorKind,
    pub message: String,
    pub retryable: bool,
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        let retryable = matches!(kind, ErrorKind::RateLimited | ErrorKind::Network);
        Self {
            kind,
            message: message.into(),
            retryable,
        }
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidInput, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::NotFound, message)
    }

    /// Unexpected failure. The detail goes to the log; the user sees a generic message.
    pub fn internal(detail: impl std::fmt::Display) -> Self {
        tracing::error!(%detail, "internal error");
        Self::new(
            ErrorKind::Internal,
            "Something went wrong. Details were written to the log.",
        )
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::internal(format!("database: {e}"))
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::StorageFull {
            return AppError::new(
                ErrorKind::DiskFull,
                "The disk is full. Free some space and retry.",
            );
        }
        AppError::internal(format!("io: {e}"))
    }
}

impl From<crate::secrets::SecretStoreError> for AppError {
    fn from(e: crate::secrets::SecretStoreError) -> Self {
        match e {
            crate::secrets::SecretStoreError::Unavailable => AppError::new(
                ErrorKind::CredentialStoreUnavailable,
                "No secure credential store is available, so accounts can't be connected. \
                 Public links still work. On Linux, install and unlock a Secret Service \
                 provider such as GNOME Keyring or KWallet.",
            ),
            crate::secrets::SecretStoreError::Other(detail) => AppError::internal(detail),
        }
    }
}

impl From<crate::core::sanitize::PathError> for AppError {
    fn from(e: crate::core::sanitize::PathError) -> Self {
        AppError::new(ErrorKind::InvalidInput, e.to_string())
    }
}
