//! # Models
//!
//! Shared data structures exchanged between the daemon, the Raft server, and
//! the runtimes. Every model is `Serialize`/`Deserialize` so it can be
//! persisted to state files or sent over the wire.
//!
//! Re-exports the per-domain submodules:
//!
//! - [`agent`]: agents, their status, reset modes, and configuration
//! - [`server`]: server identity and connection details
//! - [`computer`]: the machines a daemon runs on
//! - [`task`]: units of work an agent can claim
//! - [`message`]: chat messages routed between agents and channels
//! - [`reminder`]: scheduled, time-based reminders
//! - [`runtime`]: runtime instances and their configuration
//! - [`response`]: normalised API response envelopes

pub mod agent;
pub mod computer;
pub mod message;
pub mod reminder;
pub mod response;
pub mod runtime;
pub mod server;
pub mod task;

pub use agent::*;
pub use computer::*;
pub use message::*;
pub use reminder::*;
pub use response::*;
pub use runtime::*;
pub use server::*;
pub use task::*;
