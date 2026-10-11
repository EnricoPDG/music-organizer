//! File logging with secret redaction (research R11). Every log line passes through
//! [`redact`] before it is written, as a second line of defense behind `Secret<T>`.

use regex::Regex;
use std::io::{self, Write};
use std::path::Path;
use std::sync::LazyLock;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::MakeWriter;

static SECRET_PATTERNS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)(access_token|refresh_token|client_secret|id_token|code_verifier|\bcode)(["']?\s*[:=]\s*["']?)[^\s"'&,}]+|(authorization["']?\s*[:=]\s*["']?(bearer|basic)\s+)[^\s"',}]+"#,
    )
    .expect("valid regex")
});

/// Mask secret values in one line of text.
pub fn redact(line: &str) -> String {
    SECRET_PATTERNS
        .replace_all(line, |c: &regex::Captures| {
            match (c.get(1), c.get(2), c.get(3)) {
                (Some(k), Some(sep), _) => format!("{}{}[REDACTED]", k.as_str(), sep.as_str()),
                (_, _, Some(prefix)) => format!("{}[REDACTED]", prefix.as_str()),
                _ => "[REDACTED]".into(),
            }
        })
        .into_owned()
}

struct RedactingWriter<W: Write>(W);

impl<W: Write> Write for RedactingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let text = String::from_utf8_lossy(buf);
        self.0.write_all(redact(&text).as_bytes())?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

struct RedactingMakeWriter<M>(M);

impl<'a, M: MakeWriter<'a>> MakeWriter<'a> for RedactingMakeWriter<M> {
    type Writer = RedactingWriter<M::Writer>;
    fn make_writer(&'a self) -> Self::Writer {
        RedactingWriter(self.0.make_writer())
    }
}

/// Log to a daily rolling file in `log_dir`. Keep the returned guard alive for the app's lifetime.
pub fn init(log_dir: &Path) -> WorkerGuard {
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("music-organizer")
        .filename_suffix("log")
        .max_log_files(7)
        .build(log_dir)
        .expect("log directory is writable");
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,music_organizer_lib=debug"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_writer(RedactingMakeWriter(writer))
        .try_init();
    guard
}

#[cfg(test)]
mod tests {
    use super::redact;

    #[test]
    fn redacts_known_secret_fields() {
        let line = r#"token response {"access_token":"abc123","refresh_token": "def456"} code=xyz client_secret=s3"#;
        let out = redact(line);
        for leaked in ["abc123", "def456", "xyz", "s3"] {
            assert!(!out.contains(leaked), "{leaked} leaked in {out}");
        }
        assert!(out.contains("access_token"));
    }

    #[test]
    fn redacts_authorization_headers() {
        let out = redact("headers: Authorization: Bearer eyJhbGciOi.very.secret");
        assert!(!out.contains("eyJhbGciOi"), "{out}");
    }

    #[test]
    fn leaves_ordinary_text_alone() {
        assert_eq!(
            redact("downloaded 12 tracks to /music/codes"),
            "downloaded 12 tracks to /music/codes"
        );
    }
}
