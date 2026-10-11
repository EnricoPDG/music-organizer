//! Installs and updates the external tools (T045, research R4). yt-dlp and Deno are downloaded
//! into `<app data>/tools/` and verified against the SHA-256 checksums pinned in
//! `manifest.json`. FFmpeg is optional: a system install or a user-chosen path is used when
//! present (only Opus and MP3 output need it).

use super::{Tool, ToolRunner};
use crate::commands::settings::ToolPaths;
use crate::error::{AppError, AppResult, ErrorKind};
use crate::net::Net;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};
use tokio_util::sync::CancellationToken;

const MANIFEST: &str = include_str!("manifest.json");
/// yt-dlp is updated when its last update is older than this (or after an extractor failure).
const UPDATE_EVERY: Duration = Duration::from_secs(7 * 24 * 3600);

#[derive(Debug, Deserialize)]
struct Manifest {
    #[serde(rename = "ytDlp")]
    yt_dlp: ToolEntry,
    deno: ToolEntry,
}

#[derive(Debug, Deserialize)]
struct ToolEntry {
    version: String,
    assets: std::collections::HashMap<String, Asset>,
}

#[derive(Debug, Clone, Deserialize)]
struct Asset {
    url: String,
    sha256: String,
    archive: Option<String>,
    binary: Option<String>,
}

fn platform_key() -> String {
    let os = match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        _ => "linux",
    };
    format!("{os}-{}", std::env::consts::ARCH)
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub tool: Tool,
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    /// Installed by the app (true) or found on the system / chosen by the user (false).
    pub managed: bool,
    /// The app can install this tool itself.
    pub installable: bool,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolsStatus {
    pub tools: Vec<ToolInfo>,
    /// Required tools (yt-dlp and Deno) are present.
    pub ready: bool,
    pub installing: bool,
    pub message: Option<String>,
}

/// Resolves where each tool is. yt-dlp and Deno: a user-chosen path, else the app's own pinned
/// install. A copy found on the system is never used for them, because an old one breaks
/// downloads (yt-dlp before 2025.11 has no `--js-runtimes`). FFmpeg isn't shipped by the app,
/// so the system copy is used.
pub struct ToolLocator {
    managed_dir: PathBuf,
    overrides: RwLock<ToolPaths>,
    system_dirs: Vec<PathBuf>,
}

impl ToolLocator {
    pub fn new(managed_dir: PathBuf, overrides: ToolPaths) -> Self {
        Self::with_system_dirs(managed_dir, overrides, system_search_dirs())
    }

    /// Search `system_dirs` instead of PATH (tests).
    pub fn with_system_dirs(
        managed_dir: PathBuf,
        overrides: ToolPaths,
        system_dirs: Vec<PathBuf>,
    ) -> Self {
        Self {
            managed_dir,
            overrides: RwLock::new(overrides),
            system_dirs,
        }
    }

    /// The user chose a path for this tool.
    pub fn has_override(&self, tool: Tool) -> bool {
        let o = self.overrides.read().expect("overrides lock");
        match tool {
            Tool::YtDlp => o.yt_dlp.is_some(),
            Tool::Deno => o.deno.is_some(),
            Tool::Ffmpeg => o.ffmpeg.is_some(),
        }
    }

    pub fn set_overrides(&self, o: ToolPaths) {
        *self.overrides.write().expect("overrides lock") = o;
    }

    pub fn managed_path(&self, tool: Tool) -> PathBuf {
        self.managed_dir.join(exe(match tool {
            Tool::YtDlp => "yt-dlp",
            Tool::Deno => "deno",
            Tool::Ffmpeg => "ffmpeg",
        }))
    }

    pub fn path(&self, tool: Tool) -> Option<PathBuf> {
        self.locate(tool).map(|(p, _)| p)
    }

    /// `(path, managed)`.
    pub fn locate(&self, tool: Tool) -> Option<(PathBuf, bool)> {
        let o = self.overrides.read().expect("overrides lock").clone();
        let custom = match tool {
            Tool::YtDlp => o.yt_dlp,
            Tool::Deno => o.deno,
            Tool::Ffmpeg => o.ffmpeg,
        };
        if let Some(p) = custom.map(PathBuf::from).filter(|p| p.is_file()) {
            return Some((p, false));
        }
        let managed = self.managed_path(tool);
        if managed.is_file() {
            return Some((managed, true));
        }
        if tool != Tool::Ffmpeg {
            return None;
        }
        self.system_dirs
            .iter()
            .map(|d| d.join(exe("ffmpeg")))
            .find(|p| p.is_file())
            .map(|p| (p, false))
    }
}

/// PATH plus the usual Homebrew locations (GUI apps on macOS don't inherit the shell PATH).
fn system_search_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if cfg!(target_os = "macos") {
        dirs.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    }
    dirs
}

pub struct ToolManager {
    locator: Arc<ToolLocator>,
    net: Arc<Net>,
    installing: std::sync::atomic::AtomicBool,
}

impl ToolManager {
    pub fn new(locator: Arc<ToolLocator>, net: Arc<Net>) -> Self {
        Self {
            locator,
            net,
            installing: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn locator(&self) -> &Arc<ToolLocator> {
        &self.locator
    }

    pub async fn status(&self, runner: &dyn ToolRunner) -> ToolsStatus {
        let mut tools = Vec::new();
        for tool in Tool::ALL {
            let found = self.locator.locate(tool);
            let version = match &found {
                Some(_) => tool_version(runner, tool).await,
                None => None,
            };
            tools.push(ToolInfo {
                tool,
                installed: found.is_some(),
                version,
                path: found.as_ref().map(|(p, _)| p.to_string_lossy().to_string()),
                managed: found.as_ref().is_some_and(|(_, m)| *m),
                installable: tool != Tool::Ffmpeg,
            });
        }
        let ready = tools
            .iter()
            .filter(|t| t.tool != Tool::Ffmpeg)
            .all(|t| t.installed);
        ToolsStatus {
            tools,
            ready,
            installing: self.installing.load(std::sync::atomic::Ordering::SeqCst),
            message: None,
        }
    }

    /// Download and verify every required tool that isn't present yet.
    /// `on_progress` receives a short human-readable step description.
    pub async fn install_missing(
        &self,
        on_progress: &(dyn Fn(String) + Send + Sync),
    ) -> AppResult<()> {
        if self
            .installing
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            return Ok(());
        }
        let result = self.install_inner(on_progress).await;
        self.installing
            .store(false, std::sync::atomic::Ordering::SeqCst);
        result
    }

    async fn install_inner(&self, on_progress: &(dyn Fn(String) + Send + Sync)) -> AppResult<()> {
        let manifest: Manifest = serde_json::from_str(MANIFEST).map_err(AppError::internal)?;
        let key = platform_key();
        std::fs::create_dir_all(&self.locator.managed_dir)?;
        for (tool, entry) in [
            (Tool::YtDlp, &manifest.yt_dlp),
            (Tool::Deno, &manifest.deno),
        ] {
            if self.locator.path(tool).is_some() {
                continue;
            }
            let asset = entry.assets.get(&key).cloned().ok_or_else(|| {
                AppError::new(
                    ErrorKind::ToolMissing,
                    format!(
                        "{} isn't available for this system ({key}).",
                        tool.display_name()
                    ),
                )
            })?;
            on_progress(format!(
                "Downloading {} {}…",
                tool.display_name(),
                entry.version
            ));
            let bytes = self.download(&asset.url).await?;
            on_progress(format!("Verifying {}…", tool.display_name()));
            verify_sha256(&bytes, &asset.sha256)?;
            let dest = self.locator.managed_path(tool);
            install_bytes(&bytes, &asset, &dest)?;
            on_progress(format!("{} installed.", tool.display_name()));
        }
        Ok(())
    }

    async fn download(&self, url: &str) -> AppResult<Vec<u8>> {
        let resp = self
            .net
            .send("tools", |c| c.get(url).timeout(Duration::from_secs(600)))
            .await?;
        Ok(resp
            .bytes()
            .await
            .map_err(|e| AppError::from(crate::net::NetError::Network(e.to_string())))?
            .to_vec())
    }

    /// Update the managed yt-dlp with its own verified self-updater, at most weekly
    /// (or when `force` is set after an extractor failure).
    pub async fn update_yt_dlp(&self, runner: &dyn ToolRunner, force: bool) -> AppResult<bool> {
        let Some((path, true)) = self.locator.locate(Tool::YtDlp) else {
            return Ok(false);
        };
        let stamp = self.locator.managed_dir.join(".yt-dlp-updated");
        let fresh = std::fs::metadata(&stamp)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age < UPDATE_EVERY);
        if fresh && !force {
            return Ok(false);
        }
        let mut ignore = |_: &str| {};
        let result = runner
            .run_streaming(
                Tool::YtDlp,
                &["-U".into()],
                &mut ignore,
                &CancellationToken::new(),
            )
            .await;
        let _ = std::fs::write(&stamp, b"");
        tracing::info!(path = %path.display(), ok = result.is_ok(), "yt-dlp self-update");
        Ok(result.is_ok())
    }
}

pub fn verify_sha256(bytes: &[u8], expected_hex: &str) -> AppResult<()> {
    let got: String = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if got.eq_ignore_ascii_case(expected_hex) {
        Ok(())
    } else {
        tracing::error!(expected = expected_hex, got, "tool checksum mismatch");
        Err(AppError::new(
            ErrorKind::ToolMissing,
            "The downloaded tool didn't match its checksum, so it wasn't installed.",
        ))
    }
}

fn install_bytes(bytes: &[u8], asset: &Asset, dest: &Path) -> AppResult<()> {
    let tmp = dest.with_extension("download");
    match asset.archive.as_deref() {
        Some("zip") => {
            let want = asset.binary.as_deref().unwrap_or_default();
            let mut zip =
                zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(AppError::internal)?;
            let mut file = zip.by_name(want).map_err(AppError::internal)?;
            let mut out = std::fs::File::create(&tmp)?;
            std::io::copy(&mut file, &mut out)?;
        }
        _ => std::fs::write(&tmp, bytes)?,
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&tmp, dest)?;
    Ok(())
}

async fn tool_version(runner: &dyn ToolRunner, tool: Tool) -> Option<String> {
    let arg = if tool == Tool::Ffmpeg {
        "-version"
    } else {
        "--version"
    };
    let mut first = None;
    let mut grab = |l: &str| {
        if first.is_none() && !l.trim().is_empty() {
            first = Some(l.trim().to_string());
        }
    };
    let ok = runner
        .run_streaming(tool, &[arg.into()], &mut grab, &CancellationToken::new())
        .await
        .is_ok();
    let line = first.filter(|_| ok)?;
    // "deno 2.9.7 (stable, …)" / "ffmpeg version 7.1 Copyright…" / "2026.08.19"
    let v = line
        .split_whitespace()
        .find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .unwrap_or(&line);
    Some(v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_covers_every_supported_platform() {
        let m: Manifest = serde_json::from_str(MANIFEST).unwrap();
        for key in [
            "macos-aarch64",
            "macos-x86_64",
            "linux-x86_64",
            "linux-aarch64",
            "windows-x86_64",
            "windows-aarch64",
        ] {
            for (name, entry) in [("yt-dlp", &m.yt_dlp), ("deno", &m.deno)] {
                let a = entry
                    .assets
                    .get(key)
                    .unwrap_or_else(|| panic!("{name} missing {key}"));
                assert_eq!(a.sha256.len(), 64, "{name} {key}");
                assert!(a.url.starts_with("https://github.com/"), "{name} {key}");
            }
        }
    }

    #[test]
    fn checksum_mismatch_is_rejected() {
        let good: String = Sha256::digest(b"abc")
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert!(verify_sha256(b"abc", &good).is_ok());
        assert!(verify_sha256(b"abd", &good).is_err());
    }

    /// Regression: an old yt-dlp in /usr/local/bin (2025.03.31, no `--js-runtimes`) was picked up
    /// instead of installing the pinned copy, and every download failed.
    #[test]
    fn system_copies_are_ignored_except_ffmpeg() {
        let dir = tempfile::tempdir().unwrap();
        let system = dir.path().join("bin");
        std::fs::create_dir_all(&system).unwrap();
        for name in ["yt-dlp", "deno", "ffmpeg"] {
            std::fs::write(system.join(exe(name)), b"x").unwrap();
        }
        let loc = ToolLocator::with_system_dirs(
            dir.path().join("tools"),
            ToolPaths::default(),
            vec![system.clone()],
        );
        assert_eq!(
            loc.locate(Tool::YtDlp),
            None,
            "a system yt-dlp must not be used"
        );
        assert_eq!(loc.locate(Tool::Deno), None);
        assert_eq!(
            loc.locate(Tool::Ffmpeg),
            Some((system.join(exe("ffmpeg")), false))
        );
    }

    #[test]
    fn user_override_wins_over_managed() {
        let dir = tempfile::tempdir().unwrap();
        let managed = dir.path().join("tools");
        std::fs::create_dir_all(&managed).unwrap();
        let loc = ToolLocator::new(managed.clone(), ToolPaths::default());
        std::fs::write(loc.managed_path(Tool::YtDlp), b"x").unwrap();
        assert_eq!(
            loc.locate(Tool::YtDlp),
            Some((loc.managed_path(Tool::YtDlp), true))
        );
        let custom = dir.path().join("my-yt-dlp");
        std::fs::write(&custom, b"x").unwrap();
        loc.set_overrides(ToolPaths {
            yt_dlp: Some(custom.to_string_lossy().into()),
            ..Default::default()
        });
        assert_eq!(loc.locate(Tool::YtDlp), Some((custom, false)));
    }
}
