//! Workspace module - Workspace management.

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Workspace for an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    /// The workspace path.
    pub path: PathBuf,
    /// The files in the workspace.
    pub files: Vec<PathBuf>,
    /// The directories in the workspace.
    pub directories: Vec<PathBuf>,
}
use dashmap::DashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::time::{Duration, sleep};
use tracing::info;

/// Workspace manager for managing agent workspaces.
#[allow(dead_code)] // scaffolding for file-watch + persistence wiring (T9/T12)
pub struct WorkspaceManager {
    /// The workspaces.
    workspaces: DashMap<String, Workspace>,
    /// The state manager.
    state_manager: Arc<dyn crate::daemon::state::state_manager::StateMgr>,
    /// The workspace directory.
    workspace: PathBuf,
    /// The file watcher.
    file_watcher: Option<Box<dyn notify::Watcher>>,
}

impl WorkspaceManager {
    /// Create a new workspace manager.
    pub fn new(
        state_manager: Arc<dyn crate::daemon::state::state_manager::StateMgr>,
        workspace: PathBuf,
    ) -> Self {
        Self {
            workspaces: DashMap::new(),
            state_manager,
            workspace,
            file_watcher: None,
        }
    }

    /// Get a workspace by agent ID.
    pub fn get_workspace(&self, agent_id: &str) -> Option<Workspace> {
        self.workspaces.get(agent_id).map(|r| r.clone())
    }

    /// Add a workspace.
    pub fn add_workspace(&mut self, agent_id: String, path: PathBuf) -> String {
        let workspace = Workspace {
            path,
            files: Vec::new(),
            directories: Vec::new(),
        };

        let id = agent_id;
        self.workspaces.insert(id.clone(), workspace);
        id
    }

    /// Remove a workspace.
    pub fn remove_workspace(&mut self, agent_id: String) {
        self.workspaces.remove(&agent_id);
    }

    /// Read a file from a workspace.
    pub async fn read_file(&self, _agent_id: String, path: PathBuf) -> Result<String> {
        // Check if file exists
        let path_display = path.display().to_string();
        let full_path = self.workspace.join(path);
        if !full_path.exists() {
            return Err(anyhow::anyhow!("File not found: {path_display}"));
        }

        // Read the file
        let content = std::fs::read_to_string(&full_path)?;

        Ok(content)
    }

    /// Write a file to a workspace.
    pub async fn write_file(&self, agent_id: String, path: PathBuf, content: &str) -> Result<()> {
        // Check if workspace exists
        let workspace = self.workspaces.get(&agent_id);
        if workspace.is_none() {
            return Err(anyhow::anyhow!("Workspace not found for agent: {agent_id}"));
        }

        // Create the directory if it doesn't exist
        let full_path = self.workspace.join(path);
        if let Some(parent) = full_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        // Write the file
        std::fs::write(&full_path, content)?;

        Ok(())
    }

    /// List files in a workspace.
    pub async fn list_files(&self, agent_id: String, path: PathBuf) -> Result<Vec<PathBuf>> {
        // Check if workspace exists
        let workspace = self.workspaces.get(&agent_id);
        if workspace.is_none() {
            return Err(anyhow::anyhow!("Workspace not found for agent: {agent_id}"));
        }

        // List files
        let path_display = path.display().to_string();
        let full_path = self.workspace.join(path);
        if !full_path.exists() {
            return Err(anyhow::anyhow!("Directory not found: {path_display}"));
        }

        let mut files = Vec::new();
        for entry in walkdir::WalkDir::new(&full_path) {
            let entry = entry?;
            if entry.file_type().is_file() {
                files.push(entry.path().to_path_buf());
            }
        }

        Ok(files)
    }

    /// Get workspace info.
    pub async fn get_workspace_info(&self, agent_id: String) -> Result<Workspace> {
        // Check if workspace exists
        let workspace = self.workspaces.get(&agent_id);
        if workspace.is_none() {
            return Err(anyhow::anyhow!("Workspace not found for agent: {agent_id}"));
        }

        let ws = workspace.unwrap().clone();

        // Scan the workspace
        let mut files = Vec::new();
        let mut directories = Vec::new();

        if ws.path.exists() {
            for entry in walkdir::WalkDir::new(&ws.path) {
                let entry = entry?;
                if entry.file_type().is_file() {
                    files.push(entry.path().to_path_buf());
                } else if entry.file_type().is_dir() {
                    directories.push(entry.path().to_path_buf());
                }
            }
        }

        let updated_workspace = Workspace {
            path: ws.path.clone(),
            files,
            directories,
        };

        self.workspaces.insert(agent_id, updated_workspace.clone());

        Ok(updated_workspace)
    }

    /// Start watching a workspace.
    pub async fn start_watching(&mut self, agent_id: String) -> Result<()> {
        // This is a simplified implementation
        // In production, you would use a proper file watcher
        info!("Starting workspace watch for agent: {}", agent_id);

        // For now, just poll
        tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(5)).await;
                // Check for changes
                // TODO: Implement proper file watching
            }
        });

        Ok(())
    }

    /// Stop watching a workspace.
    pub async fn stop_watching(&mut self, agent_id: String) -> Result<()> {
        // This is a simplified implementation
        info!("Stopping workspace watch for agent: {}", agent_id);

        Ok(())
    }

    /// Get all workspaces.
    pub fn workspaces(&self) -> &DashMap<String, Workspace> {
        &self.workspaces
    }

    /// Get workspace count.
    pub fn workspace_count(&self) -> usize {
        self.workspaces.len()
    }
}
