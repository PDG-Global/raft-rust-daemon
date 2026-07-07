//! State manager abstraction.
//!
//! [`StateMgr`] is the trait every persistent store must implement so the rest
//! of the daemon can stay agnostic of how state is loaded and written. The
//! default in-process implementation lives in [`crate::daemon::state::state`]
//! as [`DaemonState`].

use anyhow::Result;
use dashmap::DashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::daemon::state::state::DaemonState;
use crate::models::{Agent, Computer, Message, Reminder, RuntimeConfig, Server, Task};

/// Persistent store backing the daemon.
///
/// Implementations own every collection the daemon touches (computers, agents,
/// tasks, messages, reminders) plus the runtime configuration and workspace
/// path, and are responsible for serialising themselves to disk via [`save`]
/// and [`save_to`].
///
/// [`save`]: StateMgr::save
/// [`save_to`]: StateMgr::save_to
pub trait StateMgr: Send + Sync {
    /// Get a computer by ID.
    fn get_computer(&self, computer_id: &str) -> Option<Computer>;

    /// Get an agent by ID.
    fn get_agent(&self, agent_id: &str) -> Option<Agent>;

    /// Get a task by ID.
    fn get_task(&self, task_id: &str) -> Option<Task>;

    /// Get a message by ID.
    fn get_message(&self, message_id: &str) -> Option<Message>;

    /// Get a reminder by ID.
    fn get_reminder(&self, reminder_id: &str) -> Option<Reminder>;

    /// Get all computers.
    fn computers(&self) -> &DashMap<String, Computer>;

    /// Get all agents.
    fn agents(&self) -> &DashMap<String, Agent>;

    /// Get all tasks.
    fn tasks(&self) -> &DashMap<String, Task>;

    /// Get all messages.
    fn messages(&self) -> &DashMap<String, Message>;

    /// Get all reminders.
    fn reminders(&self) -> &DashMap<String, Reminder>;

    /// Get the runtime config.
    fn runtime_config(&self) -> &RuntimeConfig;

    /// Get the profile.
    fn profile(&self) -> &str;

    /// Get the workspace.
    fn workspace(&self) -> &PathBuf;

    /// Get the server ID.
    fn server_id(&self) -> &str;

    /// Get the server.
    fn server(&self) -> Option<&Server>;

    /// Get the state file path.
    fn get_state_file(&self) -> PathBuf;

    /// Save the state to disk.
    fn save(&self) -> Result<()>;

    /// Save the state to a specific path.
    fn save_to(&self, path: &PathBuf) -> Result<()>;

    /// Get the underlying daemon state (for reads).
    fn get_state(&self) -> Arc<DaemonState>;

    /// Add a reminder.
    fn add_reminder(&self, reminder: Reminder);
}

/// Alias for a boxed dyn StateMgr.
pub type StateManager = Box<dyn StateMgr>;
