//! Server model.
//!
//! A server is where your team works. It holds channels, agents, computers,
//! and all the collaborative content.

use serde::{Deserialize, Serialize};

/// A server in Raft.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    /// Unique identifier for the server.
    pub id: String,
    /// The server's display name.
    pub name: String,
    /// The server's URL slug.
    pub slug: String,
    /// The server's URL.
    pub url: String,
    /// The server's owner.
    pub owner_id: String,
    /// Agents in the server.
    pub agents: Vec<String>,
    /// Channels in the server.
    pub channel_ids: Vec<String>,
    /// Members in the server.
    pub member_ids: Vec<String>,
    /// Computers in the server.
    pub computer_ids: Vec<String>,
    /// When the server was created.
    pub created_at: i64,
    /// When the server was last updated.
    pub updated_at: i64,
    /// Server-specific metadata.
    #[serde(flatten)]
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

impl Server {
    /// Create a new server.
    pub fn new(
        id: String,
        name: String,
        slug: String,
        owner_id: String,
        agents: Vec<String>,
        channel_ids: Vec<String>,
        member_ids: Vec<String>,
        computer_ids: Vec<String>,
    ) -> Self {
        let url = format!("https://app.raft.build/s/{slug}");
        Self {
            id,
            name,
            slug,
            url,
            owner_id,
            agents,
            channel_ids,
            member_ids,
            computer_ids,
            created_at: 0,
            updated_at: 0,
            metadata: serde_json::Map::new(),
        }
    }

    /// Create a new server with defaults.
    pub fn with_defaults(
        id: String,
        name: String,
        slug: String,
        owner_id: String,
        agents: Vec<String>,
        channel_ids: Vec<String>,
        member_ids: Vec<String>,
        computer_ids: Vec<String>,
    ) -> Self {
        let url = format!("https://app.raft.build/s/{slug}");
        Self {
            id,
            name,
            slug,
            url,
            owner_id,
            agents,
            channel_ids,
            member_ids,
            computer_ids,
            created_at: 0,
            updated_at: 0,
            metadata: serde_json::Map::new(),
        }
    }

    /// Add an agent to the server.
    pub fn add_agent(&mut self, agent_id: String) {
        if !self.agents.contains(&agent_id) {
            self.agents.push(agent_id);
        }
    }

    /// Remove an agent from the server.
    pub fn remove_agent(&mut self, agent_id: String) {
        self.agents.retain(|id| id != &agent_id);
    }

    /// Add a channel to the server.
    pub fn add_channel(&mut self, channel_id: String) {
        if !self.channel_ids.contains(&channel_id) {
            self.channel_ids.push(channel_id);
        }
    }

    /// Remove a channel from the server.
    pub fn remove_channel(&mut self, channel_id: String) {
        self.channel_ids.retain(|id| id != &channel_id);
    }

    /// Add a member to the server.
    pub fn add_member(&mut self, member_id: String) {
        if !self.member_ids.contains(&member_id) {
            self.member_ids.push(member_id);
        }
    }

    /// Remove a member from the server.
    pub fn remove_member(&mut self, member_id: String) {
        self.member_ids.retain(|id| id != &member_id);
    }

    /// Add a computer to the server.
    pub fn add_computer(&mut self, computer_id: String) {
        if !self.computer_ids.contains(&computer_id) {
            self.computer_ids.push(computer_id);
        }
    }

    /// Remove a computer from the server.
    pub fn remove_computer(&mut self, computer_id: String) {
        self.computer_ids.retain(|id| id != &computer_id);
    }

    /// Check if an agent is in the server.
    pub fn has_agent(&self, agent_id: &str) -> bool {
        self.agents.contains(&agent_id.to_string())
    }

    /// Check if a channel is in the server.
    pub fn has_channel(&self, channel_id: &str) -> bool {
        self.channel_ids.contains(&channel_id.to_string())
    }

    /// Check if a member is in the server.
    pub fn has_member(&self, member_id: &str) -> bool {
        self.member_ids.contains(&member_id.to_string())
    }

    /// Check if a computer is in the server.
    pub fn has_computer(&self, computer_id: &str) -> bool {
        self.computer_ids.contains(&computer_id.to_string())
    }

    /// Get all agent IDs in the server.
    pub fn agent_ids(&self) -> &Vec<String> {
        &self.agents
    }

    /// Get all channel IDs in the server.
    pub fn channel_ids(&self) -> &Vec<String> {
        &self.channel_ids
    }

    /// Get all member IDs in the server.
    pub fn member_ids(&self) -> &Vec<String> {
        &self.member_ids
    }

    /// Get all computer IDs in the server.
    pub fn computer_ids(&self) -> &Vec<String> {
        &self.computer_ids
    }
}
