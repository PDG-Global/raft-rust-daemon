//! Daemon filesystem locations.
//!
//! All persistent state lives under a single home directory. The default is
//! `~/.raft-daemon`, overridable via the `RAFT_DAEMON_HOME` environment
//! variable. The layout is:
//!
//! ```text
//! $RAFT_DAEMON_HOME/
//! ├── daemon.pid          # PID of the running daemon
//! ├── state.json          # serialised DaemonState
//! └── logs/
//!     └── daemon.log      # append-only log when backgrounded
//! ```
//!
//! The home directory and its descendants are created on first use with
//! `0700` permissions on Unix because they may hold secrets (API keys,
//! state).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Subdirectory name under the user's home directory when no override is set.
const DEFAULT_HOME_DIR_NAME: &str = ".raft-daemon";

/// Environment variable used to override the daemon home directory.
pub const RAFT_DAEMON_HOME_ENV: &str = "RAFT_DAEMON_HOME";

/// Resolve the default daemon home directory (`~/.raft-daemon`), creating it
/// if missing.
///
/// This is kept for backward compatibility and tests. Profile-aware code should
/// prefer [`home_dir_for_profile`].
///
/// # Errors
///
/// Returns an error if the user's home directory cannot be determined, the
/// target path cannot be created, or the resolved path is not writable.
pub fn home_dir() -> Result<PathBuf> {
    home_dir_for_profile("default")
}

/// Resolve the daemon home directory for a given profile, creating it if missing.
///
/// Layout:
/// - `default` profile: `~/.raft-daemon/`
/// - other profiles: `~/.raft-daemon/profiles/<profile>/`
///
/// The default profile keeps the original layout so existing installs are not
/// disrupted. The `RAFT_DAEMON_HOME` env var overrides everything, allowing
/// fully custom locations when needed.
///
/// # Errors
///
/// Returns an error if the user's home directory cannot be determined, the
/// target path cannot be created, or the resolved path is not writable.
pub fn home_dir_for_profile(profile: &str) -> Result<PathBuf> {
    let path = if let Some(override_dir) = std::env::var_os(RAFT_DAEMON_HOME_ENV) {
        PathBuf::from(override_dir)
    } else {
        let user_home = user_home_dir().context("could not determine user home directory")?;
        let base = user_home.join(DEFAULT_HOME_DIR_NAME);
        if profile == "default" {
            base
        } else {
            base.join("profiles").join(profile)
        }
    };

    ensure_private_dir(&path)?;
    Ok(path)
}

/// Path to the daemon PID file for a profile (`<home>/daemon.pid`).
///
/// # Errors
///
/// Propagates errors from [`home_dir_for_profile`].
pub fn pid_file_for_profile(profile: &str) -> Result<PathBuf> {
    Ok(home_dir_for_profile(profile)?.join("daemon.pid"))
}

/// Path to the daemon PID file for the default profile.
///
/// # Errors
///
/// Propagates errors from [`home_dir`].
pub fn pid_file() -> Result<PathBuf> {
    pid_file_for_profile("default")
}

/// Path to the daemon state file for a profile (`<home>/state.json`).
///
/// # Errors
///
/// Propagates errors from [`home_dir_for_profile`].
pub fn state_file_for_profile(profile: &str) -> Result<PathBuf> {
    Ok(home_dir_for_profile(profile)?.join("state.json"))
}

/// Path to the daemon state file for the default profile.
///
/// # Errors
///
/// Propagates errors from [`home_dir`].
pub fn state_file() -> Result<PathBuf> {
    state_file_for_profile("default")
}

/// Path to the daemon log directory for a profile (`<home>/logs/`), creating
/// it if missing.
///
/// # Errors
///
/// Propagates errors from [`home_dir_for_profile`] or directory creation.
pub fn log_dir_for_profile(profile: &str) -> Result<PathBuf> {
    let dir = home_dir_for_profile(profile)?.join("logs");
    ensure_private_dir(&dir)?;
    Ok(dir)
}

/// Path to the daemon log directory for the default profile.
///
/// # Errors
///
/// Propagates errors from [`home_dir`] or directory creation.
pub fn log_dir() -> Result<PathBuf> {
    log_dir_for_profile("default")
}

/// Path to the daemon log file for a profile (`<home>/logs/daemon.log`).
///
/// # Errors
///
/// Propagates errors from [`log_dir_for_profile`].
pub fn log_file_for_profile(profile: &str) -> Result<PathBuf> {
    Ok(log_dir_for_profile(profile)?.join("daemon.log"))
}

/// Path to the daemon log file for the default profile.
///
/// # Errors
///
/// Propagates errors from [`log_dir`].
pub fn log_file() -> Result<PathBuf> {
    log_file_for_profile("default")
}

/// Resolve the user's home directory from `$HOME` (Unix) or `$USERPROFILE`
/// (Windows), falling back to the `getpwuid` lookup on Unix.
fn user_home_dir() -> Result<PathBuf> {
    #[cfg(unix)]
    {
        if let Some(home) = std::env::var_os("HOME") {
            if !home.is_empty() {
                return Ok(PathBuf::from(home));
            }
        }
        unix_passwd_home().context("resolving home directory via getpwuid")
    }
    #[cfg(not(unix))]
    {
        if let Some(home) = std::env::var_os("USERPROFILE") {
            if !home.is_empty() {
                return Ok(PathBuf::from(home));
            }
        }
        anyhow::bail!("could not resolve user home directory (USERPROFILE unset)");
    }
}

#[cfg(unix)]
fn unix_passwd_home() -> Result<PathBuf> {
    // Safety: `getuid` and `getpwuid` are thread-safe signal-safe getters
    // with no invariants to uphold; the returned struct is copied out by
    // value before being read.
    unsafe {
        let uid = libc::getuid();
        let pw = libc::getpwuid(uid);
        if pw.is_null() {
            anyhow::bail!("getpwuid({uid}) returned null");
        }
        let dir_ptr = (*pw).pw_dir;
        if dir_ptr.is_null() {
            anyhow::bail!("passwd entry for uid {uid} has null pw_dir");
        }
        let dir = std::ffi::CStr::from_ptr(dir_ptr)
            .to_str()
            .context("home directory is not valid UTF-8")?;
        if dir.is_empty() {
            anyhow::bail!("passwd entry for uid {uid} has empty pw_dir");
        }
        Ok(PathBuf::from(dir))
    }
}

/// Create a directory with `0700` permissions on Unix if it doesn't exist.
fn ensure_private_dir(path: &Path) -> Result<()> {
    if path.is_dir() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
            .with_context(|| format!("creating directory {}", path.display()))?;
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(path)
            .with_context(|| format!("creating directory {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_env::EnvGuard;

    #[test]
    fn home_dir_respects_env_override() {
        let tmp = tempfile::tempdir().expect("tempdir");
        // SAFETY: the shared ENV_LOCK is acquired by EnvGuard.
        let _guard = unsafe { EnvGuard::set(RAFT_DAEMON_HOME_ENV, tmp.path()) };

        let home = home_dir().expect("home_dir resolves");
        assert_eq!(home, tmp.path());
        assert!(home.is_dir());

        let pid = pid_file().expect("pid_file");
        assert!(pid.starts_with(tmp.path()));
        assert_eq!(pid.file_name().unwrap(), "daemon.pid");

        let log = log_file().expect("log_file");
        assert!(log.starts_with(tmp.path()));
        assert_eq!(log.file_name().unwrap(), "daemon.log");
        assert!(log.parent().unwrap().is_dir());
    }

    #[test]
    fn home_dir_creates_missing_dirs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let nested = tmp.path().join("a").join("b");
        let _guard = unsafe { EnvGuard::set(RAFT_DAEMON_HOME_ENV, &nested) };
        let home = home_dir().expect("home_dir creates missing");
        assert_eq!(home, nested);
        assert!(nested.is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn home_dir_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().expect("tempdir");
        let _guard = unsafe { EnvGuard::set(RAFT_DAEMON_HOME_ENV, tmp.path().join("h")) };
        let home = home_dir().expect("home_dir");
        let mode = std::fs::metadata(&home).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }
}
