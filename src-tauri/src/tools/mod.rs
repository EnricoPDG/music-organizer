//! External tools (yt-dlp, Deno, FFmpeg) behind the [`ToolRunner`] trait (research R4), so tests
//! can replay recorded output with [`fake::FakeToolRunner`] instead of running real processes.

pub mod fake;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Tool {
    YtDlp,
    Deno,
    Ffmpeg,
}

impl Tool {
    pub const ALL: [Tool; 3] = [Tool::YtDlp, Tool::Deno, Tool::Ffmpeg];

    pub fn display_name(self) -> &'static str {
        match self {
            Tool::YtDlp => "yt-dlp",
            Tool::Deno => "Deno",
            Tool::Ffmpeg => "FFmpeg",
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ToolError {
    #[error("{} is not installed", .0.display_name())]
    Missing(Tool),
    #[error("tool exited with code {code:?}: {stderr}")]
    Failed { code: Option<i32>, stderr: String },
    #[error("cancelled")]
    Cancelled,
    #[error("could not parse tool output: {0}")]
    Parse(String),
}

impl From<ToolError> for crate::error::AppError {
    fn from(e: ToolError) -> Self {
        use crate::error::{AppError, ErrorKind};
        match e {
            ToolError::Missing(t) => AppError::new(
                ErrorKind::ToolMissing,
                format!(
                    "{} is not installed yet. Open Settings → Tools to install it.",
                    t.display_name()
                ),
            ),
            ToolError::Cancelled => AppError::new(ErrorKind::InvalidInput, "Cancelled."),
            other => AppError::internal(other),
        }
    }
}

/// Runs an external tool. Output lines are delivered as they arrive; cancellation kills the
/// process.
#[async_trait]
pub trait ToolRunner: Send + Sync {
    /// Run to completion and parse stdout as one JSON document.
    async fn run_json(
        &self,
        tool: Tool,
        args: &[String],
        cancel: &CancellationToken,
    ) -> Result<serde_json::Value, ToolError>;

    /// Run to completion, calling `on_line` for each stdout line (progress output).
    async fn run_streaming(
        &self,
        tool: Tool,
        args: &[String],
        on_line: &mut (dyn for<'a> FnMut(&'a str) + Send),
        cancel: &CancellationToken,
    ) -> Result<(), ToolError>;
}
