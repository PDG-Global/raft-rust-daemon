//! Unit tests for CLI.

use clap::Parser;

use raft_daemon::cli::args::CliArgs;
use raft_daemon::cli::commands::*;
use raft_daemon::daemon::update::UpdateOptions;

#[test]
fn test_cli_args_parse() {
    let args = CliArgs::parse_from(vec!["raft-daemon", "--profile", "test", "start"]);
    assert_eq!(args.profile, "test");
}

#[test]
fn test_cli_command_daemon_start() {
    let command = CliCommand::Daemon {
        command: DaemonCommand::Start {
            server_url: "wss://test".to_string(),
            api_key: "key-1".to_string(),
            profile: "test".to_string(),
            foreground: false,
            update: UpdateOptions::default(),
        },
    };

    match command {
        CliCommand::Daemon {
            command:
                DaemonCommand::Start {
                    server_url,
                    api_key,
                    profile,
                    foreground,
                    update: _,
                },
        } => {
            assert_eq!(server_url, "wss://test");
            assert_eq!(api_key, "key-1");
            assert_eq!(profile, "test");
            assert!(!foreground);
        }
        _ => panic!("Expected Daemon::Start"),
    }
}

#[test]
fn test_cli_command_agent_list() {
    let command = CliCommand::Agent {
        command: AgentCommand::List,
    };

    match command {
        CliCommand::Agent {
            command: AgentCommand::List,
        } => {}
        _ => panic!("Expected Agent::List"),
    }
}

#[test]
fn test_cli_command_server_create() {
    let command = CliCommand::Server {
        command: ServerCommand::Create {
            name: "Test Server".to_string(),
        },
    };

    match command {
        CliCommand::Server {
            command: ServerCommand::Create { name },
        } => {
            assert_eq!(name, "Test Server");
        }
        _ => panic!("Expected Server::Create"),
    }
}

#[test]
fn test_cli_command_agent_api_cli() {
    let command = CliCommand::AgentApiCli {
        args: vec!["message".to_string(), "send".to_string()],
    };

    match command {
        CliCommand::AgentApiCli { args } => {
            assert_eq!(args, vec!["message", "send"]);
        }
        _ => panic!("Expected AgentApiCli"),
    }
}
