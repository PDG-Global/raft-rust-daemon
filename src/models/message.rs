//! Message model.
//!
//! A message is a communication between agents or between humans and agents.

use serde::{Deserialize, Serialize};

/// The type of a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageType {
    /// A regular message from a human or agent.
    Message,
    /// A task message.
    Task,
    /// A reminder message.
    Reminder,
    /// A system message.
    System,
    /// A ping message.
    Ping,
}

impl std::fmt::Display for MessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageType::Message => write!(f, "message"),
            MessageType::Task => write!(f, "task"),
            MessageType::Reminder => write!(f, "reminder"),
            MessageType::System => write!(f, "system"),
            MessageType::Ping => write!(f, "ping"),
        }
    }
}

/// Metadata for a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageMetadata {
    /// The priority of the message.
    pub priority: String,
    /// The timeout in milliseconds.
    pub timeout: i64,
    /// Whether the message is read.
    pub read: bool,
    /// Whether the message is deleted.
    pub deleted: bool,
}

/// A message in a channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique identifier for the message.
    pub id: String,
    /// The type of the message.
    pub r#type: MessageType,
    /// The content of the message.
    pub content: String,
    /// The channel the message is in.
    pub channel_id: String,
    /// The thread the message is in (if any).
    pub thread_id: Option<String>,
    /// The sender of the message.
    pub sender_id: String,
    /// The message's timestamp in milliseconds.
    pub timestamp: i64,
    /// The message's metadata.
    pub metadata: MessageMetadata,
    /// Message-specific metadata.
    #[serde(flatten)]
    pub extra_metadata: serde_json::Map<String, serde_json::Value>,
}

impl Message {
    /// Create a new message.
    pub fn new(
        id: String,
        r#type: MessageType,
        content: String,
        channel_id: String,
        thread_id: Option<String>,
        sender_id: String,
        timestamp: i64,
    ) -> Self {
        Self {
            id,
            r#type,
            content,
            channel_id,
            thread_id,
            sender_id,
            timestamp,
            metadata: MessageMetadata {
                priority: "normal".to_string(),
                timeout: 30000,
                read: false,
                deleted: false,
            },
            extra_metadata: serde_json::Map::new(),
        }
    }

    /// Create a new message with defaults.
    pub fn with_defaults(
        id: String,
        r#type: MessageType,
        content: String,
        channel_id: String,
        thread_id: Option<String>,
        sender_id: String,
        timestamp: i64,
    ) -> Self {
        Self {
            id,
            r#type,
            content,
            channel_id,
            thread_id,
            sender_id,
            timestamp,
            metadata: MessageMetadata {
                priority: "normal".to_string(),
                timeout: 30000,
                read: false,
                deleted: false,
            },
            extra_metadata: serde_json::Map::new(),
        }
    }

    /// Set the message's priority.
    pub fn set_priority(&mut self, priority: String) {
        self.metadata.priority = priority;
    }

    /// Set the message's timeout.
    pub fn set_timeout(&mut self, timeout: i64) {
        self.metadata.timeout = timeout;
    }

    /// Mark the message as read.
    pub fn mark_read(&mut self) {
        self.metadata.read = true;
    }

    /// Mark the message as deleted.
    pub fn mark_deleted(&mut self) {
        self.metadata.deleted = true;
    }

    /// Check if the message is a regular message.
    pub fn is_message(&self) -> bool {
        self.r#type == MessageType::Message
    }

    /// Check if the message is a task.
    pub fn is_task(&self) -> bool {
        self.r#type == MessageType::Task
    }

    /// Check if the message is a reminder.
    pub fn is_reminder(&self) -> bool {
        self.r#type == MessageType::Reminder
    }

    /// Check if the message is a system message.
    pub fn is_system(&self) -> bool {
        self.r#type == MessageType::System
    }

    /// Check if the message is a ping.
    pub fn is_ping(&self) -> bool {
        self.r#type == MessageType::Ping
    }

    /// Check if the message is read.
    pub fn is_read(&self) -> bool {
        self.metadata.read
    }

    /// Check if the message is deleted.
    pub fn is_deleted(&self) -> bool {
        self.metadata.deleted
    }

    /// Get the message's priority.
    pub fn priority(&self) -> &str {
        &self.metadata.priority
    }

    /// Get the message's timeout.
    pub fn timeout(&self) -> i64 {
        self.metadata.timeout
    }
}
