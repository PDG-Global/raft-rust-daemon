# Changelog / 更新日志

All notable changes to this project are documented in this file.

本项目的所有重要变更都记录在此文件中。

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

格式基于 [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)，并遵循
[语义化版本](https://semver.org/spec/v2.0.0.html)。

## Upstream tracking / 上游版本跟踪

This crate is a Rust port of the Node.js package [`@botiverse/raft-daemon`](https://www.npmjs.com/package/@botiverse/raft-daemon).
To keep the two in sync, each release records the upstream npm version it tracks.

本 crate 是 Node.js 包 [`@botiverse/raft-daemon`](https://www.npmjs.com/package/@botiverse/raft-daemon) 的 Rust 移植版。为保持两者同步，每次发布都会记录所跟踪的上游 npm 版本。

| raft-daemon (Rust) | Upstream `@botiverse/raft-daemon` |
|--------------------|-----------------------------------|
| 0.69.0             | 0.69.0                            |
| 0.72.0             | 0.72.0                            |

## [0.72.0] - 2026-07-09

### Added / 新增

- **Local agent-api HTTP proxy.** The daemon binds a localhost proxy that swaps
  short-lived proxy tokens for agent `sk_agent_…` credentials and forwards
  `/internal/agent-api/*` requests to the raft server. This lets bundled CLI
  tools run without exposing real API keys.
- **本地代理 API HTTP 代理。** 守护进程绑定一个本地代理，使用短生命周期代理令牌交换代理的 `sk_agent_…` 凭证，并将 `/internal/agent-api/*` 请求转发到 Raft 服务器。这样内嵌的 CLI 工具可以在不暴露真实 API 密钥的情况下运行。
- **Self-contained `raft`/`slock` CLI.** The daemon writes wrapper scripts to
  `~/.raft-daemon/profiles/<profile>/bin/` and puts that directory on the
  agent's `PATH`. The wrappers invoke `raft-daemon cli ...` which dispatches to
  the local proxy. Commands include reminders, tasks, inbox, events, history,
  and server info.
- **自包含的 `raft`/`slock` CLI。** 守护进程将包装脚本写入 `~/.raft-daemon/profiles/<profile>/bin/`，并将该目录加入代理的 `PATH`。包装脚本调用 `raft-daemon cli ...` 并分发到本地代理。支持的命令包括提醒、任务、收件箱、事件、历史记录和服务器信息。
- **Full agent-facing task flow.** The bundled CLI supports task list, create,
  claim, update-status, and unclaim operations:
  - `raft task list --channel '#<name>'`
  - `raft task create --channel '#<name>' --title '...'`
  - `raft task claim --channel '#<name>' --task-number N`
  - `raft task update-status --channel '#<name>' --task-number N --status <status>`
  - `raft task unclaim --channel '#<name>' --task-number N`
  - Valid statuses: `todo`, `in_progress`, `in_review`, `done`, `closed`.
- **完整的代理任务流程。** 内嵌 CLI 支持任务列表、创建、认领、更新状态和取消认领：
  - `raft task list --channel '#<name>'`
  - `raft task create --channel '#<name>' --title '...'`
  - `raft task claim --channel '#<name>' --task-number N`
  - `raft task update-status --channel '#<name>' --task-number N --status <status>`
  - `raft task unclaim --channel '#<name>' --task-number N`
  - 有效状态：`todo`、`in_progress`、`in_review`、`done`、`closed`。
- **Reminder creation via message ID injection.** The daemon injects the
  incoming delivery's `message_id` into RustyCLI as `SLOCK_MESSAGE_ID`, and
  `raft reminder create` falls back to it when `--msg-id` is not provided. This
  satisfies the server's requirement for `msgId` on agent-created reminders.
- **通过消息 ID 注入创建提醒。** 守护进程将传入投递的 `message_id` 作为 `SLOCK_MESSAGE_ID` 注入到 RustyCLI，当未提供 `--msg-id` 时，`raft reminder create` 会回退使用该环境变量。这满足了服务器对代理创建提醒所需的 `msgId` 要求。
- **npm 0.72.0 frame parity.** Stub/ack handlers added for
  `agent:runtime_profile:migration`, `agent:runtime_profile:daemon_release_notice`,
  `agent:diagnostic:*`, `machine:workspace:scan`, `machine:workspace:delete`,
  `reminder.*`, and `computer:restart/upgrade`.
- **npm 0.72.0 帧协议兼容。** 为 `agent:runtime_profile:migration`、`agent:runtime_profile:daemon_release_notice`、`agent:diagnostic:*`、`machine:workspace:scan`、`machine:workspace:delete`、`reminder.*` 和 `computer:restart/upgrade` 添加了占位/确认处理器。
- **`SLOCK_CLI` environment variable.** The daemon sets this for RustyCLI so it
  can locate the bundled CLI wrappers.
- **`SLOCK_CLI` 环境变量。** 守护进程为 RustyCLI 设置此变量，使其能够定位内嵌的 CLI 包装脚本。

### Fixed / 修复

- **Timeout fallback suppressed.** When RustyCLI timed out and its output began
  with the `NO_REPLY` marker, the daemon previously posted nothing. It now posts
  a timeout error message to the chat.
- **超时回退被抑制。** 当 RustyCLI 超时且其输出以 `NO_REPLY` 标记开头时，守护进程之前不会发送任何消息。现在它会向聊天发送一条超时错误消息。
- **Self-echo and bot messages in public channels** are filtered out before
  building the prompt.
- **公共频道中的自回声和机器人消息** 在构建提示词之前被过滤掉。
- **Redundant double acknowledgments.** The agent prompt now instructs the model
  not to restate the same action twice (e.g., two "Done" sentences) after
  running a CLI tool.
- **冗余的双重确认。** 代理提示词现在指示模型在运行 CLI 工具后不要重复陈述同一动作（例如两个 "Done" 句子）。

## [0.69.0] - 2026-07-07

### Added / 新增

- **Agent chat replies now reach raft.** `agent:start` mints a per-agent
  runner credential and stores it on the agent process. `agent:deliver`
  spawns RustyCLI with the configured LLM provider key and POSTs the response
  to `/internal/agent-api/send` authenticated as the agent. The full lifecycle
  is verified end-to-end against a mock raft server.
- **代理聊天回复现在能到达 Raft。** `agent:start` 为每个代理铸造运行器凭证并存储在代理进程中。`agent:deliver` 使用配置的 LLM 提供商密钥启动 RustyCLI，并以代理身份认证后 POST 回复到 `/internal/agent-api/send`。完整的生命周期已通过模拟 Raft 服务器端到端验证。
- **Agent spawn via RustyCLI.** `agent:start` installs per-agent state in an
  `AgentProcessRegistry` and replies with `agent:session`, `agent:status: active`,
  and `agent:activity: idle`. `agent:deliver` acks immediately and spawns
  `rusty --headless --prompt ... --resume ...` in a background task, broadcasting
  `working`/`idle` activity. `agent:stop` clears the registry. Per-agent
  workspaces live under `<home>/agents/<agent_id>/`.
- **通过 RustyCLI 生成代理。** `agent:start` 在 `AgentProcessRegistry` 中安装每个代理的状态，并回复 `agent:session`、`agent:status: active` 和 `agent:activity: idle`。`agent:deliver` 立即确认并在后台任务中启动 `rusty --headless --prompt ... --resume ...`，广播 `working`/`idle` 活动。`agent:stop` 清理注册表。每个代理的工作区位于 `<home>/agents/<agent_id>/`。
- **Real daemon loop.** `raft-daemon start` backgrounds itself by default,
  opens a WebSocket to the raft server, dispatches inbound messages, sends
  liveness pings, and reconnects with exponential backoff on transient failures.
- **真实的守护进程循环。** `raft-daemon start` 默认在后台运行，打开到 Raft 服务器的 WebSocket，分发入站消息，发送存活探测，并在临时故障时使用指数退避重新连接。
- `--foreground` flag on `start` runs the daemon in the current process instead
  of spawning a detached child.
- `start` 的 `--foreground` 标志在当前进程中运行守护进程，而不是生成脱离的子进程。
- `stop` reads the PID file, sends `SIGTERM`, waits up to 10 s, then escalates to
  `SIGKILL`.
- `stop` 读取 PID 文件，发送 `SIGTERM`，等待最多 10 秒，然后升级到 `SIGKILL`。
- `status` reports `running (pid=…)` / `not running` / stale pid file.
- `status` 报告 `running (pid=…)` / `not running` / 过期 PID 文件。
- Per-user home directory at `~/.raft-daemon/` (overridable via `$RAFT_DAEMON_HOME`)
  holding `daemon.pid`, `state.json`, and `logs/daemon.log`. Created with `0700`
  permissions on Unix.
- 每个用户的主目录位于 `~/.raft-daemon/`（可通过 `$RAFT_DAEMON_HOME` 覆盖），包含 `daemon.pid`、`state.json` 和 `logs/daemon.log`。在 Unix 上以 `0700` 权限创建。
- Graceful shutdown on `SIGINT` / `SIGTERM`.
- 在 `SIGINT` / `SIGTERM` 时优雅关闭。
- Tracing setup that writes to stdout when interactive and to `logs/daemon.log`
  when detached. Honours `RUST_LOG`.
- 追踪设置：交互式时写入 stdout，脱离时写入 `logs/daemon.log`。遵循 `RUST_LOG`。

### Fixed / 修复

- **rustls CryptoProvider panic.** `ring` is installed explicitly at startup
  before any TLS handshake. `tokio-tungstenite` was switched to
  `rustls-tls-native-roots` to trust the system keychain.
- **rustls CryptoProvider 崩溃。** 在首次 TLS 握手之前显式安装 `ring`。`tokio-tungstenite` 切换到 `rustls-tls-native-roots` 以信任系统钥匙链。
- **WebSocket handshake failure.** Now uses `IntoClientRequest` so required
  upgrade headers are included.
- **WebSocket 握手失败。** 现在使用 `IntoClientRequest`，包含必要的升级头。
- **Server showed "no detected runtime".** The daemon now sends the `ready`
  frame immediately on connect with capabilities, runtimes, hostname, OS, and
  daemon version. Runtime detection scans `PATH` for common agent binaries.
- **服务器显示 "no detected runtime"。** 守护进程现在连接后立即发送 `ready` 帧，包含 capabilities、runtimes、hostname、OS 和 daemon 版本。运行时检测扫描 `PATH` 中常见的代理二进制文件。
- **`builtin` runtime is now RustyCLI-backed.** The daemon only advertises
  `builtin` when RustyCLI is discoverable, preventing queued starts that would
  fail at spawn time.
- **`builtin` 运行时现在由 RustyCLI 驱动。** 守护进程仅在 RustyCLI 可发现时 advertise `builtin`，防止在生成时失败排队启动。
- **Connection error legibility.** Non-upgrade HTTP responses now log a URL
  mismatch hint instead of just `HTTP error: 200 OK`.
- **连接错误可读性。** 非升级 HTTP 响应现在记录 URL 不匹配提示，而不是仅 `HTTP error: 200 OK`。
- **Noisy ping logging.** `ping`/`pong` messages now log at DEBUG instead of INFO.
- **嘈杂的 ping 日志。** `ping`/`pong` 消息现在以 DEBUG 级别而非 INFO 记录。

### Known Limitations / 已知限制

- **Proxy egress is not honoured.** The raft HTTP client (`reqwest`) is built
  with `.no_proxy()`. The WebSocket path *does* honour `HTTPS_PROXY`.
- **代理出口未被遵守。** Raft HTTP 客户端（`reqwest`）使用 `.no_proxy()` 构建。WebSocket 路径确实遵守 `HTTPS_PROXY`。
- **Spawn-per-delivery, no streaming.** Each `agent:deliver` is a fresh rusty
  invocation. Long-running agent process model is a follow-up.
- **每次投递生成新进程，无流式传输。** 每个 `agent:deliver` 都是一次新的 rusty 调用。长期运行的代理进程模型是后续工作。
- **No reaction / attachment / thread routing.** `agent:deliver` posts a single
  reply to the derived target.
- **无反应 / 附件 / 线程路由。** `agent:deliver` 仅向派生目标发送单一回复。

### Changed / 变更

- `execute_command` is now `async` for daemon control commands.
- `execute_command` 现在对守护进程控制命令是 `async` 的。
- `main.rs` dispatch fixed so flags after the subcommand match correctly.
- `main.rs` 分发修复，使命令后的标志正确匹配。
- The version reported by `debug version` now comes from
  `env!("CARGO_PKG_VERSION")`.
- `debug version` 报告的版本现在来自 `env!("CARGO_PKG_VERSION")`。

### Tooling / 工具

- Initial Rust port from `@botiverse/raft-daemon` 0.69.0.
- 从 `@botiverse/raft-daemon` 0.69.0 开始的初始 Rust 移植。
- Cross-compilation build scripts (macOS / Linux gnu+musl / FreeBSD).
  Codesigning and notarization are env-driven, disabled by default.
- 交叉编译构建脚本（macOS / Linux gnu+musl / FreeBSD）。代码签名和公证由环境变量驱动，默认禁用。
