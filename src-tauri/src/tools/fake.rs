//! Replays recorded tool output for tests (constitution Principle V: no live network in tests).

use super::{Tool, ToolError, ToolRunner};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio_util::sync::CancellationToken;

enum Response {
    /// Return this file's content (JSON for `run_json`, lines for `run_streaming`).
    File(PathBuf),
    /// Simulate a download: emit progress lines and copy `source` to the path after `-o`.
    Download {
        source: PathBuf,
    },
    Fail(ToolError),
}

struct Rule {
    tool: Tool,
    needle: String,
    response: Response,
}

/// Matches calls by tool and a substring of the joined arguments; the first matching rule wins.
#[derive(Default)]
pub struct FakeToolRunner {
    rules: Vec<Rule>,
    calls: Mutex<Vec<(Tool, Vec<String>)>>,
    count: AtomicUsize,
}

impl FakeToolRunner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on(mut self, tool: Tool, needle: &str, fixture: impl Into<PathBuf>) -> Self {
        self.rules.push(Rule {
            tool,
            needle: needle.into(),
            response: Response::File(fixture.into()),
        });
        self
    }

    pub fn on_download(mut self, needle: &str, source_audio: impl Into<PathBuf>) -> Self {
        self.rules.push(Rule {
            tool: Tool::YtDlp,
            needle: needle.into(),
            response: Response::Download {
                source: source_audio.into(),
            },
        });
        self
    }

    pub fn on_fail(mut self, tool: Tool, needle: &str, error: ToolError) -> Self {
        self.rules.push(Rule {
            tool,
            needle: needle.into(),
            response: Response::Fail(error),
        });
        self
    }

    /// Number of tool invocations so far.
    pub fn call_count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }

    pub fn calls(&self) -> Vec<(Tool, Vec<String>)> {
        self.calls.lock().expect("calls lock").clone()
    }

    fn find(&self, tool: Tool, args: &[String]) -> Result<&Response, ToolError> {
        self.count.fetch_add(1, Ordering::SeqCst);
        self.calls
            .lock()
            .expect("calls lock")
            .push((tool, args.to_vec()));
        let joined = args.join(" ");
        self.rules
            .iter()
            .find(|r| r.tool == tool && joined.contains(&r.needle))
            .map(|r| &r.response)
            .ok_or_else(|| ToolError::Failed {
                code: Some(1),
                stderr: format!("no fake rule for: {joined}"),
            })
    }
}

fn read(path: &PathBuf) -> Result<String, ToolError> {
    std::fs::read_to_string(path).map_err(|e| ToolError::Parse(format!("{}: {e}", path.display())))
}

#[async_trait]
impl ToolRunner for FakeToolRunner {
    async fn run_json(
        &self,
        tool: Tool,
        args: &[String],
        cancel: &CancellationToken,
    ) -> Result<serde_json::Value, ToolError> {
        if cancel.is_cancelled() {
            return Err(ToolError::Cancelled);
        }
        match self.find(tool, args)? {
            Response::File(p) => {
                serde_json::from_str(&read(p)?).map_err(|e| ToolError::Parse(e.to_string()))
            }
            Response::Download { .. } => {
                Err(ToolError::Parse("download rule used for JSON".into()))
            }
            Response::Fail(e) => Err(e.clone()),
        }
    }

    async fn run_streaming(
        &self,
        tool: Tool,
        args: &[String],
        on_line: &mut (dyn for<'a> FnMut(&'a str) + Send),
        cancel: &CancellationToken,
    ) -> Result<(), ToolError> {
        if cancel.is_cancelled() {
            return Err(ToolError::Cancelled);
        }
        match self.find(tool, args)? {
            Response::File(p) => {
                for line in read(p)?.lines() {
                    on_line(line);
                }
                Ok(())
            }
            Response::Download { source } => {
                let out = args
                    .iter()
                    .position(|a| a == "-o")
                    .and_then(|i| args.get(i + 1))
                    .ok_or_else(|| ToolError::Parse("download without -o".into()))?;
                for pct in [10, 50, 100] {
                    if cancel.is_cancelled() {
                        return Err(ToolError::Cancelled);
                    }
                    on_line(&format!("progress:{pct}"));
                    tokio::task::yield_now().await;
                }
                // Output templates use %(ext)s; the fake keeps the source file's extension.
                let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("bin");
                let out = out.replace("%(ext)s", ext);
                std::fs::copy(source, &out).map_err(|e| ToolError::Failed {
                    code: None,
                    stderr: e.to_string(),
                })?;
                Ok(())
            }
            Response::Fail(e) => Err(e.clone()),
        }
    }
}

/// Delegates to one of two runners depending on a flag (e.g. offline → online in tests).
pub struct SwitchRunner {
    off: std::sync::Arc<dyn ToolRunner>,
    on: std::sync::Arc<dyn ToolRunner>,
    flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl SwitchRunner {
    pub fn new(
        off: std::sync::Arc<dyn ToolRunner>,
        on: std::sync::Arc<dyn ToolRunner>,
        flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        Self { off, on, flag }
    }

    fn pick(&self) -> &std::sync::Arc<dyn ToolRunner> {
        if self.flag.load(Ordering::SeqCst) {
            &self.on
        } else {
            &self.off
        }
    }
}

#[async_trait]
impl ToolRunner for SwitchRunner {
    async fn run_json(
        &self,
        tool: Tool,
        args: &[String],
        cancel: &CancellationToken,
    ) -> Result<serde_json::Value, ToolError> {
        self.pick().run_json(tool, args, cancel).await
    }

    async fn run_streaming(
        &self,
        tool: Tool,
        args: &[String],
        on_line: &mut (dyn for<'a> FnMut(&'a str) + Send),
        cancel: &CancellationToken,
    ) -> Result<(), ToolError> {
        self.pick().run_streaming(tool, args, on_line, cancel).await
    }
}
