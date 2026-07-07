//! Message handler.

use crate::models::{Message, MessageType};

/// A message handler for managing messages.
#[derive(Default)]
pub struct MessageHandler {
    /// All messages.
    messages: std::collections::HashMap<String, Message>,
}

impl MessageHandler {
    /// Create a new message handler.
    pub fn new() -> Self {
        Self {
            messages: std::collections::HashMap::new(),
        }
    }

    /// Add a message.
    pub fn add_message(&mut self, message: Message) -> String {
        let id = message.id.clone();
        self.messages.insert(id.clone(), message);
        id
    }

    /// Get a message by ID.
    pub fn get_message(&self, id: &str) -> Option<Message> {
        self.messages.get(id).cloned()
    }

    /// Get all messages.
    pub fn get_all_messages(&self) -> Vec<Message> {
        self.messages.values().cloned().collect()
    }

    /// Remove a message by ID.
    pub fn remove_message(&mut self, id: &str) -> bool {
        self.messages.remove(id).is_some()
    }

    /// Get all unread messages.
    pub fn get_unread_messages(&self) -> Vec<Message> {
        self.messages
            .values()
            .filter(|m| !m.is_read())
            .cloned()
            .collect()
    }

    /// Get messages by channel ID.
    pub fn get_messages_by_channel(&self, channel_id: &str) -> Vec<Message> {
        self.messages
            .values()
            .filter(|m| m.channel_id == channel_id)
            .cloned()
            .collect()
    }

    /// Get messages by sender ID.
    pub fn get_messages_by_sender(&self, sender_id: &str) -> Vec<Message> {
        self.messages
            .values()
            .filter(|m| m.sender_id == sender_id)
            .cloned()
            .collect()
    }

    /// Get messages by type.
    pub fn get_messages_by_type(&self, msg_type: MessageType) -> Vec<Message> {
        self.messages
            .values()
            .filter(|m| m.r#type == msg_type)
            .cloned()
            .collect()
    }
}
