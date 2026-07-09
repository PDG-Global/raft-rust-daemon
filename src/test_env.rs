//! Test-only helpers for environment-variable isolation.
//!
//! Environment variables are process-wide mutable state. Tests that read or
//! write them concurrently are inherently racy. This module provides a single
//! global reentrant lock and a small RAII guard so tests can mutate env vars
//! in a critical section and restore the previous value on drop.

use std::ffi::{OsStr, OsString};

use parking_lot::ReentrantMutex;

/// Global reentrant lock serializing env-var mutations across threads.
///
/// Reentrant so that a single test can create multiple guards (e.g. one per
/// env var) without deadlocking.
static ENV_LOCK: ReentrantMutex<()> = ReentrantMutex::new(());

/// RAII guard that captures an env var's current value, sets it to a new
/// value, and restores the prior value on drop.  The guard is reentrant on the
/// same thread so multiple guards can be created within one test.
pub(crate) struct EnvGuard {
    key: &'static str,
    prior: Option<OsString>,
    _lock: parking_lot::ReentrantMutexGuard<'static, ()>,
}

impl EnvGuard {
    /// Set `key` to `value` and return a guard that restores it on drop.
    ///
    /// # Safety
    ///
    /// This must be called while no other thread reads `key`. The global lock
    /// provides this within the test process.
    pub(crate) unsafe fn set(key: &'static str, value: impl AsRef<OsStr>) -> Self {
        let _lock = ENV_LOCK.lock();
        let prior = std::env::var_os(key);
        unsafe { std::env::set_var(key, value) }
        Self { key, prior, _lock }
    }

    /// Remove `key` and return a guard that restores it on drop.
    ///
    /// # Safety
    ///
    /// Same as [`EnvGuard::set`].
    pub(crate) unsafe fn remove(key: &'static str) -> Self {
        let _lock = ENV_LOCK.lock();
        let prior = std::env::var_os(key);
        unsafe { std::env::remove_var(key) }
        Self { key, prior, _lock }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            match self.prior.take() {
                Some(val) => std::env::set_var(self.key, val),
                None => std::env::remove_var(self.key),
            }
        }
    }
}
