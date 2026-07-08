//! PID file management for the daemon.
//!
//! A single PID file at `<home>/daemon.pid` records the running daemon's
//! process ID. It is used by `stop` and `status` to address the live process
//! and is removed by the daemon on graceful shutdown. Writes are `0600` to
//! avoid leaking process metadata on shared systems.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

/// Read the PID from a PID file.
///
/// Returns `Ok(None)` if the file does not exist. Returns an error if the
/// file exists but cannot be read or does not contain a valid integer.
///
/// # Errors
///
/// See above.
pub fn read_pid(path: &Path) -> Result<Option<i32>> {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            let trimmed = content.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            let pid: i32 = trimmed
                .parse()
                .with_context(|| format!("invalid PID in {}", path.display()))?;
            Ok(Some(pid))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).with_context(|| format!("reading PID file {}", path.display())),
    }
}

/// Write the PID file with `0600` permissions on Unix, creating its parent
/// directory if missing.
///
/// # Errors
///
/// Returns an error if the parent directory cannot be created or the file
/// cannot be written.
pub fn write_pid(path: &Path, pid: i32) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating parent for {}", path.display()))?;
    }

    let mut open_opts = OpenOptions::new();
    open_opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    open_opts.mode(0o600);

    let mut file = open_opts
        .open(path)
        .with_context(|| format!("opening PID file {}", path.display()))?;
    writeln!(file, "{pid}").with_context(|| format!("writing PID file {}", path.display()))?;

    // Belt-and-braces: ensure the mode is correct even if the file already
    // existed with looser permissions from a previous run.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("setting permissions on {}", path.display()))?;
    }

    Ok(())
}

/// Remove the PID file if it exists. Best-effort; errors are swallowed
/// because shutdown must not fail on a stale file.
pub fn remove_pid(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// Probe whether a process is alive by sending signal 0.
///
/// Returns `true` if the process exists (including when we lack permission to
/// signal it). Returns `false` only when the OS confirms the PID is unused.
#[cfg(unix)]
pub fn is_alive(pid: i32) -> bool {
    // Safety: kill(2) with signal 0 is defined to perform no signal delivery;
    // it only checks for process existence and permission. The only state
    // touched is `errno`, which is thread-local on this platform.
    let rc = unsafe { libc::kill(pid, 0) };
    if rc == 0 {
        return true;
    }
    let err = std::io::Error::last_os_error();
    match err.raw_os_error() {
        Some(libc::ESRCH) => false,
        // EPERM means the process exists but is owned by another user.
        _ => true,
    }
}

#[cfg(not(unix))]
pub fn is_alive(pid: i32) -> bool {
    // Best-effort on non-Unix: assume alive. Non-Unix daemon support is not
    // exercised today (the daemon spawns via setsid on Unix only).
    let _ = pid;
    true
}

/// Send a Unix signal to a process. Returns `true` on success.
#[cfg(unix)]
pub fn send_signal(pid: i32, sig: i32) -> bool {
    // Safety: kill(2) takes a PID and signal; no invariants to uphold beyond
    // passing a valid signal number, which the caller is responsible for.
    unsafe { libc::kill(pid, sig) == 0 }
}

#[cfg(not(unix))]
pub fn send_signal(_pid: i32, _sig: i32) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_pid_returns_none_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("missing.pid");
        assert!(read_pid(&path).unwrap().is_none());
    }

    #[test]
    fn write_then_read_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("daemon.pid");
        write_pid(&path, 4242).unwrap();
        assert_eq!(read_pid(&path).unwrap(), Some(4242));
    }

    #[test]
    fn read_pid_ignores_whitespace() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("daemon.pid");
        std::fs::write(&path, "  1337\n").unwrap();
        assert_eq!(read_pid(&path).unwrap(), Some(1337));
    }

    #[test]
    fn read_pid_errors_on_garbage() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("daemon.pid");
        std::fs::write(&path, "not a number").unwrap();
        assert!(read_pid(&path).is_err());
    }

    #[test]
    fn empty_pid_file_yields_none() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("daemon.pid");
        std::fs::write(&path, "   \n").unwrap();
        assert!(read_pid(&path).unwrap().is_none());
    }

    #[test]
    fn remove_pid_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nope.pid");
        // Should not panic on missing file.
        remove_pid(&path);
        write_pid(&path, 1).unwrap();
        remove_pid(&path);
        assert!(!path.exists());
        // Removing again is still fine.
        remove_pid(&path);
    }

    #[test]
    fn self_is_alive() {
        let me = pid_from_process(std::process::id());
        assert!(is_alive(me));
    }

    #[cfg(unix)]
    #[test]
    fn nonexistent_process_is_not_alive() {
        // PID 0 is the scheduler and we may not signal it; use a very high
        // PID that is overwhelmingly unlikely to exist.
        let unlikely = 2_000_000;
        // Sanity-check via the same primitive so the assertion is meaningful.
        let rc = unsafe { libc::kill(unlikely, 0) };
        if rc == 0 {
            // Process exists; skip the assertion rather than fail.
            return;
        }
        assert!(!is_alive(unlikely));
    }

    /// Helper for tests: convert `u32` PID to `i32` without panicking.
    fn pid_from_process(pid: u32) -> i32 {
        i32::try_from(pid).unwrap_or(i32::MAX)
    }
}
