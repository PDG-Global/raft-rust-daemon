//! Daemon state.
//!
//! [`DaemonState`] is the default in-memory [`StateMgr`](crate::daemon::state::state_manager::StateMgr) implementation. It
//! keeps every live collection in [`DashMap`]s for lock-free concurrent reads
//! and serialises to a single JSON file on disk.

use anyhow::Result;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

use crate::models::{Agent, Computer, Message, Reminder, RuntimeConfig, Server, Task};

/// In-memory snapshot of everything the daemon knows about.
///
/// One instance lives for the lifetime of the process and is shared between
/// the agent manager, the WebSocket server, and the task scheduler. Mutations
/// go through the methods on [`StateMgr`](crate::daemon::state::state_manager::StateMgr);
/// reads can borrow the underlying [`DashMap`] directly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonState {
    /// The server ID.
    pub server_id: String,
    /// The server object.
    pub server: Option<Server>,
    /// The computers in the server.
    pub computers: DashMap<String, Computer>,
    /// The agents in the server.
    pub agents: DashMap<String, Agent>,
    /// The tasks in the server.
    pub tasks: DashMap<String, Task>,
    /// The messages in the server.
    pub messages: DashMap<String, Message>,
    /// The reminders in the server.
    pub reminders: DashMap<String, Reminder>,
    /// The runtime configuration.
    pub runtime_config: RuntimeConfig,
    /// The current profile.
    pub profile: String,
    /// The workspace directory.
    pub workspace: PathBuf,
    /// When the daemon started.
    pub started_at: i64,
    /// When the daemon was last updated.
    pub updated_at: i64,
    /// Daemon-specific metadata.
    #[serde(flatten)]
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

impl DaemonState {
    /// Create a new daemon state.
    pub fn new(
        server_id: String,
        server: Option<Server>,
        runtime_config: RuntimeConfig,
        profile: String,
        workspace: PathBuf,
    ) -> Self {
        Self {
            server_id,
            server,
            computers: DashMap::new(),
            agents: DashMap::new(),
            tasks: DashMap::new(),
            messages: DashMap::new(),
            reminders: DashMap::new(),
            runtime_config,
            profile,
            workspace,
            started_at: chrono::Utc::now().timestamp_millis(),
            updated_at: chrono::Utc::now().timestamp_millis(),
            metadata: serde_json::Map::new(),
        }
    }

    /// Load state from a file.
    pub fn load(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let state: DaemonState = serde_json::from_str(&content)?;
        Ok(state)
    }

    /// Save state to a file.
    ///
    /// The file is written with `0600` permissions because it may contain
    /// secrets such as API keys.
    pub fn save(&self, path: &PathBuf) -> Result<()> {
        use std::os::unix::fs::OpenOptionsExt;
        let content = serde_json::to_string_pretty(self)?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        use std::io::Write;
        file.write_all(content.as_bytes())?;
        Ok(())
    }

    /// Get a computer by ID.
    pub fn get_computer(&self, computer_id: &str) -> Option<Computer> {
        self.computers.get(computer_id).map(|r| r.clone())
    }

    /// Get an agent by ID.
    pub fn get_agent(&self, agent_id: &str) -> Option<Agent> {
        self.agents.get(agent_id).map(|r| r.clone())
    }

    /// Get a task by ID.
    pub fn get_task(&self, task_id: &str) -> Option<Task> {
        self.tasks.get(task_id).map(|r| r.clone())
    }

    /// Get a message by ID.
    pub fn get_message(&self, message_id: &str) -> Option<Message> {
        self.messages.get(message_id).map(|r| r.clone())
    }

    /// Get a reminder by ID.
    pub fn get_reminder(&self, reminder_id: &str) -> Option<Reminder> {
        self.reminders.get(reminder_id).map(|r| r.clone())
    }

    /// Get all computers.
    pub fn computers(&self) -> &DashMap<String, Computer> {
        &self.computers
    }

    /// Get all agents.
    pub fn agents(&self) -> &DashMap<String, Agent> {
        &self.agents
    }

    /// Get all tasks.
    pub fn tasks(&self) -> &DashMap<String, Task> {
        &self.tasks
    }

    /// Get all messages.
    pub fn messages(&self) -> &DashMap<String, Message> {
        &self.messages
    }

    /// Get all reminders.
    pub fn reminders(&self) -> &DashMap<String, Reminder> {
        &self.reminders
    }

    /// Get the runtime config.
    pub fn runtime_config(&self) -> &RuntimeConfig {
        &self.runtime_config
    }

    /// Get the profile.
    pub fn profile(&self) -> &str {
        &self.profile
    }

    /// Get the workspace.
    pub fn workspace(&self) -> &PathBuf {
        &self.workspace
    }

    /// Get the server ID.
    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    /// Get the server.
    pub fn server(&self) -> Option<&Server> {
        self.server.as_ref()
    }

    /// Update the state.
    pub fn update(&mut self) {
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }
}

/// State manager implementation for DaemonState.
impl crate::daemon::state::state_manager::StateMgr for crate::daemon::state::state::DaemonState {
    fn get_computer(&self, computer_id: &str) -> Option<crate::models::Computer> {
        self.computers.get(computer_id).map(|r| r.clone())
    }

    fn get_agent(&self, agent_id: &str) -> Option<crate::models::Agent> {
        self.agents.get(agent_id).map(|r| r.clone())
    }

    fn get_task(&self, task_id: &str) -> Option<crate::models::Task> {
        self.tasks.get(task_id).map(|r| r.clone())
    }

    fn get_message(&self, message_id: &str) -> Option<crate::models::Message> {
        self.messages.get(message_id).map(|r| r.clone())
    }

    fn get_reminder(&self, reminder_id: &str) -> Option<crate::models::Reminder> {
        self.reminders.get(reminder_id).map(|r| r.clone())
    }

    fn computers(&self) -> &DashMap<String, crate::models::Computer> {
        &self.computers
    }

    fn agents(&self) -> &DashMap<String, crate::models::Agent> {
        &self.agents
    }

    fn tasks(&self) -> &DashMap<String, crate::models::Task> {
        &self.tasks
    }

    fn messages(&self) -> &DashMap<String, crate::models::Message> {
        &self.messages
    }

    fn reminders(&self) -> &DashMap<String, crate::models::Reminder> {
        &self.reminders
    }

    fn runtime_config(&self) -> &crate::models::RuntimeConfig {
        &self.runtime_config
    }

    fn profile(&self) -> &str {
        &self.profile
    }

    fn workspace(&self) -> &std::path::PathBuf {
        &self.workspace
    }

    fn server_id(&self) -> &str {
        &self.server_id
    }

    fn server(&self) -> Option<&crate::models::Server> {
        self.server.as_ref()
    }

    fn get_state_file(&self) -> std::path::PathBuf {
        self.workspace.join("state.json")
    }

    fn save(&self) -> Result<()> {
        let path = self.workspace.join("state.json");
        self.save_to(&path)
    }

    fn save_to(&self, path: &std::path::PathBuf) -> Result<()> {
        self.save(path)
    }

    fn get_state(&self) -> Arc<DaemonState> {
        Arc::new(self.clone())
    }

    fn add_reminder(&self, reminder: crate::models::Reminder) {
        self.reminders.insert(reminder.id.clone(), reminder);
    }
}
