//! Secrets live only in the OS credential store (research R11). [`Secret`] keeps values out of
//! logs, debug output and serialized data.

use serde::{Serialize, Serializer};
use std::fmt;

const REDACTED: &str = "[REDACTED]";
/// Credential store service name for every entry the app writes.
pub const SERVICE: &str = "music-organizer";

/// A value that must never be printed or serialized. Use [`Secret::expose`] at the point of use.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret<T>(T);

impl<T> Secret<T> {
    pub fn new(value: T) -> Self {
        Self(value)
    }

    pub fn expose(&self) -> &T {
        &self.0
    }
}

impl<T> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

impl<T> fmt::Display for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

impl<T> Serialize for Secret<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(REDACTED)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SecretStoreError {
    /// No usable credential store (e.g. no Secret Service on a minimal Linux desktop).
    /// The app never falls back to plaintext.
    #[error("no secure credential store is available on this system")]
    Unavailable,
    #[error("credential store error: {0}")]
    Other(String),
}

impl From<keyring::Error> for SecretStoreError {
    fn from(e: keyring::Error) -> Self {
        match e {
            keyring::Error::NoStorageAccess(_)
            | keyring::Error::PlatformFailure(_)
            | keyring::Error::NoDefaultStore => SecretStoreError::Unavailable,
            other => SecretStoreError::Other(other.to_string()),
        }
    }
}

/// Store `value` under `key` in the OS credential store.
pub fn store(key: &str, value: &Secret<String>) -> Result<(), SecretStoreError> {
    keyring::Entry::new(SERVICE, key)?.set_password(value.expose())?;
    Ok(())
}

/// Load the value stored under `key`, or `None` when there is no entry.
pub fn load(key: &str) -> Result<Option<Secret<String>>, SecretStoreError> {
    match keyring::Entry::new(SERVICE, key)?.get_password() {
        Ok(v) => Ok(Some(Secret::new(v))),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Delete the entry under `key`. Deleting a missing entry is not an error.
pub fn delete(key: &str) -> Result<(), SecretStoreError> {
    match keyring::Entry::new(SERVICE, key)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}
