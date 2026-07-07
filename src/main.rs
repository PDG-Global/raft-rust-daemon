//! Raft daemon - Agent lifecycle management.

use anyhow::Result;
use clap::Parser;

use raft_daemon::cli;
use raft_daemon::cli::args::CliArgs;
use raft_daemon::cli::commands::CliCommand;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Parse CLI args
    let args = CliArgs::parse_from(std::env::args_os());

    // Execute command
    match args.command.as_slice() {
        [c] if c == "start" => {
            let command = CliCommand::Daemon {
                command: cli::commands::DaemonCommand::Start {
                    server_url: args.server_url(),
                    api_key: args.api_key()?,
                    profile: args.profile(),
                },
            };
            cli::commands::execute_command(&command).await?;
        }
        [c] if c == "stop" => {
            let command = CliCommand::Daemon {
                command: cli::commands::DaemonCommand::Stop,
            };
            cli::commands::execute_command(&command).await?;
        }
        [c] if c == "restart" => {
            let command = CliCommand::Daemon {
                command: cli::commands::DaemonCommand::Restart,
            };
            cli::commands::execute_command(&command).await?;
        }
        [c] if c == "status" => {
            let command = CliCommand::Daemon {
                command: cli::commands::DaemonCommand::Status,
            };
            cli::commands::execute_command(&command).await?;
        }
        _ => {
            // Default to agent commands
            if args.command.len() > 1 {
                let command = match args.command[1].as_str() {
                    "list" => CliCommand::Agent {
                        command: cli::commands::AgentCommand::List,
                    },
                    "get" => CliCommand::Agent {
                        command: cli::commands::AgentCommand::Get {
                            agent_id: args.command[2].clone(),
                        },
                    },
                    "create" => CliCommand::Agent {
                        command: cli::commands::AgentCommand::Create {
                            name: args.command[2].clone(),
                            description: args.command[3].clone(),
                            runtime: args.command.get(4).cloned().unwrap_or_default(),
                            role: args
                                .command
                                .get(5)
                                .cloned()
                                .unwrap_or_else(|| "member".to_string()),
                            reset_mode: args
                                .command
                                .get(6)
                                .cloned()
                                .unwrap_or_else(|| "restart".to_string()),
                            workspace: args.command.get(7).cloned().unwrap_or_default(),
                        },
                    },
                    "update" => CliCommand::Agent {
                        command: cli::commands::AgentCommand::Update {
                            agent_id: args.command[2].clone(),
                            name: args.command.get(3).cloned(),
                            description: args.command.get(4).cloned(),
                            runtime: args.command.get(5).cloned(),
                            role: args.command.get(6).cloned(),
                            reset_mode: args.command.get(7).cloned(),
                            workspace: args.command.get(8).cloned(),
                        },
                    },
                    "delete" => CliCommand::Agent {
                        command: cli::commands::AgentCommand::Delete {
                            agent_id: args.command[2].clone(),
                        },
                    },
                    "start" => CliCommand::Agent {
                        command: cli::commands::AgentCommand::Start {
                            agent_id: args.command[2].clone(),
                        },
                    },
                    "stop" => CliCommand::Agent {
                        command: cli::commands::AgentCommand::Stop {
                            agent_id: args.command[2].clone(),
                        },
                    },
                    "restart" => CliCommand::Agent {
                        command: cli::commands::AgentCommand::Restart {
                            agent_id: args.command[2].clone(),
                        },
                    },
                    "reset" => CliCommand::Agent {
                        command: cli::commands::AgentCommand::Reset {
                            agent_id: args.command[2].clone(),
                            mode: args
                                .command
                                .get(3)
                                .cloned()
                                .unwrap_or_else(|| "session_reset".to_string()),
                        },
                    },
                    "status" => CliCommand::Agent {
                        command: cli::commands::AgentCommand::Status {
                            agent_id: args.command[2].clone(),
                        },
                    },
                    _ => {
                        eprintln!("Unknown command: {}", args.command[1]);
                        std::process::exit(1);
                    }
                };
                cli::commands::execute_command(&command).await?;
            } else {
                eprintln!("Usage: raft-daemon <command> [args...]");
                eprintln!();
                eprintln!("Commands:");
                eprintln!("  start              Start the daemon");
                eprintln!("  stop               Stop the daemon");
                eprintln!("  restart            Restart the daemon");
                eprintln!("  status             Show daemon status");
                eprintln!("  agent <command>    Manage agents");
                eprintln!("  server <command>   Manage servers");
                eprintln!("  computer <command> Manage computers");
                eprintln!("  task <command>     Manage tasks");
                eprintln!("  message <command>  Manage messages");
                eprintln!("  reminder <command> Manage reminders");
                eprintln!("  profile <command>  Manage profiles");
                eprintln!("  debug <command>    Debug commands");
            }
        }
    }

    Ok(())
}
