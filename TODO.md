# Notification Service - TODO List

## 待完成項目

### 1. 配置驗證 (中優先級)

**位置**: `src/config/settings.rs`

**需求**:
在 `Settings::new()` 中加入配置驗證，防止無效配置導致運行時錯誤。

**驗證項目**:
- [ ] JWT_SECRET 長度至少 32 字元
- [ ] Redis URL 格式正確
- [ ] Port 在有效範圍內 (1-65535)
- [ ] 超時設定為正數
- [ ] Backend 設定為有效值 ("memory", "redis", "postgres")

---

### 2. 健康檢查擴展 (中優先級)

**位置**: `src/api/handlers.rs`

**需求**:
擴展 `/health` 端點，包含更多診斷資訊。

**新增欄位**:
- [ ] `postgres: Option<PostgresHealth>` - PostgreSQL 連線狀態
- [ ] `connections: ConnectionHealth` - 當前連線統計
- [ ] `uptime_seconds: u64` - 服務運行時間
- [ ] `queue: QueueHealth` - 佇列狀態
- [ ] `cluster: Option<ClusterHealth>` - 叢集狀態 (如啟用)

---

### 3. API 錯誤類型擴展 (低優先級)

**位置**: `src/error/mod.rs`

**需求**:
增加更精細的錯誤類型以改善 API 回應。

**新增錯誤**:
- [ ] `RateLimitExceeded(String)`
- [ ] `ConnectionLimitExceeded(String)`
- [ ] `Queue(String)`
- [ ] `Timeout(String)`
- [ ] `ClusterError(String)`

---

### 4. 效能優化 (低優先級)

**Dispatcher 批次處理**

**位置**: `src/notification/dispatcher.rs:249-296`

**需求**:
對於大量用戶的通知發送，實作分批處理以減少記憶體壓力。

```rust
const BATCH_SIZE: usize = 100;

pub async fn send_to_users(&self, user_ids: &[String], event: NotificationEvent) -> DeliveryResult {
    for batch in user_ids.chunks(BATCH_SIZE) {
        // 批次處理
    }
}
```

---

**連線管理記憶體優化**

**位置**: `src/connection_manager/manager.rs:16-27`

**需求**:
考慮使用 `SmallVec` 替代 `HashSet<Uuid>` 存儲用戶連線 ID。

```rust
use smallvec::SmallVec;
pub(crate) user_index: DashMap<String, SmallVec<[Uuid; 4]>>,
```

---

## 已完成項目

### Legacy Backend 遷移 ✅

**完成日期**: 2026-01-01

將 `NotificationDispatcher` 從使用 legacy in-memory 實作遷移至可切換的 backend trait objects。

**變更摘要**:
- [x] `src/notification/dispatcher.rs` - 新增 `with_backends()` 方法，使用 `queue_backend` 和 `ack_backend` trait objects
- [x] `src/server/state.rs` - 移除 `message_queue` 和 `ack_tracker` legacy 欄位
- [x] `src/websocket/handler.rs` - 使用 `queue_backend.drain()` 替代 `message_queue.replay()`，使用 `ack_backend` 替代 `ack_tracker`
- [x] `src/sse/handler.rs` - 同樣的 backend trait 遷移
- [x] `src/shutdown/mod.rs` - 更新 `GracefulShutdown` 使用 `queue_backend`
- [x] `src/main.rs` - 更新 `GracefulShutdown` 初始化
- [x] `src/api/handlers.rs` - 更新 metrics 收集使用 backend traits
- [x] `tests/components_integration.rs` - 更新測試使用 `create_queue_backend()` 和 `create_ack_backend()` factory 函數
- [x] 編譯驗證通過 (184 tests passed)

---

### Graceful Shutdown 功能 ✅

- [x] 新增 `ServerMessage::Shutdown` 變體
- [x] 建立 `src/shutdown/mod.rs` 模組
- [x] 實作 `GracefulShutdown` handler
- [x] 整合至 `main.rs`
- [x] 編譯驗證通過

---

## 技術債務追蹤

| 項目 | 位置 | 優先級 | 狀態 |
|-----|------|--------|-----|
| Legacy Queue/ACK 遷移 | `server/state.rs` | 高 | ✅ 已完成 |
| 配置驗證 | `config/settings.rs` | 中 | 待處理 |
| 健康檢查擴展 | `api/handlers.rs` | 中 | 待處理 |
| 錯誤類型擴展 | `error/mod.rs` | 低 | 待處理 |
| Dispatcher 批次處理 | `notification/dispatcher.rs` | 低 | 待處理 |
| SmallVec 優化 | `connection_manager/manager.rs` | 低 | 待處理 |

---

*最後更新: 2026-01-01*
