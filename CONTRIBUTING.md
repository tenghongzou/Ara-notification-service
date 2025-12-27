# 貢獻指南 (Contributing Guide)

感謝您對 Ara Notification Service 的興趣！本文檔提供貢獻程式碼的指引與規範。

## 目錄

- [開發環境設置](#開發環境設置)
- [程式碼規範](#程式碼規範)
- [Git 工作流程](#git-工作流程)
- [Pull Request 流程](#pull-request-流程)
- [測試規範](#測試規範)
- [文檔規範](#文檔規範)
- [問題回報](#問題回報)

---

## 開發環境設置

### 系統需求

| 工具 | 版本 | 用途 |
|------|------|------|
| Rust | 1.75+ | 主要開發語言 |
| Redis | 7.0+ | Pub/Sub 訊息佇列 |
| Docker | 24.0+ | 容器化部署 (選用) |
| Git | 2.40+ | 版本控制 |

### 初始設置

```bash
# 1. Clone 專案
git clone https://github.com/your-org/ara-infra.git
cd ara-infra/services/notification

# 2. 安裝 Rust 工具鏈
rustup update stable
rustup component add clippy rustfmt

# 3. 複製環境變數
cp .env.example .env
# 編輯 .env 設定 JWT_SECRET

# 4. 啟動 Redis (Docker)
docker run -d --name redis -p 6379:6379 redis:7-alpine

# 5. 建構專案
cargo build

# 6. 執行測試
cargo test

# 7. 啟動開發伺服器
cargo run
```

### IDE 設置

**VS Code 推薦擴充套件：**

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

**rust-analyzer 設定：**

```json
{
  "rust-analyzer.checkOnSave.command": "clippy",
  "rust-analyzer.cargo.features": "all"
}
```

---

## 程式碼規範

### Rust 風格指南

我們遵循 [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/) 與官方格式化規範。

#### 格式化

```bash
# 執行格式化
cargo fmt

# 檢查格式（CI 使用）
cargo fmt -- --check
```

#### Lint 檢查

```bash
# 執行 Clippy
cargo clippy -- -D warnings

# 包含所有 targets
cargo clippy --all-targets --all-features -- -D warnings
```

### 命名規範

| 項目 | 規範 | 範例 |
|------|------|------|
| 結構體 | PascalCase | `ConnectionManager`, `NotificationEvent` |
| 函數/方法 | snake_case | `send_to_user()`, `get_connection()` |
| 常數 | SCREAMING_SNAKE_CASE | `MAX_CONNECTIONS`, `DEFAULT_TIMEOUT` |
| 模組 | snake_case | `connection_manager`, `websocket` |
| 型別參數 | 單一大寫或描述性 | `T`, `E`, `Item` |
| 生命週期 | 簡短小寫 | `'a`, `'conn` |

### 錯誤處理

```rust
// ✓ 使用自定義錯誤型別
pub enum ConnectionError {
    TotalLimitExceeded { current: usize, max: usize },
    UserLimitExceeded { user_id: String, current: usize, max: usize },
}

// ✓ 實作 Display 與 Error
impl std::fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TotalLimitExceeded { current, max } => {
                write!(f, "Total connection limit exceeded ({}/{})", current, max)
            }
            // ...
        }
    }
}

// ✓ 使用 Result 而非 panic
pub fn register(...) -> Result<Arc<ConnectionHandle>, ConnectionError> {
    // ...
}

// ✗ 避免 unwrap()，除非有明確理由
let value = map.get(&key).unwrap(); // 不推薦

// ✓ 使用 expect() 並提供上下文
let value = map.get(&key).expect("key should exist after insert");

// ✓ 或使用 ? 運算子
let value = map.get(&key).ok_or(MyError::KeyNotFound)?;
```

### 文檔註解

```rust
/// 連線管理器，維護所有 WebSocket 連線的索引。
///
/// # 設計說明
///
/// 使用三個 DashMap 索引實現 O(1) 查詢：
/// - `connections`: connection_id → ConnectionHandle
/// - `user_index`: user_id → Set<connection_id>
/// - `channel_index`: channel → Set<connection_id>
///
/// # 範例
///
/// ```rust
/// let manager = ConnectionManager::new();
/// let (tx, rx) = mpsc::channel(32);
/// let handle = manager.register("user-1".into(), vec![], tx)?;
/// ```
///
/// # 錯誤
///
/// 當連線數超過限制時回傳 [`ConnectionError`]。
pub struct ConnectionManager {
    // ...
}
```

### 模組結構

```
src/
├── lib.rs              # 公開模組匯出
├── main.rs             # 進入點
├── config/
│   ├── mod.rs          # 模組宣告
│   └── settings.rs     # 設定實作
├── feature_name/
│   ├── mod.rs          # 模組宣告 + 公開型別
│   ├── types.rs        # 資料結構
│   ├── impl.rs         # 實作邏輯
│   └── tests.rs        # 單元測試 (選用)
```

---

## Git 工作流程

### 分支策略

```
main (master)
├── develop           # 開發整合分支
│   ├── feature/xxx   # 功能分支
│   ├── fix/xxx       # 錯誤修復
│   └── refactor/xxx  # 重構分支
└── release/v0.x.x    # 發布分支
```

### 分支命名

| 類型 | 格式 | 範例 |
|------|------|------|
| 功能 | `feature/{描述}` | `feature/message-queue` |
| 修復 | `fix/{issue-id}-{描述}` | `fix/123-redis-reconnect` |
| 重構 | `refactor/{描述}` | `refactor/connection-manager` |
| 文檔 | `docs/{描述}` | `docs/api-update` |
| 測試 | `test/{描述}` | `test/load-testing` |

### Commit 訊息規範

遵循 [Conventional Commits](https://www.conventionalcommits.org/)：

```
<type>(<scope>): <subject>

<body>

<footer>
```

**類型 (type)：**

| 類型 | 說明 |
|------|------|
| `feat` | 新功能 |
| `fix` | 錯誤修復 |
| `docs` | 文檔變更 |
| `style` | 格式化（不影響程式邏輯） |
| `refactor` | 重構（不新增功能或修復錯誤） |
| `perf` | 效能優化 |
| `test` | 測試相關 |
| `chore` | 建構、工具、依賴更新 |

**範例：**

```
feat(queue): implement user message queue for offline delivery

Add in-memory circular buffer per user to store notifications
when client is disconnected. Messages are replayed on reconnect.

- Add UserMessageQueue struct with configurable size limit
- Integrate with ConnectionManager for disconnect detection
- Add replay mechanism in WebSocket handler

Closes #42
```

```
fix(redis): improve reconnection with exponential backoff

Replace fixed 5-second retry with exponential backoff starting
at 100ms, capped at 30 seconds, with 10% jitter.

Fixes #38
```

---

## Pull Request 流程

### PR 檢查清單

在提交 PR 前，請確認：

- [ ] 程式碼通過 `cargo fmt -- --check`
- [ ] 程式碼通過 `cargo clippy -- -D warnings`
- [ ] 所有測試通過 `cargo test`
- [ ] 新功能有對應的單元測試
- [ ] 公開 API 有文檔註解
- [ ] CHANGELOG.md 已更新（如適用）
- [ ] 相關文檔已更新（README, API.md 等）

### PR 標題格式

```
[類型] 簡短描述
```

範例：
- `[Feature] Add message queue for offline delivery`
- `[Fix] Redis reconnection with exponential backoff`
- `[Docs] Update API documentation for batch endpoint`

### PR 描述模板

```markdown
## 變更說明

簡述這個 PR 做了什麼變更。

## 變更類型

- [ ] 新功能 (feat)
- [ ] 錯誤修復 (fix)
- [ ] 重構 (refactor)
- [ ] 文檔 (docs)
- [ ] 測試 (test)
- [ ] 其他

## 關聯 Issue

Closes #XXX

## 測試方式

描述如何測試這些變更。

## 截圖（如適用）

附上相關截圖或 GIF。

## 檢查清單

- [ ] 我已閱讀貢獻指南
- [ ] 程式碼已格式化並通過 Lint
- [ ] 新增或更新了測試
- [ ] 文檔已更新
```

### Code Review 指南

**審查重點：**

1. **正確性** - 程式碼是否正確解決問題？
2. **安全性** - 是否有安全漏洞？
3. **效能** - 是否有效能問題？
4. **可讀性** - 程式碼是否易於理解？
5. **測試** - 測試覆蓋是否足夠？
6. **文檔** - 是否有適當的文檔？

**審查禮儀：**

- 提供建設性回饋
- 解釋「為什麼」而不只是「什麼」
- 區分「必須修改」與「建議修改」
- 適時給予正面回饋

---

## 測試規範

### 測試結構

```rust
// src/feature/mod.rs 或 src/feature/tests.rs

#[cfg(test)]
mod tests {
    use super::*;

    // 測試命名：test_{方法名}_{場景}_{預期結果}

    #[test]
    fn test_register_success() {
        // Arrange
        let manager = ConnectionManager::new();
        let (tx, _rx) = mpsc::channel(32);

        // Act
        let result = manager.register("user-1".into(), vec![], tx);

        // Assert
        assert!(result.is_ok());
        let handle = result.unwrap();
        assert_eq!(handle.user_id, "user-1");
    }

    #[test]
    fn test_register_exceeds_limit_returns_error() {
        // ...
    }

    #[tokio::test]
    async fn test_async_operation() {
        // 非同步測試
    }
}
```

### 測試類型

| 類型 | 位置 | 說明 |
|------|------|------|
| 單元測試 | `src/**/mod.rs` 或 `tests.rs` | 測試單一函數或模組 |
| 整合測試 | `tests/*.rs` | 測試多個模組互動 |
| 負載測試 | `tests/load/*.js` | K6 效能測試 |

### 執行測試

```bash
# 執行所有測試
cargo test

# 執行特定測試
cargo test test_register

# 顯示輸出
cargo test -- --nocapture

# 執行忽略的測試
cargo test -- --ignored

# 測試覆蓋率（需安裝 tarpaulin）
cargo tarpaulin --out Html
```

### Mock 與 Test Doubles

```rust
// 使用 trait 進行依賴注入
#[async_trait]
pub trait NotificationStore {
    async fn save(&self, event: &NotificationEvent) -> Result<()>;
}

// 測試用 mock
pub struct MockStore {
    pub saved: Arc<Mutex<Vec<NotificationEvent>>>,
}

#[async_trait]
impl NotificationStore for MockStore {
    async fn save(&self, event: &NotificationEvent) -> Result<()> {
        self.saved.lock().unwrap().push(event.clone());
        Ok(())
    }
}

#[tokio::test]
async fn test_with_mock_store() {
    let store = Arc::new(MockStore::default());
    let dispatcher = Dispatcher::new(store.clone());

    dispatcher.dispatch(...).await;

    assert_eq!(store.saved.lock().unwrap().len(), 1);
}
```

---

## 文檔規範

### 文檔類型

| 文檔 | 位置 | 用途 |
|------|------|------|
| README.md | 根目錄 | 專案概述、快速開始 |
| CLAUDE.md | 根目錄 | Claude Code 開發指引 |
| CONTRIBUTING.md | 根目錄 | 貢獻指南 |
| CHANGELOG.md | 根目錄 | 版本變更記錄 |
| docs/API.md | docs/ | API 規格文檔 |
| docs/ARCHITECTURE.md | docs/ | 系統架構設計 |
| docs/ROADMAP.md | docs/ | 開發路線圖 |

### Markdown 格式規範

```markdown
# 一級標題（文檔標題）

簡短介紹。

## 二級標題（主要章節）

### 三級標題（子章節）

正文內容。

**粗體** 用於強調關鍵詞。

`程式碼` 用於內嵌程式碼。

```rust
// 程式碼區塊使用適當的語言標記
fn example() {}
```

| 欄位 | 說明 |
|------|------|
| 表格 | 對齊 |

- 項目清單
- 使用連字號

1. 編號清單
2. 使用數字
```

### CHANGELOG 格式

遵循 [Keep a Changelog](https://keepachangelog.com/)：

```markdown
# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added
- 新增訊息佇列系統支援離線推播 (#42)

### Changed
- 改進 Redis 重連策略，使用指數退避 (#38)

### Fixed
- 修復高併發下的連線洩漏問題 (#35)

### Deprecated
- `old_api` 將在 v1.0.0 移除，請使用 `new_api`

### Removed
- 移除已棄用的 `legacy_endpoint`

### Security
- 修復 JWT 驗證繞過漏洞 (CVE-XXXX-XXXX)

## [0.1.0] - 2025-12-27

### Added
- 初始版本發布
- WebSocket 即時推播
- JWT 認證
- Redis Pub/Sub 整合
```

---

## 問題回報

### Issue 模板

**Bug 回報：**

```markdown
## Bug 描述

簡述問題。

## 重現步驟

1. 步驟一
2. 步驟二
3. 發生錯誤

## 預期行為

描述預期應該發生什麼。

## 實際行為

描述實際發生了什麼。

## 環境資訊

- OS: [e.g. Ubuntu 22.04]
- Rust version: [e.g. 1.75.0]
- Redis version: [e.g. 7.2]
- 專案版本: [e.g. 0.1.0]

## 相關日誌

```
貼上相關錯誤日誌
```

## 額外資訊

任何其他有助於診斷問題的資訊。
```

**功能請求：**

```markdown
## 功能描述

簡述想要的功能。

## 動機

為什麼需要這個功能？解決什麼問題？

## 建議方案

如果有的話，描述可能的實作方式。

## 替代方案

是否考慮過其他解決方案？

## 額外資訊

相關連結或參考資料。
```

---

## 聯絡方式

- **Issue Tracker**: GitHub Issues
- **討論區**: GitHub Discussions
- **Email**: team@example.com

---

感謝您的貢獻！
