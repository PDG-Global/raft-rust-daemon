//! CLI arguments for the Raft daemon.

use anyhow::Result;
use clap::Parser;

/// The Raft daemon CLI.
#[derive(Debug, Parser)]
#[command(name = "raft-daemon")]
#[command(author = "Raft Contributors")]
#[command(version = "0.1.0")]
#[command(about = "Raft daemon - Agent lifecycle management")]
#[command(long_about = "
Raft daemon is a background service that manages AI agents in a Raft server.

It handles:
- Agent lifecycle (start, stop, restart, reset)
- Message routing and delivery
- Task claiming and completion
- Reminders and scheduled tasks
- Workspace management

Usage:
  raft-daemon start --server-url <url> --api-key <key>
  raft-daemon stop
  raft-daemon status
")]
pub struct CliArgs {
    /// The profile to use.
    #[arg(short, long, default_value = "default")]
    pub profile: String,

    /// Override the server URL.
    #[arg(long)]
    pub server_url: Option<String>,

    /// Override the API key.
    #[arg(long)]
    pub api_key: Option<String>,

    /// Enable debug mode.
    #[arg(long)]
    pub debug: bool,

    /// Enable verbose logging.
    #[arg(short, long)]
    pub verbose: bool,

    /// Run the daemon in the foreground instead of detaching.
    ///
    /// When invoking `start`, the default behaviour is to spawn a detached
    /// child and return. Pass `--foreground` (either before or after the
    /// subcommand) to take over the terminal and run the daemon loop in the
    /// current process. This flag is also set internally by the spawned
    /// background child.
    #[arg(long)]
    pub foreground: bool,

    /// The command to run.
    #[arg(trailing_var_arg = true)]
    pub command: Vec<String>,
}

impl CliArgs {
    /// Get the effective server URL.
    pub fn server_url(&self) -> String {
        self.server_url.clone().unwrap_or_else(|| {
            // Default from profile or environment
            std::env::var("RAFT_SERVER_URL").unwrap_or_else(|_| "api.raft.build".to_string())
        })
    }

    /// Get the effective API key.
    ///
    /// Returns an error if no key is configured, rather than silently
    /// substituting a placeholder that would be sent to the server as an
    /// invalid credential.
    ///
    /// # Errors
    ///
    /// Returns an error if no API key is provided via `--api-key`, a profile,
    /// or the `RAFT_API_KEY` environment variable.
    pub fn api_key(&self) -> Result<String> {
        if let Some(key) = &self.api_key {
            return Ok(key.clone());
        }
        std::env::var("RAFT_API_KEY").map_err(|_| {
            anyhow::anyhow!(
                "no API key provided: set --api-key, configure it in a profile, \
                 or export RAFT_API_KEY"
            )
        })
    }

    /// Get the profile name.
    pub fn profile(&self) -> String {
        self.profile.clone()
    }

    /// Check if debug mode is enabled.
    pub fn is_debug(&self) -> bool {
        self.debug
    }

    /// Check if verbose mode is enabled.
    pub fn is_verbose(&self) -> bool {
        self.verbose
    }

    /// Check if the daemon should run in the foreground.
    ///
    /// Returns `true` if either `--foreground` was parsed as a top-level flag
    /// (e.g. `raft-daemon --foreground start`) **or** it appears in the
    /// trailing command vector (e.g. `raft-daemon start --foreground`). The
    /// latter is not parsed by clap because of `trailing_var_arg`, so we scan
    /// the command tail explicitly.
    pub fn is_foreground(&self) -> bool {
        self.foreground || self.command.iter().any(|c| c == "--foreground")
    }

    /// Get the command arguments.
    pub fn command(&self) -> Vec<String> {
        self.command.clone()
    }
}
