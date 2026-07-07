//! Reminder manager.

use crate::models::Reminder;
use dashmap::DashMap;

/// A reminder manager for managing reminders.
pub struct ReminderManager {
    /// All reminders.
    reminders: DashMap<String, Reminder>,
}

impl ReminderManager {
    /// Create a new reminder manager.
    pub fn new() -> Self {
        Self {
            reminders: DashMap::new(),
        }
    }

    /// Add a reminder.
    pub fn add_reminder(&self, reminder: Reminder) -> String {
        let id = reminder.id.clone();
        self.reminders.insert(id.clone(), reminder);
        id
    }

    /// Get a reminder by ID.
    pub fn get_reminder(&self, id: &str) -> Option<Reminder> {
        self.reminders.get(id).map(|r| r.clone())
    }

    /// Get all reminders.
    pub fn get_all_reminders(&self) -> Vec<Reminder> {
        self.reminders.iter().map(|kv| kv.value().clone()).collect()
    }

    /// Remove a reminder by ID.
    pub fn remove_reminder(&self, id: &str) -> bool {
        self.reminders.remove(id).is_some()
    }

    /// Get all active reminders.
    pub fn get_active_reminders(&self) -> Vec<Reminder> {
        self.reminders
            .iter()
            .filter(|kv| kv.value().is_active())
            .map(|kv| kv.value().clone())
            .collect()
    }

    /// Get all snoozed reminders.
    pub fn get_snoozed_reminders(&self) -> Vec<Reminder> {
        self.reminders
            .iter()
            .filter(|kv| kv.value().is_snoozed())
            .map(|kv| kv.value().clone())
            .collect()
    }

    /// Get reminders by author ID.
    pub fn get_reminders_by_author(&self, author_id: &str) -> Vec<Reminder> {
        self.reminders
            .iter()
            .filter(|kv| kv.value().author_id == author_id)
            .map(|kv| kv.value().clone())
            .collect()
    }
}
