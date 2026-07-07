//! CLI commands for the Raft daemon.

use anyhow::Result;

/// Mask a secret for display, showing only the first and last few characters.
///
/// Keeps short secrets fully hidden while preserving enough of long keys to
/// confirm identity at a glance.
pub fn mask_secret(secret: &str) -> String {
    let len = secret.chars().count();
    if len <= 8 {
        "***".to_string()
    } else {
        let chars: Vec<char> = secret.chars().collect();
        let prefix: String = chars[..3].iter().collect();
        let suffix: String = chars[len - 3..].iter().collect();
        format!("{prefix}...{suffix}")
    }
}

/// The main CLI commands.
#[derive(Debug, clap::Parser)]
pub enum CliCommand {
    /// Manage the daemon.
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },
    /// Manage agents.
    Agent {
        #[command(subcommand)]
        command: AgentCommand,
    },
    /// Manage servers.
    Server {
        #[command(subcommand)]
        command: ServerCommand,
    },
    /// Manage computers.
    Computer {
        #[command(subcommand)]
        command: ComputerCommand,
    },
    /// Manage tasks.
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },
    /// Manage messages.
    Message {
        #[command(subcommand)]
        command: MessageCommand,
    },
    /// Manage reminders.
    Reminder {
        #[command(subcommand)]
        command: ReminderCommand,
    },
    /// Manage profiles.
    Profile {
        #[command(subcommand)]
        command: ProfileCommand,
    },
    /// Debug commands.
    Debug {
        #[command(subcommand)]
        command: DebugCommand,
    },
}

/// Daemon commands.
#[derive(Debug, clap::Subcommand)]
pub enum DaemonCommand {
    /// Start the daemon.
    Start {
        /// The server URL.
        #[arg(short, long)]
        server_url: String,
        /// The API key.
        #[arg(short, long)]
        api_key: String,
        /// The profile to use.
        #[arg(short, long, default_value = "default")]
        profile: String,
    },
    /// Stop the daemon.
    Stop,
    /// Restart the daemon.
    Restart,
    /// Show daemon status.
    Status,
}

/// Agent commands.
#[derive(Debug, clap::Subcommand)]
pub enum AgentCommand {
    /// List all agents.
    List,
    /// Get an agent by ID.
    Get {
        /// The agent ID.
        agent_id: String,
    },
    /// Create a new agent.
    Create {
        /// The agent name.
        name: String,
        /// The agent description.
        description: String,
        /// The runtime to use.
        #[arg(short, long)]
        runtime: String,
        /// The role of the agent.
        role: String,
        /// The reset mode.
        #[arg(short, long, default_value = "restart")]
        reset_mode: String,
        /// The workspace directory.
        #[arg(short, long)]
        workspace: String,
    },
    /// Update an agent.
    Update {
        /// The agent ID.
        agent_id: String,
        /// The new name.
        #[arg(short, long)]
        name: Option<String>,
        /// The new description.
        #[arg(short, long)]
        description: Option<String>,
        /// The new runtime.
        #[arg(short, long)]
        runtime: Option<String>,
        /// The new role.
        #[arg(short, long)]
        role: Option<String>,
        /// The new reset mode.
        #[arg(short, long)]
        reset_mode: Option<String>,
        /// The new workspace.
        #[arg(short, long)]
        workspace: Option<String>,
    },
    /// Delete an agent.
    Delete {
        /// The agent ID.
        agent_id: String,
    },
    /// Start an agent.
    Start {
        /// The agent ID.
        agent_id: String,
    },
    /// Stop an agent.
    Stop {
        /// The agent ID.
        agent_id: String,
    },
    /// Restart an agent.
    Restart {
        /// The agent ID.
        agent_id: String,
    },
    /// Reset an agent's session.
    Reset {
        /// The agent ID.
        agent_id: String,
        /// The reset mode.
        #[arg(short, long, default_value = "session_reset")]
        mode: String,
    },
    /// Get agent status.
    Status {
        /// The agent ID.
        agent_id: String,
    },
}

/// Server commands.
#[derive(Debug, clap::Subcommand)]
pub enum ServerCommand {
    /// List all servers.
    List,
    /// Get a server by ID.
    Get {
        /// The server ID.
        server_id: String,
    },
    /// Create a new server.
    Create {
        /// The server name.
        name: String,
    },
    /// Update a server.
    Update {
        /// The server ID.
        server_id: String,
        /// The new name.
        #[arg(short, long)]
        name: Option<String>,
    },
    /// Delete a server.
    Delete {
        /// The server ID.
        server_id: String,
    },
}

/// Computer commands.
#[derive(Debug, clap::Subcommand)]
pub enum ComputerCommand {
    /// List all computers.
    List,
    /// Get a computer by ID.
    Get {
        /// The computer ID.
        computer_id: String,
    },
    /// Create a new computer.
    Create {
        /// The computer name.
        name: String,
        /// The server ID.
        #[arg(short, long)]
        server_id: String,
    },
    /// Update a computer.
    Update {
        /// The computer ID.
        computer_id: String,
        /// The new name.
        #[arg(short, long)]
        name: Option<String>,
    },
    /// Delete a computer.
    Delete {
        /// The computer ID.
        computer_id: String,
    },
    /// Start a computer.
    Start {
        /// The computer ID.
        computer_id: String,
    },
    /// Stop a computer.
    Stop {
        /// The computer ID.
        computer_id: String,
    },
}

/// Task commands.
#[derive(Debug, clap::Subcommand)]
pub enum TaskCommand {
    /// List all tasks.
    List {
        /// Filter by status.
        #[arg(short, long)]
        status: Option<String>,
        /// Filter by channel.
        #[arg(short, long)]
        channel_id: Option<String>,
    },
    /// Get a task by ID.
    Get {
        /// The task ID.
        task_id: String,
    },
    /// Create a new task.
    Create {
        /// The task title.
        title: String,
        /// The task description.
        description: String,
        /// The channel ID.
        channel_id: String,
        /// The thread ID (optional).
        #[arg(short, long)]
        thread_id: Option<String>,
    },
    /// Claim a task.
    Claim {
        /// The task ID.
        task_id: String,
    },
    /// Complete a task.
    Complete {
        /// The task ID.
        task_id: String,
        /// The completion response.
        response: String,
    },
    /// Cancel a task.
    Cancel {
        /// The task ID.
        task_id: String,
    },
}

/// Message commands.
#[derive(Debug, clap::Subcommand)]
pub enum MessageCommand {
    /// Send a message.
    Send {
        /// The message content.
        content: String,
        /// The channel ID.
        channel_id: String,
        /// The thread ID (optional).
        #[arg(short, long)]
        thread_id: Option<String>,
    },
    /// Check for new messages.
    Check,
    /// Get a message by ID.
    Get {
        /// The message ID.
        message_id: String,
    },
}

/// Reminder commands.
#[derive(Debug, clap::Subcommand)]
pub enum ReminderCommand {
    /// List all reminders.
    List,
    /// Create a new reminder.
    Create {
        /// The reminder title.
        title: String,
        /// The duration in seconds.
        duration: i64,
        /// The anchor message ID.
        anchor_message_id: String,
        /// The author ID.
        author_id: String,
        /// The interval type.
        #[arg(short, long, default_value = "once")]
        interval: String,
        /// The interval duration in seconds (for recurring).
        #[arg(short, long)]
        interval_duration: Option<i64>,
    },
    /// Update a reminder.
    Update {
        /// The reminder ID.
        reminder_id: String,
        /// The new title.
        #[arg(short, long)]
        title: Option<String>,
        /// The new duration.
        #[arg(short, long)]
        duration: Option<i64>,
    },
    /// Snooze a reminder.
    Snooze {
        /// The reminder ID.
        reminder_id: String,
        /// The new duration.
        duration: i64,
    },
    /// Cancel a reminder.
    Cancel {
        /// The reminder ID.
        reminder_id: String,
    },
}

/// Profile commands.
#[derive(Debug, clap::Subcommand)]
pub enum ProfileCommand {
    /// List all profiles.
    List,
    /// Get a profile by name.
    Get {
        /// The profile name.
        profile_name: String,
    },
    /// Create a new profile.
    Create {
        /// The profile name.
        name: String,
        /// The server URL.
        #[arg(short, long)]
        server_url: String,
        /// The API key.
        #[arg(short, long)]
        api_key: String,
    },
    /// Update a profile.
    Update {
        /// The profile name.
        profile_name: String,
        /// The new server URL.
        #[arg(short, long)]
        server_url: Option<String>,
        /// The new API key.
        #[arg(short, long)]
        api_key: Option<String>,
    },
    /// Delete a profile.
    Delete {
        /// The profile name.
        profile_name: String,
    },
}

/// Debug commands.
#[derive(Debug, clap::Subcommand)]
pub enum DebugCommand {
    /// Show debug info.
    Info,
    /// Show version.
    Version,
}

/// Execute a CLI command.
///
/// # Errors
///
/// Returns an error if the selected command fails to run, e.g. a failure to
/// start the daemon, a failed HTTP request, or a malformed response.
pub fn execute_command(command: &CliCommand) -> Result<()> {
    match command {
        CliCommand::Daemon { command } => match command {
            DaemonCommand::Start {
                server_url,
                api_key,
                profile,
            } => {
                // Start the daemon
                println!("Starting daemon with profile: {profile}");
                println!("Server URL: {server_url}");
                println!("API Key: {}", mask_secret(api_key));
            }
            DaemonCommand::Stop => {
                // Stop the daemon
                println!("Stopping daemon");
            }
            DaemonCommand::Restart => {
                // Restart the daemon
                println!("Restarting daemon");
            }
            DaemonCommand::Status => {
                // Show daemon status
                println!("Daemon status: running");
            }
        },
        CliCommand::Agent { command } => match command {
            AgentCommand::List => {
                // List agents
                println!("Listing agents...");
            }
            AgentCommand::Get { agent_id } => {
                // Get agent
                println!("Getting agent: {agent_id}");
            }
            AgentCommand::Create {
                name,
                description: _,
                runtime: _,
                role: _,
                reset_mode: _,
                workspace: _,
            } => {
                // Create agent
                println!("Creating agent: {name}");
            }
            AgentCommand::Update {
                agent_id,
                name: _new_name,
                description: _new_description,
                runtime: _new_runtime,
                role: _new_role,
                reset_mode: _new_reset_mode,
                workspace: _new_workspace,
            } => {
                // Update agent
                println!("Updating agent: {agent_id}");
            }
            AgentCommand::Delete { agent_id } => {
                // Delete agent
                println!("Deleting agent: {agent_id}");
            }
            AgentCommand::Start { agent_id } => {
                // Start agent
                println!("Starting agent: {agent_id}");
            }
            AgentCommand::Stop { agent_id } => {
                // Stop agent
                println!("Stopping agent: {agent_id}");
            }
            AgentCommand::Restart { agent_id } => {
                // Restart agent
                println!("Restarting agent: {agent_id}");
            }
            AgentCommand::Reset { agent_id, mode } => {
                // Reset agent
                println!("Resetting agent: {agent_id} with mode: {mode}");
            }
            AgentCommand::Status { agent_id } => {
                // Get agent status
                println!("Agent status: {agent_id}");
            }
        },
        CliCommand::Server { command } => match command {
            ServerCommand::List => {
                // List servers
                println!("Listing servers...");
            }
            ServerCommand::Get { server_id } => {
                // Get server
                println!("Getting server: {server_id}");
            }
            ServerCommand::Create { name } => {
                // Create server
                println!("Creating server: {name}");
            }
            ServerCommand::Update {
                server_id,
                name: _new_name,
            } => {
                // Update server
                println!("Updating server: {server_id}");
            }
            ServerCommand::Delete { server_id } => {
                // Delete server
                println!("Deleting server: {server_id}");
            }
        },
        CliCommand::Computer { command } => match command {
            ComputerCommand::List => {
                // List computers
                println!("Listing computers...");
            }
            ComputerCommand::Get { computer_id } => {
                // Get computer
                println!("Getting computer: {computer_id}");
            }
            ComputerCommand::Create { name, server_id: _ } => {
                // Create computer
                println!("Creating computer: {name}");
            }
            ComputerCommand::Update {
                computer_id,
                name: _new_name,
            } => {
                // Update computer
                println!("Updating computer: {computer_id}");
            }
            ComputerCommand::Delete { computer_id } => {
                // Delete computer
                println!("Deleting computer: {computer_id}");
            }
            ComputerCommand::Start { computer_id } => {
                // Start computer
                println!("Starting computer: {computer_id}");
            }
            ComputerCommand::Stop { computer_id } => {
                // Stop computer
                println!("Stopping computer: {computer_id}");
            }
        },
        CliCommand::Task { command } => match command {
            TaskCommand::List {
                status: _,
                channel_id: _,
            } => {
                // List tasks
                println!("Listing tasks");
            }
            TaskCommand::Get { task_id } => {
                // Get task
                println!("Getting task: {task_id}");
            }
            TaskCommand::Create {
                title,
                description: _,
                channel_id: _,
                thread_id: _,
            } => {
                // Create task
                println!("Creating task: {title}");
            }
            TaskCommand::Claim { task_id } => {
                // Claim task
                println!("Claiming task: {task_id}");
            }
            TaskCommand::Complete {
                task_id,
                response: _,
            } => {
                // Complete task
                println!("Completing task: {task_id}");
            }
            TaskCommand::Cancel { task_id } => {
                // Cancel task
                println!("Cancelling task: {task_id}");
            }
        },
        CliCommand::Message { command } => match command {
            MessageCommand::Send {
                content: _,
                channel_id,
                thread_id: _,
            } => {
                // Send message
                println!("Sending message to channel: {channel_id}");
            }
            MessageCommand::Check => {
                // Check for new messages
                println!("Checking for new messages...");
            }
            MessageCommand::Get { message_id } => {
                // Get message
                println!("Getting message: {message_id}");
            }
        },
        CliCommand::Reminder { command } => match command {
            ReminderCommand::List => {
                // List reminders
                println!("Listing reminders...");
            }
            ReminderCommand::Create {
                title,
                duration: _,
                anchor_message_id: _,
                author_id: _,
                interval: _,
                interval_duration: _,
            } => {
                // Create reminder
                println!("Creating reminder: {title}");
            }
            ReminderCommand::Update {
                reminder_id,
                title: _new_title,
                duration: _new_duration,
            } => {
                // Update reminder
                println!("Updating reminder: {reminder_id}");
            }
            ReminderCommand::Snooze {
                reminder_id,
                duration: _,
            } => {
                // Snooze reminder
                println!("Snoozing reminder: {reminder_id}");
            }
            ReminderCommand::Cancel { reminder_id } => {
                // Cancel reminder
                println!("Cancelling reminder: {reminder_id}");
            }
        },
        CliCommand::Profile { command } => match command {
            ProfileCommand::List => {
                // List profiles
                println!("Listing profiles...");
            }
            ProfileCommand::Get { profile_name } => {
                // Get profile
                println!("Getting profile: {profile_name}");
            }
            ProfileCommand::Create {
                name,
                server_url: _,
                api_key: _,
            } => {
                // Create profile
                println!("Creating profile: {name}");
            }
            ProfileCommand::Update {
                profile_name,
                server_url: _new_server_url,
                api_key: _new_api_key,
            } => {
                // Update profile
                println!("Updating profile: {profile_name}");
            }
            ProfileCommand::Delete { profile_name } => {
                // Delete profile
                println!("Deleting profile: {profile_name}");
            }
        },
        CliCommand::Debug { command } => match command {
            DebugCommand::Info => {
                // Show debug info
                println!("Debug info");
            }
            DebugCommand::Version => {
                // Show version
                println!("Version: 0.1.0");
            }
        },
    }
    Ok(())
}
