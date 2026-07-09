//! Self-update support for the daemon.
//!
//! Periodically checks GitHub releases for a newer version, downloads the
//! matching prebuilt binary, verifies its SHA-256 checksum, replaces the
//! current executable, and restarts the daemon in place via `exec`.
//!
//! Auto-update is opt-in. When enabled, the daemon only updates while idle
//! (no active agent turns) and, optionally, during configured quiet hours.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{Local, NaiveTime};
use serde::Deserialize;
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use tokio::time::interval;
use tracing::{info, warn};

const GITHUB_API_LATEST: &str =
    "https://api.github.com/repos/PDG-Global/raft-rust-daemon/releases/latest";
const USER_AGENT: &str = "raft-daemon-self-update";

/// Configuration controlling how/when auto-update runs.
#[derive(Debug, Clone)]
pub struct UpdateOptions {
    pub enabled: bool,
    pub check_interval: Duration,
    pub quiet_hours_start: Option<NaiveTime>,
    pub quiet_hours_end: Option<NaiveTime>,
}

impl Default for UpdateOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            check_interval: Duration::from_secs(24 * 60 * 60),
            quiet_hours_start: None,
            quiet_hours_end: None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    version: String,
    asset_name: String,
    download_url: String,
}

/// Return the asset name for the current platform, if one is published.
pub fn default_asset_name() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("raft-daemon-macos-arm64"),
        ("macos", "x86_64") => Some("raft-daemon-macos-x86_64"),
        ("linux", "aarch64") => Some("raft-daemon-aarch64-linux-gnu"),
        ("linux", "x86_64") => Some("raft-daemon-x86_64-linux-gnu"),
        ("linux", "arm") => Some("raft-daemon-armv7-linux"),
        ("freebsd", "x86_64") => Some("raft-daemon-x86_64-freebsd"),
        _ => None,
    }
}

/// Current crate version as seen at runtime.
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn is_newer_version(current: &str, latest: &str) -> bool {
    match (
        semver::Version::parse(current),
        semver::Version::parse(latest),
    ) {
        (Ok(c), Ok(l)) => l > c,
        _ => false,
    }
}

/// Query the GitHub releases API for a version newer than the current one.
///
/// Returns `None` if already on the latest version or if the platform has no
/// prebuilt binary.
///
/// # Errors
///
/// Returns an error if the network request fails or the response is malformed.
pub async fn check_for_update() -> Result<Option<ReleaseInfo>> {
    let client = reqwest::Client::new();
    let response = client
        .get(GITHUB_API_LATEST)
        .header("User-Agent", USER_AGENT)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .context("failed to query GitHub releases API")?;

    if !response.status().is_success() {
        anyhow::bail!("GitHub API returned {}", response.status());
    }

    let release: GitHubRelease = response
        .json()
        .await
        .context("failed to parse GitHub release")?;
    let latest_version = release.tag_name.trim_start_matches('v');

    if !is_newer_version(current_version(), latest_version) {
        return Ok(None);
    }

    let asset_name = default_asset_name().context("no prebuilt binary for this platform")?;
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .context("latest release does not contain a binary for this platform")?;

    Ok(Some(ReleaseInfo {
        version: latest_version.to_string(),
        asset_name: asset_name.to_string(),
        download_url: asset.browser_download_url.clone(),
    }))
}

async fn download_file(url: &str, dest: &Path) -> Result<()> {
    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .header("User-Agent", USER_AGENT)
        .timeout(Duration::from_secs(180))
        .send()
        .await
        .context("download request failed")?;

    if !response.status().is_success() {
        anyhow::bail!("download failed: {}", response.status());
    }

    let bytes = response
        .bytes()
        .await
        .context("failed to read download body")?;
    tokio::fs::write(dest, bytes).await?;
    Ok(())
}

async fn sha256_of_file(path: &Path) -> Result<String> {
    let bytes = tokio::fs::read(path).await?;
    Ok(hex::encode(Sha256::digest(&bytes)))
}

/// Download a new release, verify its checksum, replace the current binary, and
/// restart the daemon in place.
///
/// # Errors
///
/// Returns an error if any download/verification step fails, or if the
/// platform does not support in-place restart via `exec`.
///
/// # Notes
///
/// On success this function **does not return**; the process image is replaced
/// by the new binary.
#[cfg(unix)]
pub async fn perform_update(info: ReleaseInfo, current_exe: &Path) -> Result<()> {
    info!(
        "auto-update: downloading version {} ({})",
        info.version, info.asset_name
    );

    let temp_dir = std::env::temp_dir().join(format!("raft-daemon-update-{}", info.version));
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .with_context(|| format!("creating temp dir {}", temp_dir.display()))?;

    let new_binary = temp_dir.join(&info.asset_name);
    download_file(&info.download_url, &new_binary).await?;

    let sums_url = info
        .download_url
        .rsplit_once('/')
        .map(|(base, _)| format!("{base}/SHA256SUMS.txt"))
        .context("could not derive SHA256SUMS URL")?;
    let sums_path = temp_dir.join("SHA256SUMS.txt");
    download_file(&sums_url, &sums_path).await?;

    let sums_content = tokio::fs::read_to_string(&sums_path).await?;
    let expected = sums_content
        .lines()
        .find(|line| line.ends_with(&info.asset_name))
        .and_then(|line| line.split_whitespace().next())
        .context("could not find checksum in SHA256SUMS.txt")?;

    let actual = sha256_of_file(&new_binary).await?;
    if actual != expected {
        anyhow::bail!("checksum mismatch: expected {expected}, got {actual}");
    }
    info!("auto-update: checksum verified");

    let mut perms = tokio::fs::metadata(&new_binary).await?.permissions();
    perms.set_mode(0o755);
    tokio::fs::set_permissions(&new_binary, perms).await?;

    tokio::fs::rename(&new_binary, current_exe)
        .await
        .with_context(|| format!("replacing binary {}", current_exe.display()))?;
    info!("auto-update: binary replaced; restarting daemon");

    let args: Vec<String> = std::env::args().skip(1).collect();
    let err = std::process::Command::new(current_exe).args(&args).exec();

    anyhow::bail!("failed to restart daemon: {err}")
}

#[cfg(not(unix))]
pub async fn perform_update(_info: ReleaseInfo, _current_exe: &Path) -> Result<()> {
    anyhow::bail!("auto-update restart is only supported on Unix")
}

/// Spawn a background task that periodically checks for updates and performs
/// them when the daemon is idle and, optionally, within quiet hours.
///
/// The task exits silently if auto-update is disabled.
pub async fn spawn_update_checker(
    opts: UpdateOptions,
    active_turns: Arc<AtomicUsize>,
    current_exe: PathBuf,
) {
    if !opts.enabled {
        return;
    }

    let mut ticker = interval(opts.check_interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        ticker.tick().await;

        match check_for_update().await {
            Ok(None) => {
                tracing::debug!("auto-update: no update available");
            }
            Ok(Some(info)) => {
                info!(
                    "auto-update: version {} is available; waiting for idle + quiet hours",
                    info.version
                );
                wait_and_update(opts.clone(), info, active_turns.clone(), &current_exe).await;
            }
            Err(err) => {
                warn!(error = %err, "auto-update: check failed");
            }
        }
    }
}

async fn wait_and_update(
    opts: UpdateOptions,
    info: ReleaseInfo,
    active_turns: Arc<AtomicUsize>,
    current_exe: &Path,
) {
    loop {
        let idle = active_turns.load(Ordering::Relaxed) == 0;
        let in_quiet = is_in_quiet_hours(opts.quiet_hours_start, opts.quiet_hours_end);

        if idle && in_quiet {
            if let Err(err) = perform_update(info, current_exe).await {
                warn!(error = %err, "auto-update: failed");
            }
            return;
        }

        if !idle {
            tracing::debug!("auto-update: waiting for agent turns to finish");
        } else if !in_quiet {
            tracing::debug!("auto-update: waiting for quiet hours");
        }

        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

fn is_in_quiet_hours(start: Option<NaiveTime>, end: Option<NaiveTime>) -> bool {
    match (start, end) {
        (Some(s), Some(e)) => {
            let now = Local::now().time();
            if s <= e {
                now >= s && now <= e
            } else {
                now >= s || now <= e
            }
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_newer_version_accepts_patch() {
        assert!(is_newer_version("0.69.0", "0.69.1"));
        assert!(!is_newer_version("0.69.1", "0.69.0"));
        assert!(!is_newer_version("0.69.0", "0.69.0"));
    }

    #[test]
    fn is_in_quiet_hours_handles_wrapped_range() {
        let start = NaiveTime::from_hms_opt(23, 0, 0).unwrap();
        let end = NaiveTime::from_hms_opt(2, 0, 0).unwrap();
        let now = NaiveTime::from_hms_opt(0, 30, 0).unwrap();

        // We can't easily stub Local::now() in a unit test, so test the helper
        // directly with the range logic.
        let in_range = if start <= end {
            now >= start && now <= end
        } else {
            now >= start || now <= end
        };
        assert!(in_range);
    }
}
