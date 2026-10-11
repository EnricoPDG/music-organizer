//! Runs the real tools as child processes (T046). yt-dlp gets the managed Deno runtime
//! (required for YouTube since 2025.11) and FFmpeg when one is available.

use super::manager::ToolLocator;
use super::{Tool, ToolError, ToolRunner};
use async_trait::async_trait;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

/// Keep only the end of stderr: errors are on the last lines.
const STDERR_TAIL: usize = 8 * 1024;

pub struct ProcessRunner {
    locator: Arc<ToolLocator>,
}

impl ProcessRunner {
    pub fn new(locator: Arc<ToolLocator>) -> Self {
        Self { locator }
    }

    fn command(&self, tool: Tool, args: &[String]) -> Result<Command, ToolError> {
        let path = self.locator.path(tool).ok_or(ToolError::Missing(tool))?;
        let mut cmd = Command::new(path);
        if tool == Tool::YtDlp {
            // Ignore any user yt-dlp config so behavior is predictable.
            cmd.arg("--ignore-config");
            if let Some(deno) = self.locator.path(Tool::Deno) {
                cmd.arg("--js-runtimes")
                    .arg(format!("deno:{}", deno.display()));
            }
            if let Some(ffmpeg) = self.locator.path(Tool::Ffmpeg) {
                cmd.arg("--ffmpeg-location").arg(ffmpeg);
            }
        }
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        {
            // CREATE_NO_WINDOW: don't flash a console window for each call.
            cmd.creation_flags(0x0800_0000);
        }
        Ok(cmd)
    }
}

fn tail(bytes: &[u8]) -> String {
    let start = bytes.len().saturating_sub(STDERR_TAIL);
    String::from_utf8_lossy(&bytes[start..]).into_owned()
}

#[async_trait]
impl ToolRunner for ProcessRunner {
    fn available(&self, tool: Tool) -> bool {
        self.locator.path(tool).is_some()
    }

    async fn run_json(
        &self,
        tool: Tool,
        args: &[String],
        cancel: &CancellationToken,
    ) -> Result<serde_json::Value, ToolError> {
        let child = self
            .command(tool, args)?
            .spawn()
            .map_err(|e| ToolError::Failed {
                code: None,
                stderr: e.to_string(),
            })?;
        let output = tokio::select! {
            out = child.wait_with_output() => out.map_err(|e| ToolError::Failed { code: None, stderr: e.to_string() })?,
            _ = cancel.cancelled() => return Err(ToolError::Cancelled),
        };
        // yt-dlp exits non-zero when some playlist entries fail but still prints the JSON.
        match serde_json::from_slice(&output.stdout) {
            Ok(v) => Ok(v),
            Err(_) if !output.status.success() => Err(ToolError::Failed {
                code: output.status.code(),
                stderr: tail(&output.stderr),
            }),
            Err(e) => Err(ToolError::Parse(e.to_string())),
        }
    }

    async fn run_streaming(
        &self,
        tool: Tool,
        args: &[String],
        on_line: &mut (dyn for<'a> FnMut(&'a str) + Send),
        cancel: &CancellationToken,
    ) -> Result<(), ToolError> {
        let mut child = self
            .command(tool, args)?
            .spawn()
            .map_err(|e| ToolError::Failed {
                code: None,
                stderr: e.to_string(),
            })?;
        let stdout = child.stdout.take().expect("piped stdout");
        let mut stderr = child.stderr.take().expect("piped stderr");
        let stderr_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            let _ = stderr.read_to_end(&mut buf).await;
            buf
        });
        let mut lines = BufReader::new(stdout).lines();
        loop {
            tokio::select! {
                line = lines.next_line() => match line {
                    Ok(Some(l)) => on_line(&l),
                    Ok(None) => break,
                    Err(e) => return Err(ToolError::Failed { code: None, stderr: e.to_string() }),
                },
                _ = cancel.cancelled() => {
                    let _ = child.kill().await;
                    return Err(ToolError::Cancelled);
                }
            }
        }
        let status = child.wait().await.map_err(|e| ToolError::Failed {
            code: None,
            stderr: e.to_string(),
        })?;
        let stderr = stderr_task.await.unwrap_or_default();
        if status.success() {
            Ok(())
        } else {
            Err(ToolError::Failed {
                code: status.code(),
                stderr: tail(&stderr),
            })
        }
    }
}
