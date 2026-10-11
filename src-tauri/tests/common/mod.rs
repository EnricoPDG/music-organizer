//! Shared helpers for integration tests.

use std::path::PathBuf;

/// Absolute path to a file under `tests/fixtures/`.
#[allow(dead_code)]
pub fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(rel)
}
