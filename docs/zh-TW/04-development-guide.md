# 開發指南

本文件說明如何開發、測試與擴展 Ara Notification Service。

---

## 開發環境設置

### 必要工具

```bash
# 安裝 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 確認版本
rustc --version  # >= 1.75.0
cargo --version

# 安裝開發工具
cargo install cargo-watch   # 自動重新編譯
cargo install cargo-expand  # 展開巨集
cargo install cargo-udeps   # 檢查未使用依賴
```

### IDE 設置

#### VS Code

推薦擴充套件：

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

設定檔 (`.vscode/settings.json`)：

```json
{
  "rust-analyzer.checkOnSave.command": "clippy",
  "rust-analyzer.cargo.features": "all",
  "editor.formatOnSave": true
}
```

---

## 程式碼規範

### 格式化

```bash
# 格式化程式碼
cargo fmt

# 檢查格式
cargo fmt --check
```

### Linting

```bash
# 執行 Clippy
cargo clippy

# 嚴格模式
cargo clippy -- -D warnings
```

### 常用 Clippy 設定

```toml
# Cargo.toml
[lints.clippy]
pedantic = "warn"
nursery = "warn"
```

---

## 專案結構規範

### 模組組織

```rust
// 每個模組資料夾應包含 mod.rs 或使用 mod.rs 模式
src/
├── feature/
│   ├── mod.rs          // 模組入口、公開 API
│   ├── types.rs        // 類型定義
│   ├── handler.rs      // 處理器邏輯
│   └── tests.rs        // 單元測試 (選用)
```

### 命名規範

| 類型 | 規範 | 範例 |
|------|------|------|
| 模組 | snake_case | `connection_manager` |
| 結構體 | PascalCase | `ConnectionHandle` |
| 函數 | snake_case | `send_notification` |
| 常數 | SCREAMING_SNAKE_CASE | `MAX_CONNECTIONS` |
| 型別參數 | 單字母大寫 | `T`, `E`, `R` |

### 錯誤處理

使用 `thiserror` 定義錯誤類型：

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

## 新增模組指南

### 1. 建立模組結構

```bash
mkdir -p src/my_feature
touch src/my_feature/mod.rs
touch src/my_feature/types.rs
touch src/my_feature/handler.rs
```

### 2. 定義類型

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

### 3. 實作處理器

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
        // 處理邏輯
        Ok(())
    }
}
```

### 4. 匯出模組

```rust
// src/my_feature/mod.rs
mod types;
mod handler;

pub use types::*;
pub use handler::*;
```

### 5. 註冊到 lib.rs

```rust
// src/lib.rs
pub mod my_feature;
```

### 6. 整合到 AppState

```rust
// src/server/state.rs
use crate::my_feature::MyFeatureHandler;

pub struct AppState {
    // ... 其他欄位
    pub my_feature: Arc<MyFeatureHandler>,
}
```

---

## 測試指南

### 單元測試

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

### 執行測試

```bash
# 執行所有測試
cargo test

# 執行特定測試
cargo test test_process

# 顯示輸出
cargo test -- --nocapture

# 執行特定模組測試
cargo test my_feature::

# 執行整合測試
cargo test --test integration
```

### 測試覆蓋率

```bash
# 安裝 tarpaulin
cargo install cargo-tarpaulin

# 產生覆蓋率報告
cargo tarpaulin --out Html
```

---

## 負載測試

### K6 測試

```bash
# 安裝 K6
# macOS: brew install k6
# Linux: snap install k6

# 設定環境變數
export JWT_TOKEN="your-jwt-token"
export API_KEY="your-api-key"
export BASE_URL="http://localhost:8081"

# 執行測試
cd tests/load

# WebSocket 測試
./run-tests.sh websocket baseline

# HTTP API 測試
./run-tests.sh http-api high

# 端對端測試
./run-tests.sh e2e stress
```

### 測試 Profile

| Profile | VUs | 持續時間 | 用途 |
|---------|-----|---------|------|
| smoke | 1-5 | 30s | 快速驗證 |
| baseline | 10-50 | 2m | 基準測試 |
| medium | 50-200 | 5m | 中等負載 |
| high | 200-500 | 10m | 高負載 |
| stress | 500-1000 | 15m | 壓力測試 |
| soak | 100 | 30m | 長時間穩定性 |
| spike | 10-1000-10 | 5m | 突發流量 |

---

## 除錯技巧

### 日誌等級

```bash
# 詳細日誌
RUST_LOG=debug cargo run

# 模組級別日誌
RUST_LOG=ara_notification_service::websocket=trace cargo run

# JSON 格式
RUST_LOG=info,json=true cargo run
```

### 常用日誌模式

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

### 效能分析

```bash
# 安裝 flamegraph
cargo install flamegraph

# 產生火焰圖
cargo flamegraph --bin ara-notification-service

# 使用 perf (Linux)
perf record -g target/release/ara-notification-service
perf report
```

---

## 發布流程

### 1. 版本更新

```toml
# Cargo.toml
[package]
version = "1.1.0"
```

### 2. 更新 CHANGELOG

```markdown
## [1.1.0] - 2024-01-15

### Added
- New feature X

### Changed
- Improved Y performance

### Fixed
- Bug in Z
```

### 3. 建構 Release

```bash
# 建構優化版本
cargo build --release

# 檢查二進位大小
ls -lh target/release/ara-notification-service

# 執行所有測試
cargo test --release
```

### 4. Docker 映像

```bash
# 建構映像
docker build -t ara-notification-service:1.1.0 .

# 標記 latest
docker tag ara-notification-service:1.1.0 ara-notification-service:latest

# 推送 (如有 registry)
docker push your-registry/ara-notification-service:1.1.0
```

---

## 常見開發問題

### 編譯緩慢

```bash
# 使用 sccache
cargo install sccache
export RUSTC_WRAPPER=sccache

# 或使用 mold linker (Linux)
sudo apt install mold
RUSTFLAGS="-C link-arg=-fuse-ld=mold" cargo build
```

### 相依性衝突

```bash
# 檢查相依性樹
cargo tree

# 檢查特定套件
cargo tree -i tokio

# 更新相依性
cargo update
```

### 記憶體洩漏

```bash
# 使用 valgrind
valgrind --leak-check=full target/debug/ara-notification-service

# 使用 heaptrack
heaptrack target/debug/ara-notification-service
```

---

## 相關文件

- [系統架構](./01-architecture.md)
- [API 參考](./03-api-reference.md)
- [進階功能](./05-advanced-features.md)
- [可觀測性](./06-observability.md)

