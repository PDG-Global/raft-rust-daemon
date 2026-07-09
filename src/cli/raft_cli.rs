//! Agent-facing `raft`/`slock` CLI.
//!
//! This is the CLI that the bundled wrapper scripts invoke. It reads the
//! agent-api configuration from environment variables set by the daemon and
//! forwards requests to the local proxy or directly to the raft server.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde_json::json;

use crate::daemon::agent::raft_http_client;

/// Arguments for the bundled `raft`/`slock` CLI.
#[derive(Debug, Parser)]
#[command(name = "raft")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Raft agent CLI")]
pub struct RaftCliArgs {
    #[command(subcommand)]
    pub command: RaftCommand,
}

/// Top-level commands.
#[derive(Debug, Subcommand)]
pub enum RaftCommand {
    /// Send a message.
    Message {
        #[command(subcommand)]
        command: MessageCommand,
    },
    /// Manage reminders.
    Reminder {
        #[command(subcommand)]
        command: ReminderCommand,
    },
    /// Manage tasks.
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },
    /// Read the inbox.
    Inbox,
    /// Read recent events.
    Events {
        /// Only events after this cursor / timestamp.
        #[arg(long)]
        since: Option<String>,
        /// Maximum events to return.
        #[arg(long)]
        limit: Option<u64>,
    },
    /// Read channel history.
    History {
        /// Channel to filter by.
        #[arg(long)]
        channel: Option<String>,
    },
    /// Get server info.
    Server,
}

/// Message subcommands.
#[derive(Debug, Subcommand)]
pub enum MessageCommand {
    /// Send a message.
    Send {
        /// Target descriptor (e.g. `#channel` or `dm:@user`).
        #[arg(long)]
        target: String,
        /// Message content.
        #[arg(long)]
        content: String,
        /// Highest seq seen up to this send.
        #[arg(long)]
        seen_up_to_seq: Option<i64>,
    },
}

/// Reminder subcommands.
#[derive(Debug, Subcommand)]
pub enum ReminderCommand {
    /// Create a reminder.
    Create {
        /// Reminder title.
        #[arg(long)]
        title: String,
        /// Absolute fire time (ISO-8601 or server format).
        #[arg(long)]
        fire_at: Option<String>,
        /// Relative delay in seconds.
        #[arg(long)]
        delay_seconds: Option<i64>,
        /// Channel to post the reminder in.
        #[arg(long)]
        channel: Option<String>,
        /// Optional JSON payload.
        #[arg(long)]
        payload: Option<String>,
    },
    /// List reminders.
    List {
        /// Filter by status.
        #[arg(long)]
        status: Option<String>,
    },
    /// Delete a reminder.
    Delete {
        /// Reminder ID.
        id: String,
    },
}

/// Task subcommands.
#[derive(Debug, Subcommand)]
pub enum TaskCommand {
    /// List tasks.
    List {
        /// Filter by channel.
        #[arg(long)]
        channel: Option<String>,
    },
    /// Create tasks.
    Create {
        /// Channel ID.
        #[arg(long)]
        channel: String,
        /// Task titles.
        #[arg(long, required = true)]
        title: Vec<String>,
    },
    /// Claim tasks.
    Claim {
        /// Channel ID.
        #[arg(long)]
        channel: Option<String>,
        /// Task numbers to claim.
        #[arg(long)]
        task_number: Vec<i64>,
        /// Message IDs to claim.
        #[arg(long)]
        message_id: Vec<String>,
    },
    /// Update a task status.
    UpdateStatus {
        /// Channel ID.
        #[arg(long)]
        channel: String,
        /// Task number.
        #[arg(long)]
        task_number: i64,
        /// New status.
        #[arg(long)]
        status: String,
    },
}

/// Run the agent-facing CLI with the given arguments.
///
/// This parses `args` as `raft <subcommand> ...` and dispatches to the local
/// agent-api proxy or the raft server.
///
/// # Errors
///
/// Returns an error if the environment is not configured, the request fails,
/// or the server returns a non-2xx status.
pub async fn run_cli(args: &[String]) -> Result<()> {
    let mut parsed_args = vec!["raft".to_string()];
    parsed_args.extend(args.iter().cloned());
    let cli = RaftCliArgs::parse_from(parsed_args);

    let (base_url, token) = resolve_config()?;
    let client = raft_http_client().context("building agent-api client")?;

    let value = dispatch(cli.command, &client, &base_url, &token).await?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

/// Resolve the target base URL and bearer token from the environment.
fn resolve_config() -> Result<(String, String)> {
    if let (Ok(proxy_url), Ok(token_file)) = (
        std::env::var("SLOCK_AGENT_PROXY_URL"),
        std::env::var("SLOCK_AGENT_PROXY_TOKEN_FILE"),
    ) {
        let token = std::fs::read_to_string(&token_file)
            .with_context(|| format!("reading proxy token file {token_file}"))?;
        return Ok((proxy_url, token.trim().to_string()));
    }

    if let (Ok(server_url), Ok(token_file)) = (
        std::env::var("SLOCK_SERVER_URL"),
        std::env::var("SLOCK_AGENT_TOKEN_FILE"),
    ) {
        let token = std::fs::read_to_string(&token_file)
            .with_context(|| format!("reading agent token file {token_file}"))?;
        let base = format!("{}/internal/agent-api", server_url.trim_end_matches('/'));
        return Ok((base, token.trim().to_string()));
    }

    anyhow::bail!(
        "no agent API configuration: set SLOCK_AGENT_PROXY_URL and SLOCK_AGENT_PROXY_TOKEN_FILE, \
         or SLOCK_SERVER_URL and SLOCK_AGENT_TOKEN_FILE"
    )
}

async fn dispatch(
    command: RaftCommand,
    client: &reqwest::Client,
    base_url: &str,
    token: &str,
) -> Result<serde_json::Value> {
    let base = base_url.trim_end_matches('/');
    match command {
        RaftCommand::Message {
            command:
                MessageCommand::Send {
                    target,
                    content,
                    seen_up_to_seq,
                },
        } => {
            let mut body = json!({
                "target": target,
                "content": content,
            });
            if let Some(seq) = seen_up_to_seq {
                body["seenUpToSeq"] = json!(seq);
            }
            post_json(client, &format!("{base}/send"), token, &body).await
        }
        RaftCommand::Reminder {
            command:
                ReminderCommand::Create {
                    title,
                    fire_at,
                    delay_seconds,
                    channel,
                    payload,
                },
        } => {
            let mut body = json!({ "title": title });
            if let Some(t) = fire_at {
                body["fireAt"] = json!(t);
            }
            if let Some(d) = delay_seconds {
                body["delaySeconds"] = json!(d);
            }
            if let Some(c) = channel {
                body["channel"] = json!(c);
            }
            if let Some(p) = payload {
                body["payload"] = serde_json::from_str(&p).unwrap_or(json!(p));
            }
            post_json(client, &format!("{base}/reminders"), token, &body).await
        }
        RaftCommand::Reminder {
            command: ReminderCommand::List { status },
        } => {
            let mut req = client.get(format!("{base}/reminders"));
            if let Some(s) = status {
                req = req.query(&[("status", s)]);
            }
            send_json(req, token).await
        }
        RaftCommand::Reminder {
            command: ReminderCommand::Delete { id },
        } => {
            delete_json(client, &format!("{base}/reminders/{id}"), token).await
        }
        RaftCommand::Task {
            command: TaskCommand::List { channel },
        } => {
            let mut req = client.get(format!("{base}/tasks"));
            if let Some(c) = channel {
                req = req.query(&[("channel", c)]);
            }
            send_json(req, token).await
        }
        RaftCommand::Task {
            command: TaskCommand::Create { channel, title },
        } => {
            let tasks: Vec<serde_json::Value> = title
                .into_iter()
                .map(|t| json!({ "title": t }))
                .collect();
            let body = json!({
                "channel": channel,
                "tasks": tasks,
            });
            post_json(client, &format!("{base}/tasks"), token, &body).await
        }
        RaftCommand::Task {
            command:
                TaskCommand::Claim {
                    channel,
                    task_number,
                    message_id,
                },
        } => {
            let mut body = serde_json::Map::new();
            if let Some(c) = channel {
                body.insert("channel".to_string(), json!(c));
            }
            if !task_number.is_empty() {
                body.insert("task_numbers".to_string(), json!(task_number));
            }
            if !message_id.is_empty() {
                body.insert("message_ids".to_string(), json!(message_id));
            }
            post_json(
                client,
                &format!("{base}/tasks/claim"),
                token,
                &serde_json::Value::Object(body),
            )
            .await
        }
        RaftCommand::Task {
            command:
                TaskCommand::UpdateStatus {
                    channel,
                    task_number,
                    status,
                },
        } => {
            let body = json!({
                "channel": channel,
                "task_number": task_number,
                "status": status,
            });
            post_json(client, &format!("{base}/tasks/update-status"), token, &body).await
        }
        RaftCommand::Inbox => get_json(client, &format!("{base}/inbox"), token).await,
        RaftCommand::Events { since, limit } => {
            let mut req = client.get(format!("{base}/events"));
            if let Some(s) = since {
                req = req.query(&[("since", s)]);
            }
            if let Some(l) = limit {
                req = req.query(&[("limit", l.to_string())]);
            }
            send_json(req, token).await
        }
        RaftCommand::History { channel } => {
            let mut req = client.get(format!("{base}/history"));
            if let Some(c) = channel {
                req = req.query(&[("channel", c)]);
            }
            send_json(req, token).await
        }
        RaftCommand::Server => get_json(client, &format!("{base}/server"), token).await,
    }
}

async fn post_json(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value> {
    send_json(client.post(url).json(body), token).await
}

async fn get_json(
    client: &reqwest::Client,
    url: &str,
    token: &str,
) -> Result<serde_json::Value> {
    send_json(client.get(url), token).await
}

async fn delete_json(
    client: &reqwest::Client,
    url: &str,
    token: &str,
) -> Result<serde_json::Value> {
    send_json(client.delete(url), token).await
}

async fn send_json(
    mut req: reqwest::RequestBuilder,
    token: &str,
) -> Result<serde_json::Value> {
    req = req
        .bearer_auth(token)
        .header("X-Slock-Client", "raft-daemon-cli");
    let resp = req.send().await.context("sending agent-api request")?;
    let status = resp.status();
    let text = resp.text().await.context("reading agent-api response")?;
    if !status.is_success() {
        anyhow::bail!("agent-api request failed: HTTP {status}: {text}");
    }
    serde_json::from_str(&text)
        .with_context(|| format!("decoding agent-api response: {text}"))
}

/// Format a generic Unix wrapper script for `raft` or `slock`.
#[cfg(unix)]
fn format_sh_wrapper(exe: &str) -> String {
    format!("#!/bin/sh\nexec \"{exe}\" cli \"$@\"\n")
}

/// Format a generic Windows wrapper batch file for `raft` or `slock`.
#[cfg(not(unix))]
fn format_cmd_wrapper(exe: &str) -> String {
    format!("@echo off\n\"{exe}\" cli %*\n")
}

/// Write generic `raft` and `slock` wrapper scripts into `<home>/bin`.
///
/// The wrappers are thin shims that re-exec the current binary as
/// `raft-daemon cli "$@"`, inheriting the agent-specific environment variables
/// set by the daemon when it spawns RustyCLI.
///
/// # Errors
///
/// Returns an error if the wrapper directory cannot be created or the scripts
/// cannot be written.
pub fn ensure_cli_wrappers(home: &std::path::Path) -> Result<PathBuf> {
    let bin_dir = home.join("bin");
    std::fs::create_dir_all(&bin_dir)
        .with_context(|| format!("creating wrapper dir {}", bin_dir.display()))?;

    let exe = std::env::current_exe().context("locating current executable")?;
    let exe_str = exe.to_str().context("non-UTF8 executable path")?;

    #[cfg(unix)]
    {
        let raft = bin_dir.join("raft");
        let slock = bin_dir.join("slock");
        write_unix_wrapper(&raft, exe_str)?;
        write_unix_wrapper(&slock, exe_str)?;
    }
    #[cfg(not(unix))]
    {
        let raft = bin_dir.join("raft.cmd");
        let slock = bin_dir.join("slock.cmd");
        std::fs::write(&raft, format_cmd_wrapper(exe_str))
            .with_context(|| format!("writing wrapper {}", raft.display()))?;
        std::fs::write(&slock, format_cmd_wrapper(exe_str))
            .with_context(|| format!("writing wrapper {}", slock.display()))?;
    }

    Ok(bin_dir)
}

#[cfg(unix)]
fn write_unix_wrapper(path: &std::path::Path, exe: &str) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o755)
        .open(path)
        .with_context(|| format!("opening wrapper {}", path.display()))?;
    file.write_all(format_sh_wrapper(exe).as_bytes())
        .with_context(|| format!("writing wrapper {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_message_send() {
        let args = RaftCliArgs::parse_from([
            "raft",
            "message",
            "send",
            "--target",
            "#general",
            "--content",
            "hello",
        ]);
        match args.command {
            RaftCommand::Message {
                command: MessageCommand::Send { target, content, .. },
            } => {
                assert_eq!(target, "#general");
                assert_eq!(content, "hello");
            }
            _ => panic!("expected message send"),
        }
    }

    #[test]
    fn parse_task_create_requires_titles() {
        let err = RaftCliArgs::try_parse_from([
            "raft",
            "task",
            "create",
            "--channel",
            "ch_1",
        ]);
        assert!(err.is_err(), "task create should require --title");
    }

    #[test]
    fn parse_task_create_accepts_multiple_titles() {
        let args = RaftCliArgs::parse_from([
            "raft",
            "task",
            "create",
            "--channel",
            "ch_1",
            "--title",
            "one",
            "--title",
            "two",
        ]);
        match args.command {
            RaftCommand::Task {
                command: TaskCommand::Create { channel, title },
            } => {
                assert_eq!(channel, "ch_1");
                assert_eq!(title, vec!["one", "two"]);
            }
            _ => panic!("expected task create"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn unix_wrapper_execs_current_binary() {
        let wrapper = format_sh_wrapper("/usr/local/bin/raft-daemon");
        assert!(wrapper.starts_with("#!/bin/sh\n"));
        assert!(wrapper.contains("exec \"/usr/local/bin/raft-daemon\" cli \"$@\""));
    }

    #[test]
    fn ensure_cli_wrappers_creates_scripts() {
        let tmp = tempfile::tempdir().unwrap();
        let bin_dir = ensure_cli_wrappers(tmp.path()).unwrap();
        assert!(bin_dir.starts_with(tmp.path()));
        assert!(bin_dir.join("raft").exists());
        assert!(bin_dir.join("slock").exists());
    }
}
