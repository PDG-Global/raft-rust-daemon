# Contributing to Raft Daemon (Rust)

Thank you for your interest in contributing! This guide will help you get started.

## Getting Started

1. **Fork the repository**
2. **Clone your fork**
3. **Install dependencies**
   ```bash
   cargo install
   ```
4. **Run tests**
   ```bash
   cargo test
   ```
5. **Make your changes**
6. **Run tests again**
7. **Submit a pull request**

## Development Setup

### Prerequisites

- Rust 1.75+
- Cargo
- (Optional) RustyCLI for testing

### Local Development

```bash
# Install dependencies
cargo install

# Run in watch mode
cargo watch -x "run"

# Run tests
cargo test

# Build
cargo build --release
```

## Code Style

- Follow Rust API guidelines
- Use `#[derive(Debug, Clone, Serialize, Deserialize)]` for structs
- Use `async fn` for async functions
- Use `Result<T, E>` for fallible functions
- Add documentation to all public items
- Use `tracing` for logging

## Testing

### Unit Tests

```bash
cargo test --lib
```

### Integration Tests

```bash
cargo test --test integration
```

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test --test integration::test_name

# Run with coverage
cargo install cargo-tarpaulin
cargo tarpaulin --out html
```

## Pull Request Guidelines

1. **Branch name**: `feature/short-description` or `fix/issue-number`
2. **Commit message**: Follow [Conventional Commits](https://www.conventionalcommits.org/)
3. **Tests**: Include tests for new functionality
4. **Documentation**: Update README.md if needed
5. **Dependencies**: Update Cargo.toml if adding new dependencies

## Code Review

When submitting a PR, please ensure:

- [ ] All tests pass
- [ ] Code follows Rust style guidelines
- [ ] Documentation is up to date
- [ ] No unnecessary dependencies
- [ ] Error handling is appropriate
- [ ] Logging is sufficient but not excessive

## Architecture

The daemon is organized into modules:

- **cli/** - Command-line interface
- **daemon/** - Core daemon functionality
- **models/** - Data models
- **runtime/** - Runtime implementations
- **tests/** - Unit and integration tests

## Runtime Drivers

### Adding a New Runtime Driver

1. Create a new file in `src/runtime/drivers/`
2. Implement the `Runtime` trait
3. Add to `src/runtime/mod.rs`
4. Add feature flag in `Cargo.toml`
5. Add tests

Example:

```rust
// src/runtime/drivers/my_runtime.rs
pub struct MyRuntime {
    // ...
}

#[async_trait]
impl Runtime for MyRuntime {
    async fn initialize(&self, config: RuntimeConfig) -> Result<()> {
        // Initialize
        Ok(())
    }

    async fn handle_message(&self, message: Message) -> Result<AgentResponse> {
        // Handle message
        Ok(AgentResponse {
            content: "response".to_string(),
            metadata: serde_json::Map::new(),
        })
    }

    // ... other methods
}
```

## Debugging

### Logging

```rust
tracing::info!("Info message");
tracing::warn!("Warning message");
tracing::error!("Error message");
tracing::debug!("Debug message");
```

### Run with tracing

```bash
RUST_LOG=raft_daemon=debug cargo run
```

### Debug builds

```bash
cargo run --features debug
```

## Performance

### Profiling

```bash
cargo install cargo-llvm-cov
cargo llvm-cov --locked
```

### Optimizing

- Use `tokio::sync::Mutex` for shared state
- Use `dashmap` for concurrent access
- Use `parking_lot` for lock-free synchronization
- Avoid unnecessary allocations

## Security

- Never commit secrets
- Use environment variables for sensitive data
- Validate all user input
- Handle errors gracefully

## Questions?

- Check the [README.md](README.md)
- Look at existing code
- Ask in the [issues](https://github.com/PDG-Global/raft-rust-daemon/issues)

## Code of Conduct

Be kind and respectful to others. We value everyone's contribution.
