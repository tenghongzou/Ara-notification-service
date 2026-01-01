# Development Guide

This document describes how to develop, test, and extend Ara Notification Service.

---

## Development Environment Setup

### Required Tools

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Verify versions
rustc --version  # >= 1.75.0
cargo --version

# Install development tools
cargo install cargo-watch   # Auto-recompile
cargo install cargo-expand  # Expand macros
cargo install cargo-udeps   # Check unused dependencies
```

### IDE Setup

#### VS Code

Recommended extensions:

```json
{
  "recommendations": [
    "rust-lang.rust-analyzer",
    "tamasfe.even-better-toml",
    "serayuzgur.crates",
    "vadimcn.vscode-lldb"
  ]
}
```

Settings file (`.vscode/settings.json`):

```json
{
  "rust-analyzer.checkOnSave.command": "clippy",
  "rust-analyzer.cargo.features": "all",
  "editor.formatOnSave": true
}
```

---

## Code Standards

### Formatting

```bash
# Format code
cargo fmt

# Check formatting
cargo fmt --check
```

### Linting

```bash
# Run Clippy
cargo clippy

# Strict mode
cargo clippy -- -D warnings
```

### Common Clippy Settings

```toml
# Cargo.toml
[lints.clippy]
pedantic = "warn"
nursery = "warn"
```

---

## Project Structure Standards

### Module Organization

```rust
// Each module folder should contain mod.rs or use mod.rs pattern
src/
├── feature/
│   ├── mod.rs          // Module entry, public API
│   ├── types.rs        // Type definitions
│   ├── handler.rs      // Handler logic
│   └── tests.rs        // Unit tests (optional)
```

### Naming Conventions

| Type | Convention | Example |
|------|-----------|---------|
| Module | snake_case | `connection_manager` |
| Struct | PascalCase | `ConnectionHandle` |
| Function | snake_case | `send_notification` |
| Constant | SCREAMING_SNAKE_CASE | `MAX_CONNECTIONS` |
| Type Parameter | Single uppercase letter | `T`, `E`, `R` |

### Error Handling

Use `thiserror` to define error types:

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum NotificationError {
    #[error("User not found: {0}")]
    UserNotFound(String),

    #[error("Connection limit exceeded")]
    ConnectionLimitExceeded,

    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),

    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),
}
```

---

## Adding New Module Guide

### 1. Create Module Structure

```bash
mkdir -p src/my_feature
touch src/my_feature/mod.rs
touch src/my_feature/types.rs
touch src/my_feature/handler.rs
```

### 2. Define Types

```rust
// src/my_feature/types.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyFeatureConfig {
    pub enabled: bool,
    pub max_items: usize,
}

impl Default for MyFeatureConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_items: 100,
        }
    }
}
```

### 3. Implement Handler

```rust
// src/my_feature/handler.rs
use std::sync::Arc;
use crate::my_feature::types::MyFeatureConfig;

pub struct MyFeatureHandler {
    config: MyFeatureConfig,
}

impl MyFeatureHandler {
    pub fn new(config: MyFeatureConfig) -> Self {
        Self { config }
    }

    pub async fn process(&self, data: &str) -> Result<(), anyhow::Error> {
        if !self.config.enabled {
            return Ok(());
        }
        // Processing logic
        Ok(())
    }
}
```

### 4. Export Module

```rust
// src/my_feature/mod.rs
mod types;
mod handler;

pub use types::*;
pub use handler::*;
```

### 5. Register in lib.rs

```rust
// src/lib.rs
pub mod my_feature;
```

### 6. Integrate with AppState

```rust
// src/server/state.rs
use crate::my_feature::MyFeatureHandler;

pub struct AppState {
    // ... other fields
    pub my_feature: Arc<MyFeatureHandler>,
}
```

---

## Testing Guide

### Unit Tests

```rust
// src/my_feature/handler.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MyFeatureConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.max_items, 100);
    }

    #[tokio::test]
    async fn test_process_when_disabled() {
        let handler = MyFeatureHandler::new(MyFeatureConfig::default());
        let result = handler.process("test").await;
        assert!(result.is_ok());
    }
}
```

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_process

# Show output
cargo test -- --nocapture

# Run module tests
cargo test my_feature::

# Run integration tests
cargo test --test integration
```

### Test Coverage

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Generate coverage report
cargo tarpaulin --out Html
```

---

## Load Testing

### K6 Testing

```bash
# Install K6
# macOS: brew install k6
# Linux: snap install k6

# Set environment variables
export JWT_TOKEN="your-jwt-token"
export API_KEY="your-api-key"
export BASE_URL="http://localhost:8081"

# Run tests
cd tests/load

# WebSocket test
./run-tests.sh websocket baseline

# HTTP API test
./run-tests.sh http-api high

# End-to-end test
./run-tests.sh e2e stress
```

### Test Profiles

| Profile | VUs | Duration | Purpose |
|---------|-----|----------|---------|
| smoke | 1-5 | 30s | Quick verification |
| baseline | 10-50 | 2m | Baseline testing |
| medium | 50-200 | 5m | Medium load |
| high | 200-500 | 10m | High load |
| stress | 500-1000 | 15m | Stress testing |
| soak | 100 | 30m | Long-term stability |
| spike | 10-1000-10 | 5m | Burst traffic |

---

## Debugging Tips

### Log Levels

```bash
# Verbose logging
RUST_LOG=debug cargo run

# Module-level logging
RUST_LOG=ara_notification_service::websocket=trace cargo run

# JSON format
RUST_LOG=info,json=true cargo run
```

### Common Logging Patterns

```rust
use tracing::{debug, info, warn, error, instrument};

#[instrument(skip(self), fields(user_id = %user_id))]
pub async fn send_to_user(&self, user_id: &str, event: NotificationEvent) {
    info!("Sending notification");

    if let Err(e) = self.do_send(user_id, &event).await {
        error!(error = %e, "Failed to send notification");
    }

    debug!(?event, "Notification details");
}
```

### Performance Profiling

```bash
# Install flamegraph
cargo install flamegraph

# Generate flamegraph
cargo flamegraph --bin ara-notification-service

# Using perf (Linux)
perf record -g target/release/ara-notification-service
perf report
```

---

## Release Process

### 1. Version Update

```toml
# Cargo.toml
[package]
version = "1.1.0"
```

### 2. Update CHANGELOG

```markdown
## [1.1.0] - 2024-01-15

### Added
- New feature X

### Changed
- Improved Y performance

### Fixed
- Bug in Z
```

### 3. Build Release

```bash
# Build optimized version
cargo build --release

# Check binary size
ls -lh target/release/ara-notification-service

# Run all tests
cargo test --release
```

### 4. Docker Image

```bash
# Build image
docker build -t ara-notification-service:1.1.0 .

# Tag as latest
docker tag ara-notification-service:1.1.0 ara-notification-service:latest

# Push (if registry configured)
docker push your-registry/ara-notification-service:1.1.0
```

---

## Common Development Issues

### Slow Compilation

```bash
# Use sccache
cargo install sccache
export RUSTC_WRAPPER=sccache

# Or use mold linker (Linux)
sudo apt install mold
RUSTFLAGS="-C link-arg=-fuse-ld=mold" cargo build
```

### Dependency Conflicts

```bash
# Check dependency tree
cargo tree

# Check specific package
cargo tree -i tokio

# Update dependencies
cargo update
```

### Memory Leaks

```bash
# Using valgrind
valgrind --leak-check=full target/debug/ara-notification-service

# Using heaptrack
heaptrack target/debug/ara-notification-service
```

---

## Related Documentation

- [System Architecture](./01-architecture.md)
- [API Reference](./03-api-reference.md)
- [Advanced Features](./05-advanced-features.md)
- [Observability](./06-observability.md)

