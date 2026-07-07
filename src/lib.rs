//! # raft-daemon
//!
//! A Rust-native port of the [`@botiverse/raft-daemon`][npm] package providing
//! agent lifecycle management for the Raft platform.
//!
//! [npm]: https://www.npmjs.com/package/@botiverse/raft-daemon
//!
//! ## Overview
//!
//! The daemon connects to a Raft server over WebSocket, manages a pool of AI
//! agents, routes messages, claims tasks, schedules reminders, and tracks
//! per-agent workspaces. Runtime behaviour is pluggable through the
//! [`runtime::Runtime`] trait, with a built-in driver and an optional RustyCLI
//! driver.
//!
//! ## Modules
//!
//! | Module | Responsibility |
//! |--------|----------------|
//! | [`models`] | Shared data structures: agents, servers, computers, tasks, messages, reminders, runtimes |
//! | [`daemon`] | Core daemon: state, agent lifecycle, message routing, task claiming, workspace, APM, tracing |
//! | [`runtime`] | Runtime trait and driver implementations (built-in, RustyCLI) |
//! | [`cli`] | Command-line argument parsing and command dispatch |
//!
//! ## Feature flags
//!
//! - `default` enables `rusty-runtime`, which pulls in the `rusty-cli` crate for
//!   the RustyCLI driver.
//! - `builtin-runtime` enables only the built-in runtime.
//! - `testing` exposes test helpers.
//! - `local-trace` and `trace-upload` configure OpenTelemetry export.
//!
//! ## Example
//!
//! Constructing daemon state and looking up an agent through the manager:
//!
//! ```no_run
//! use std::path::PathBuf;
//! use std::sync::Arc;
//!
//! use raft_daemon::daemon::agent::AgentManager;
//! use raft_daemon::daemon::state::state::DaemonState;
//! use raft_daemon::daemon::state::state_manager::StateMgr;
//! use raft_daemon::models::{Agent, ResetMode, RuntimeConfig};
//!
//! let state = DaemonState::new(
//!     "srv_1".into(),
//!     None,
//!     RuntimeConfig {
//!         model: "gpt-4o".into(),
//!         tools: vec![],
//!         parameters: serde_json::json!({}),
//!     },
//!     "default".into(),
//!     PathBuf::from("/tmp/raft"),
//! );
//! let state: Arc<dyn StateMgr> = Arc::new(state);
//! let agents = AgentManager::new(state);
//! let id = agents.add_agent(Agent::with_defaults(
//!     "ag_1".into(),
//!     "Docs".into(),
//!     "writes docs".into(),
//!     "member".into(),
//!     "gpt-4o".into(),
//!     "cpu_1".into(),
//!     "srv_1".into(),
//!     vec![],
//!     ResetMode::Restart,
//!     "/tmp/raft/ag_1".into(),
//! ));
//! assert!(agents.get_agent(&id).is_some());
//! ```

pub mod cli;
pub mod daemon;
pub mod models;
pub mod runtime;
