//! Runtime model.
//!
//! A runtime is the AI engine that powers an agent.

use serde::{Deserialize, Serialize};

/// A runtime configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// The AI model to use.
    pub model: String,
    /// Tools the runtime has access to.
    pub tools: Vec<String>,
    /// Additional parameters for the model.
    pub parameters: serde_json::Value,
}

/// A runtime instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Runtime {
    /// Unique identifier for the runtime.
    pub id: String,
    /// The runtime's name.
    pub name: String,
    /// The runtime's type.
    pub r#type: String,
    /// The model the runtime uses.
    pub model: String,
    /// The runtime's status.
    pub status: RuntimeStatus,
    /// The computer the runtime runs on.
    pub computer_id: String,
    /// When the runtime was created.
    pub created_at: i64,
    /// When the runtime was last updated.
    pub updated_at: i64,
    /// When the runtime was last used.
    pub last_used_at: i64,
    /// The runtime's API key.
    pub api_key: String,
    /// The runtime's server URL.
    pub server_url: String,
    /// Runtime-specific metadata.
    #[serde(flatten)]
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

/// The status of a runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeStatus {
    /// The runtime is ready to use.
    Ready,
    /// The runtime is starting up.
    Starting,
    /// The runtime is busy.
    Busy,
    /// The runtime has an error.
    Error,
    /// The runtime is offline.
    Offline,
}

impl std::fmt::Display for RuntimeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeStatus::Ready => write!(f, "ready"),
            RuntimeStatus::Starting => write!(f, "starting"),
            RuntimeStatus::Busy => write!(f, "busy"),
            RuntimeStatus::Error => write!(f, "error"),
            RuntimeStatus::Offline => write!(f, "offline"),
        }
    }
}

impl Runtime {
    /// Create a new runtime.
    pub fn new(
        id: String,
        name: String,
        r#type: String,
        model: String,
        computer_id: String,
        api_key: String,
        server_url: String,
    ) -> Self {
        Self {
            id,
            name,
            r#type,
            model,
            status: RuntimeStatus::Ready,
            computer_id,
            created_at: 0,
            updated_at: 0,
            last_used_at: 0,
            api_key,
            server_url,
            metadata: serde_json::Map::new(),
        }
    }

    /// Create a new runtime with defaults.
    pub fn with_defaults(
        id: String,
        name: String,
        r#type: String,
        model: String,
        computer_id: String,
        api_key: String,
        server_url: String,
    ) -> Self {
        Self {
            id,
            name,
            r#type,
            model,
            status: RuntimeStatus::Ready,
            computer_id,
            created_at: 0,
            updated_at: 0,
            last_used_at: 0,
            api_key,
            server_url,
            metadata: serde_json::Map::new(),
        }
    }

    /// Set the runtime's status.
    pub fn set_status(&mut self, status: RuntimeStatus) {
        self.status = status;
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }

    /// Check if the runtime is ready.
    pub fn is_ready(&self) -> bool {
        self.status == RuntimeStatus::Ready
    }

    /// Check if the runtime is starting.
    pub fn is_starting(&self) -> bool {
        self.status == RuntimeStatus::Starting
    }

    /// Check if the runtime is busy.
    pub fn is_busy(&self) -> bool {
        self.status == RuntimeStatus::Busy
    }

    /// Check if the runtime has an error.
    pub fn is_error(&self) -> bool {
        self.status == RuntimeStatus::Error
    }

    /// Check if the runtime is offline.
    pub fn is_offline(&self) -> bool {
        self.status == RuntimeStatus::Offline
    }
}
