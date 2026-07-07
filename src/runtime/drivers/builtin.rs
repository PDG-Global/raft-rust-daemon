//! Built-in runtime driver.
//!
//! This driver implements the Runtime trait using a simple built-in runtime.
//! It's kept as a fallback for environments without RustyCLI.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command as TokioCommand};
use tokio::time::{sleep, timeout};

use crate::daemon::Workspace;
use crate::models::{
    AgentResponse, Command, CommandResult, Message, RuntimeConfig, RuntimeStatus, Task, TaskResult,
};
use crate::runtime::Runtime;

/// Built-in runtime configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltInRuntimeConfig {
    /// The built-in runtime binary path.
    pub binary_path: PathBuf,
    /// The profile to use.
    pub profile: String,
    /// The server URL.
    pub server_url: String,
    /// The API key.
    pub api_key: String,
    /// The workspace directory.
    pub workspace: PathBuf,
    /// Additional parameters.
    #[serde(flatten)]
    pub parameters: serde_json::Value,
}

/// Built-in runtime.
pub struct BuiltInRuntime {
    /// The runtime config.
    config: BuiltInRuntimeConfig,
    /// The built-in runtime process.
    process: Option<Child>,
    /// The stdin sender.
    stdin: Option<ChildStdin>,
    /// The stdout reader.
    stdout: Option<ChildStdout>,
    /// The stderr reader.
    stderr: Option<ChildStderr>,
    /// The runtime state.
    state: RuntimeStatus,
}

impl BuiltInRuntime {
    /// Create a new built-in runtime.
    pub async fn new(config: BuiltInRuntimeConfig) -> Result<Self> {
        // Check if binary exists
        if !config.binary_path.exists() {
            return Err(anyhow::anyhow!(
                "Built-in runtime binary not found at: {}",
                config.binary_path.display()
            ));
        }

        // Pass the API key via an environment variable rather than a CLI
        // argument so it is not visible to other users via `ps` / `/proc`.
        let process = TokioCommand::new(&config.binary_path)
            .env("RAFT_API_KEY", &config.api_key)
            .arg("--profile")
            .arg(&config.profile)
            .arg("--server")
            .arg(&config.server_url)
            .arg("--workspace")
            .arg(
                config
                    .workspace
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("Invalid workspace path"))?,
            )
            .arg("--")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to start built-in runtime process")?;

        // Wait for initialization
        sleep(Duration::from_secs(2)).await;

        Ok(Self {
            config,
            process: Some(process),
            stdin: None,
            stdout: None,
            stderr: None,
            state: RuntimeStatus::Ready,
        })
    }

    /// Initialize the runtime.
    pub fn initialize(&mut self, _config: RuntimeConfig) -> Result<()> {
        // Already initialized
        self.state = RuntimeStatus::Ready;
        Ok(())
    }

    /// Handle a message.
    pub async fn handle_message(&mut self, message: Message) -> Result<AgentResponse> {
        // Check if process is running
        let process = self
            .process
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Built-in runtime process not running"))?;

        // Check if process is still running
        if process.try_wait()?.is_some() {
            // Restart the process
            self.restart().await?;
        }

        // Send message to process
        let content = serde_json::json!({
            "type": "message",
            "id": message.id,
            "timestamp": message.timestamp,
            "channel_id": message.channel_id,
            "thread_id": message.thread_id,
            "content": message.content,
            "metadata": message.metadata,
        });

        let content_str = serde_json::to_string(&content)?;

        // Send via stdin
        if let Some(ref mut stdin) = self.stdin {
            stdin.write_all(content_str.as_bytes()).await?;
            stdin.flush().await?;
        }

        // Wait for response
        let timeout_ms = message.metadata.timeout.max(0) as u64;
        let timeout_duration = Duration::from_millis(timeout_ms);
        let response = timeout(timeout_duration, async {
            if let Some(ref mut stdout) = self.stdout {
                let mut buf = Vec::new();
                stdout.read_to_end(&mut buf).await?;
                let response_str = String::from_utf8_lossy(&buf).to_string();
                Ok(AgentResponse {
                    id: message.id.clone(),
                    content: response_str,
                    metadata: serde_json::Map::new(),
                })
            } else {
                Err(anyhow::anyhow!("No stdout"))
            }
        })
        .await
        .map_err(|e| anyhow::anyhow!("Timeout waiting for response: {e}"))??;

        self.state = RuntimeStatus::Busy;

        Ok(response)
    }

    /// Handle a task.
    pub async fn handle_task(&mut self, task: Task) -> Result<TaskResult> {
        // Check if process is running
        let process = self
            .process
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Built-in runtime process not running"))?;

        // Check if process is still running
        if process.try_wait()?.is_some() {
            // Restart the process
            self.restart().await?;
        }

        // Send task to process
        let content = serde_json::json!({
            "type": "task",
            "id": task.id,
            "timestamp": task.created_at,
            "title": task.title,
            "description": task.description,
            "channel_id": task.channel_id,
            "thread_id": task.thread_id,
            "metadata": task.metadata,
        });

        let content_str = serde_json::to_string(&content)?;

        // Send via stdin
        if let Some(ref mut stdin) = self.stdin {
            stdin.write_all(content_str.as_bytes()).await?;
            stdin.flush().await?;
        }

        // Wait for response
        let timeout_duration = Duration::from_secs(60);
        let result = timeout(timeout_duration, async {
            if let Some(ref mut stdout) = self.stdout {
                let mut buf = Vec::new();
                stdout.read_to_end(&mut buf).await?;
                let response_str = String::from_utf8_lossy(&buf).to_string();
                Ok(TaskResult {
                    id: task.id.clone(),
                    content: response_str,
                    status: "completed".to_string(),
                    metadata: serde_json::Map::new(),
                })
            } else {
                Err(anyhow::anyhow!("No stdout"))
            }
        })
        .await
        .map_err(|e| anyhow::anyhow!("Timeout waiting for response: {e}"))??;

        self.state = RuntimeStatus::Busy;

        Ok(result)
    }

    /// Execute a command.
    pub async fn execute_command(&mut self, command: Command) -> Result<CommandResult> {
        // Check if process is running
        let process = self
            .process
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Built-in runtime process not running"))?;

        // Check if process is still running
        if process.try_wait()?.is_some() {
            // Restart the process
            self.restart().await?;
        }

        // Send command to process
        let command_id = command.id.clone();
        let content = serde_json::json!({
            "type": "command",
            "content": command.content,
            "args": command.args,
            "metadata": command.metadata,
        });

        let content_str = serde_json::to_string(&content)?;

        // Send via stdin
        if let Some(ref mut stdin) = self.stdin {
            stdin.write_all(content_str.as_bytes()).await?;
            stdin.flush().await?;
        }

        // Wait for response
        let timeout_duration = Duration::from_secs(30);
        let result = timeout(timeout_duration, async {
            if let Some(ref mut stdout) = self.stdout {
                let mut buf = Vec::new();
                stdout.read_to_end(&mut buf).await?;
                let response_str = String::from_utf8_lossy(&buf).to_string();

                // Parse exit code
                let exit_code = if response_str.contains("exit_code:") {
                    response_str
                        .split("exit_code:")
                        .nth(1)
                        .and_then(|s| s.split_whitespace().next())
                        .and_then(|s| s.trim().parse().ok())
                        .unwrap_or(0)
                } else {
                    0
                };

                Ok(CommandResult {
                    id: command_id.clone(),
                    output: response_str,
                    exit_code,
                    metadata: serde_json::Map::new(),
                })
            } else {
                Err(anyhow::anyhow!("No stdout"))
            }
        })
        .await
        .map_err(|e| anyhow::anyhow!("Timeout waiting for response: {e}"))??;

        Ok(result)
    }

    /// Read a file.
    pub async fn read_file(&mut self, path: PathBuf) -> Result<String> {
        // Check if process is running
        let process = self
            .process
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Built-in runtime process not running"))?;

        // Check if process is still running
        if process.try_wait()?.is_some() {
            // Restart the process
            self.restart().await?;
        }

        // Send read command
        let content = serde_json::json!({
            "type": "read_file",
            "path": path.to_string_lossy(),
        });

        let content_str = serde_json::to_string(&content)?;

        // Send via stdin
        if let Some(ref mut stdin) = self.stdin {
            stdin.write_all(content_str.as_bytes()).await?;
            stdin.flush().await?;
        }

        // Wait for response
        let timeout_duration = Duration::from_secs(10);
        let response = timeout(timeout_duration, async {
            if let Some(ref mut stdout) = self.stdout {
                let mut buf = Vec::new();
                stdout.read_to_end(&mut buf).await?;
                let response_str = String::from_utf8_lossy(&buf).to_string();
                Ok(response_str)
            } else {
                Err(anyhow::anyhow!("No stdout"))
            }
        })
        .await
        .map_err(|e| anyhow::anyhow!("Timeout waiting for response: {e}"))??;

        Ok(response)
    }

    /// Write a file.
    pub async fn write_file(&mut self, path: PathBuf, content: &str) -> Result<()> {
        // Check if process is running
        let process = self
            .process
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Built-in runtime process not running"))?;

        // Check if process is still running
        if process.try_wait()?.is_some() {
            // Restart the process
            self.restart().await?;
        }

        // Send write command
        let content = serde_json::json!({
            "type": "write_file",
            "path": path.to_string_lossy(),
            "content": content,
        });

        let content_str = serde_json::to_string(&content)?;

        // Send via stdin
        if let Some(ref mut stdin) = self.stdin {
            stdin.write_all(content_str.as_bytes()).await?;
            stdin.flush().await?;
        }

        // Wait for response
        let timeout_duration = Duration::from_secs(10);
        timeout(timeout_duration, async {
            if let Some(ref mut stderr) = self.stderr {
                let mut buf = Vec::new();
                stderr.read_to_end(&mut buf).await?;
                // Check for errors
                if buf.windows(5).any(|w| w == b"Error") {
                    return Err(anyhow::anyhow!("Write failed"));
                }
            }
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("Timeout waiting for response: {e}"))??;

        Ok(())
    }

    /// List files in a directory.
    pub async fn list_files(&mut self, path: PathBuf) -> Result<Vec<PathBuf>> {
        // Check if process is running
        let process = self
            .process
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Built-in runtime process not running"))?;

        // Check if process is still running
        if process.try_wait()?.is_some() {
            // Restart the process
            self.restart().await?;
        }

        // Send list command
        let content = serde_json::json!({
            "type": "list_files",
            "path": path.to_string_lossy(),
        });

        let content_str = serde_json::to_string(&content)?;

        // Send via stdin
        if let Some(ref mut stdin) = self.stdin {
            stdin.write_all(content_str.as_bytes()).await?;
            stdin.flush().await?;
        }

        // Wait for response
        let timeout_duration = Duration::from_secs(10);
        let response = timeout(timeout_duration, async {
            if let Some(ref mut stdout) = self.stdout {
                let mut buf = Vec::new();
                stdout.read_to_end(&mut buf).await?;
                let response_str = String::from_utf8_lossy(&buf).to_string();

                // Parse file paths
                let files: Vec<PathBuf> = response_str
                    .lines()
                    .filter(|line| line.starts_with("file:"))
                    .map(|line| {
                        let path = line.split("file:").nth(1).unwrap_or("");
                        PathBuf::from(path.trim())
                    })
                    .collect();

                Ok(files)
            } else {
                Err(anyhow::anyhow!("No stdout"))
            }
        })
        .await
        .map_err(|e| anyhow::anyhow!("Timeout waiting for response: {e}"))??;

        Ok(response)
    }

    /// Get the workspace.
    pub async fn get_workspace(&mut self) -> Result<Workspace> {
        // Check if process is running
        let process = self
            .process
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Built-in runtime process not running"))?;

        // Check if process is still running
        if process.try_wait()?.is_some() {
            // Restart the process
            self.restart().await?;
        }

        // Send workspace command
        let content = serde_json::json!({
            "type": "workspace",
        });

        let content_str = serde_json::to_string(&content)?;

        // Send via stdin
        if let Some(ref mut stdin) = self.stdin {
            stdin.write_all(content_str.as_bytes()).await?;
            stdin.flush().await?;
        }

        // Wait for response
        let timeout_duration = Duration::from_secs(10);
        let response = timeout(timeout_duration, async {
            if let Some(ref mut stdout) = self.stdout {
                let mut buf = Vec::new();
                stdout.read_to_end(&mut buf).await?;
                let response_str = String::from_utf8_lossy(&buf).to_string();

                // Parse workspace
                let path = PathBuf::from(&self.config.workspace);
                let files: Vec<PathBuf> = response_str
                    .lines()
                    .filter(|line| line.starts_with("file:"))
                    .map(|line| {
                        let path = line.split("file:").nth(1).unwrap_or("");
                        PathBuf::from(path.trim())
                    })
                    .collect();
                let directories: Vec<PathBuf> = response_str
                    .lines()
                    .filter(|line| line.starts_with("dir:"))
                    .map(|line| {
                        let path = line.split("dir:").nth(1).unwrap_or("");
                        PathBuf::from(path.trim())
                    })
                    .collect();

                Ok(Workspace {
                    path: path.clone(),
                    files,
                    directories,
                })
            } else {
                Err(anyhow::anyhow!("No stdout"))
            }
        })
        .await
        .map_err(|e| anyhow::anyhow!("Timeout waiting for response: {e}"))??;

        Ok(response)
    }

    /// Shutdown the runtime.
    pub async fn shutdown(&mut self) -> Result<()> {
        // Kill the process
        if let Some(mut process) = self.process.take() {
            process.kill().await?;
            process.wait().await?;
        }

        self.state = RuntimeStatus::Offline;
        Ok(())
    }

    /// Restart the runtime.
    pub async fn restart(&mut self) -> Result<()> {
        // Shutdown the current process
        if let Some(mut process) = self.process.take() {
            process.kill().await?;
            process.wait().await?;
        }

        // Pass the API key via env var (see `new`).
        let process = TokioCommand::new(&self.config.binary_path)
            .env("RAFT_API_KEY", &self.config.api_key)
            .arg("--profile")
            .arg(&self.config.profile)
            .arg("--server")
            .arg(&self.config.server_url)
            .arg("--workspace")
            .arg(
                self.config
                    .workspace
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("Invalid workspace path"))?,
            )
            .arg("--")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to start built-in runtime process")?;

        self.process = Some(process);
        self.state = RuntimeStatus::Ready;

        Ok(())
    }
}

#[async_trait]
impl Runtime for BuiltInRuntime {
    async fn initialize(&self, _config: RuntimeConfig) -> Result<()> {
        Ok(())
    }

    async fn handle_message(&self, message: Message) -> Result<AgentResponse> {
        // Create a new runtime instance for this message
        let runtime = BuiltInRuntime::new(self.config.clone()).await?;
        runtime.handle_message(message).await
    }

    async fn handle_task(&self, task: Task) -> Result<TaskResult> {
        // Create a new runtime instance for this task
        let runtime = BuiltInRuntime::new(self.config.clone()).await?;
        runtime.handle_task(task).await
    }

    async fn execute_command(&self, command: Command) -> Result<CommandResult> {
        // Create a new runtime instance for this command
        let runtime = BuiltInRuntime::new(self.config.clone()).await?;
        runtime.execute_command(command).await
    }

    async fn read_file(&self, path: PathBuf) -> Result<String> {
        // Create a new runtime instance for this file read
        let runtime = BuiltInRuntime::new(self.config.clone()).await?;
        runtime.read_file(path).await
    }

    async fn write_file(&self, path: PathBuf, content: &str) -> Result<()> {
        // Create a new runtime instance for this file write
        let runtime = BuiltInRuntime::new(self.config.clone()).await?;
        runtime.write_file(path, content).await
    }

    async fn list_files(&self, path: PathBuf) -> Result<Vec<PathBuf>> {
        // Create a new runtime instance for this file list
        let runtime = BuiltInRuntime::new(self.config.clone()).await?;
        runtime.list_files(path).await
    }

    async fn get_workspace(&self) -> Result<Workspace> {
        // Create a new runtime instance for this workspace
        let runtime = BuiltInRuntime::new(self.config.clone()).await?;
        runtime.get_workspace().await
    }

    async fn shutdown(&self) -> Result<()> {
        // Create a new runtime instance for shutdown
        let runtime = BuiltInRuntime::new(self.config.clone()).await?;
        runtime.shutdown().await
    }
}
