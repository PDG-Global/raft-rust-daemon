# Raft Daemon (Rust)

[![crates.io](https://img.shields.io/crates/v/raft-daemon.svg)](https://crates.io/crates/raft-daemon)
[![Documentation](https://docs.rs/raft-daemon/badge.svg)](https://docs.rs/raft-daemon)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A Rust-native port of the `@botiverse/raft-daemon` npm package for agent lifecycle management.

Read this in other languages: [简体中文](README.zh-CN.md)

## Features

- **Agent lifecycle management** — start, stop, restart, and reset agents via the raft server
- **Message delivery** — receive messages from raft and dispatch them to agents
- **Agent-facing `raft`/`slock` CLI** — bundled commands for reminders, tasks, inbox, and history via a local agent-api proxy
- **Local agent-api proxy** — swaps short-lived proxy tokens for agent credentials and forwards calls to the raft server
- **RustyCLI-backed runtime** — the default `builtin` runtime is powered by RustyCLI
- **Multi-profile support** — run multiple isolated daemon instances with `--profile`
- **Running-agent persistence** — started agents are saved to `state.json` and restored on reconnect
- **Workspace management** — per-agent memory via `MEMORY.md` and `notes/`
- **Background operation** — `start` spawns a detached child by default; use `--foreground` to keep it in the terminal
- **Optional self-update** — automatically download and install new releases from GitHub while idle and during quiet hours

## Installation

### Quick install script

The easiest way to install on macOS, Linux, or FreeBSD is the release install
script. It detects your platform and architecture, downloads the matching
binary, verifies the SHA-256 checksum, and installs it to `/usr/local/bin`:

```bash
curl -L -o install.sh https://github.com/PDG-Global/raft-rust-daemon/releases/latest/download/install.sh
chmod +x install.sh
./install.sh
```

To install to a custom location (e.g. `~/.local/bin`):

```bash
./install.sh --prefix ~/.local
```

### Prebuilt binaries

Download the binary for your platform from the [GitHub releases](https://github.com/PDG-Global/raft-rust-daemon/releases) page, make it executable, and place it on your `PATH`.

```bash
# Example: macOS Apple Silicon
curl -L -o raft-daemon https://github.com/PDG-Global/raft-rust-daemon/releases/latest/download/raft-daemon-macos-arm64
chmod +x raft-daemon
sudo mv raft-daemon /usr/local/bin/
```

Verify the checksum:

```bash
shasum -a 256 -c SHA256SUMS.txt
```

### Cargo

```bash
cargo install raft-daemon
```

## Building from source

Debug build:

```bash
cargo build
```

Optimised release build:

```bash
cargo build --release
```

The resulting binary is `target/release/raft-daemon`.

### Cross-compiling release binaries

The included `./build-release.sh` script builds for all supported targets and signs/notarises macOS binaries. For a single target:

```bash
rustup target add aarch64-unknown-linux-gnu
cargo build --release --target aarch64-unknown-linux-gnu
```

## Runtime requirement

The default `builtin` runtime is powered by **RustyCLI**. Install it alongside this daemon:

```bash
curl -L https://rustycli.com/install | bash
```

The daemon discovers `rusty` via `$RAFT_RUSTY_BINARY`, then `rusty`, `rustycli`, or `rusty-cli` on `$PATH`. If RustyCLI is not installed, the daemon reports an empty runtime list and cannot start agents.

Both `builtin` and `rusty` advertise different runtime names to the raft server but invoke the same RustyCLI binary.

## Usage

```bash
# Start the daemon (spawns a detached background process)
raft-daemon --server-url https://api.raft.build --api-key <key> start

# Start in the foreground
raft-daemon --server-url https://api.raft.build --api-key <key> --foreground start

# Stop the daemon
raft-daemon stop

# Show daemon status
raft-daemon status

# Restart requires stop then start (so options can be refreshed)
raft-daemon stop && raft-daemon --server-url https://api.raft.build --api-key <key> start

# Use a different profile (isolated home at ~/.raft-daemon/profiles/<name>/)
raft-daemon --profile opusfab --server-url https://api.raft.build --api-key <key> start
raft-daemon --profile opusfab stop
raft-daemon --profile opusfab status

# Enable automatic self-update (checks every 24 hours by default)
raft-daemon --server-url https://api.raft.build --api-key <key> --auto-update start

# Enable automatic self-update with custom quiet hours (02:00-04:00)
raft-daemon --server-url https://api.raft.build --api-key <key> --auto-update --auto-update-quiet-hours-start 02:00 --auto-update-quiet-hours-end 04:00 start

# Enable automatic self-update with a custom check interval (12 hours)
raft-daemon --server-url https://api.raft.build --api-key <key> --auto-update --auto-update-interval 12 start
```

### Environment variables

| Variable | Description |
|----------|-------------|
| `RAFT_SERVER_URL` | Default server URL (default: `https://api.raft.build`) |
| `RAFT_API_KEY` | Default API key |
| `RAFT_DAEMON_HOME` | Override the daemon state directory (`~/.raft-daemon`) |
| `RAFT_RUSTY_BINARY` | Path to the RustyCLI binary |
| `RUST_LOG` | tracing filter, e.g. `info,raft_daemon=debug` |

### Automatic self-update

You can opt in to automatic updates. The daemon will periodically check the
[GitHub releases](https://github.com/PDG-Global/raft-rust-daemon/releases) page;
when a newer version is available it downloads the matching prebuilt binary,
verifies the SHA-256 checksum, replaces the current executable, and restarts
in place.

To avoid interrupting active work, updates only happen when:

- No agent turn is currently running, and
- The current time is inside the configured quiet-hours window (if any).

If quiet hours are not configured, the daemon may update whenever it is idle.

```bash
raft-daemon --server-url https://api.raft.build --api-key <key> \
  --auto-update \
  --auto-update-interval 24 \
  --auto-update-quiet-hours-start 02:00 \
  --auto-update-quiet-hours-end 04:00 \
  start
```

The restart uses `exec` on Unix, so the daemon keeps the same PID and the
profile's PID file remains valid.

### Agent-facing CLI (`raft` / `slock`)

When an agent starts, the daemon creates `raft` and `slock` wrapper scripts in
`~/.raft-daemon/profiles/<profile>/bin/` and puts that directory on the agent's
`PATH`. Inside RustyCLI, the agent can use these commands to interact with the
Raft server through the local agent-api proxy:

```bash
# Reminders
raft reminder create --title "Follow up with Jeremy" --fire-at "2026-07-10T14:00:00Z"
raft reminder list
raft reminder delete <reminder-id>

# Tasks (channel is the channel name with a leading #, e.g. '#Marketing')
raft task list --channel '#Marketing'
raft task create --channel '#Marketing' --title "Write blog post"
raft task claim --channel '#Marketing' --task-number 7
raft task update-status --channel '#Marketing' --task-number 7 --status done
raft task unclaim --channel '#Marketing' --task-number 7

# Inbox, history, and server info
raft inbox
raft history --channel '#Marketing'
raft server
```

The proxy token, proxy URL, and current message ID are passed to RustyCLI as
environment variables, so agents do not need to provide `--msg-id` or API keys.

### Daemon agent management (scaffolding)

The `agent` subcommands are parsed and dispatched, but they currently print
placeholders. Agents are started and stopped by the raft server via the daemon
WebSocket.

```bash
raft-daemon agent list
raft-daemon agent get <agent_id>
raft-daemon agent start <agent_id>
raft-daemon agent stop <agent_id>
raft-daemon agent restart <agent_id>
raft-daemon agent reset <agent_id> --mode <mode>
raft-daemon agent status <agent_id>
```

## Configuration layout

Each profile is isolated under its own home directory.

```
~/.raft-daemon/                          # default profile
~/.raft-daemon/profiles/<name>/          # named profile
├── agents/<agent_id>/
│   ├── MEMORY.md
│   ├── notes/
│   └── ...RustyCLI workspace files
├── logs/daemon.log
├── state.json                           # persisted running agents
└── daemon.pid
```

## Architecture

```
raft-daemon-rust/
├── Cargo.toml
├── README.md
├── README.zh-CN.md
├── CHANGELOG.md
├── install.sh
├── build-release.sh
├── src/
│   ├── main.rs
│   ├── cli/
│   │   ├── args.rs
│   │   ├── commands.rs
│   │   ├── mod.rs
│   │   └── raft_cli.rs          # bundled raft/slock agent CLI
│   ├── daemon/
│   │   ├── mod.rs
│   │   ├── runner.rs
│   │   ├── proxy.rs             # local agent-api HTTP proxy
│   │   ├── agent/
│   │   │   ├── mod.rs
│   │   │   ├── manager.rs
│   │   │   ├── process.rs
│   │   │   └── raft_client.rs
│   │   ├── computer.rs
│   │   ├── server.rs
│   │   ├── task/
│   │   │   ├── mod.rs
│   │   │   └── manager.rs
│   │   ├── message/
│   │   │   ├── mod.rs
│   │   │   └── handler.rs
│   │   ├── reminder/
│   │   │   ├── mod.rs
│   │   │   └── manager.rs
│   │   ├── runtime/
│   │   │   ├── mod.rs
│   │   │   └── manager.rs
│   │   ├── apm/
│   │   │   ├── mod.rs
│   │   │   └── metrics.rs
│   │   ├── workspace.rs
│   │   ├── paths.rs
│   │   ├── pidfile.rs
│   │   ├── state/
│   │   │   └── mod.rs
│   │   ├── trace.rs
│   │   └── handlers.rs
│   ├── models/
│   │   ├── mod.rs
│   │   ├── agent.rs
│   │   ├── server.rs
│   │   ├── computer.rs
│   │   ├── task.rs
│   │   ├── message.rs
│   │   ├── reminder.rs
│   │   ├── runtime.rs
│   │   └── response.rs
│   └── runtime/
│       ├── mod.rs
│       └── drivers/
│           ├── mod.rs
│           ├── builtin.rs
│           └── rusty.rs
├── tests/
│   └── unit/
└── scripts/
```

## Development

```bash
# Clone the repository
git clone https://github.com/PDG-Global/raft-rust-daemon.git
cd raft-daemon-rust

# Run tests
cargo test

# Run clippy
cargo clippy

# Build release binary
cargo build --release
```

## Contributing

Contributions are welcome! Please see the [CONTRIBUTING.md](CONTRIBUTING.md) file for details.

## License

This project is licensed under the [MIT License](LICENSE).

## Security

Found a security issue? Please see [SECURITY.md](SECURITY.md) for responsible disclosure details. Do not open a public issue for security vulnerabilities.

## Acknowledgments

- [Raft](https://raft.build) - The original platform
- [RustyCLI](https://rustycli.com) - The Rust runtime driver
