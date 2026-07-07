//! Computer model.
//!
//! A computer is a machine connected to a server. Agents run on computers.

use serde::{Deserialize, Serialize};

/// The status of a computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComputerStatus {
    /// The computer is online and connected.
    Online,
    /// The computer is starting up.
    Starting,
    /// The computer is offline.
    Offline,
    /// The computer is being removed.
    Removing,
}

impl std::fmt::Display for ComputerStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComputerStatus::Online => write!(f, "online"),
            ComputerStatus::Starting => write!(f, "starting"),
            ComputerStatus::Offline => write!(f, "offline"),
            ComputerStatus::Removing => write!(f, "removing"),
        }
    }
}

/// A computer connected to a server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Computer {
    /// Unique identifier for the computer.
    pub id: String,
    /// The computer's display name.
    pub name: String,
    /// The computer's status.
    pub status: ComputerStatus,
    /// The server the computer belongs to.
    pub server_id: String,
    /// The daemon's API key for this computer.
    pub api_key: String,
    /// The daemon's server URL.
    pub server_url: String,
    /// The computer's setup command.
    pub setup_command: String,
    /// When the computer was created.
    pub created_at: i64,
    /// When the computer was last updated.
    pub updated_at: i64,
    /// When the computer was last seen.
    pub last_seen: i64,
    /// Agents running on the computer.
    pub agent_ids: Vec<String>,
    /// Detected runtimes on the computer.
    pub runtimes: Vec<String>,
    /// Computer-specific metadata.
    #[serde(flatten)]
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

impl Computer {
    /// Create a new computer.
    pub fn new(
        id: String,
        name: String,
        server_id: String,
        api_key: String,
        server_url: String,
        setup_command: String,
    ) -> Self {
        Self {
            id,
            name,
            status: ComputerStatus::Online,
            server_id,
            api_key,
            server_url,
            setup_command,
            created_at: 0,
            updated_at: 0,
            last_seen: 0,
            agent_ids: Vec::new(),
            runtimes: Vec::new(),
            metadata: serde_json::Map::new(),
        }
    }

    /// Create a new computer with defaults.
    pub fn with_defaults(
        id: String,
        name: String,
        server_id: String,
        api_key: String,
        server_url: String,
        setup_command: String,
    ) -> Self {
        Self {
            id,
            name,
            status: ComputerStatus::Online,
            server_id,
            api_key,
            server_url,
            setup_command,
            created_at: 0,
            updated_at: 0,
            last_seen: 0,
            agent_ids: Vec::new(),
            runtimes: Vec::new(),
            metadata: serde_json::Map::new(),
        }
    }

    /// Set the computer's status.
    pub fn set_status(&mut self, status: ComputerStatus) {
        self.status = status;
        self.last_seen = chrono::Utc::now().timestamp_millis();
    }

    /// Add an agent to the computer.
    pub fn add_agent(&mut self, agent_id: String) {
        if !self.agent_ids.contains(&agent_id) {
            self.agent_ids.push(agent_id);
        }
    }

    /// Remove an agent from the computer.
    pub fn remove_agent(&mut self, agent_id: &str) {
        self.agent_ids.retain(|id| id != agent_id);
    }

    /// Add a runtime to the computer.
    pub fn add_runtime(&mut self, runtime: String) {
        if !self.runtimes.contains(&runtime) {
            self.runtimes.push(runtime);
        }
    }

    /// Check if an agent is on the computer.
    pub fn has_agent(&self, agent_id: &str) -> bool {
        self.agent_ids.contains(&agent_id.to_string())
    }

    /// Check if a runtime is on the computer.
    pub fn has_runtime(&self, runtime: &str) -> bool {
        self.runtimes.contains(&runtime.to_string())
    }

    /// Check if the computer is online.
    pub fn is_online(&self) -> bool {
        self.status == ComputerStatus::Online
    }

    /// Check if the computer is offline.
    pub fn is_offline(&self) -> bool {
        self.status == ComputerStatus::Offline
    }

    /// Check if the computer is starting.
    pub fn is_starting(&self) -> bool {
        self.status == ComputerStatus::Starting
    }

    /// Check if the computer is being removed.
    pub fn is_removing(&self) -> bool {
        self.status == ComputerStatus::Removing
    }
}
