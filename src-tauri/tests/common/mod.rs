//! Shared helpers for integration tests.

pub mod contract;

use music_organizer_lib::net::{Net, RetryPolicy};
use music_organizer_lib::platforms::AdapterCtx;
use music_organizer_lib::tools::ToolRunner;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Absolute path to a file under `tests/fixtures/`.
#[allow(dead_code)]
pub fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(rel)
}

#[allow(dead_code)]
pub fn fixture_str(rel: &str) -> String {
    std::fs::read_to_string(fixture(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// Network client with fast retries for tests.
#[allow(dead_code)]
pub fn test_net() -> Arc<Net> {
    Arc::new(Net::new(RetryPolicy {
        base: Duration::from_millis(5),
        max_attempts: 3,
        timeout: Duration::from_secs(5),
    }))
}

#[allow(dead_code)]
pub fn ctx(net: Arc<Net>, tools: Arc<dyn ToolRunner>) -> AdapterCtx {
    AdapterCtx {
        http: net,
        tools,
        token: None,
        cancel: CancellationToken::new(),
    }
}
