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
    ///
    /// # Errors
    ///
    /// Returns an error if the runtime implementation fails to initialise.
    async fn initialize(&self, config: RuntimeConfig) -> Result<()>;

    /// Handle a message.
    ///
    /// # Errors
    ///
    /// Returns an error if the runtime implementation fails to process the
    /// message.
    async fn handle_message(&self, message: Message) -> Result<AgentResponse>;

    /// Handle a task.
    ///
    /// # Errors
    ///
    /// Returns an error if the runtime implementation fails to complete the
    /// task.
    async fn handle_task(&self, task: Task) -> Result<TaskResult>;

    /// Execute a command.
    ///
    /// # Errors
    ///
    /// Returns an error if the command fails to execute.
    async fn execute_command(&self, command: Command) -> Result<CommandResult>;

    /// Read a file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read.
    async fn read_file(&self, path: PathBuf) -> Result<String>;

    /// Write a file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    async fn write_file(&self, path: PathBuf, content: &str) -> Result<()>;

    /// List files in a directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be read.
    async fn list_files(&self, path: PathBuf) -> Result<Vec<PathBuf>>;

    /// Get the workspace.
    ///
    /// # Errors
    ///
    /// Returns an error if the workspace cannot be resolved.
    async fn get_workspace(&self) -> Result<Workspace>;

    /// Shutdown the runtime.
    ///
    /// # Errors
    ///
    /// Returns an error if the runtime fails to shut down cleanly.
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
    ///
    /// # Errors
    ///
    /// Propagates any error from the underlying [`Runtime::initialize`].
    pub async fn initialize(&self) -> Result<()> {
        self.runtime.initialize(self.config.clone()).await
    }

    /// Handle a message.
    ///
    /// # Errors
    ///
    /// Propagates any error from the underlying [`Runtime::handle_message`].
    pub async fn handle_message(&self, message: Message) -> Result<AgentResponse> {
        self.runtime.handle_message(message).await
    }

    /// Handle a task.
    ///
    /// # Errors
    ///
    /// Propagates any error from the underlying [`Runtime::handle_task`].
    pub async fn handle_task(&self, task: Task) -> Result<TaskResult> {
        self.runtime.handle_task(task).await
    }

    /// Execute a command.
    ///
    /// # Errors
    ///
    /// Propagates any error from the underlying [`Runtime::execute_command`].
    pub async fn execute_command(&self, command: Command) -> Result<CommandResult> {
        self.runtime.execute_command(command).await
    }

    /// Read a file.
    ///
    /// # Errors
    ///
    /// Propagates any error from the underlying [`Runtime::read_file`].
    pub async fn read_file(&self, path: PathBuf) -> Result<String> {
        self.runtime.read_file(path).await
    }

    /// Write a file.
    ///
    /// # Errors
    ///
    /// Propagates any error from the underlying [`Runtime::write_file`].
    pub async fn write_file(&self, path: PathBuf, content: &str) -> Result<()> {
        self.runtime.write_file(path, content).await
    }

    /// List files in a directory.
    ///
    /// # Errors
    ///
    /// Propagates any error from the underlying [`Runtime::list_files`].
    pub async fn list_files(&self, path: PathBuf) -> Result<Vec<PathBuf>> {
        self.runtime.list_files(path).await
    }

    /// Get the workspace.
    ///
    /// # Errors
    ///
    /// Propagates any error from the underlying [`Runtime::get_workspace`].
    pub async fn get_workspace(&self) -> Result<Workspace> {
        self.runtime.get_workspace().await
    }

    /// Shutdown the runtime.
    ///
    /// # Errors
    ///
    /// Propagates any error from the underlying [`Runtime::shutdown`].
    pub async fn shutdown(&self) -> Result<()> {
        self.runtime.shutdown().await
    }
}
