//! Reminder model.
//!
//! A reminder is a scheduled wake-up signal for an agent.

use serde::{Deserialize, Serialize};

/// The interval type for a reminder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReminderInterval {
    /// One-time reminder.
    Once,
    /// Recurring reminder.
    Recurring,
}

impl std::fmt::Display for ReminderInterval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReminderInterval::Once => write!(f, "once"),
            ReminderInterval::Recurring => write!(f, "recurring"),
        }
    }
}

/// A reminder in a server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reminder {
    /// Unique identifier for the reminder.
    pub id: String,
    /// The reminder's title.
    pub title: String,
    /// The duration of the reminder in seconds.
    pub duration: i64,
    /// The interval type of the reminder.
    pub interval: ReminderInterval,
    /// The interval duration in seconds (for recurring reminders).
    pub interval_duration: i64,
    /// The message the reminder is anchored to.
    pub anchor_message_id: String,
    /// The author of the reminder.
    pub author_id: String,
    /// When the reminder was created.
    pub created_at: i64,
    /// When the reminder was last updated.
    pub updated_at: i64,
    /// When the reminder was last fired.
    pub last_fired_at: Option<i64>,
    /// The number of times the reminder has fired.
    pub fired_count: i32,
    /// The reminder's status.
    pub status: ReminderStatus,
    /// Reminder-specific metadata.
    #[serde(flatten)]
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

/// The status of a reminder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReminderStatus {
    /// The reminder is active.
    Active,
    /// The reminder is snoozed.
    Snoozed,
    /// The reminder was cancelled.
    Cancelled,
}

impl std::fmt::Display for ReminderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReminderStatus::Active => write!(f, "active"),
            ReminderStatus::Snoozed => write!(f, "snoozed"),
            ReminderStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl Reminder {
    /// Create a new reminder.
    pub fn new(
        id: String,
        title: String,
        duration: i64,
        interval: ReminderInterval,
        interval_duration: i64,
        anchor_message_id: String,
        author_id: String,
    ) -> Self {
        Self {
            id,
            title,
            duration,
            interval,
            interval_duration,
            anchor_message_id,
            author_id,
            created_at: 0,
            updated_at: 0,
            last_fired_at: None,
            fired_count: 0,
            status: ReminderStatus::Active,
            metadata: serde_json::Map::new(),
        }
    }

    /// Create a new reminder with defaults.
    pub fn with_defaults(
        id: String,
        title: String,
        duration: i64,
        interval: ReminderInterval,
        interval_duration: i64,
        anchor_message_id: String,
        author_id: String,
    ) -> Self {
        Self {
            id,
            title,
            duration,
            interval,
            interval_duration,
            anchor_message_id,
            author_id,
            created_at: 0,
            updated_at: 0,
            last_fired_at: None,
            fired_count: 0,
            status: ReminderStatus::Active,
            metadata: serde_json::Map::new(),
        }
    }

    /// Snooze the reminder.
    pub fn snooze(&mut self, new_duration: i64) {
        self.duration = new_duration;
        self.status = ReminderStatus::Snoozed;
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }

    /// Update the reminder.
    pub fn update(&mut self, new_title: String, new_duration: i64) {
        self.title = new_title;
        self.duration = new_duration;
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }

    /// Cancel the reminder.
    pub fn cancel(&mut self) {
        self.status = ReminderStatus::Cancelled;
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }

    /// Fire the reminder.
    pub fn fire(&mut self) {
        self.last_fired_at = Some(chrono::Utc::now().timestamp_millis());
        self.fired_count += 1;
        if self.interval == ReminderInterval::Recurring {
            self.status = ReminderStatus::Active;
        } else {
            self.status = ReminderStatus::Cancelled;
        }
        self.updated_at = self.last_fired_at.unwrap();
    }

    /// Check if the reminder is active.
    pub fn is_active(&self) -> bool {
        self.status == ReminderStatus::Active
    }

    /// Check if the reminder is snoozed.
    pub fn is_snoozed(&self) -> bool {
        self.status == ReminderStatus::Snoozed
    }

    /// Check if the reminder is cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.status == ReminderStatus::Cancelled
    }

    /// Check if the reminder is one-time.
    pub fn is_once(&self) -> bool {
        self.interval == ReminderInterval::Once
    }

    /// Check if the reminder is recurring.
    pub fn is_recurring(&self) -> bool {
        self.interval == ReminderInterval::Recurring
    }

    /// Get the next fire time for a recurring reminder.
    pub fn next_fire_time(&self, now: i64) -> Option<i64> {
        if self.interval != ReminderInterval::Recurring {
            return None;
        }
        if self.last_fired_at.is_none() {
            return Some(now + self.duration);
        }
        let last_fired = self.last_fired_at.unwrap();
        Some(last_fired + self.interval_duration)
    }
}
