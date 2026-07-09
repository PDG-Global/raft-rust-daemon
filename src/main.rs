//! Raft daemon - Agent lifecycle management.

use anyhow::Result;
use clap::Parser;

use raft_daemon::cli;
use raft_daemon::cli::args::CliArgs;
use raft_daemon::cli::commands::{AgentCommand, CliCommand, DaemonCommand};

#[tokio::main]
async fn main() -> Result<()> {
    // Install the rustls CryptoProvider before any TLS handshake runs.
    //
    // rustls 0.23 refuses to auto-pick a provider when more than one is
    // linked. Our direct dep asks for `ring`, but `tokio-tungstenite`'s
    // `__rustls-tls` feature pulls in `aws-lc-rs` too, so without this call
    // any `wss://` connection panics with "Could not automatically determine
    // the process-level CryptoProvider".
    //
    // `install_default` is idempotent: it returns `Err` only if another
    // provider is already installed, in which case we keep the existing one.
    let _ = rustls::crypto::ring::default_provider().install_default();

    // argv[0] dispatch: if the binary is invoked as `raft` or `slock` (e.g.
    // via a symlink), run the bundled agent-facing CLI directly.
    if let Some(argv0) = std::env::args_os().next().map(std::path::PathBuf::from) {
        if argv0
            .file_stem()
            .is_some_and(|stem| stem == "raft" || stem == "slock")
        {
            let args: Vec<String> = std::env::args().skip(1).collect();
            return cli::raft_cli::run_cli(&args).await;
        }
    }

    // Parse CLI args.
    let args = CliArgs::parse_from(std::env::args_os());

    // Dispatch on the trailing command's first token. We use `first()` rather
    // than slice patterns so flags after the subcommand (e.g.
    // `raft-daemon --foreground start`) still match.
    match args.command.first().map(String::as_str) {
        // === daemon control ===
        Some("start") => {
            let command = CliCommand::Daemon {
                command: DaemonCommand::Start {
                    server_url: args.server_url(),
                    api_key: args.api_key()?,
                    profile: args.profile(),
                    foreground: args.is_foreground(),
                    update: args.update_options(),
                },
            };
            cli::commands::execute_command(&command).await?;
        }
        Some("stop") => {
            let command = CliCommand::Daemon {
                command: DaemonCommand::Stop {
                    profile: args.profile(),
                },
            };
            cli::commands::execute_command(&command).await?;
        }
        Some("status") => {
            let command = CliCommand::Daemon {
                command: DaemonCommand::Status {
                    profile: args.profile(),
                },
            };
            cli::commands::execute_command(&command).await?;
        }
        Some("restart") => {
            let command = CliCommand::Daemon {
                command: DaemonCommand::Restart {
                    profile: args.profile(),
                },
            };
            cli::commands::execute_command(&command).await?;
        }

        // === agent subcommands ===
        Some("agent") => {
            let command = parse_agent_command(&args.command);
            cli::commands::execute_command(&command).await?;
        }

        // === bundled raft/slock CLI ===
        Some("cli") => {
            let command = CliCommand::AgentApiCli {
                args: args.command.iter().skip(1).cloned().collect(),
            };
            cli::commands::execute_command(&command).await?;
        }

        // === help ===
        _ => {
            print_usage();
            if args.command.is_empty() {
                return Ok(());
            }
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Parse `agent <subcommand> [args...]` into a [`CliCommand`].
///
/// Mirrors the simple positional layout used by the npm CLI: every subcommand
/// has a fixed number of positionals. Flags like `--runtime` are kept inline
/// as best-effort; the daemon isn't agent-driven yet, so this is scaffolding
/// for when runtime wiring lands.
fn parse_agent_command(command: &[String]) -> CliCommand {
    let Some(sub) = command.get(1).map(String::as_str) else {
        eprintln!("Usage: raft-daemon agent <command> [args...]");
        eprintln!();
        eprintln!("Commands:");
        eprintln!("  list                          List all agents");
        eprintln!("  get <agent_id>                Get an agent by ID");
        eprintln!("  start <agent_id>              Start an agent");
        eprintln!("  stop <agent_id>               Stop an agent");
        eprintln!("  status <agent_id>             Get an agent's status");
        std::process::exit(1);
    };

    let get = |idx: usize| command.get(idx).cloned().unwrap_or_default();
    let get_or = |idx: usize, default: &str| {
        command
            .get(idx)
            .cloned()
            .unwrap_or_else(|| default.to_string())
    };

    let sub_command = match sub {
        "list" => AgentCommand::List,
        "get" => AgentCommand::Get { agent_id: get(2) },
        "create" => AgentCommand::Create {
            name: get(2),
            description: get(3),
            runtime: get_or(4, ""),
            role: get_or(5, "member"),
            reset_mode: get_or(6, "restart"),
            workspace: get(7),
        },
        "update" => AgentCommand::Update {
            agent_id: get(2),
            name: command.get(3).cloned(),
            description: command.get(4).cloned(),
            runtime: command.get(5).cloned(),
            role: command.get(6).cloned(),
            reset_mode: command.get(7).cloned(),
            workspace: command.get(8).cloned(),
        },
        "delete" => AgentCommand::Delete { agent_id: get(2) },
        "start" => AgentCommand::Start { agent_id: get(2) },
        "stop" => AgentCommand::Stop { agent_id: get(2) },
        "restart" => AgentCommand::Restart { agent_id: get(2) },
        "reset" => AgentCommand::Reset {
            agent_id: get(2),
            mode: get_or(3, "session_reset"),
        },
        "status" => AgentCommand::Status { agent_id: get(2) },
        _ => {
            eprintln!("Unknown agent command: {sub}");
            std::process::exit(1);
        }
    };

    CliCommand::Agent {
        command: sub_command,
    }
}

/// Print the top-level usage banner.
fn print_usage() {
    eprintln!(
        "raft-daemon {} - agent lifecycle management",
        env!("CARGO_PKG_VERSION")
    );
    eprintln!();
    eprintln!("Usage: raft-daemon [options] <command> [args...]");
    eprintln!();
    eprintln!("Daemon commands:");
    eprintln!("  --server-url <url> --api-key <key> start");
    eprintln!("                               Start the daemon (backgrounds by default)");
    eprintln!("      --foreground             Run in the foreground instead");
    eprintln!("  stop                         Stop the running daemon");
    eprintln!("  status                       Show daemon status");
    eprintln!("  restart                      Stop then start (use stop + start for now)");
    eprintln!();
    eprintln!("Other commands:");
    eprintln!("  agent <command>              Manage agents (list, get, start, stop, ...)");
    eprintln!();
    eprintln!("Common options:");
    eprintln!(
        "      --server-url <url>       Server URL (default: $RAFT_SERVER_URL or https://api.raft.build)"
    );
    eprintln!("      --api-key <key>          API key (default: $RAFT_API_KEY)");
    eprintln!("      --profile <name>         Profile name (default: 'default')");
    eprintln!("      --foreground             Run the daemon in the foreground");
    eprintln!("  -v, --verbose                Verbose logging");
    eprintln!("      --debug                  Debug mode");
    eprintln!();
    eprintln!("Environment:");
    eprintln!("  RAFT_SERVER_URL              Default server URL");
    eprintln!("  RAFT_API_KEY                 Default API key");
    eprintln!("  RAFT_DAEMON_HOME             Override daemon state directory (~/.raft-daemon)");
    eprintln!("  RUST_LOG                     tracing filter (e.g. 'info,raft_daemon=debug')");
}
