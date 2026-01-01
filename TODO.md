# Notification Service - TODO List

## 待完成項目

*目前無待處理項目*

---

## 已完成項目

### 配置驗證 ✅

**完成日期**: 2026-01-01

**位置**: `src/config/settings.rs`

**變更摘要**:
- [x] JWT_SECRET 長度至少 32 字元
- [x] Redis URL 格式正確 (redis:// 或 rediss://)
- [x] Port 在有效範圍內 (1-65535)
- [x] 超時設定為正數
- [x] Backend 設定為有效值 ("memory", "redis", "postgres")
- [x] OTEL 採樣率在 0.0-1.0 範圍內
- [x] 資料庫連線池大小 > 0
- [x] 新增 `validate()` 方法並整合至 `Settings::new()`
- [x] 新增 12 個驗證相關測試

---

### 健康檢查擴展 ✅

**完成日期**: 2026-01-01

**位置**: `src/api/handlers.rs`, `src/server/state.rs`

**變更摘要**:
- [x] `postgres: Option<PostgresHealth>` - PostgreSQL 連線狀態
- [x] `connections: ConnectionHealth` - 當前連線統計
- [x] `uptime_seconds: u64` - 服務運行時間 (透過 `AppState.start_time`)
- [x] `queue: QueueHealth` - 佇列狀態
- [x] `cluster: Option<ClusterHealth>` - 叢集狀態 (如啟用)

---

### API 錯誤類型擴展 ✅

**完成日期**: 2026-01-01

**位置**: `src/error/mod.rs`

**新增錯誤類型**:
- [x] `RateLimitExceeded(String)` - HTTP 429
- [x] `ConnectionLimitExceeded(String)` - HTTP 429
- [x] `Queue(String)` - HTTP 503
- [x] `Timeout(String)` - HTTP 504
- [x] `ClusterError(String)` - HTTP 503

---

### 效能優化 ✅

**完成日期**: 2026-01-01

**Dispatcher 批次處理**

**位置**: `src/notification/dispatcher.rs`

**變更摘要**:
- [x] 新增 `USER_BATCH_SIZE = 100` 常數
- [x] `send_to_users()` 使用 `chunks()` 分批處理用戶
- [x] 每批次收集所有連線後統一發送
- [x] 離線用戶訊息批次排隊

**連線管理記憶體優化**

**位置**: `src/connection_manager/manager.rs`

**變更摘要**:
- [x] 新增 `smallvec` 依賴
- [x] `user_index` 使用 `SmallVec<[Uuid; 4]>` 替代 `HashSet<Uuid>`
- [x] 大多數用戶 (1-4 連線) 避免堆積分配
- [x] 更新 `register()` 使用 `push()`
- [x] 更新 `unregister()` 使用 `retain()`

---

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
| 配置驗證 | `config/settings.rs` | 中 | ✅ 已完成 |
| 健康檢查擴展 | `api/handlers.rs` | 中 | ✅ 已完成 |
| 錯誤類型擴展 | `error/mod.rs` | 低 | ✅ 已完成 |
| Dispatcher 批次處理 | `notification/dispatcher.rs` | 低 | ✅ 已完成 |
| SmallVec 優化 | `connection_manager/manager.rs` | 低 | ✅ 已完成 |

---

*最後更新: 2026-01-01*
