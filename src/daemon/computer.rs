//! Computer module - Computer management.

use anyhow::Result;
use dashmap::DashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::time::{Duration, sleep};
use tracing::info;

use crate::daemon::state::state_manager::StateMgr;
use crate::models::{Computer, ComputerStatus};

/// Computer manager for managing computers.
#[allow(dead_code)] // state_manager + workspace reserved for future persistence + file wiring
pub struct ComputerManager {
    /// The computers.
    computers: DashMap<String, Computer>,
    /// The state manager.
    state_manager: Arc<dyn StateMgr>,
    /// The workspace directory.
    workspace: PathBuf,
}

impl ComputerManager {
    /// Create a new computer manager.
    pub fn new(state_manager: Arc<dyn StateMgr>, workspace: PathBuf) -> Self {
        Self {
            computers: DashMap::new(),
            state_manager,
            workspace,
        }
    }

    /// Get a computer by ID.
    pub fn get_computer(&self, computer_id: &str) -> Option<Computer> {
        self.computers.get(computer_id).map(|c| c.clone())
    }

    /// Add a computer.
    pub fn add_computer(&self, computer: Computer) -> String {
        let id = computer.id.clone();
        self.computers.insert(id.clone(), computer);
        id
    }

    /// Remove a computer.
    pub fn remove_computer_sync(&self, computer_id: &str) {
        self.computers.remove(computer_id);
    }

    /// Start a computer.
    pub async fn start_computer(&self, computer_id: String) -> Result<()> {
        let name = {
            let mut computer = self
                .computers
                .get_mut(&computer_id)
                .ok_or_else(|| anyhow::anyhow!("Computer not found"))?;
            computer.set_status(ComputerStatus::Starting);
            computer.name.clone()
        };
        self.state_manager.save()?;

        info!("Starting computer: {}", name);
        // Start the computer's daemon
        // This would spawn the daemon process on the computer
        // For now, just simulate it
        sleep(Duration::from_secs(2)).await;

        if let Some(mut computer) = self.computers.get_mut(&computer_id) {
            computer.set_status(ComputerStatus::Online);
        }
        self.state_manager.save()?;

        Ok(())
    }

    /// Stop a computer.
    pub async fn stop_computer(&self, computer_id: String) -> Result<()> {
        let name = {
            let mut computer = self
                .computers
                .get_mut(&computer_id)
                .ok_or_else(|| anyhow::anyhow!("Computer not found"))?;
            computer.set_status(ComputerStatus::Offline);
            computer.name.clone()
        };
        self.state_manager.save()?;

        info!("Stopping computer: {}", name);
        // Stop the computer's daemon
        // This would kill the daemon process on the computer
        // For now, just simulate it
        sleep(Duration::from_secs(2)).await;

        Ok(())
    }

    /// Reconnect a computer.
    pub async fn reconnect_computer(&self, computer_id: String) -> Result<()> {
        let name = {
            let mut computer = self
                .computers
                .get_mut(&computer_id)
                .ok_or_else(|| anyhow::anyhow!("Computer not found"))?;
            computer.set_status(ComputerStatus::Starting);
            computer.name.clone()
        };
        self.state_manager.save()?;

        info!("Reconnecting computer: {}", name);
        // Reconnect the computer
        // This would regenerate the setup command and have the user run it
        // For now, just simulate it
        sleep(Duration::from_secs(2)).await;

        if let Some(mut computer) = self.computers.get_mut(&computer_id) {
            computer.set_status(ComputerStatus::Online);
        }
        self.state_manager.save()?;

        Ok(())
    }

    /// Remove a computer.
    pub async fn remove_computer(&self, computer_id: String) -> Result<()> {
        {
            let mut computer = self
                .computers
                .get_mut(&computer_id)
                .ok_or_else(|| anyhow::anyhow!("Computer not found"))?;
            info!("Removing computer: {}", computer.name);
            computer.set_status(ComputerStatus::Removing);
        }
        self.state_manager.save()?;

        // Remove the computer
        // This would remove the daemon and workspace on the computer
        // For now, just simulate it
        sleep(Duration::from_secs(2)).await;
        self.remove_computer_sync(&computer_id);

        Ok(())
    }

    /// Get all computers.
    pub fn computers(&self) -> &DashMap<String, Computer> {
        &self.computers
    }

    /// Get online computers.
    pub fn online_computers(&self) -> Vec<Computer> {
        self.computers
            .iter()
            .filter(|entry| entry.value().is_online())
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Get offline computers.
    pub fn offline_computers(&self) -> Vec<Computer> {
        self.computers
            .iter()
            .filter(|entry| entry.value().is_offline())
            .map(|entry| entry.value().clone())
            .collect()
    }
}
