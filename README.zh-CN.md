# Raft Daemon（Rust 版）

[![crates.io](https://img.shields.io/crates/v/raft-daemon.svg)](https://crates.io/crates/raft-daemon)
[![Documentation](https://docs.rs/raft-daemon/badge.svg)](https://docs.rs/raft-daemon)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

`@botiverse/raft-daemon` npm 包的 Rust 原生移植版，用于代理生命周期管理。

## 功能

- **代理生命周期管理** — 通过 Raft 服务器启动、停止、重启和重置代理
- **消息投递** — 从 Raft 接收消息并分发给代理
- **RustyCLI 运行时** — 默认的 `builtin` 运行时由 RustyCLI 驱动
- **多配置文件支持** — 使用 `--profile` 运行多个相互隔离的守护进程实例
- **运行中代理持久化** — 已启动的代理保存到 `state.json`，重连后自动恢复
- **工作区管理** — 每个代理通过 `MEMORY.md` 和 `notes/` 维护记忆
- **后台运行** — `start` 默认生成脱离的子进程；使用 `--foreground` 保持在前台
- **可选自动更新** — 在空闲且处于安静时段时自动从 GitHub 下载并安装新版本

## 安装

### 预编译二进制文件

从 [GitHub releases](https://github.com/PDG-Global/raft-rust-daemon/releases) 页面下载适合你平台的二进制文件，赋予执行权限并放到 `PATH` 中。

```bash
# 示例：macOS Apple Silicon
curl -L -o raft-daemon https://github.com/PDG-Global/raft-rust-daemon/releases/latest/download/raft-daemon-macos-arm64
chmod +x raft-daemon
sudo mv raft-daemon /usr/local/bin/
```

验证校验和：

```bash
shasum -a 256 -c SHA256SUMS.txt
```

### Cargo

```bash
cargo install raft-daemon
```

## 从源码构建

调试构建：

```bash
cargo build
```

优化发布构建：

```bash
cargo build --release
```

构建产物位于 `target/release/raft-daemon`。

### 交叉编译发布二进制文件

项目包含 `./build-release.sh` 脚本，用于为所有支持的目标构建，并对 macOS 二进制文件进行签名和公证。针对单个目标：

```bash
rustup target add aarch64-unknown-linux-gnu
cargo build --release --target aarch64-unknown-linux-gnu
```

## 运行时依赖

默认的 `builtin` 运行时由 **RustyCLI** 驱动。请与本守护进程一起安装：

```bash
curl -L https://rustycli.com/install | bash
```

守护进程按以下顺序查找 RustyCLI：`$RAFT_RUSTY_BINARY`，然后是 `PATH` 中的 `rusty`、`rustycli` 或 `rusty-cli`。如果未安装 RustyCLI，守护进程会报告空的运行时列表，无法启动代理。

`builtin` 和 `rusty` 向 Raft 服务器 advertise 不同的运行时名称，但调用的是同一个 RustyCLI 二进制文件。

## 使用

```bash
# 启动守护进程（默认生成脱离的后台子进程）
raft-daemon --server-url https://api.raft.build --api-key <key> start

# 前台运行
raft-daemon --server-url https://api.raft.build --api-key <key> --foreground start

# 停止守护进程
raft-daemon stop

# 查看状态
raft-daemon status

# 重启需要先停止再启动（以刷新配置）
raft-daemon stop && raft-daemon --server-url https://api.raft.build --api-key <key> start

# 使用不同的配置文件（独立主目录 ~/.raft-daemon/profiles/<name>/）
raft-daemon --profile opusfab --server-url https://api.raft.build --api-key <key> start
raft-daemon --profile opusfab stop
raft-daemon --profile opusfab status

# 启用自动自更新（默认每 24 小时检查一次）
raft-daemon --server-url https://api.raft.build --api-key <key> --auto-update start

# 启用自动自更新并设置安静时段（02:00-04:00）
raft-daemon --server-url https://api.raft.build --api-key <key> --auto-update --auto-update-quiet-hours-start 02:00 --auto-update-quiet-hours-end 04:00 start

# 启用自动自更新并设置自定义检查间隔（12 小时）
raft-daemon --server-url https://api.raft.build --api-key <key> --auto-update --auto-update-interval 12 start
```

### 环境变量

| 变量 | 说明 |
|------|------|
| `RAFT_SERVER_URL` | 默认服务器地址（默认：`https://api.raft.build`） |
| `RAFT_API_KEY` | 默认 API 密钥 |
| `RAFT_DAEMON_HOME` | 覆盖守护进程状态目录（默认：`~/.raft-daemon`） |
| `RAFT_RUSTY_BINARY` | RustyCLI 二进制文件路径 |
| `RUST_LOG` | tracing 过滤器，例如 `info,raft_daemon=debug` |

### 自动自更新

你可以开启自动更新。守护进程会定期检查
[GitHub releases](https://github.com/PDG-Global/raft-rust-daemon/releases)
页面；当有新版本可用时，它会下载对应的预编译二进制文件，验证 SHA-256
校验和，替换当前可执行文件，并在原地重启。

为避免打断正在进行的工作，更新仅在以下条件下执行：

- 没有正在运行的代理 turn，且
- 当前时间处于配置的安静时段内（如果设置了的话）。

如果未配置安静时段，守护进程在空闲时即可更新。

```bash
raft-daemon --server-url https://api.raft.build --api-key <key> \
  --auto-update \
  --auto-update-interval 24 \
  --auto-update-quiet-hours-start 02:00 \
  --auto-update-quiet-hours-end 04:00 \
  start
```

在 Unix 上，重启使用 `exec`，因此守护进程保持相同的 PID，配置文件中的
PID 文件也仍然有效。

### 代理管理（脚手架）

`agent` 子命令已解析和分发，但目前仅打印占位符。代理的实际启动和停止由 Raft 服务器通过守护进程 WebSocket 控制。

```bash
raft-daemon agent list
raft-daemon agent get <agent_id>
raft-daemon agent start <agent_id>
raft-daemon agent stop <agent_id>
raft-daemon agent restart <agent_id>
raft-daemon agent reset <agent_id> --mode <mode>
raft-daemon agent status <agent_id>
```

## 配置目录结构

每个配置文件拥有独立的根目录。

```
~/.raft-daemon/                          # 默认配置文件
~/.raft-daemon/profiles/<name>/          # 命名配置文件
├── agents/<agent_id>/
│   ├── MEMORY.md
│   ├── notes/
│   └── ...RustyCLI 工作区文件
├── logs/daemon.log
├── state.json                           # 持久化的运行中代理
└── daemon.pid
```

## 架构

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

## 开发

```bash
# 克隆仓库
git clone https://github.com/PDG-Global/raft-rust-daemon.git
cd raft-daemon-rust

# 运行测试
cargo test

# 运行 clippy
cargo clippy

# 构建发布二进制文件
cargo build --release
```

## 贡献

欢迎贡献！详情请参阅 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 许可证

本项目采用 [MIT 许可证](LICENSE)。

## 安全

发现安全问题？请参阅 [SECURITY.md](SECURITY.md) 进行负责任披露。请勿公开提交安全漏洞相关 issue。

## 致谢

- [Raft](https://raft.build) - 原始平台
- [RustyCLI](https://rustycli.com) - Rust 运行时驱动

---

[English](README.md) | [简体中文](README.zh-CN.md)
