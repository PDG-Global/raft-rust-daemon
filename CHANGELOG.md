# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Upstream tracking

This crate is a Rust port of the Node.js package [`@botiverse/raft-daemon`](https://www.npmjs.com/package/@botiverse/raft-daemon).
To keep the two in sync, each release records the upstream npm version it tracks.

| raft-daemon (Rust) | Upstream `@botiverse/raft-daemon` |
|--------------------|-----------------------------------|
| 0.69.0             | 0.69.0                            |

## [Unreleased]

### Added

- **Agent chat replies now reach raft.** `agent:start` mints a per-agent
  `sk_agent_…` runner credential via
  `POST /internal/computer/runners/<agent_id>/credentials` (mirroring npm
  `requestRunnerCredentialOnce`) and stores it on the agent process.
  `agent:deliver` spawns RustyCLI with the LLM provider key from
  `config.provider.apiKey` (computer settings flow server-side, exactly as
  the operator configured on raft.build) and, on success, POSTs the
  response to `/internal/agent-api/send` authenticated as the agent. The
  full chain — `agent:start` → mint → `agent:deliver` → spawn → POST →
  `agent:stop` — is verified end-to-end against a mock raft server.
- **Agent spawn via RustyCLI.** `agent:start` now installs per-agent state
  (`session_id`, `model`, `workspace`, `launch_id`) in an
  `AgentProcessRegistry` and replies `agent:session` + `agent:status: active`
  + `agent:activity: idle`. `agent:deliver` acks immediately (so the server
  stops the retry storm observed earlier) and spawns
  `rusty --headless --prompt <message> --resume <session> --model <model>
  --cwd <workspace>` in a background task, broadcasting
  `agent:activity: working` → `error`/`idle` around the spawn. `agent:stop`
  clears the registry and replies `inactive`. Per-agent workspaces live under
  `<home>/agents/<agent_id>/`.
- **Real daemon loop.** `raft-daemon start` now actually runs: it backgrounds
  itself by default (spawning a detached child via `setsid()`), opens a
  WebSocket connection to `<server_url>/daemon/connect?key=<api_key>`,
  dispatches inbound messages, sends liveness pings, and reconnects with
  exponential backoff (1s → 30s ceiling) on transient failures.
- `--foreground` flag on `start` runs the daemon in the current process
  instead of spawning a detached child. Both `raft-daemon --foreground start`
  and `raft-daemon start --foreground` work.
- `stop` reads the PID file, sends `SIGTERM`, waits up to 10 s, then
  escalates to `SIGKILL`.
- `status` reports `running (pid=…)` / `not running` / stale pid file.
- Per-user home directory at `~/.raft-daemon/` (overridable via
  `$RAFT_DAEMON_HOME`) holding `daemon.pid`, `state.json`, and
  `logs/daemon.log`. Created with `0700` permissions on Unix because the
  state and logs may carry API keys.
- Graceful shutdown on `SIGINT` / `SIGTERM`: closes the WebSocket, removes
  the PID file, drains the writer task.
- Tracing setup that writes to stdout when interactive and to
  `logs/daemon.log` when detached. Honours `RUST_LOG`.

### Fixed

- **rustls CryptoProvider panic.** Any `wss://` connection panicked with
  *"Could not automatically determine the process-level CryptoProvider"*.
  `ring` is now installed explicitly at process startup before any TLS
  handshake runs. `tokio-tungstenite` was also switched from the
  `__rustls-tls` umbrella feature to `rustls-tls-native-roots`, which both
  silences the `ring`/`aws-lc-rs` ambiguity and trusts the system keychain
  (fixing `invalid peer certificate: UnknownIssuer` behind corporate
  proxies).
- **WebSocket handshake failure.** The client was building an
  `http::Request` by hand and passing it to `connect_async`, which skipped
  the required upgrade headers (`Sec-WebSocket-Key`, `Upgrade: websocket`,
  …). Now uses `IntoClientRequest` so the handshake actually completes.
- **Server showed "no detected runtime".** The daemon never sent the
  post-handshake `ready` frame, so the raft UI had no list of installed
  runtimes and refused to schedule work. The daemon now sends
  `{"type":"ready", capabilities, runtimes, runningAgents, hostname, os,
  daemonVersion}` immediately on connect, mirroring `DaemonCore.handleConnect()`
  in the npm package. Runtime detection scans `PATH` for `claude`, `codex`,
  `agy`, `kimi`, `copilot`, `cursor-agent`, `gemini`, `opencode`, `pi`.
- **`builtin` runtime is now RustyCLI-backed.** Per the migration plan, the
  npm in-process pi SDK is replaced by RustyCLI in this port. We now only
  advertise `builtin` to the server when the RustyCLI binary is actually
  discoverable (via `$RAFT_RUSTY_BINARY`, or `which rusty` / `rustycli` /
  `rusty-cli`). Without RustyCLI installed we report an empty runtime list
  rather than claiming a runtime we can't actually drive — this prevents
  the server from queuing `agent:start` requests that would fail at spawn
  time.
- **Connection error legibility.** A non-upgrade HTTP response (e.g. when
  pointing at a web UI like `app.raft.build` instead of `api.raft.build`)
  now logs a hint pointing at the URL mismatch instead of just
  `HTTP error: 200 OK`.
- **Noisy ping logging.** Routine `ping`/`pong` messages now log at DEBUG
  instead of INFO, so the 30-second liveness heartbeat doesn't drown out
  the real activity in the log.

### Known Limitations

- **Proxy egress is not honoured.** The raft HTTP client (`reqwest`) is built
  with `.no_proxy()` because corporate `HTTP_PROXY` env vars commonly break
  localhost connections and strip `Authorization` headers. Operators who
  need proxy egress to reach `api.raft.build` should override once we add
  per-host proxy support. The WebSocket path *does* honour `HTTPS_PROXY` via
  `tokio-tungstenite`.
- **Spawn-per-delivery, no streaming.** Each `agent:deliver` is a fresh
  `rusty --prompt …` invocation. Continuity across deliveries uses
  `--resume <session_id>`. The agent's response is delivered as a single
  chat reply when rusty exits, not streamed turn-by-turn. A long-running
  agent process model (matching npm's `AgentProcessManager`) is a follow-up.
- **No reaction / attachment / thread routing.** `agent:deliver` posts a
  single reply to the derived target. Thread replies, reactions, and
  attachment uploads aren't yet wired even though the daemon mints
  credentials with those scopes.

### Changed

- `execute_command` is now `async` because the daemon control commands
  (`start` / `stop` / `status` / `restart`) drive async I/O.
- `main.rs` dispatch switched from slice patterns (`[c] if c == "start"`)
  to `first().map(String::as_str)` so flags after the subcommand
  (e.g. `start --foreground`) match correctly.
- The version reported by `debug version` now comes from
  `env!("CARGO_PKG_VERSION")` instead of a hardcoded `0.1.0`.

### Tooling

- Initial Rust port from `@botiverse/raft-daemon` 0.69.0.
- Cross-compilation build scripts (macOS / Linux gnu+musl / FreeBSD).
  Codesigning and notarization are env-driven (`CODESIGN_IDENTITY`, `NOTARY_PROFILE`),
  disabled by default. Removed hardcoded signing identity and Android build step.
