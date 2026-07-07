//! Runtime module - Runtime trait and implementations.

use anyhow::Result;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;

use crate::daemon::Workspace;
use crate::models::{
    AgentResponse, Command, CommandResult, Message, RuntimeConfig, Task, TaskResult,
};

/// Runtime trait for agent runtimes.
#[async_trait]
pub trait Runtime: Send + Sync {
    /// Initialize the runtime.
    async fn initialize(&self, config: RuntimeConfig) -> Result<()>;

    /// Handle a message.
    async fn handle_message(&self, message: Message) -> Result<AgentResponse>;

    /// Handle a task.
    async fn handle_task(&self, task: Task) -> Result<TaskResult>;

    /// Execute a command.
    async fn execute_command(&self, command: Command) -> Result<CommandResult>;

    /// Read a file.
    async fn read_file(&self, path: PathBuf) -> Result<String>;

    /// Write a file.
    async fn write_file(&self, path: PathBuf, content: &str) -> Result<()>;

    /// List files in a directory.
    async fn list_files(&self, path: PathBuf) -> Result<Vec<PathBuf>>;

    /// Get the workspace.
    async fn get_workspace(&self) -> Result<Workspace>;

    /// Shutdown the runtime.
    async fn shutdown(&self) -> Result<()>;
}

/// Runtime handle for interacting with a runtime.
pub struct RuntimeHandle {
    /// The runtime.
    runtime: Arc<dyn Runtime>,
    /// The runtime config.
    config: RuntimeConfig,
}

impl RuntimeHandle {
    /// Create a new runtime handle.
    pub fn new(runtime: Arc<dyn Runtime>, config: RuntimeConfig) -> Self {
        Self { runtime, config }
    }

    /// Initialize the runtime.
    pub async fn initialize(&self) -> Result<()> {
        self.runtime.initialize(self.config.clone()).await
    }

    /// Handle a message.
    pub async fn handle_message(&self, message: Message) -> Result<AgentResponse> {
        self.runtime.handle_message(message).await
    }

    /// Handle a task.
    pub async fn handle_task(&self, task: Task) -> Result<TaskResult> {
        self.runtime.handle_task(task).await
    }

    /// Execute a command.
    pub async fn execute_command(&self, command: Command) -> Result<CommandResult> {
        self.runtime.execute_command(command).await
    }

    /// Read a file.
    pub async fn read_file(&self, path: PathBuf) -> Result<String> {
        self.runtime.read_file(path).await
    }

    /// Write a file.
    pub async fn write_file(&self, path: PathBuf, content: &str) -> Result<()> {
        self.runtime.write_file(path, content).await
    }

    /// List files in a directory.
    pub async fn list_files(&self, path: PathBuf) -> Result<Vec<PathBuf>> {
        self.runtime.list_files(path).await
    }

    /// Get the workspace.
    pub async fn get_workspace(&self) -> Result<Workspace> {
        self.runtime.get_workspace().await
    }

    /// Shutdown the runtime.
    pub async fn shutdown(&self) -> Result<()> {
        self.runtime.shutdown().await
    }
}
