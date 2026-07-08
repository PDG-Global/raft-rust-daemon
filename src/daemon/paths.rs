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

/// Resolve the daemon home directory, creating it if missing.
///
/// # Errors
///
/// Returns an error if the user's home directory cannot be determined, the
/// target path cannot be created, or the resolved path is not writable.
pub fn home_dir() -> Result<PathBuf> {
    let path = if let Some(override_dir) = std::env::var_os(RAFT_DAEMON_HOME_ENV) {
        PathBuf::from(override_dir)
    } else {
        let user_home = user_home_dir().context("could not determine user home directory")?;
        user_home.join(DEFAULT_HOME_DIR_NAME)
    };

    ensure_private_dir(&path)?;
    Ok(path)
}

/// Path to the daemon PID file (`<home>/daemon.pid`).
///
/// # Errors
///
/// Propagates errors from [`home_dir`].
pub fn pid_file() -> Result<PathBuf> {
    Ok(home_dir()?.join("daemon.pid"))
}

/// Path to the daemon state file (`<home>/state.json`).
///
/// # Errors
///
/// Propagates errors from [`home_dir`].
pub fn state_file() -> Result<PathBuf> {
    Ok(home_dir()?.join("state.json"))
}

/// Path to the daemon log directory (`<home>/logs/`), creating it if missing.
///
/// # Errors
///
/// Propagates errors from [`home_dir`] or directory creation.
pub fn log_dir() -> Result<PathBuf> {
    let dir = home_dir()?.join("logs");
    ensure_private_dir(&dir)?;
    Ok(dir)
}

/// Path to the daemon log file (`<home>/logs/daemon.log`).
///
/// # Errors
///
/// Propagates errors from [`log_dir`].
pub fn log_file() -> Result<PathBuf> {
    Ok(log_dir()?.join("daemon.log"))
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

    #[test]
    fn home_dir_respects_env_override() {
        let tmp = tempfile::tempdir().expect("tempdir");
        // SAFETY: tests are single-threaded by default; env var scoping is
        // local and the variable is restored on drop.
        let _guard = test_env::set_env(RAFT_DAEMON_HOME_ENV, tmp.path());

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
        let _guard = test_env::set_env(RAFT_DAEMON_HOME_ENV, &nested);
        let home = home_dir().expect("home_dir creates missing");
        assert_eq!(home, nested);
        assert!(nested.is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn home_dir_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().expect("tempdir");
        let _guard = test_env::set_env(RAFT_DAEMON_HOME_ENV, tmp.path().join("h"));
        let home = home_dir().expect("home_dir");
        let mode = std::fs::metadata(&home).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    /// Test helper: scope an environment variable to a single test and restore
    /// the prior value on drop. Tests in this file run single-threaded so this
    /// is safe.
    struct EnvGuard {
        key: &'static str,
        prior: Option<std::ffi::OsString>,
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // Safety: tests in this module are single-threaded; the env var
            // mutation here is restoring the prior value, which is exactly
            // what the safety contract requires (no concurrent reads of env).
            unsafe {
                match self.prior.take() {
                    Some(val) => std::env::set_var(self.key, val),
                    None => std::env::remove_var(self.key),
                }
            }
        }
    }

    mod test_env {
        use super::*;
        pub(super) fn set_env(key: &'static str, value: impl AsRef<std::path::Path>) -> EnvGuard {
            let prior = std::env::var_os(key);
            // Safety: tests are single-threaded within this module.
            unsafe {
                std::env::set_var(key, value.as_ref());
            }
            EnvGuard { key, prior }
        }
    }
}
