//! Response models.

use serde::{Deserialize, Serialize};

/// An agent response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    /// The response ID.
    pub id: String,
    /// The response content.
    pub content: String,
    /// The response metadata.
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

/// A task result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    /// The result ID.
    pub id: String,
    /// The result content.
    pub content: String,
    /// The result status.
    pub status: String,
    /// The result metadata.
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

/// A command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    /// The command ID.
    pub id: String,
    /// The command content.
    pub content: String,
    /// The command arguments.
    pub args: Vec<String>,
    /// The command metadata.
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

/// A command result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult {
    /// The result ID.
    pub id: String,
    /// The result output.
    pub output: String,
    /// The exit code.
    pub exit_code: i32,
    /// The result metadata.
    pub metadata: serde_json::Map<String, serde_json::Value>,
}
