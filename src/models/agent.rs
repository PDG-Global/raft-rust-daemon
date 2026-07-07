//! Agent model.
//!
//! An agent is an AI teammate in a server. It has a persistent identity,
//! joins channels, claims tasks, and remembers context across sessions.

use serde::{Deserialize, Serialize};

/// The status of an agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentStatus {
    /// The agent is running and available.
    Online,
    /// The agent is actively working on something.
    Busy,
    /// The agent hit an error.
    Error,
    /// The agent's process is not running.
    Offline,
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentStatus::Online => write!(f, "online"),
            AgentStatus::Busy => write!(f, "busy"),
            AgentStatus::Error => write!(f, "error"),
            AgentStatus::Offline => write!(f, "offline"),
        }
    }
}

/// The reset mode for an agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResetMode {
    /// Resume the existing session.
    Restart,
    /// Clear the conversation context.
    SessionReset,
    /// Clear both conversation context and workspace.
    FullReset,
}

impl std::fmt::Display for ResetMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResetMode::Restart => write!(f, "restart"),
            ResetMode::SessionReset => write!(f, "session_reset"),
            ResetMode::FullReset => write!(f, "full_reset"),
        }
    }
}

/// Configuration for creating an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// The agent's display name and @mention handle.
    pub name: String,
    /// What the agent does.
    pub description: String,
    /// The runtime that powers the agent.
    pub runtime: String,
    /// The role the agent has in the server.
    pub role: String,
    /// How to reset the agent.
    pub reset_mode: ResetMode,
}

/// An agent in a server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    /// Unique identifier for the agent.
    pub id: String,
    /// The agent's display name.
    pub name: String,
    /// What the agent does.
    pub description: String,
    /// The agent's role in the server.
    pub role: String,
    /// The runtime powering the agent.
    pub runtime: String,
    /// The current status of the agent.
    pub status: AgentStatus,
    /// The computer the agent runs on.
    pub computer_id: String,
    /// The server the agent belongs to.
    pub server_id: String,
    /// Channels the agent has joined.
    pub channel_ids: Vec<String>,
    /// The agent's runtime configuration.
    pub runtime_config: crate::models::RuntimeConfig,
    /// When the agent was created.
    pub created_at: i64,
    /// When the agent was last active.
    pub last_active: i64,
    /// When the agent was last seen.
    pub last_seen: i64,
    /// The agent's reset mode.
    pub reset_mode: ResetMode,
    /// The agent's workspace directory.
    pub workspace: String,
    /// Agent-specific metadata.
    #[serde(flatten)]
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

impl Agent {
    /// Create a new agent.
    ///
    /// Constructors for plain data records reasonably exceed clippy's
    /// argument limit; the explicitness aids call-site readability.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        name: String,
        description: String,
        role: String,
        runtime: String,
        computer_id: String,
        server_id: String,
        channel_ids: Vec<String>,
        runtime_config: crate::models::RuntimeConfig,
        reset_mode: ResetMode,
        workspace: String,
    ) -> Self {
        Self {
            id,
            name,
            description,
            role,
            runtime,
            status: AgentStatus::Offline,
            computer_id,
            server_id,
            channel_ids,
            runtime_config,
            created_at: 0,
            last_active: 0,
            last_seen: 0,
            reset_mode,
            workspace,
            metadata: serde_json::Map::new(),
        }
    }

    /// Create a new agent with default values.
    #[allow(clippy::too_many_arguments)]
    pub fn with_defaults(
        id: String,
        name: String,
        description: String,
        role: String,
        runtime: String,
        computer_id: String,
        server_id: String,
        channel_ids: Vec<String>,
        reset_mode: ResetMode,
        workspace: String,
    ) -> Self {
        Self {
            id,
            name,
            description,
            role,
            runtime,
            status: AgentStatus::Offline,
            computer_id,
            server_id,
            channel_ids,
            runtime_config: crate::models::RuntimeConfig {
                model: String::new(),
                tools: Vec::new(),
                parameters: serde_json::json!({}),
            },
            created_at: 0,
            last_active: 0,
            last_seen: 0,
            reset_mode,
            workspace,
            metadata: serde_json::Map::new(),
        }
    }

    /// Set the agent's status.
    pub fn set_status(&mut self, status: AgentStatus) {
        self.status = status;
        self.last_active = chrono::Utc::now().timestamp_millis();
    }

    /// Add a channel to the agent's joined channels.
    pub fn join_channel(&mut self, channel_id: String) {
        if !self.channel_ids.contains(&channel_id) {
            self.channel_ids.push(channel_id);
        }
    }

    /// Leave a channel from the agent's joined channels.
    pub fn leave_channel(&mut self, channel_id: &str) {
        self.channel_ids.retain(|id| id != channel_id);
    }

    /// Check if the agent is a member of a channel.
    pub fn is_member_of_channel(&self, channel_id: &str) -> bool {
        self.channel_ids.contains(&channel_id.to_string())
    }

    /// Check if the agent is online.
    pub fn is_online(&self) -> bool {
        self.status == AgentStatus::Online
    }

    /// Check if the agent is busy.
    pub fn is_busy(&self) -> bool {
        self.status == AgentStatus::Busy
    }

    /// Check if the agent is an error.
    pub fn is_error(&self) -> bool {
        self.status == AgentStatus::Error
    }

    /// Check if the agent is offline.
    pub fn is_offline(&self) -> bool {
        self.status == AgentStatus::Offline
    }

    /// Check if the agent is a member of a server.
    pub fn is_member_of_server(&self, server_id: &str) -> bool {
        self.server_id == server_id
    }

    /// Check if the agent is an admin.
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }

    /// Check if the agent is a member.
    pub fn is_member(&self) -> bool {
        self.role == "member"
    }
}
