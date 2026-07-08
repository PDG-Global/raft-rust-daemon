# Raft Daemon (Rust)

[![CI](https://github.com/PDG-Global/raft-rust-daemon/actions/workflows/ci.yml/badge.svg)](https://github.com/PDG-Global/raft-rust-daemon/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/raft-daemon.svg)](https://crates.io/crates/raft-daemon)
[![Documentation](https://docs.rs/raft-daemon/badge.svg)](https://docs.rs/raft-daemon)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A Rust-native port of the `@botiverse/raft-daemon` npm package for agent lifecycle management.

## Features

- **Agent lifecycle management** - Start, stop, restart, reset agents
- **Runtime drivers** - Support for multiple runtimes including RustyCLI
- **Message routing** - Deliver messages to agents
- **Task claiming** - Assign tasks to agents
- **Reminders** - Schedule and manage reminders
- **Workspace management** - Agent file storage and memory
- **APM & Observability** - Metrics, tracing, and health checks
- **CLI** - Command-line interface for managing the daemon

## Installation

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

Cross targets are supported via the standard Rust toolchain. For example, to
build for Linux on an Apple Silicon host:

```bash
rustup target add aarch64-unknown-linux-gnu
cargo build --release --target aarch64-unknown-linux-gnu
```

For static musl, FreeBSD, and other targets, install the matching target with
`rustup target add` and the appropriate cross linker, then build against that
target triple. Distributors that need to sign and notarise macOS binaries should
do so with their own Developer ID credentials outside the build, e.g.:

```bash
codesign --force --options runtime --sign "<Developer ID Application: ...>" \
    target/release/raft-daemon
xcrun notarytool submit target/release/raft-daemon.zip \
    --keychain-profile "<your profile>" --wait
```

## Usage

```bash
# Start the daemon
raft-daemon start --server-url <url> --api-key <key>

# Stop the daemon
raft-daemon stop

# Restart the daemon
raft-daemon restart

# Show daemon status
raft-daemon status

# Manage agents
raft-daemon agent list
raft-daemon agent get <agent_id>
raft-daemon agent create --name <name> --description <desc> --runtime <runtime>
raft-daemon agent update <agent_id> --name <name>
raft-daemon agent delete <agent_id>
raft-daemon agent start <agent_id>
raft-daemon agent stop <agent_id>
raft-daemon agent restart <agent_id>
raft-daemon agent reset <agent_id> --mode <mode>
raft-daemon agent status <agent_id>

# Manage servers
raft-daemon server list
raft-daemon server get <server_id>
raft-daemon server create --name <name>

# Manage computers
raft-daemon computer list
raft-daemon computer get <computer_id>
raft-daemon computer create --name <name> --server-id <server_id>

# Manage tasks
raft-daemon task list
raft-daemon task get <task_id>
raft-daemon task create --title <title> --description <desc> --channel-id <channel_id>
raft-daemon task claim <task_id>
raft-daemon task complete <task_id> --response <response>
raft-daemon task cancel <task_id>

# Manage messages
raft-daemon message send --content <content> --channel-id <channel_id>
raft-daemon message check
raft-daemon message get <message_id>

# Manage reminders
raft-daemon reminder list
raft-daemon reminder create --title <title> --duration <duration> --anchor-message-id <message_id> --author-id <author_id>
raft-daemon reminder update <reminder_id> --title <title>
raft-daemon reminder snooze <reminder_id> --duration <duration>
raft-daemon reminder cancel <reminder_id>

# Manage profiles
raft-daemon profile list
raft-daemon profile get <profile_name>
raft-daemon profile create --name <name> --server-url <url> --api-key <key>
raft-daemon profile update <profile_name> --server-url <url>
raft-daemon profile delete <profile_name>

# Debug commands
raft-daemon debug info
raft-daemon debug version
```

## Runtime Drivers

The daemon supports multiple runtime drivers:

### RustyCLI

RustyCLI is the terminal-native coding agent that powers both the `rusty` and
`builtin` runtimes. It is a single binary with zero telemetry, "bring your own
model" support, context-aware edits across your whole tree, command sandboxing
with approvals, and diff-before-apply behaviour.

**Install RustyCLI alongside this daemon:**

```bash
curl -L https://rustycli.com/install.sh | bash
```

To use RustyCLI directly:

```bash
raft-daemon start --server-url <url> --api-key <key> --runtime rusty
```

### Built-in

The built-in runtime is the default runtime for this daemon. It is backed by
RustyCLI and requires the `rusty` binary to be installed and available on
`$PATH` (or via `$RAFT_RUSTY_BINARY`). If RustyCLI is not installed, the daemon
reports an empty runtime list and will not be able to start agents.

To use the built-in runtime:

```bash
raft-daemon start --server-url <url> --api-key <key> --runtime builtin
```

Both `rusty` and `builtin` ultimately invoke the same RustyCLI binary; the only
difference is which runtime name is advertised to the raft server.

## Architecture

```
raft-daemon-rust/
├── Cargo.toml
├── README.md
├── src/
│   ├── main.rs
│   ├── cli/
│   │   ├── mod.rs
│   │   ├── commands.rs
│   │   └── args.rs
│   ├── daemon/
│   │   ├── mod.rs
│   │   ├── state.rs
│   │   ├── agent.rs
│   │   ├── computer.rs
│   │   ├── server.rs
│   │   ├── task.rs
│   │   ├── message.rs
│   │   ├── reminder.rs
│   │   ├── workspace.rs
│   │   ├── runtime.rs
│   │   ├── handlers.rs
│   │   ├── apm.rs
│   │   └── trace.rs
│   ├── models/
│   │   ├── mod.rs
│   │   ├── agent.rs
│   │   ├── server.rs
│   │   ├── computer.rs
│   │   ├── task.rs
│   │   ├── message.rs
│   │   ├── reminder.rs
│   │   └── runtime.rs
│   └── runtime/
│       ├── mod.rs
│       ├── driver.rs
│       ├── rusty.rs
│       ├── builtin.rs
│       └── runtime.rs
├── tests/
│   ├── unit/
│   └── integration/
└── scripts/
    └── generate-models.sh
```

## Models

### Agent

An agent is an AI teammate in a server. It has:

- A persistent identity
- Channels it has joined
- Tasks it can claim
- Memory across sessions

### Server

A server is where your team works. It holds:

- Channels
- Agents
- Members
- Computers

### Computer

A computer is a machine connected to a server. Agents run on computers.

### Task

A task is a unit of work that can be assigned to an agent.

### Message

A message is a communication between agents or between humans and agents.

### Reminder

A reminder is a scheduled wake-up signal for an agent.

### Runtime

A runtime is the AI engine that powers an agent.

## Development

```bash
# Clone the repository
git clone https://github.com/PDG-Global/raft-rust-daemon.git
cd raft-daemon-rust

# Install dependencies
cargo install

# Run tests
cargo test

# Build
cargo build --release

# Run
cargo run --release
```

## Contributing

Contributions are welcome! Please see the [CONTRIBUTING.md](CONTRIBUTING.md) file for details.

## License

This project is licensed under the [MIT License](LICENSE).

## Security

Found a security issue? Please see [SECURITY.md](SECURITY.md) for responsible
disclosure details. Do not open a public issue for security vulnerabilities.

## Acknowledgments

- [Raft](https://raft.build) - The original npm package
- [RustyCLI](https://rustycli.com) - The Rust runtime driver
