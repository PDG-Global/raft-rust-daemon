# Raft Daemon (Rust)

[![crates.io](https://img.shields.io/crates/v/raft-daemon.svg)](https://crates.io/crates/raft-daemon)
[![Documentation](https://docs.rs/raft-daemon/badge.svg)](https://docs.rs/raft-daemon)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A Rust-native port of the `@botiverse/raft-daemon` npm package for agent lifecycle management.

## Features

- **Agent lifecycle management** — start, stop, restart, and reset agents via the raft server
- **Message delivery** — receive messages from raft and dispatch them to agents
- **RustyCLI-backed runtime** — the default `builtin` runtime is powered by RustyCLI
- **Multi-profile support** — run multiple isolated daemon instances with `--profile`
- **Running-agent persistence** — started agents are saved to `state.json` and restored on reconnect
- **Workspace management** — per-agent memory via `MEMORY.md` and `notes/`
- **Background operation** — `start` spawns a detached child by default; use `--foreground` to keep it in the terminal

## Installation

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
curl -L https://rustycli.com/install.sh | bash
```

The daemon discovers `rusty` via `$RAFT_RUSTY_BINARY`, then `rusty`, `rustycli`, or `rusty-cli` on `$PATH`. If RustyCLI is not installed, the daemon reports an empty runtime list and cannot start agents.

Both `builtin` and `rusty` advertise different runtime names to the raft server but invoke the same RustyCLI binary.

## Usage

```bash
# Start the daemon (spawns a detached background process)
raft-daemon start --server-url https://api.raft.build --api-key <key>

# Start in the foreground
raft-daemon start --server-url https://api.raft.build --api-key <key> --foreground

# Stop the daemon
raft-daemon stop

# Show daemon status
raft-daemon status

# Restart requires stop then start (so options can be refreshed)
raft-daemon stop && raft-daemon start --server-url https://api.raft.build --api-key <key>

# Use a different profile (isolated home at ~/.raft-daemon/profiles/<name>/)
raft-daemon --profile opusfab start --server-url https://api.raft.build --api-key <key>
raft-daemon --profile opusfab stop
raft-daemon --profile opusfab status
```

### Environment variables

| Variable | Description |
|----------|-------------|
| `RAFT_SERVER_URL` | Default server URL (default: `https://api.raft.build`) |
| `RAFT_API_KEY` | Default API key |
| `RAFT_DAEMON_HOME` | Override the daemon state directory (`~/.raft-daemon`) |
| `RAFT_RUSTY_BINARY` | Path to the RustyCLI binary |
| `RUST_LOG` | tracing filter, e.g. `info,raft_daemon=debug` |

### Agent management (scaffolding)

The `agent` subcommands are parsed and dispatched, but they currently print placeholders. Agents are started and stopped by the raft server via the daemon WebSocket.

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
├── build-release.sh
├── src/
│   ├── main.rs
│   ├── cli/
│   │   ├── args.rs
│   │   ├── commands.rs
│   │   └── mod.rs
│   ├── daemon/
│   │   ├── mod.rs
│   │   ├── runner.rs
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
