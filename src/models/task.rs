//! Task model.
//!
//! A task is a unit of work that can be assigned to an agent.

use serde::{Deserialize, Serialize};

/// The status of a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    /// The task is waiting to be claimed.
    Pending,
    /// The task has been claimed by an agent.
    Claimed,
    /// The task is being worked on.
    InProgress,
    /// The task has been completed.
    Completed,
    /// The task was cancelled.
    Cancelled,
    /// The task failed.
    Failed,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "pending"),
            TaskStatus::Claimed => write!(f, "claimed"),
            TaskStatus::InProgress => write!(f, "in_progress"),
            TaskStatus::Completed => write!(f, "completed"),
            TaskStatus::Cancelled => write!(f, "cancelled"),
            TaskStatus::Failed => write!(f, "failed"),
        }
    }
}

/// A task in a server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique identifier for the task.
    pub id: String,
    /// The task's title.
    pub title: String,
    /// The task's description.
    pub description: String,
    /// The task's status.
    pub status: TaskStatus,
    /// The channel the task is in.
    pub channel_id: String,
    /// The thread the task is in (if any).
    pub thread_id: Option<String>,
    /// The agent assigned to the task.
    pub assigned_to: String,
    /// When the task was created.
    pub created_at: i64,
    /// When the task was last updated.
    pub updated_at: i64,
    /// When the task was claimed.
    pub claimed_at: Option<i64>,
    /// When the task was completed.
    pub completed_at: Option<i64>,
    /// When the task was cancelled.
    pub cancelled_at: Option<i64>,
    /// When the task failed.
    pub failed_at: Option<i64>,
    /// The agent's response to the task.
    pub response: Option<String>,
    /// Task-specific metadata.
    #[serde(flatten)]
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

impl Task {
    /// Create a new task.
    pub fn new(
        id: String,
        title: String,
        description: String,
        channel_id: String,
        thread_id: Option<String>,
        assigned_to: String,
    ) -> Self {
        Self {
            id,
            title,
            description,
            status: TaskStatus::Pending,
            channel_id,
            thread_id,
            assigned_to,
            created_at: 0,
            updated_at: 0,
            claimed_at: None,
            completed_at: None,
            cancelled_at: None,
            failed_at: None,
            response: None,
            metadata: serde_json::Map::new(),
        }
    }

    /// Create a new task with defaults.
    pub fn with_defaults(
        id: String,
        title: String,
        description: String,
        channel_id: String,
        thread_id: Option<String>,
        assigned_to: String,
    ) -> Self {
        // If the task is created with an assignee, it starts in the Claimed state;
        // an unassigned task starts Pending.
        let status = if assigned_to.is_empty() {
            TaskStatus::Pending
        } else {
            TaskStatus::Claimed
        };
        Self {
            id,
            title,
            description,
            status,
            channel_id,
            thread_id,
            assigned_to,
            created_at: 0,
            updated_at: 0,
            claimed_at: None,
            completed_at: None,
            cancelled_at: None,
            failed_at: None,
            response: None,
            metadata: serde_json::Map::new(),
        }
    }

    /// Set the task's status.
    pub fn set_status(&mut self, status: TaskStatus) {
        self.status = status;
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }

    /// Claim the task.
    pub fn claim(&mut self, agent_id: String) {
        self.status = TaskStatus::Claimed;
        self.assigned_to = agent_id;
        self.claimed_at = Some(chrono::Utc::now().timestamp_millis());
        self.updated_at = self.claimed_at.unwrap();
    }

    /// Complete the task.
    pub fn complete(&mut self, response: String) {
        self.status = TaskStatus::Completed;
        self.response = Some(response);
        self.completed_at = Some(chrono::Utc::now().timestamp_millis());
        self.updated_at = self.completed_at.unwrap();
    }

    /// Cancel the task.
    pub fn cancel(&mut self) {
        self.status = TaskStatus::Cancelled;
        self.cancelled_at = Some(chrono::Utc::now().timestamp_millis());
        self.updated_at = self.cancelled_at.unwrap();
    }

    /// Fail the task.
    pub fn fail(&mut self, error: String) {
        self.status = TaskStatus::Failed;
        self.metadata
            .insert("error".to_string(), serde_json::json!(error));
        self.failed_at = Some(chrono::Utc::now().timestamp_millis());
        self.updated_at = self.failed_at.unwrap();
    }

    /// Check if the task is pending.
    pub fn is_pending(&self) -> bool {
        self.status == TaskStatus::Pending
    }

    /// Check if the task is claimed.
    pub fn is_claimed(&self) -> bool {
        self.status == TaskStatus::Claimed
    }

    /// Check if the task is in progress.
    pub fn is_in_progress(&self) -> bool {
        self.status == TaskStatus::InProgress
    }

    /// Check if the task is completed.
    pub fn is_completed(&self) -> bool {
        self.status == TaskStatus::Completed
    }

    /// Check if the task is cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.status == TaskStatus::Cancelled
    }

    /// Check if the task failed.
    pub fn is_failed(&self) -> bool {
        self.status == TaskStatus::Failed
    }

    /// Check if the task is done (completed or failed).
    pub fn is_done(&self) -> bool {
        matches!(self.status, TaskStatus::Completed | TaskStatus::Failed)
    }

    /// Check if the task is assigned to an agent.
    pub fn is_assigned(&self) -> bool {
        !self.assigned_to.is_empty()
    }

    /// Get the task's duration in milliseconds.
    pub fn duration_ms(&self) -> Option<i64> {
        match self.status {
            TaskStatus::Completed => {
                let start = self.claimed_at?;
                let end = self.completed_at?;
                Some(end - start)
            }
            TaskStatus::Failed => {
                let start = self.claimed_at?;
                let end = self.failed_at?;
                Some(end - start)
            }
            _ => None,
        }
    }
}
