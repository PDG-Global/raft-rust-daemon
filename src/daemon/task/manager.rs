//! Task manager.

use crate::models::{Task, TaskStatus};
use dashmap::DashMap;

/// A task manager for managing tasks.
pub struct TaskManager {
    /// All tasks.
    tasks: DashMap<String, Task>,
}

impl TaskManager {
    /// Create a new task manager.
    pub fn new() -> Self {
        Self {
            tasks: DashMap::new(),
        }
    }

    /// Add a task.
    pub fn add_task(&self, task: Task) -> String {
        let id = task.id.clone();
        self.tasks.insert(id.clone(), task);
        id
    }

    /// Get a task by ID.
    pub fn get_task(&self, id: &str) -> Option<Task> {
        self.tasks.get(id).map(|r| r.clone())
    }

    /// Get all tasks.
    pub fn get_all_tasks(&self) -> Vec<Task> {
        self.tasks.iter().map(|kv| kv.value().clone()).collect()
    }

    /// Remove a task by ID.
    pub fn remove_task(&self, id: &str) -> bool {
        self.tasks.remove(id).is_some()
    }

    /// Get all pending tasks.
    pub fn get_pending_tasks(&self) -> Vec<Task> {
        self.tasks
            .iter()
            .filter(|kv| kv.value().status == TaskStatus::Pending)
            .map(|kv| kv.value().clone())
            .collect()
    }

    /// Get all claimed tasks.
    pub fn get_claimed_tasks(&self) -> Vec<Task> {
        self.tasks
            .iter()
            .filter(|kv| kv.value().status == TaskStatus::Claimed)
            .map(|kv| kv.value().clone())
            .collect()
    }

    /// Get all in-progress tasks.
    pub fn get_in_progress_tasks(&self) -> Vec<Task> {
        self.tasks
            .iter()
            .filter(|kv| kv.value().status == TaskStatus::InProgress)
            .map(|kv| kv.value().clone())
            .collect()
    }

    /// Get all completed tasks.
    pub fn get_completed_tasks(&self) -> Vec<Task> {
        self.tasks
            .iter()
            .filter(|kv| kv.value().status == TaskStatus::Completed)
            .map(|kv| kv.value().clone())
            .collect()
    }

    /// Get all done tasks (completed or failed).
    pub fn get_done_tasks(&self) -> Vec<Task> {
        self.tasks
            .iter()
            .filter(|kv| kv.value().is_done())
            .map(|kv| kv.value().clone())
            .collect()
    }

    /// Get tasks by channel ID.
    pub fn get_tasks_by_channel(&self, channel_id: &str) -> Vec<Task> {
        self.tasks
            .iter()
            .filter(|kv| kv.value().channel_id == channel_id)
            .map(|kv| kv.value().clone())
            .collect()
    }

    /// Get tasks by agent ID.
    pub fn get_tasks_by_agent(&self, agent_id: &str) -> Vec<Task> {
        self.tasks
            .iter()
            .filter(|kv| kv.value().assigned_to == agent_id)
            .map(|kv| kv.value().clone())
            .collect()
    }
}
