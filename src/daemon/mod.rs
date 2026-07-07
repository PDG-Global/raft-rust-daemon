//! # Daemon core
//!
//! In-process state and orchestration that backs the running daemon.
//!
//! - [`state`]: persistent [`DaemonState`] and the [`StateMgr`] trait
//! - [`agent`]: the [`AgentManager`] owns the live agent pool and routes work
//! - [`computer`]: tracking of machines the daemon can schedule onto
//! - [`server`]: WebSocket connection and protocol handling for a Raft server
//! - [`task`]: task lifecycle and claiming
//! - [`message`]: inbound/outbound message routing
//! - [`reminder`]: scheduling and dispatch of timed reminders
//! - [`workspace`]: per-agent filesystem workspaces
//! - [`runtime`]: runtime lifecycle glue
//! - [`apm`]: application performance monitoring hooks
//! - [`trace`]: OpenTelemetry tracing setup
//!
//! [`DaemonState`]: state::DaemonState
//! [`StateMgr`]: state::StateMgr
//! [`AgentManager`]: agent::AgentManager

pub mod agent;
pub mod apm;
pub mod computer;
pub mod message;
pub mod reminder;
pub mod runtime;
pub mod server;
pub mod state;
pub mod task;
pub mod trace;
pub mod workspace;

pub use workspace::*;
