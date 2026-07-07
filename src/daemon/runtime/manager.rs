//! Runtime manager.

use crate::models::Runtime;
use dashmap::DashMap;

/// A runtime manager for managing runtimes.
#[derive(Default)]
pub struct RuntimeManager {
    /// All runtimes.
    runtimes: DashMap<String, Runtime>,
}

impl RuntimeManager {
    /// Create a new runtime manager.
    pub fn new() -> Self {
        Self {
            runtimes: DashMap::new(),
        }
    }

    /// Add a runtime.
    pub fn add_runtime(&self, runtime: Runtime) -> String {
        let id = runtime.id.clone();
        self.runtimes.insert(id.clone(), runtime);
        id
    }

    /// Get a runtime by ID.
    pub fn get_runtime(&self, id: &str) -> Option<Runtime> {
        self.runtimes.get(id).map(|r| r.clone())
    }

    /// Get all runtimes.
    pub fn get_all_runtimes(&self) -> Vec<Runtime> {
        self.runtimes.iter().map(|kv| kv.value().clone()).collect()
    }

    /// Remove a runtime by ID.
    pub fn remove_runtime(&self, id: &str) -> bool {
        self.runtimes.remove(id).is_some()
    }

    /// Get all ready runtimes.
    pub fn get_ready_runtimes(&self) -> Vec<Runtime> {
        self.runtimes
            .iter()
            .filter(|kv| kv.value().is_ready())
            .map(|kv| kv.value().clone())
            .collect()
    }

    /// Get all offline runtimes.
    pub fn get_offline_runtimes(&self) -> Vec<Runtime> {
        self.runtimes
            .iter()
            .filter(|kv| kv.value().is_offline())
            .map(|kv| kv.value().clone())
            .collect()
    }

    /// Get runtimes by computer ID.
    pub fn get_runtimes_by_computer(&self, computer_id: &str) -> Vec<Runtime> {
        self.runtimes
            .iter()
            .filter(|kv| kv.value().computer_id == computer_id)
            .map(|kv| kv.value().clone())
            .collect()
    }
}
