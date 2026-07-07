//! Agent lifecycle and routing.
//!
//! [`AgentManager`] owns the live agent pool that the daemon schedules work
//! onto. It wraps a [`StateMgr`]
//! so that agent changes can be persisted alongside the rest of the daemon
//! state.

use crate::daemon::state::state_manager::StateMgr;
use crate::models::Agent;
use dashmap::DashMap;
use std::sync::Arc;

/// Owns the live agent pool and routes work to agents.
#[allow(dead_code)] // state_manager reserved for future persistence wiring
pub struct AgentManager {
    /// The state manager.
    state_manager: Arc<dyn crate::daemon::state::state_manager::StateMgr>,
    /// All agents.
    agents: DashMap<String, Agent>,
}

impl AgentManager {
    /// Create a new agent manager.
    pub fn new(state_manager: Arc<dyn StateMgr>) -> Self {
        Self {
            state_manager,
            agents: DashMap::new(),
        }
    }

    /// Add an agent.
    pub fn add_agent(&self, agent: Agent) -> String {
        let id = agent.id.clone();
        self.agents.insert(id.clone(), agent);
        id
    }

    /// Get an agent by ID.
    pub fn get_agent(&self, id: &str) -> Option<Agent> {
        self.agents.get(id).map(|r| r.clone())
    }

    /// Get all agents.
    pub fn get_all_agents(&self) -> Vec<Agent> {
        self.agents.iter().map(|kv| kv.value().clone()).collect()
    }

    /// Remove an agent by ID.
    pub fn remove_agent(&self, id: &str) -> bool {
        self.agents.remove(id).is_some()
    }

    /// Get online agents.
    pub fn get_online_agents(&self) -> Vec<Agent> {
        self.agents
            .iter()
            .filter(|kv| kv.value().is_online())
            .map(|kv| kv.value().clone())
            .collect()
    }

    /// Get busy agents.
    pub fn get_busy_agents(&self) -> Vec<Agent> {
        self.agents
            .iter()
            .filter(|kv| kv.value().is_busy())
            .map(|kv| kv.value().clone())
            .collect()
    }

    /// Get error agents.
    pub fn get_error_agents(&self) -> Vec<Agent> {
        self.agents
            .iter()
            .filter(|kv| kv.value().is_error())
            .map(|kv| kv.value().clone())
            .collect()
    }

    /// Get offline agents.
    pub fn get_offline_agents(&self) -> Vec<Agent> {
        self.agents
            .iter()
            .filter(|kv| kv.value().is_offline())
            .map(|kv| kv.value().clone())
            .collect()
    }

    /// Get agents by server ID.
    pub fn get_agents_by_server(&self, server_id: &str) -> Vec<Agent> {
        self.agents
            .iter()
            .filter(|kv| kv.value().server_id == server_id)
            .map(|kv| kv.value().clone())
            .collect()
    }

    /// Get agents by computer ID.
    pub fn get_agents_by_computer(&self, computer_id: &str) -> Vec<Agent> {
        self.agents
            .iter()
            .filter(|kv| kv.value().computer_id == computer_id)
            .map(|kv| kv.value().clone())
            .collect()
    }

    /// Handle a message intended for an agent.
    pub fn handle_message(
        &self,
        message: crate::models::Message,
        agent_id: String,
    ) -> anyhow::Result<crate::models::AgentResponse> {
        Ok(crate::models::AgentResponse {
            id: message.id,
            content: format!("Agent {agent_id} acknowledged the message"),
            metadata: message.extra_metadata,
        })
    }

    /// Claim a task for an agent.
    pub fn claim_task(&self, task_id: String, agent_id: String) -> anyhow::Result<()> {
        if let Some(mut agent) = self.agents.get_mut(&agent_id) {
            agent.status = crate::models::AgentStatus::Busy;
        }
        tracing::debug!(task_id = %task_id, agent_id = %agent_id, "task claimed");
        Ok(())
    }

    /// Handle a task assigned to an agent.
    pub fn handle_task(
        &self,
        task: crate::models::Task,
        agent_id: String,
    ) -> anyhow::Result<crate::models::TaskResult> {
        Ok(crate::models::TaskResult {
            id: task.id,
            content: format!("Agent {agent_id} completed the task"),
            status: "completed".to_string(),
            metadata: task.metadata,
        })
    }
}
