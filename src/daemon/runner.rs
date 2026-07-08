//! Daemon orchestration.
//!
//! This module wires the `start` / `stop` / `status` / `restart` CLI commands
//! to a real, long-running daemon process. Two execution modes are supported:
//!
//! - **Background (default)**: `start` spawns a detached child running the
//!   daemon in `--foreground` mode, prints its PID, and returns. The child
//!   survives the parent shell via `setsid()` and writes its logs to
//!   `<home>/logs/daemon.log`.
//! - **Foreground**: `start --foreground` runs the WebSocket event loop in
//!   the current process, logging to stdout when interactive and to the log
//!   file otherwise. Honours `SIGINT`/`SIGTERM` for graceful shutdown.
//!
//! The event loop connects to `<server_url>/daemon/connect?key=<api_key>`,
//! dispatches inbound messages to the appropriate manager
//! ([`crate::daemon::agent`], [`crate::daemon::task`], …), and reconnects with
//! exponential backoff on transient failures.

use std::io::IsTerminal;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use futures::stream::StreamExt;
use futures::{SinkExt, stream::SplitSink};
use tokio::process::{Child, Command as TokioCommand};
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use crate::daemon::agent::{
    AgentProcess, AgentProcessRegistry, SendBody, mint_runner_credential,
    run_one_turn, send_agent_message, ProviderConfig,
};
use crate::daemon::paths;
use crate::daemon::pidfile;
use crate::daemon::state::{DaemonState, StateMgr};
use crate::models::RuntimeConfig;

/// Reconnect backoff ceiling.
const MAX_RECONNECT_BACKOFF: Duration = Duration::from_secs(30);
/// Initial reconnect backoff.
const INITIAL_RECONNECT_BACKOFF: Duration = Duration::from_secs(1);
/// Interval between outbound liveness pings.
const PING_INTERVAL: Duration = Duration::from_secs(30);
/// Grace period after which a liveness probe forces a reconnect.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);
/// Marker RustyCLI should output when it chooses not to reply. The daemon will
/// not post this marker (or an empty response) to raft.
const NO_REPLY_MARKER: &str = "NO_REPLY";

/// Options describing how to start the daemon.
#[derive(Debug, Clone)]
pub struct DaemonOptions {
    /// The server URL (e.g. `https://api.raft.build`).
    pub server_url: String,
    /// The API key for authentication.
    pub api_key: String,
    /// The profile name.
    pub profile: String,
    /// Run in the foreground (true) or spawn a detached child (false).
    pub foreground: bool,
}

impl DaemonOptions {
    /// Build the WebSocket URL for `/daemon/connect?key=<api_key>`.
    ///
    /// Accepts the server URL as `https://`, `http://`, `wss://`, or `ws://`,
    /// normalising HTTP to WS. A bare host defaults to `wss://`. Refuses to
    /// silently downgrade from `wss://` to `ws://`.
    ///
    /// # Errors
    ///
    /// Returns an error if the URL has an unsupported scheme.
    fn ws_url(&self) -> Result<String> {
        let trimmed = self.server_url.trim_end_matches('/');
        let scheme_base = if let Some(rest) = trimmed.strip_prefix("https://") {
            format!("wss://{rest}")
        } else if let Some(rest) = trimmed.strip_prefix("http://") {
            format!("ws://{rest}")
        } else if trimmed.starts_with("wss://") || trimmed.starts_with("ws://") {
            trimmed.to_string()
        } else if trimmed.contains("://") {
            anyhow::bail!(
                "unsupported scheme in server_url: {trimmed}; use https:// or wss://"
            );
        } else {
            format!("wss://{trimmed}")
        };
        Ok(format!(
            "{scheme_base}/daemon/connect?key={}",
            self.api_key
        ))
    }
}

/// Outcome of [`start`].
#[derive(Debug)]
pub enum StartOutcome {
    /// The daemon was spawned in the background with this PID.
    Spawned(u32),
    /// The daemon ran in the foreground and exited cleanly.
    ForegroundFinished,
}

/// Status report from [`status`].
#[derive(Debug, Clone, Copy)]
pub enum StatusReport {
    /// The daemon is running with this PID.
    Running(i32),
    /// No PID file found; the daemon is not configured to run.
    NotConfigured,
    /// A PID file exists but the process is dead (the file is stale).
    Stale(i32),
}

impl StatusReport {
    /// Whether the daemon is alive.
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running(_))
    }
}

impl std::fmt::Display for StatusReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Running(pid) => write!(f, "running (pid={pid})"),
            Self::NotConfigured => write!(f, "not running"),
            Self::Stale(pid) => write!(f, "not running (stale pid file for pid={pid})"),
        }
    }
}

// ============================================================
// start
// ============================================================

/// Start the daemon.
///
/// Spawns a detached background child unless `opts.foreground` is set, in
/// which case the daemon loop runs in the current process and does not
/// return until shutdown.
///
/// # Errors
///
/// Returns an error if a daemon is already running, the background child
/// cannot be spawned, or — when running in the foreground — the daemon loop
/// fails fatally.
pub async fn start(opts: DaemonOptions) -> Result<StartOutcome> {
    if opts.foreground {
        run_foreground(opts).await?;
        Ok(StartOutcome::ForegroundFinished)
    } else {
        let pid = spawn_background(&opts)?;
        Ok(StartOutcome::Spawned(pid))
    }
}

/// Spawn a detached child running the daemon in foreground mode.
fn spawn_background(opts: &DaemonOptions) -> Result<u32> {
    let pid_path = paths::pid_file_for_profile(&opts.profile)?;
    refuse_if_already_running(&pid_path)?;

    let exe = std::env::current_exe().context("locating current executable")?;
    let log_file = open_log_writer(&opts.profile)?;

    // Build the std command first so we can attach Unix-specific pre_exec
    // (setsid) before converting into a tokio command.
    let mut std_cmd = std::process::Command::new(&exe);
    std_cmd
        .arg("--foreground")
        .arg("--server-url")
        .arg(&opts.server_url)
        .arg("--api-key")
        .arg(&opts.api_key)
        .arg("--profile")
        .arg(&opts.profile)
        .arg("start")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    detach_process(&mut std_cmd);

    let mut cmd = TokioCommand::from(std_cmd);
    cmd.stdout(Stdio::from(log_file.try_clone()?))
        .stderr(Stdio::from(log_file));

    let child = cmd.spawn().context("spawning daemon child")?;
    let pid = child.id().context("child has no PID")?;
    // Detach: do not wait on the child. Dropping `Child` without `wait` leaks
    // the handle but leaves the process running, which is what we want here.
    detach_child(child);

    // Persist the PID the parent observed. The child will overwrite this with
    // its own PID once `run_foreground` starts; if the child crashes before
    // then, this file lets `stop` clean up the (now-dead) entry.
    let pid_i32 = i32::try_from(pid).unwrap_or(i32::MAX);
    pidfile::write_pid(&pid_path, pid_i32)?;

    let home = paths::home_dir_for_profile(&opts.profile)?;
    println!("raft daemon started (pid={pid})");
    println!("  home:    {}", home.display());
    println!("  logs:    {}", paths::log_file_for_profile(&opts.profile)?.display());
    println!("  server:  {}", opts.server_url);
    println!("  profile: {}", opts.profile);
    println!("\nUse `raft-daemon stop` to shut down.");

    Ok(pid)
}

/// Open the daemon log file in append mode for redirecting the spawned
/// child's stdout/stderr.
fn open_log_writer(profile: &str) -> Result<std::fs::File> {
    let path = paths::log_file_for_profile(profile)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&path)
            .with_context(|| format!("opening log file {}", path.display()))
    }
    #[cfg(not(unix))]
    {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("opening log file {}", path.display()))
    }
}

/// Configure the spawned child to fully detach from the controlling
/// terminal: new session via `setsid()`, new process group, ignored `SIGHUP`.
#[cfg(unix)]
fn detach_process(cmd: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;

    // Place the child in its own process group so terminal signals aimed at
    // the parent's group don't propagate to it.
    cmd.process_group(0);

    // Safety: `setsid()` is async-signal-safe and is called after fork but
    // before exec. We ignore its return value because failure (e.g.
    // EPERM if already a session leader) is non-fatal for our purposes.
    unsafe {
        cmd.pre_exec(|| {
            let _ = libc::setsid();
            // Reset sigmask to defaults so the child isn't inheriting
            // blocked signals from the parent.
            let _ = libc::sigprocmask(libc::SIG_SETMASK, std::ptr::null(), std::ptr::null_mut());
            // Ignore SIGHUP so terminal hangup doesn't kill the daemon.
            let mut act: libc::sigaction = std::mem::zeroed();
            act.sa_sigaction = libc::SIG_IGN;
            let act_ptr = std::ptr::addr_of!(act);
            let _ = libc::sigaction(libc::SIGHUP, act_ptr, std::ptr::null_mut());
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn detach_process(_cmd: &mut std::process::Command) {
    // No-op: background daemon mode is Unix-only for now.
}

/// Forget the child handle without killing the process.
///
/// Tokio's `Child` does not kill on drop unless `kill_on_drop(true)` was set,
/// which we never do, so dropping is sufficient. We null out the captured
/// stdio pipes explicitly so they don't linger waiting for EOF that may never
/// come once the parent process exits.
fn detach_child(mut child: Child) {
    drop(child.stdout.take());
    drop(child.stderr.take());
    drop(child);
}

/// Bail out if a daemon is already running. Stale PID files are removed.
///
/// Returns `Ok(())` if the PID file is absent, dead, or already points at the
/// current process (which is the case when the parent spawner wrote the
/// child's PID before the child took over).
fn refuse_if_already_running(pid_path: &std::path::Path) -> Result<()> {
    let Some(existing) = pidfile::read_pid(pid_path)? else {
        return Ok(());
    };
    let me = i32::try_from(std::process::id()).unwrap_or(i32::MAX);
    if existing == me {
        // The parent spawned us and pre-wrote our PID; that's fine.
        return Ok(());
    }
    if pidfile::is_alive(existing) {
        anyhow::bail!("daemon already running (pid={existing})");
    }
    warn!(pid = existing, "removing stale PID file");
    pidfile::remove_pid(pid_path);
    Ok(())
}

// ============================================================
// foreground run loop
// ============================================================

/// Run the daemon in the foreground until shutdown is requested.
async fn run_foreground(opts: DaemonOptions) -> Result<()> {
    init_tracing(&opts.profile)?;

    let home = paths::home_dir_for_profile(&opts.profile)?;
    let pid_path = paths::pid_file_for_profile(&opts.profile)?;
    refuse_if_already_running(&pid_path)?;
    let my_pid = i32::try_from(std::process::id()).unwrap_or(i32::MAX);
    pidfile::write_pid(&pid_path, my_pid)?;
    info!(pid = my_pid, version = env!("CARGO_PKG_VERSION"), "raft daemon starting (foreground)");
    info!(server_url = %opts.server_url, profile = %opts.profile, "configuration loaded");
    info!(home = %home.display(), "daemon home");

    let state = build_initial_state(&opts, &home);
    let state: Arc<dyn StateMgr> = Arc::new(state);

    // Ensure the agents directory exists before any agent:start arrives so
    // per-agent workspace creation is just a `create_dir_all` away.
    let agents_dir = home.join("agents");
    if let Err(err) = std::fs::create_dir_all(&agents_dir) {
        warn!(error = %err, "failed to create agents dir; agent starts will fail");
    }

    // Per-connection registry of running agents. Lives for the lifetime of
    // the process; cleared entries' RustyCLI child processes are awaited /
    // dropped via `agent:stop`.
    let agents: Arc<AgentProcessRegistry> = Arc::new(AgentProcessRegistry::new());

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    spawn_signal_handler(shutdown_tx);

    let loop_result = run_event_loop(opts.clone(), state, agents, opts.api_key.clone(), shutdown_rx).await;

    // Always clean up the PID file on exit, even on error.
    pidfile::remove_pid(&pid_path);
    info!("raft daemon shutdown complete");

    loop_result
}

/// Construct the initial `DaemonState`, loading from disk if present.
fn build_initial_state(opts: &DaemonOptions, home: &std::path::Path) -> DaemonState {
    let state_path = home.join("state.json");
    if state_path.exists() {
        match DaemonState::load(&state_path) {
            Ok(mut loaded) => {
                // Re-use the persisted state but clear server identity and
                // update the workspace/profile in case the daemon was moved
                // between profiles or directories.
                loaded.server_id.clear();
                loaded.server = None;
                loaded.profile.clone_from(&opts.profile);
                loaded.workspace.clone_from(&home.to_path_buf());
                tracing::debug!("loaded existing state.json");
                return loaded;
            }
            Err(err) => {
                warn!(error = %err, "could not load existing state.json; starting fresh");
            }
        }
    }
    DaemonState::new(
        // The server_id is learned from the server once connected; use a
        // placeholder until then.
        String::new(),
        None,
        RuntimeConfig {
            model: String::new(),
            tools: Vec::new(),
            parameters: serde_json::json!({}),
        },
        opts.profile.clone(),
        home.to_path_buf(),
    )
}

/// Run the WebSocket event loop, reconnecting with exponential backoff until
/// shutdown is requested or a fatal error occurs.
async fn run_event_loop(
    opts: DaemonOptions,
    state: Arc<dyn StateMgr>,
    agents: Arc<AgentProcessRegistry>,
    api_key: String,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<()> {
    let mut backoff = INITIAL_RECONNECT_BACKOFF;

    loop {
        if *shutdown_rx.borrow() {
            info!("shutdown requested; exiting event loop");
            return Ok(());
        }

        let ws_url = opts.ws_url()?;
        info!(url = %redact_url(&ws_url), "connecting to server");

        match connect_async_with_opts(&ws_url).await {
            Ok((ws, response)) => {
                info!(
                    status = response
                        .status()
                        .as_u16(),
                    "connected to server"
                );
                backoff = INITIAL_RECONNECT_BACKOFF;
                match drive_connection(
                    ws,
                    state.clone(),
                    agents.clone(),
                    opts.server_url.clone(),
                    api_key.clone(),
                    opts.profile.clone(),
                    &mut shutdown_rx,
                )
                .await
                {
                    Ok(ConnectionOutcome::Shutdown) => return Ok(()),
                    Ok(ConnectionOutcome::Disconnected) => {
                        warn!("connection lost; will reconnect");
                    }
                    Err(err) => {
                        warn!(error = %err, "connection driver error; will reconnect");
                    }
                }
            }
            Err(err) => {
                let backoff_ms = u64::try_from(backoff.as_millis()).unwrap_or(u64::MAX);
                warn!(
                    error = %friendly_connect_error(&err),
                    backoff_ms = backoff_ms,
                    "connection failed"
                );
            }
        }

        // Wait either the backoff duration or a shutdown signal, whichever
        // comes first.
        tokio::select! {
            () = tokio::time::sleep(backoff) => {}
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    return Ok(());
                }
            }
        }
        backoff = (backoff * 2).min(MAX_RECONNECT_BACKOFF);
    }
}

/// Establish the WebSocket connection with sane defaults.
///
/// Builds the request via `IntoClientRequest` so the WebSocket upgrade
/// headers (`Sec-WebSocket-Key`, `Upgrade: websocket`, …) are populated
/// automatically; building an `http::Request` by hand and passing it
/// directly to `connect_async` skips that step and the handshake fails
/// with "Missing, duplicated or incorrect header sec-websocket-key".
async fn connect_async_with_opts(
    url: &str,
) -> Result<(
    WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tokio_tungstenite::tungstenite::handshake::client::Response,
)> {
    let mut request = url.into_client_request()?;
    {
        let headers = request.headers_mut();
        // Header values must be ASCII; if the env-supplied strings somehow
        // aren't, that's a programming error worth surfacing as a bail.
        headers.insert(
            http::header::USER_AGENT,
            format!("raft-daemon/{}", env!("CARGO_PKG_VERSION"))
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid User-Agent value: {e}"))?,
        );
        headers.insert(
            "X-Slock-Client",
            "raft-daemon-rust"
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid X-Slock-Client value: {e}"))?,
        );
        // Intentionally not offering Sec-WebSocket-Protocol: tungstenite
        // strictly requires the server to echo one back if we offer any,
        // which most Raft servers don't.
    }
    connect_async(request).await.map_err(Into::into)
}

/// Why the connection ended.
enum ConnectionOutcome {
    /// Graceful shutdown was requested by the operator.
    Shutdown,
    /// The connection was lost and should be re-established.
    Disconnected,
}

/// Drive a single connection: spawn the writer + ping tasks, then read
/// inbound messages until shutdown or until the socket closes.
async fn drive_connection(
    ws: WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    state: Arc<dyn StateMgr>,
    agents: Arc<AgentProcessRegistry>,
    server_url: String,
    api_key: String,
    profile: String,
    shutdown_rx: &mut watch::Receiver<bool>,
) -> Result<ConnectionOutcome> {
    let (write, mut read) = ws.split();
    let (outbound_tx, outbound_rx) = mpsc::channel::<WsMessage>(64);

    let writer_task = tokio::spawn(run_writer(write, outbound_rx));
    let ping_handle = spawn_pinger(outbound_tx.clone());

    // Send the daemon `ready` frame so the server knows who we are and which
    // runtimes are installed. Without this the raft UI shows "no detected
    // runtime" and refuses to schedule agent work onto this machine.
    let running: Vec<String> = agents.agent_ids();
    if let Err(err) = send_ready_frame(&outbound_tx, &running).await {
        warn!(error = %err, "failed to send ready frame");
    }

    // Re-announce any agents that were persisted as running from a previous
    // daemon session. This keeps the UI showing them as online after a daemon
    // restart without waiting for the server to send new `agent:start` frames.
    for (agent_id, payload) in state.running_agents() {
        if agents.contains(&agent_id) {
            tracing::debug!(agent_id = %agent_id, "agent already restored; skipping duplicate start");
            continue;
        }
        info!(agent_id = %agent_id, "restoring persisted running agent");
        if let Err(err) = start_agent(
            &outbound_tx,
            &agents,
            &state,
            &server_url,
            &api_key,
            &profile,
            &payload,
        )
        .await
        {
            warn!(error = %err, agent_id = %agent_id, "failed to restore running agent");
        }
    }

    let mut outcome = ConnectionOutcome::Disconnected;

    loop {
        tokio::select! {
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    info!("shutdown requested; closing connection");
                    let _ = outbound_tx.send(WsMessage::Close(None)).await;
                    outcome = ConnectionOutcome::Shutdown;
                    break;
                }
            }
            msg = read.next() => {
                match msg {
                    Some(Ok(WsMessage::Text(text))) => {
                        if let Err(err) = handle_server_message(
                            &text,
                            &outbound_tx,
                            &state,
                            &agents,
                            &server_url,
                            &api_key,
                            &profile,
                        ).await {
                            warn!(error = %err, "error handling server message");
                        }
                    }
                    Some(Ok(WsMessage::Binary(data))) => {
                        tracing::debug!(len = data.len(), "ignoring binary frame");
                    }
                    Some(Ok(WsMessage::Ping(data))) => {
                        let _ = outbound_tx.send(WsMessage::Pong(data)).await;
                    }
                    Some(Ok(WsMessage::Pong(_))) => {
                        tracing::trace!("pong received");
                    }
                    Some(Ok(WsMessage::Close(reason))) => {
                        info!(reason = ?reason.map(|r| r.reason.to_string()), "server closed connection");
                        break;
                    }
                    Some(Ok(WsMessage::Frame(_))) => {
                        // tungstenite handles Raw frames internally; this arm
                        // is unreachable in normal use but covers future variants.
                    }
                    Some(Err(err)) => {
                        warn!(error = %err, "websocket read error");
                        break;
                    }
                    None => {
                        info!("websocket stream ended");
                        break;
                    }
                }
            }
        }
    }

    ping_handle.abort();
    // Tell the writer to drain and exit by dropping the sender.
    drop(outbound_tx);
    let _ = writer_task.await;
    Ok(outcome)
}

/// Background writer: pull outbound messages from the channel and push them
/// to the socket. Exits when the channel is closed.
async fn run_writer(
    mut sink: SplitSink<
        WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
        WsMessage,
    >,
    mut outbound_rx: mpsc::Receiver<WsMessage>,
) {
    while let Some(msg) = outbound_rx.recv().await {
        if let Err(err) = sink.send(msg).await {
            warn!(error = %err, "websocket send failed");
            break;
        }
    }
    let _ = sink.close().await;
}

/// Spawn the periodic liveness pinger. Returns a handle that the caller can
/// cancel when the connection ends.
fn spawn_pinger(tx: mpsc::Sender<WsMessage>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(PING_INTERVAL);
        // First tick fires immediately; skip it.
        interval.tick().await;
        loop {
            interval.tick().await;
            if tx.send(WsMessage::Ping(Vec::new())).await.is_err() {
                break;
            }
        }
    })
}

// ============================================================
// server message dispatch
// ============================================================

/// Handle a single inbound server message.
///
/// Parses the envelope and routes by `type`. Unknown kinds are logged at
/// debug level so the daemon doesn't crash on protocol additions from newer
/// servers.
///
/// # Errors
///
/// Returns an error if the response cannot be sent over the channel.
async fn handle_server_message(
    text: &str,
    outbound: &mpsc::Sender<WsMessage>,
    state: &Arc<dyn StateMgr>,
    agents: &Arc<AgentProcessRegistry>,
    server_url: &str,
    api_key: &str,
    profile: &str,
) -> Result<()> {
    let value: serde_json::Value = serde_json::from_str(text).context("parsing server JSON")?;
    let kind = value
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // Routine pings/pongs log at DEBUG so INFO logs aren't flooded every
    // 30 seconds by the liveness heartbeat.
    if matches!(kind, "ping" | "pong") {
        tracing::debug!(kind = kind, "inbound liveness message");
    } else {
        info!(kind = kind, "inbound server message");
    }

    match kind {
        "ping" => {
            send_json(outbound, serde_json::json!({"type": "pong"})).await?;
            tracing::debug!("sent pong");
        }
        "pong" => {
            // Server's liveness ack to our outbound ping; nothing to do.
        }
        "ready" | "hello" => {
            let server_id = value
                .get("server_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let machine_id = value
                .get("machine_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            info!(server_id = server_id, machine_id = machine_id, "server ready");
            // TODO: persist observed server/machine identity into state.
        }

        // ---- agent lifecycle ----

        // Server wants us to host an agent. Install per-agent state and
        // acknowledge with `agent:session` + `agent:status: active` so the
        // server routes future deliveries here. We do NOT spawn RustyCLI
        // eagerly; that happens on the first `agent:deliver`.
        "agent:start" => {
            start_agent(outbound, agents, state, server_url, api_key, profile, &value).await?;
        }

        // Server delivered a chat message to an agent. We ack immediately so
        // the server doesn't retry, then spawn RustyCLI in the background to
        // actually process the prompt. The agent's chat reply is logged for
        // now — wiring it into raft chat requires the runner-credential /
        // POST-as-agent path which isn't yet implemented.
        "agent:deliver" => {
            let agent_id = value
                .get("agentId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let delivery_id = value.get("deliveryId").cloned();
            let launch_id = agents
                .with(&agent_id, |p| p.launch_id.clone())
                .flatten();
            let seq = value
                .get("seq")
                .and_then(serde_json::Value::as_i64)
                .or_else(|| {
                    value
                        .pointer("/message/seq")
                        .and_then(serde_json::Value::as_i64)
                })
                .unwrap_or(0);
            let prompt = value
                .pointer("/message/content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // Dump the raw delivery payload so operators can inspect schema
            // details (sender, mentions, channel, memory, etc.) in debug logs.
            tracing::debug!(
                agent_id = %agent_id,
                seq = seq,
                delivery = %value,
                "raw agent:deliver payload"
            );

            // Ack first so the server stops retrying — see npm
            // `sendDeliveryAck` at chunk-URPIDKXK.js:21942.
            send_json(
                outbound,
                serde_json::json!({
                    "type": "agent:deliver:ack",
                    "agentId": agent_id,
                    "seq": seq,
                    "deliveryId": delivery_id,
                }),
            )
            .await?;
            tracing::debug!(agent_id = %agent_id, seq = seq, "acked delivery");

            if !agents.contains(&agent_id) {
                warn!(agent_id = %agent_id, "delivery for unknown agent — dropping prompt");
                return Ok(());
            }
            if prompt.is_empty() {
                tracing::debug!(agent_id = %agent_id, "delivery has empty prompt — nothing to spawn");
                return Ok(());
            }

            // Spawn the turn in the background so the read loop isn't blocked.
            // RustyCLI invocations can take seconds-to-minutes.
            let agents_clone = Arc::clone(agents);
            let outbound_clone = outbound.clone();
            let server_url_clone = server_url.to_string();
            let launch_id_clone = launch_id.clone();
            let agent_id_for_task = agent_id.clone();
            let delivery_clone = value.clone();
            tokio::spawn(async move {
                run_agent_turn(
                    &agents_clone,
                    &agent_id_for_task,
                    &delivery_clone,
                    &server_url_clone,
                    launch_id_clone.as_deref(),
                    &outbound_clone,
                )
                .await;
            });
        }

        "agent:workspace:list" => {
            let agent_id = value
                .get("agentId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let dir_path = value
                .get("dirPath")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let include_hidden = value
                .get("includeHidden")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);

            let workspace = agents
                .with(agent_id, |p| p.workspace.clone())
                .unwrap_or_else(|| {
                    paths::home_dir_for_profile(profile)
                        .map(|h| h.join("agents").join(agent_id))
                        .unwrap_or_default()
                });

            let files = list_workspace_files(&workspace, dir_path, include_hidden);
            let mut payload = serde_json::Map::new();
            payload.insert("type".to_string(), serde_json::json!("agent:workspace:file_tree"));
            payload.insert("agentId".to_string(), serde_json::json!(agent_id));
            payload.insert("files".to_string(), serde_json::json!(files));
            if !dir_path.is_empty() {
                payload.insert("dirPath".to_string(), serde_json::json!(dir_path));
            }
            payload.insert("includeHidden".to_string(), serde_json::json!(include_hidden));
            send_json(outbound, serde_json::Value::Object(payload)).await?;
        }

        "agent:workspace:read" => {
            let agent_id = value
                .get("agentId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let file_path = value
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let request_id = value.get("requestId").cloned();

            let workspace = agents
                .with(agent_id, |p| p.workspace.clone())
                .unwrap_or_else(|| {
                    paths::home_dir_for_profile(profile)
                        .map(|h| h.join("agents").join(agent_id))
                        .unwrap_or_default()
                });

            let result = read_workspace_file(&workspace, file_path);
            let mut payload = serde_json::Map::new();
            payload.insert("type".to_string(), serde_json::json!("agent:workspace:file_content"));
            payload.insert("agentId".to_string(), serde_json::json!(agent_id));
            if let Some(rid) = request_id {
                payload.insert("requestId".to_string(), rid);
            }
            match result {
                Ok(ReadResult::Text { content, size }) => {
                    payload.insert("content".to_string(), serde_json::json!(content));
                    payload.insert("binary".to_string(), serde_json::json!(false));
                    payload.insert("size".to_string(), serde_json::json!(size));
                    payload.insert("encoding".to_string(), serde_json::json!("utf-8"));
                }
                Ok(ReadResult::Binary { size }) => {
                    payload.insert("content".to_string(), serde_json::Value::Null);
                    payload.insert("binary".to_string(), serde_json::json!(true));
                    payload.insert("size".to_string(), serde_json::json!(size));
                }
                Err(_) => {
                    payload.insert("content".to_string(), serde_json::Value::Null);
                    payload.insert("binary".to_string(), serde_json::json!(false));
                    payload.insert("size".to_string(), serde_json::json!(0));
                }
            }
            send_json(outbound, serde_json::Value::Object(payload)).await?;
        }

        "agent:activity_probe" => {
            // Server is checking if the agent is still alive. Respond with an
            // online activity so the UI shows the agent as available and the
            // probe is acknowledged.
            let agent_id = value
                .get("agentId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let probe_id = value
                .get("probeId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if agent_id.is_empty() || probe_id.is_empty() {
                tracing::debug!("ignoring agent:activity_probe without agentId or probeId");
            } else if let Some((client_seq, launch_id)) = agents.with(agent_id, |p| {
                (p.next_activity_client_seq(), p.launch_id.as_ref().map(|s| serde_json::json!(s)))
            }) {
                let launch_ref = launch_id.as_ref();
                if let Err(err) = send_agent_activity(
                    outbound,
                    agent_id,
                    "online",
                    "online",
                    "Agent ready",
                    "idle",
                    launch_ref,
                    client_seq,
                    Some(probe_id),
                )
                .await
                {
                    warn!(error = %err, agent_id = %agent_id, "failed to respond to activity probe");
                }
            } else {
                tracing::debug!(agent_id = %agent_id, "ignoring agent:activity_probe for unknown agent");
            }
        }

        "agent:stop" => {
            let agent_id = value
                .get("agentId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let launch_id = value.get("launchId").cloned();
            info!(agent_id = %agent_id, "agent:stop received");
            state.remove_running_agent(&agent_id);
            match state.save() {
                Ok(()) => {
                    info!(agent_id = %agent_id, "removed persisted running agent");
                }
                Err(err) => {
                    warn!(error = %err, agent_id = %agent_id, "failed to persist agent stop");
                }
            }
            if agents.remove(&agent_id).is_some() {
                info!(agent_id = %agent_id, "removed agent from registry");
            }
            send_agent_status(outbound, &agent_id, "inactive", launch_id.as_ref()).await?;
        }

        "agent:reset-workspace" | "agent:inbox:purge" => {
            let agent_id = value
                .get("agentId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            info!(agent_id = agent_id, kind = kind, "agent maintenance message — no-op (no agent-side state to reset yet)");
        }

        // Server wants the agent's available skills. We have no agent
        // process, so report empty.
        "agent:skills:list" => {
            let agent_id = value
                .get("agentId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            send_json(
                outbound,
                serde_json::json!({
                    "type": "agent:skills:list_result",
                    "agentId": agent_id,
                    "global": [],
                    "workspace": [],
                }),
            )
            .await?;
        }

        "agent:message"
        | "agent:task"
        | "agent:reminder"
        | "agent:reset"
        | "agent:wake" => {
            // Forward to the agent manager. The current AgentManager has
            // placeholder responses; full dispatch lands in a follow-up.
            dispatch_to_agent(&value, state);
        }
        "task:assign" | "task:cancel" | "task:update" => {
            let task_id = value
                .get("task_id")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            info!(kind = kind, task_id = task_id, "task event received");
        }
        "reminder:fire" | "reminder:snooze" | "reminder:cancel" => {
            let reminder_id = value
                .get("reminder_id")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            info!(kind = kind, reminder_id = reminder_id, "reminder event received");
        }

        // ---- machine-level requests ----

        // Server wants the list of models a runtime can serve. We don't yet
        // know how to introspect RustyCLI's model list, so report
        // `unsupported` and let the server fall back to its defaults.
        "machine:runtime_models:detect" => {
            let request_id = value.get("requestId").cloned();
            let runtime = value
                .get("runtime")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            send_json(
                outbound,
                serde_json::json!({
                    "type": "machine:runtime_models:result",
                    "requestId": request_id,
                    "error": "unsupported",
                }),
            )
            .await?;
            tracing::debug!(runtime = runtime, "declined runtime_models:detect (not implemented)");
        }

        _ => {
            tracing::debug!(kind = kind, raw = %value, "unhandled server message");
        }
    }

    Ok(())
}

/// Install a running agent from the server's `agent:start` payload and announce
/// it as online. Also persists the payload so the agent can be restored on a
/// future daemon restart.
async fn start_agent(
    outbound: &mpsc::Sender<WsMessage>,
    agents: &Arc<AgentProcessRegistry>,
    state: &Arc<dyn StateMgr>,
    server_url: &str,
    api_key: &str,
    profile: &str,
    value: &serde_json::Value,
) -> Result<()> {
    let agent_id = value
        .get("agentId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if agent_id.is_empty() {
        warn!("agent:start payload missing agentId; skipping");
        return Ok(());
    }
    let launch_id = value.get("launchId").cloned();
    let config = value.get("config").cloned().unwrap_or(serde_json::json!({}));
    let runtime = config
        .get("runtime")
        .and_then(|v| v.as_str())
        .unwrap_or("builtin");
    let model = config
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let provider_config = ProviderConfig::from_config(&config);
    let provider_id = provider_config.provider_id.as_deref().unwrap_or("");
    let provider_kind = provider_config.kind.as_deref().unwrap_or("");
    let base_url = provider_config.base_url.as_deref().unwrap_or("");
    let api_key_present = provider_config
        .api_key
        .as_deref()
        .is_some_and(|s| !s.is_empty());

    if provider_id.is_empty() && !api_key_present {
        tracing::debug!(
            agent_id = %agent_id,
            config = %config,
            "agent:start config has no provider details; will rely on env fallback",
        );
    }

    info!(
        agent_id = %agent_id,
        runtime = runtime,
        model = model,
        provider_id = provider_id,
        provider_kind = provider_kind,
        base_url_present = !base_url.is_empty(),
        api_key_present = api_key_present,
        "agent:start received",
    );

    let session_id_from_config = config
        .get("sessionId")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let home = match paths::home_dir_for_profile(profile) {
        Ok(h) => h,
        Err(err) => {
            warn!(error = %err, "could not resolve daemon home; agent start fails");
            send_agent_status(outbound, &agent_id, "inactive", launch_id.as_ref()).await?;
            return Ok(());
        }
    };

    match AgentProcess::from_start(&agent_id, &config, launch_id.as_ref(), &home) {
        Ok(mut process) => {
            if let Some(sid) = session_id_from_config {
                process.session_id = Some(sid);
            }
            let session_id = process
                .session_id
                .clone()
                .unwrap_or_else(|| format!("sess_{}", uuid::Uuid::new_v4()));
            process.session_id = Some(session_id.clone());

            match mint_runner_credential(server_url, api_key, &agent_id, &process.runtime).await {
                Ok(cred) => {
                    info!(
                        agent_id = %agent_id,
                        credential_id = ?cred.credential_id,
                        "minted raft runner credential",
                    );
                    process.agent_credential_key = Some(cred.api_key);
                    process.agent_credential_id = cred.credential_id;
                }
                Err(err) => {
                    warn!(
                        error = %err,
                        agent_id = %agent_id,
                        "failed to mint raft runner credential; \
                         agent will run but cannot post chat replies",
                    );
                }
            }

            let idle_client_seq = process.next_activity_client_seq();
            agents.install(process);

            state.set_running_agent(&agent_id, value.clone());
            match state.save() {
                Ok(()) => {
                    info!(agent_id = %agent_id, "persisted running agent");
                }
                Err(err) => {
                    warn!(error = %err, agent_id = %agent_id, "failed to persist running agent");
                }
            }

            let mut session_payload = serde_json::json!({
                "type": "agent:session",
                "agentId": agent_id,
                "sessionId": session_id,
            });
            if let Some(lid) = launch_id.as_ref() {
                session_payload["launchId"] = lid.clone();
            }
            send_json(outbound, session_payload).await?;
            send_agent_status(outbound, &agent_id, "active", launch_id.as_ref()).await?;
            send_agent_activity(
                outbound,
                &agent_id,
                "online",
                "online",
                "Agent ready",
                "idle",
                launch_id.as_ref(),
                idle_client_seq,
                None,
            )
            .await?;
        }
        Err(err) => {
            warn!(error = %err, agent_id = %agent_id, "failed to install agent process");
            send_agent_status(outbound, &agent_id, "inactive", launch_id.as_ref()).await?;
            send_agent_activity(
                outbound,
                &agent_id,
                "offline",
                "offline",
                &format!("Failed to start: {err}"),
                "runtime_error",
                launch_id.as_ref(),
                0,
                None,
            )
            .await?;
        }
    }
    Ok(())
}

/// Forward an agent-bound message into the existing agent manager.
///
/// For now we extract a debug log; full agent routing (looking up the
/// `AgentManager` for the configured runtime, claiming the channel, etc.)
/// lands once the runtime driver plumbing is finished.
fn dispatch_to_agent(value: &serde_json::Value, _state: &Arc<dyn StateMgr>) {
    let agent_id = value
        .get("agent_id")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    info!(agent_id = agent_id, "agent dispatch (scaffold)");
}

/// Result of reading a workspace file for `agent:workspace:read`.
enum ReadResult {
    Text { content: String, size: u64 },
    Binary { size: u64 },
}

const TEXT_MAX_BYTES: u64 = 1_048_576; // 1 MB

/// Read a file from an agent workspace for the `agent:workspace:read` request.
/// Returns text for known text extensions, or a binary marker for other files.
/// Errors are returned for access outside the workspace, directories, or I/O
/// failures.
fn read_workspace_file(
    workspace: &std::path::Path,
    file_path: &str,
) -> Result<ReadResult> {
    if file_path.is_empty() {
        anyhow::bail!("empty file path");
    }

    let full_path = workspace.join(file_path).canonicalize()?;
    let workspace_canon = workspace.canonicalize().unwrap_or_else(|_| workspace.to_path_buf());
    let prefix = format!("{}{sep}", workspace_canon.display(), sep = std::path::MAIN_SEPARATOR);
    if !full_path.starts_with(&prefix) && full_path != workspace_canon {
        anyhow::bail!("access denied: path escapes workspace");
    }

    let metadata = std::fs::metadata(&full_path)?;
    if metadata.is_dir() {
        anyhow::bail!("cannot read a directory");
    }

    let extension = full_path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default();
    let is_text = extension.is_empty()
        || matches!(
            extension.as_str(),
            "md" | "txt" | "json" | "js" | "ts" | "jsx" | "tsx" | "yaml" | "yml"
                | "toml" | "log" | "csv" | "xml" | "html" | "css" | "sh" | "py"
                | "rs"
        );

    if is_text {
        if metadata.len() > TEXT_MAX_BYTES {
            anyhow::bail!("text file too large");
        }
        let content = std::fs::read_to_string(&full_path)?;
        Ok(ReadResult::Text {
            content,
            size: metadata.len(),
        })
    } else {
        Ok(ReadResult::Binary {
            size: metadata.len(),
        })
    }
}

/// List the files/directories inside an agent workspace for the
/// `agent:workspace:list` request. Returns entries relative to the agent
/// workspace root. Skips `node_modules`, filters hidden entries based on
/// `include_hidden`, and refuses paths that escape the workspace.
fn list_workspace_files(
    workspace: &std::path::Path,
    dir_path: &str,
    include_hidden: bool,
) -> Vec<serde_json::Value> {
    let target_dir = if dir_path.is_empty() {
        workspace.to_path_buf()
    } else {
        let resolved = workspace.join(dir_path).canonicalize().unwrap_or_default();
        let workspace_canon = workspace.canonicalize().unwrap_or_else(|_| workspace.to_path_buf());
        let prefix = format!("{}{sep}", workspace_canon.display(), sep = std::path::MAIN_SEPARATOR);
        if !resolved.starts_with(&prefix) && resolved != workspace_canon {
            return Vec::new();
        }
        resolved
    };

    let mut entries = match std::fs::read_dir(&target_dir) {
        Ok(iter) => iter.filter_map(std::result::Result::ok).collect::<Vec<_>>(),
        Err(_) => return Vec::new(),
    };

    entries.sort_by(|a, b| {
        let a_dir = a.file_type().is_ok_and(|t| t.is_dir());
        let b_dir = b.file_type().is_ok_and(|t| t.is_dir());
        match (a_dir, b_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.file_name().cmp(&b.file_name()),
        }
    });

    let mut out = Vec::new();
    for entry in entries {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let is_hidden = name_str.starts_with('.');
        if name_str == "node_modules" {
            continue;
        }
        if is_hidden && !include_hidden {
            continue;
        }

        let full_path = entry.path();
        let relative = full_path
            .strip_prefix(workspace)
            .map_or_else(|_| full_path.clone(), std::path::Path::to_path_buf);
        let is_directory = entry.file_type().is_ok_and(|t| t.is_dir());
        let size = if is_directory {
            0
        } else {
            std::fs::metadata(&full_path)
                .map_or(0, |m| m.len())
        };
        let modified_at = std::fs::metadata(&full_path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| chrono::DateTime::from_timestamp(
                i64::try_from(t.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs()).ok()?,
                0,
            ))
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default();

        out.push(serde_json::json!({
            "name": name_str.to_string(),
            "path": relative.display().to_string(),
            "isDirectory": is_directory,
            "size": size,
            "modifiedAt": modified_at,
            "isHidden": is_hidden,
        }));
    }
    out
}

/// Send an `agent:status` frame (`active` / `inactive` / `error`).
async fn send_agent_status(
    outbound: &mpsc::Sender<WsMessage>,
    agent_id: &str,
    status: &str,
    launch_id: Option<&serde_json::Value>,
) -> Result<()> {
    let mut payload = serde_json::Map::new();
    payload.insert("type".to_string(), serde_json::json!("agent:status"));
    payload.insert("agentId".to_string(), serde_json::json!(agent_id));
    payload.insert("status".to_string(), serde_json::json!(status));
    if let Some(lid) = launch_id {
        payload.insert("launchId".to_string(), lid.clone());
    }
    send_json(outbound, serde_json::Value::Object(payload)).await
}

/// Send an `agent:activity` frame with a single status entry. Mirrors
/// `AgentProcessManager.broadcastActivity` in npm but stripped of the
/// trajectory / heartbeat machinery — we just need enough to make the UI
/// show "working" / "idle" honestly.
#[allow(clippy::too_many_arguments)]
async fn send_agent_activity(
    outbound: &mpsc::Sender<WsMessage>,
    agent_id: &str,
    activity: &str,
    activity_kind: &str,
    detail: &str,
    detail_kind: &str,
    launch_id: Option<&serde_json::Value>,
    client_seq: u64,
    probe_id: Option<&str>,
) -> Result<()> {
    let launch_id_str = launch_id.and_then(|v| v.as_str()).unwrap_or("legacy");
    let producer_fact_id = format!("daemon_activity:{agent_id}:{launch_id_str}:{client_seq}");
    let mut payload = serde_json::Map::new();
    payload.insert("type".to_string(), serde_json::json!("agent:activity"));
    payload.insert("agentId".to_string(), serde_json::json!(agent_id));
    payload.insert("activity".to_string(), serde_json::json!(activity));
    payload.insert("activityKind".to_string(), serde_json::json!(activity_kind));
    payload.insert("detail".to_string(), serde_json::json!(detail));
    payload.insert("detailKind".to_string(), serde_json::json!(detail_kind));
    payload.insert(
        "entries".to_string(),
        serde_json::json!([{
            "kind": "status",
            "activity": activity,
            "activityKind": activity_kind,
            "detail": detail,
            "detailKind": detail_kind,
        }]),
    );
    payload.insert("clientSeq".to_string(), serde_json::json!(client_seq));
    payload.insert(
        "producerFactId".to_string(),
        serde_json::json!(producer_fact_id),
    );
    if let Some(lid) = launch_id {
        payload.insert("launchId".to_string(), lid.clone());
    }
    if let Some(pid) = probe_id {
        payload.insert("probeId".to_string(), serde_json::json!(pid));
    }
    send_json(outbound, serde_json::Value::Object(payload)).await
}

/// Decide whether an agent should respond to an `agent:deliver` payload and,
/// if so, build a prompt for RustyCLI that mirrors the npm daemon's stdin
/// format.
///
/// Returns `None` when the delivery should be ignored (self-echo, bot chatter
/// in a public channel, etc.). The reason is logged at debug level so
/// operators can audit the decision.
fn prepare_delivery_prompt(
    agent_id: &str,
    name: &str,
    description: &str,
    delivery: &serde_json::Value,
) -> Option<String> {
    let msg = delivery.get("message")?;
    let content = msg
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if content.is_empty() {
        tracing::debug!(agent_id = %agent_id, "skipping delivery: empty message content");
        return None;
    }

    // Sender id can be in a couple of field names depending on server version.
    let sender_id = msg
        .get("sender_id")
        .or_else(|| msg.get("senderId"))
        .and_then(|v| v.as_str());
    if let Some(sid) = sender_id {
        if sid == agent_id {
            tracing::debug!(agent_id = %agent_id, "skipping delivery: self-echo");
            return None;
        }
    }

    let channel_kind = msg
        .get("channel_kind")
        .or_else(|| msg.get("channel_type"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let is_dm = channel_kind == "dm";

    let sender_type = msg
        .get("sender_type")
        .or_else(|| msg.get("senderType"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if sender_type == "bot" && !is_dm {
        tracing::debug!(
            agent_id = %agent_id,
            sender_id = ?sender_id,
            "skipping delivery: bot message in public channel"
        );
        return None;
    }

    let target = crate::daemon::agent::derive_target(delivery).unwrap_or_default();
    let msg_id = msg
        .get("message_id")
        .and_then(|v| v.as_str())
        .unwrap_or("-");
    let time = msg
        .get("timestamp")
        .and_then(serde_json::Value::as_i64)
        .and_then(|ts| chrono::DateTime::from_timestamp_millis(ts).map(|dt| dt.to_rfc3339()))
        .unwrap_or_else(|| "-".to_string());
    let sender_name = msg
        .get("sender_name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // RustyCLI reads MEMORY.md and notes/ from the workspace automatically,
    // so the daemon no longer injects them into the prompt.
    let context_header = format!("You are {name}. {description}\n\n");

    let instruction = if is_dm {
        format!(
            "## Required response behavior\n\n\
             - This is a direct message to you. You MUST respond.\n\
             - Do not output `{NO_REPLY_MARKER}` for any direct message unless it is clearly spam or completely unrelated to your role.\n\
             - This instruction overrides any other guidance about staying silent or minimizing conversation.\n\
             - Respond helpfully and concisely. Complete all your work before stopping."
        )
    } else {
        format!(
            "## Required response behavior\n\n\
             - You are in a team channel. If this message is addressed to you, the team, the channel, or falls within your role, respond helpfully and concisely.\n\
             - Only output `{NO_REPLY_MARKER}` for messages that are clearly irrelevant, private side-conversations, or do not require your input.\n\
             - This instruction overrides any other guidance about staying silent or minimizing conversation.\n\
             - Complete all your work before stopping."
        )
    };

    Some(format!(
        "{context_header}New message received:\n\n[target={target} msg={msg_id} time={time} type={sender_type}] @{sender_name}: {content}\n\n{instruction}"
    ))
}

/// Run one RustyCLI turn for an inbound delivery.
///
/// - Broadcasts `agent:activity: working` before spawn.
/// - Spawns rusty via [`run_one_turn`] with the agent's prompt.
/// - On success: POSTs the response to raft as the agent via
///   `/internal/agent-api/send`, then broadcasts `agent:activity: idle`.
/// - On failure: broadcasts `agent:activity: error` with the failure detail.
///
/// Posting the reply requires a previously-minted `sk_agent_…` credential
/// (see `mint_runner_credential`). If we don't have one we still log the
/// response so the operator can verify the agent is thinking.
async fn run_agent_turn(
    agents: &Arc<AgentProcessRegistry>,
    agent_id: &str,
    delivery: &serde_json::Value,
    server_url: &str,
    launch_id: Option<&str>,
    outbound: &mpsc::Sender<WsMessage>,
) {
    let launch_value = launch_id.map(|s| serde_json::json!(s));
    let launch_ref = launch_value.as_ref();

    // Snapshot the process state so we don't hold the dashmap guard across
    // the (potentially long) rusty invocation.
    let snapshot = agents.with(agent_id, std::clone::Clone::clone);
    let Some(process) = snapshot else {
        warn!(agent_id = agent_id, "agent vanished before turn started");
        return;
    };

    // Decide whether to respond and build a prompt that mirrors the npm
    // daemon's stdin format (target, sender, context) plus an explicit
    // instruction to only reply when addressed.
    let Some(prompt) = prepare_delivery_prompt(agent_id, &process.name, &process.description, delivery) else {
        return;
    };

    // Serialize turns for this agent. RustyCLI keeps a SQLite task registry
    // in the workspace; concurrent invocations for the same agent collide with
    // "database is locked" errors.
    let _turn_guard = process.turn_lock.lock().await;

    if let Err(err) = send_agent_activity(outbound, agent_id, "working", "working", "Thinking…", "message_received", launch_ref, process.next_activity_client_seq(), None).await {
        warn!(error = %err, "failed to broadcast working activity");
    }

    let result = run_one_turn(&process, &prompt).await;

    match result {
        Ok(response) => {
            let trimmed = response.trim();
            info!(agent_id = agent_id, response_len = trimmed.len(), "rusty turn completed");
            tracing::info!(target: "raft_daemon::agent::response", agent_id = agent_id, response = %trimmed);

            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case(NO_REPLY_MARKER) {
                tracing::debug!(agent_id = %agent_id, "rusty chose not to reply; skipping raft post");
            } else {
                // POST the response to raft so it shows up in chat.
                post_agent_reply(&process, delivery, server_url, trimmed).await;
            }

            if let Err(err) =
                send_agent_activity(outbound, agent_id, "online", "online", "Idle", "idle", launch_ref, process.next_activity_client_seq(), None).await
            {
                warn!(error = %err, "failed to broadcast idle activity");
            }
        }
        Err(err) => {
            warn!(error = %err, agent_id = agent_id, "rusty turn failed");
            let _ = send_agent_activity(
                outbound,
                agent_id,
                "error",
                "error",
                &format!("RustyCLI error: {err}"),
                "runtime_error",
                launch_ref,
                process.next_activity_client_seq(),
                None,
            )
            .await;
        }
    }
}

/// POST rusty's response to raft as the agent.
///
/// Skips silently if:
/// - The agent has no minted `sk_agent_…` credential (mint failed at start).
/// - The delivery message doesn't contain enough info to derive a target.
async fn post_agent_reply(
    process: &AgentProcess,
    delivery: &serde_json::Value,
    server_url: &str,
    response: &str,
) {
    let Some(agent_key) = process.agent_credential_key.as_ref() else {
        tracing::debug!(
            agent_id = %process.agent_id,
            "skipping raft reply post: no agent credential was minted at start",
        );
        return;
    };

    let Some(target) = crate::daemon::agent::derive_target(delivery) else {
        warn!(
            agent_id = %process.agent_id,
            "could not derive reply target from delivery; reply will not be posted. raw_delivery = {delivery}",
        );
        return;
    };

    let seen_up_to_seq = delivery
        .get("seq")
        .and_then(serde_json::Value::as_i64)
        .or_else(|| {
            delivery
                .pointer("/message/seq")
                .and_then(serde_json::Value::as_i64)
        });

    let body = SendBody {
        target,
        content: response.to_string(),
        seen_up_to_seq,
    };

    match send_agent_message(server_url, agent_key, &body).await {
        Ok(resp) => {
            info!(
                agent_id = %process.agent_id,
                message_id = ?resp.message_id,
                state = ?resp.state,
                "posted agent reply to raft",
            );
        }
        Err(err) => {
            warn!(
                error = %err,
                agent_id = %process.agent_id,
                "failed to post agent reply to raft",
            );
        }
    }
}

/// Serialise a JSON value and queue it for send.
async fn send_json(outbound: &mpsc::Sender<WsMessage>, value: serde_json::Value) -> Result<()> {
    let text = serde_json::to_string(&value)?;
    outbound
        .send(WsMessage::Text(text))
        .await
        .map_err(|e| anyhow::anyhow!("queueing outbound message: {e}"))
}

/// Build and queue the daemon `ready` frame.
///
/// The raft server uses this to populate the "detected runtimes" UI and to
/// decide whether this daemon is eligible to receive agent work. Skipping it
/// leaves the server showing "no detected runtime" and unable to start
/// agents on this machine.
///
/// Mirrors `DaemonCore.handleConnect()` in the npm daemon at
/// `chunk-URPIDKXK.js:22330`.
/// Send the daemon `ready` frame, advertising installed runtimes and the set
/// of agents that were already running before this connection.
async fn send_ready_frame(
    outbound: &mpsc::Sender<WsMessage>,
    running_agents: &[String],
) -> Result<()> {
    let runtimes = detect_runtimes();
    info!(runtimes = ?runtimes, "sending ready frame");

    let mut ready = serde_json::json!({
        "type": "ready",
        "capabilities": [
            "agent:start",
            "agent:stop",
            "agent:deliver",
            "workspace:files",
        ],
        "runtimes": runtimes,
        "runningAgents": running_agents,
        "hostname": hostname_str(),
        "os": format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        "daemonVersion": env!("CARGO_PKG_VERSION"),
    });
    if let Some(version) = computer_version() {
        ready["computerVersion"] = serde_json::json!(version);
    }
    send_json(outbound, ready).await
}

/// Known external runtimes and the binary name they are installed as.
///
/// The `id` matches what the raft server expects in the `runtimes` array of
/// the daemon `ready` frame (see npm `RUNTIMES` at `chunk-URPIDKXK.js:3322`).
/// The `binary` is what we look up on `PATH` to decide whether the runtime is
/// installed locally.
const KNOWN_RUNTIMES: &[(&str, &str)] = &[
    ("claude", "claude"),
    ("codex", "codex"),
    ("antigravity", "agy"),
    ("kimi", "kimi"),
    ("copilot", "copilot"),
    ("cursor", "cursor-agent"),
    ("gemini", "gemini"),
    ("opencode", "opencode"),
    ("pi", "pi"),
];

/// Binary names RustyCLI might be installed under, in preference order.
///
/// The homebrew/curl installer ships `rusty`; some older docs mention
/// `rustycli` and `rusty-cli`. Whatever is found first wins.
const RUSTY_BINARY_CANDIDATES: &[&str] = &["rusty", "rustycli", "rusty-cli"];

/// Environment variable that overrides RustyCLI detection by pointing
/// directly at the binary.
pub const RAFT_RUSTY_BINARY_ENV: &str = "RAFT_RUSTY_BINARY";

/// Detect installed runtimes by scanning `PATH` for each known binary.
///
/// Reports `"builtin"` only when RustyCLI is actually installed. The migration
/// plan (`transient-hatching-boot.md`) replaces the npm in-process pi runtime
/// with RustyCLI, so unlike npm we cannot truthfully advertise `builtin`
/// without a backing binary — claiming otherwise would let the server schedule
/// `agent:start` calls we'd then have to fail.
fn detect_runtimes() -> Vec<String> {
    let mut found: Vec<String> = Vec::new();

    // `builtin` is gated on RustyCLI actually being available.
    if resolve_rustycli_path().is_some() {
        found.push("builtin".to_string());
    } else {
        info!(
            env = RAFT_RUSTY_BINARY_ENV,
            candidates = ?RUSTY_BINARY_CANDIDATES,
            "RustyCLI binary not found; not advertising `builtin` runtime \
             (agents cannot be started until it is installed)"
        );
    }

    for (id, binary) in KNOWN_RUNTIMES {
        if which::which(binary).is_ok() {
            found.push((*id).to_string());
        }
    }
    found
}

/// Resolve the RustyCLI binary path.
///
/// Order:
/// 1. `$RAFT_RUSTY_BINARY` env var (must point at an existing file).
/// 2. `which::which(name)` for each name in [`RUSTY_BINARY_CANDIDATES`].
///
/// Returns `None` if no candidate is found, in which case `builtin` is not
/// advertised to the server.
pub(crate) fn resolve_rustycli_path() -> Option<std::path::PathBuf> {
    if let Some(path_str) = std::env::var_os(RAFT_RUSTY_BINARY_ENV) {
        let path = std::path::PathBuf::from(path_str);
        if path.is_file() {
            return Some(path);
        }
        warn!(
            path = %path.display(),
            env = RAFT_RUSTY_BINARY_ENV,
            "configured RustyCLI path does not point at a file; falling back to PATH lookup",
        );
    }
    for name in RUSTY_BINARY_CANDIDATES {
        if let Ok(path) = which::which(name) {
            return Some(path);
        }
    }
    None
}

/// Best-effort machine hostname for the `ready` frame.
fn hostname_str() -> String {
    // Prefer `hostname::get` (uses `gethostname(3)`); fall back to
    // `whoami::fallible::hostname` (platform-specific host APIs) and finally
    // to an empty string if both fail.
    hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .or_else(|| whoami::fallible::hostname().ok())
        .unwrap_or_default()
}

/// Optional computer version advertised in the `ready` frame.
///
/// Mirrors the npm daemon's `RAFT_COMPUTER_VERSION` env var support. The
/// server may use this to gate feature compatibility.
fn computer_version() -> Option<String> {
    std::env::var("RAFT_COMPUTER_VERSION")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

// ============================================================
// stop / status / restart
// ============================================================

/// Stop the running daemon, if any.
///
/// Sends `SIGTERM`, waits up to [`SHUTDOWN_GRACE`] for the process to exit,
/// and falls back to `SIGKILL` if it's still alive.
///
/// # Errors
///
/// Returns an error if no daemon is running or it fails to terminate.
pub async fn stop(profile: &str) -> Result<()> {
    let pid_path = paths::pid_file_for_profile(profile)?;
    let pid = pidfile::read_pid(&pid_path)?
        .ok_or_else(|| anyhow::anyhow!("daemon not running"))?;

    if !pidfile::is_alive(pid) {
        pidfile::remove_pid(&pid_path);
        anyhow::bail!("daemon not running (stale PID file removed)");
    }

    info!(pid = pid, "sending SIGTERM");
    if !pidfile::send_signal(pid, libc::SIGTERM) {
        anyhow::bail!("failed to signal process {pid}");
    }

    let deadline = tokio::time::Instant::now() + SHUTDOWN_GRACE;
    loop {
        if !pidfile::is_alive(pid) {
            pidfile::remove_pid(&pid_path);
            println!("daemon stopped (pid={pid})");
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    warn!(pid = pid, "daemon did not exit on SIGTERM; escalating to SIGKILL");
    let _ = pidfile::send_signal(pid, libc::SIGKILL);
    for _ in 0..50 {
        if !pidfile::is_alive(pid) {
            pidfile::remove_pid(&pid_path);
            println!("daemon killed (pid={pid})");
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    anyhow::bail!("daemon did not terminate after SIGKILL");
}

/// Check daemon status.
///
/// # Errors
///
/// Returns an error if the PID file cannot be read.
pub fn status(profile: &str) -> Result<StatusReport> {
    let pid_path = paths::pid_file_for_profile(profile)?;
    match pidfile::read_pid(&pid_path)? {
        None => Ok(StatusReport::NotConfigured),
        Some(pid) if pidfile::is_alive(pid) => Ok(StatusReport::Running(pid)),
        Some(pid) => Ok(StatusReport::Stale(pid)),
    }
}

/// Restart the daemon.
///
/// Stops any running instance (ignoring "not running" errors) and then
/// starts a fresh one with the given options.
///
/// # Errors
///
/// Propagates errors from [`start`]; stop failures other than
/// "not running" are also propagated.
pub async fn restart(opts: DaemonOptions) -> Result<StartOutcome> {
    if let Err(err) = stop(&opts.profile).await {
        let msg = err.to_string();
        if !msg.contains("not running") {
            return Err(err);
        }
    }
    start(opts).await
}

// ============================================================
// signals + tracing
// ============================================================

/// Spawn a background task that listens for `SIGINT` and `SIGTERM` and flips
/// the shutdown flag.
fn spawn_signal_handler(shutdown_tx: watch::Sender<bool>) {
    tokio::spawn(async move {
        let sigint = match signal(SignalKind::interrupt()) {
            Ok(s) => s,
            Err(err) => {
                error!(error = %err, "failed to install SIGINT handler");
                return;
            }
        };
        let sigterm = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(err) => {
                error!(error = %err, "failed to install SIGTERM handler");
                return;
            }
        };
        let mut sigint = sigint;
        let mut sigterm = sigterm;

        tokio::select! {
            _ = sigint.recv() => info!(signal = "SIGINT", "shutdown requested"),
            _ = sigterm.recv() => info!(signal = "SIGTERM", "shutdown requested"),
        }

        let _ = shutdown_tx.send(true);
    });
}

/// Initialise tracing.
///
/// When stdout is a TTY (i.e. the user is running in the foreground), logs
/// go to stdout with a friendly format. Otherwise they go to the daemon log
/// file. Uses `try_init` so the first initialiser wins; this is safe because
/// only one tracing setup runs per process.
fn init_tracing(profile: &str) -> Result<()> {
    use tracing_subscriber::fmt;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,raft_daemon=info"));

    let interactive = std::io::stdout().is_terminal();
    if interactive {
        let _ = fmt()
            .with_env_filter(filter)
            .with_target(false)
            .try_init();
        return Ok(());
    }

    let path = paths::log_file_for_profile(profile)?;
    #[cfg(unix)]
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&path)
    };
    #[cfg(not(unix))]
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path);
    let file = file.with_context(|| format!("opening log file {}", path.display()))?;

    let _ = fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(file)
        .try_init();
    Ok(())
}

// ============================================================
// misc helpers
// ============================================================

/// Strip the API key from a WebSocket URL for log output.
fn redact_url(url: &str) -> String {
    if let Some(idx) = url.find("key=") {
        format!("{}key=***", &url[..idx])
    } else {
        url.to_string()
    }
}

/// Wrap a `connect_async` error with a hint when the underlying cause is a
/// non-upgrade HTTP response, which is what you get when `server_url` points
/// at a web UI (e.g. `app.raft.build`) instead of the API (`api.raft.build`).
///
/// Accepts `anyhow::Error` (what [`connect_async_with_opts`] returns) and
/// walks the source chain to find the underlying `tungstenite::Error`.
fn friendly_connect_error(err: &anyhow::Error) -> String {
    let raw = err.to_string();
    let http_status = err
        .chain()
        .find_map(|cause| cause.downcast_ref::<tokio_tungstenite::tungstenite::Error>())
        .and_then(|e| match e {
            tokio_tungstenite::tungstenite::Error::Http(resp) => Some(resp.status().as_u16()),
            _ => None,
        });

    if let Some(code) = http_status {
        if (200..300).contains(&code) {
            return format!(
                "HTTP {code} — server responded without WebSocket upgrade; \
                 check that server_url is the raft API \
                 (e.g. https://api.raft.build), not a web UI"
            );
        }
        if (400..500).contains(&code) {
            return format!(
                "HTTP {code} — client error; check that --api-key is valid \
                 and server_url is correct"
            );
        }
        if code >= 500 {
            return format!("HTTP {code} — server-side error; will retry");
        }
    }
    raw
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_url_normalises_https_to_wss() {
        let opts = DaemonOptions {
            server_url: "https://api.raft.build".into(),
            api_key: "sk_test".into(),
            profile: "default".into(),
            foreground: false,
        };
        assert_eq!(
            opts.ws_url().unwrap(),
            "wss://api.raft.build/daemon/connect?key=sk_test"
        );
    }

    #[test]
    fn ws_url_normalises_http_to_ws() {
        let opts = DaemonOptions {
            server_url: "http://localhost:8080/".into(),
            api_key: "sk_test".into(),
            profile: "default".into(),
            foreground: false,
        };
        assert_eq!(
            opts.ws_url().unwrap(),
            "ws://localhost:8080/daemon/connect?key=sk_test"
        );
    }

    #[test]
    fn ws_url_passes_through_ws_scheme() {
        let opts = DaemonOptions {
            server_url: "wss://api.raft.build".into(),
            api_key: "sk_test".into(),
            profile: "default".into(),
            foreground: false,
        };
        assert_eq!(
            opts.ws_url().unwrap(),
            "wss://api.raft.build/daemon/connect?key=sk_test"
        );
    }

    #[test]
    fn ws_url_defaults_bare_host_to_wss() {
        let opts = DaemonOptions {
            server_url: "api.raft.build".into(),
            api_key: "sk_test".into(),
            profile: "default".into(),
            foreground: false,
        };
        assert_eq!(
            opts.ws_url().unwrap(),
            "wss://api.raft.build/daemon/connect?key=sk_test"
        );
    }

    #[test]
    fn ws_url_rejects_unknown_scheme() {
        let opts = DaemonOptions {
            server_url: "ftp://example.com".into(),
            api_key: "k".into(),
            profile: "default".into(),
            foreground: false,
        };
        assert!(opts.ws_url().is_err());
    }

    #[test]
    fn redact_url_hides_api_key() {
        let url = "wss://api.raft.build/daemon/connect?key=sk_secret_123";
        assert_eq!(
            redact_url(url),
            "wss://api.raft.build/daemon/connect?key=***"
        );
    }

    #[test]
    fn redact_url_passes_through_when_no_key() {
        let url = "wss://api.raft.build/health";
        assert_eq!(redact_url(url), url);
    }

    #[test]
    fn friendly_error_hints_on_2xx_non_upgrade() {
        let err: anyhow::Error = tokio_tungstenite::tungstenite::Error::Http(
            tokio_tungstenite::tungstenite::http::Response::builder()
                .status(200)
                .body(Some(Vec::new()))
                .unwrap(),
        )
        .into();
        let msg = friendly_connect_error(&err);
        assert!(
            msg.contains("without WebSocket upgrade"),
            "got: {msg}"
        );
        assert!(msg.contains("api.raft.build"));
    }

    #[test]
    fn friendly_error_hints_on_4xx() {
        let err: anyhow::Error = tokio_tungstenite::tungstenite::Error::Http(
            tokio_tungstenite::tungstenite::http::Response::builder()
                .status(401)
                .body(Some(Vec::new()))
                .unwrap(),
        )
        .into();
        let msg = friendly_connect_error(&err);
        assert!(msg.contains("client error"), "got: {msg}");
    }

    #[test]
    fn friendly_error_passes_through_non_http() {
        let err: anyhow::Error = tokio_tungstenite::tungstenite::Error::Io(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "refused",
        ))
        .into();
        let msg = friendly_connect_error(&err);
        assert!(msg.contains("refused"), "got: {msg}");
    }

    #[test]
    fn detect_runtimes_includes_builtin_iff_rustycli_present() {
        let runtimes = detect_runtimes();
        let has_builtin = runtimes.iter().any(|r| r == "builtin");
        let rusty_resolved = resolve_rustycli_path().is_some();
        assert_eq!(
            has_builtin,
            rusty_resolved,
            "`builtin` should be advertised iff RustyCLI is installed; \
             got builtin={has_builtin}, rusty_resolved={rusty_resolved}",
        );
    }

    #[test]
    fn detect_runtimes_only_contains_known_ids() {
        let known: std::collections::HashSet<&str> = std::collections::HashSet::from([
            "builtin",
            "claude",
            "codex",
            "antigravity",
            "kimi",
            "copilot",
            "cursor",
            "gemini",
            "opencode",
            "pi",
        ]);
        let runtimes = detect_runtimes();
        for r in &runtimes {
            assert!(known.contains(r.as_str()), "unknown runtime reported: {r}");
        }
    }

    #[test]
    fn resolve_rustycli_finds_binary_on_path() {
        // `rusty` is installed at /opt/homebrew/bin/rusty on this machine.
        // If that ever changes, this test should be updated rather than
        // silently deleted.
        let resolved = resolve_rustycli_path();
        assert!(
            resolved.is_some(),
            "expected RustyCLI to be discovered via PATH or {RAFT_RUSTY_BINARY_ENV}"
        );
    }

    #[test]
    fn hostname_str_does_not_panic() {
        let _ = hostname_str();
    }

    #[test]
    fn status_report_displays() {
        assert_eq!(StatusReport::Running(123).to_string(), "running (pid=123)");
        assert_eq!(StatusReport::NotConfigured.to_string(), "not running");
        assert_eq!(
            StatusReport::Stale(456).to_string(),
            "not running (stale pid file for pid=456)"
        );
    }

    #[test]
    fn prepare_delivery_prompt_skips_self_echo() {
        let agent_id = "ag_123";
        let delivery = serde_json::json!({
            "message": {
                "content": "hello",
                "sender_id": agent_id,
                "sender_name": "agent",
                "channel_type": "channel",
                "channel_name": "general",
            }
        });
        assert!(prepare_delivery_prompt(agent_id, "Agent", "A test agent", &delivery).is_none());
    }

    #[test]
    fn prepare_delivery_prompt_skips_bot_in_public_channel() {
        let delivery = serde_json::json!({
            "message": {
                "content": "hello",
                "sender_id": "ag_other",
                "sender_type": "bot",
                "sender_name": "other-bot",
                "channel_type": "channel",
                "channel_name": "general",
            }
        });
        assert!(prepare_delivery_prompt("ag_123", "Agent", "A test agent", &delivery).is_none());
    }

    #[test]
    fn prepare_delivery_prompt_allows_bot_in_dm() {
        let delivery = serde_json::json!({
            "message": {
                "content": "hello",
                "sender_id": "ag_other",
                "sender_type": "bot",
                "sender_name": "other-bot",
                "channel_type": "dm",
                "sender_name": "other-bot",
            }
        });
        let prompt = prepare_delivery_prompt("ag_123", "Agent", "A test agent", &delivery).unwrap();
        assert!(prompt.contains("hello"));
        assert!(prompt.contains("New message received:"));
    }

    #[test]
    fn prepare_delivery_prompt_allows_human_in_channel() {
        let delivery = serde_json::json!({
            "message": {
                "content": "can you help?",
                "sender_id": "user_42",
                "sender_type": "human",
                "sender_name": "alice",
                "channel_type": "channel",
                "channel_name": "general",
            }
        });
        let prompt = prepare_delivery_prompt("ag_123", "Agent", "A test agent", &delivery).unwrap();
        assert!(prompt.contains("can you help?"));
        assert!(prompt.contains("@alice"));
        assert!(prompt.contains("#general"));
        assert!(prompt.contains("You are in a team channel"));
        assert!(prompt.contains("NO_REPLY"));
    }

    #[test]
    fn prepare_delivery_prompt_formats_payload_like_npm() {
        let delivery = serde_json::json!({
            "message": {
                "message_id": "msg_abc",
                "timestamp": 1700000000000_i64,
                "content": "fix the bug",
                "sender_id": "user_1",
                "sender_type": "human",
                "sender_name": "bob",
                "channel_type": "channel",
                "channel_name": "dev",
            }
        });
        let prompt = prepare_delivery_prompt("ag_123", "Coder", "Fixes bugs", &delivery).unwrap();
        assert!(prompt.contains("You are Coder. Fixes bugs"));
        assert!(prompt.contains("[target=#dev msg=msg_abc"));
        assert!(prompt.contains("type=human] @bob: fix the bug"));
        assert!(prompt.contains("You are in a team channel"));
    }
}
