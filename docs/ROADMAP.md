# 開發路線圖 (Development Roadmap)

本文檔詳細規劃 Ara Notification Service 的進階功能開發計畫，包含優先級、技術規格與實作指引。

## 目錄

- [專案現況評估](#專案現況評估)
- [開發階段規劃](#開發階段規劃)
- [Phase 1: 關鍵基礎設施](#phase-1-關鍵基礎設施)
- [Phase 2: 功能完善](#phase-2-功能完善)
- [Phase 3: 可觀測性與擴展](#phase-3-可觀測性與擴展)
- [Phase 4: 進階功能](#phase-4-進階功能)
- [技術債務清單](#技術債務清單)
- [里程碑與時程](#里程碑與時程)

---

## 專案現況評估

### 功能完成度評分

| 類別 | 分數 | 說明 |
|------|------|------|
| 程式碼組織 | 9/10 | 清晰的模組結構，低耦合度 |
| 錯誤處理 | 9/10 | 完整的 AppError，生產模式遮蔽 |
| 效能 | 8/10 | 無鎖設計，支援 10K 連線 |
| 安全性 | 8/10 | JWT、API Key、CORS、輸入驗證 |
| 文檔 | 10/10 | 完整的架構與 API 文檔 |
| 測試覆蓋 | 6/10 | 有單元測試，缺整合測試與負載測試 |
| 可擴展性 | 7/10 | 支援水平擴展，但 Redis 單點 |
| 可靠性 | 7/10 | Graceful shutdown，但無訊息持久化 |

**總體評分：8.0/10** - 適用於不需訊息持久化的即時推播場景

### 與業界標準比較

| 功能 | Socket.io | Firebase | Pusher | Ara (現況) |
|------|-----------|----------|--------|-----------|
| 即時推播 | ✓ | ✓ | ✓ | ✓ |
| 頻道訂閱 | ✓ (Rooms) | ✓ (Topics) | ✓ | ✓ |
| 離線佇列 | - | ✓ | ✓ | ✓ |
| 送達確認 | - | ✓ | ✓ | ✓ |
| 在線狀態 | ✓ | - | ✓ | ✗ |
| 限流 | - | ✓ | ✓ | ✓ |
| 批次發送 | ✓ | ✓ | ✓ | ✓ |

---

## 開發階段規劃

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           開發路線圖總覽                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Phase 1                Phase 2               Phase 3           Phase 4    │
│  關鍵基礎設施            功能完善               可觀測性           進階功能    │
│  ════════════           ══════════            ══════════         ═══════    │
│                                                                             │
│  ┌─────────────┐       ┌─────────────┐       ┌────────────┐    ┌─────────┐ │
│  │ 訊息佇列     │       │ 批次發送    │       │ Prometheus │    │ 模板系統│ │
│  │ (離線支援)   │       │ API        │       │ 指標匯出   │    │         │ │
│  └─────────────┘       └─────────────┘       └────────────┘    └─────────┘ │
│                                                                             │
│  ┌─────────────┐       ┌─────────────┐       ┌────────────┐    ┌─────────┐ │
│  │ 請求限流     │       │ 頻道資訊    │       │ 負載測試   │    │ 多租戶  │ │
│  │ (Token      │       │ 端點        │       │ 套件      │    │ 支援    │ │
│  │  Bucket)    │       └─────────────┘       └────────────┘    └─────────┘ │
│  └─────────────┘                                                            │
│                        ┌─────────────┐       ┌────────────┐    ┌─────────┐ │
│  ┌─────────────┐       │ 客戶端 ACK  │       │ 分散式追蹤 │    │ SSE     │ │
│  │ Redis 高可用 │       │ 協議        │       │ (OTEL)    │    │ 備援    │ │
│  └─────────────┘       └─────────────┘       └────────────┘    └─────────┘ │
│                                                                             │
│  優先級: CRITICAL       優先級: HIGH          優先級: MEDIUM    優先級: LOW │
│  預估: 2 週             預估: 2 週            預估: 2 週        預估: 2 週  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Phase 1: 關鍵基礎設施

**優先級：CRITICAL | 預估時程：2 週**

### 1.1 訊息佇列系統 (Message Queue) ✅ 已完成

**狀態：已實作** - 完整的離線訊息佇列系統

**實作位置：** `src/queue/mod.rs`

**主要功能：**
- `UserMessageQueue` - 每用戶記憶體佇列，使用 DashMap + VecDeque 實作
- `enqueue()` - 當用戶離線時將訊息加入佇列
- `replay()` - 用戶重新連線時按序重播所有訊息
- `cleanup_expired()` - 清理過期訊息
- FIFO 溢出處理 - 佇列滿時自動丟棄最舊訊息

**配置選項：**

```toml
[queue]
enabled = true
max_size_per_user = 100
message_ttl_seconds = 3600  # 1 小時
cleanup_interval_seconds = 300  # 5 分鐘
```

**環境變數：**

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `QUEUE_ENABLED` | 啟用離線佇列 | `false` |
| `QUEUE_MAX_SIZE_PER_USER` | 每用戶最大佇列 | `100` |
| `QUEUE_MESSAGE_TTL_SECONDS` | 訊息存活時間 (秒) | `3600` |
| `QUEUE_CLEANUP_INTERVAL_SECONDS` | 清理間隔 (秒) | `300` |

**測試案例：** (8 個單元測試)
- [x] 用戶離線後訊息入佇列
- [x] 重新連線後重播所有佇列訊息
- [x] 佇列滿時丟棄最舊訊息 (FIFO)
- [x] TTL 過期訊息自動清理
- [x] 佇列統計資訊
- [x] 多用戶隔離

---

### 1.2 請求限流 (Rate Limiting) ✅ 已完成

**狀態：已實作** - Token Bucket 演算法限流系統

**問題：** 惡意客戶端可發送大量請求耗盡資源

**解決方案：Token Bucket 演算法**

已實作的核心結構：

```rust
// src/ratelimit/mod.rs
pub struct RateLimiter {
    /// IP 層級限流（HTTP 請求）
    ip_buckets: DashMap<IpAddr, BucketEntry>,
    /// API Key 層級限流（HTTP 請求）
    key_buckets: DashMap<String, BucketEntry>,
    /// 配置
    config: RateLimitConfig,
}

pub struct TokenBucket {
    tokens: AtomicU32,
    last_refill: AtomicI64,
    capacity: u32,
    refill_rate: u32,  // tokens per second
}

pub struct RateLimitConfig {
    pub enabled: bool,                    // 是否啟用
    pub http_requests_per_second: u32,    // 預設 100
    pub http_burst_size: u32,             // 預設 200
    pub ws_connections_per_minute: u32,   // 預設 10
    pub ws_messages_per_second: u32,      // 預設 50
    pub cleanup_interval_seconds: u64,    // 預設 60
}
```

**中介層整合：**

```rust
// src/server/middleware.rs
pub async fn rate_limit_middleware(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // 根據 API Key 或 IP 進行限流
    let key = extract_api_key(&req).unwrap_or_else(|| addr.ip().to_string());
    match state.rate_limiter.check_http(&key) {
        RateLimitResult::Allowed { remaining, limit, reset_at } => {
            // 加入 X-RateLimit-* headers
            next.run(req).await
        }
        RateLimitResult::Denied { retry_after, limit, reset_at } => {
            // 回傳 429 Too Many Requests
        }
    }
}

pub async fn ws_rate_limit_middleware(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // WebSocket 連線限流（每 IP）
    match state.rate_limiter.check_ws_connection(addr.ip()) {
        RateLimitResult::Allowed { .. } => next.run(req).await,
        RateLimitResult::Denied { retry_after, .. } => // 429
    }
}
```

**回應格式：**

```http
HTTP/1.1 429 Too Many Requests
Retry-After: 1
X-RateLimit-Limit: 100
X-RateLimit-Remaining: 0
X-RateLimit-Reset: 1735300000
Content-Type: application/json

{
  "error": {
    "code": "RATE_LIMITED",
    "message": "Too many requests, please retry after 1 seconds"
  }
}
```

**測試覆蓋：** 9 個單元測試

---

### 1.3 Redis 高可用 ✅ 已完成

**狀態：已實作** - 熔斷器模式與指數退避重連

**問題：** Redis 單點故障導致訊息中斷

**解決方案：**

已實作的核心結構：

```rust
// src/redis/mod.rs
pub struct CircuitBreaker {
    state: AtomicU8,  // 0=Closed, 1=Open, 2=HalfOpen
    failure_count: AtomicU32,
    success_count: AtomicU32,
    last_state_change: AtomicI64,
    config: CircuitBreakerConfig,
}

pub struct ExponentialBackoff {
    config: BackoffConfig,
    current_delay_ms: u64,
    attempt: u32,
}

pub struct RedisHealth {
    status: AtomicU8,  // Healthy, Reconnecting, CircuitOpen
    reconnection_attempts: AtomicU32,
    total_reconnections: AtomicU32,
}
```

**配置選項：**

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `REDIS_CIRCUIT_BREAKER_FAILURE_THRESHOLD` | 開啟熔斷器的失敗次數 | `5` |
| `REDIS_CIRCUIT_BREAKER_SUCCESS_THRESHOLD` | 關閉熔斷器的成功次數 | `2` |
| `REDIS_CIRCUIT_BREAKER_RESET_TIMEOUT_SECONDS` | 熔斷器重置超時 | `30` |
| `REDIS_BACKOFF_INITIAL_DELAY_MS` | 初始退避延遲 | `100` |
| `REDIS_BACKOFF_MAX_DELAY_MS` | 最大退避延遲 | `30000` |

**健康檢查端點：**

```json
// GET /health
{
  "status": "healthy",
  "version": "0.1.0",
  "redis": {
    "status": "healthy",
    "connected": true
  }
}

// GET /stats (redis section)
{
  "redis": {
    "status": "healthy",
    "connected": true,
    "circuit_breaker_state": "closed",
    "circuit_breaker_failures": 0,
    "reconnection_attempts": 0,
    "total_reconnections": 1
  }
}
```

**測試覆蓋：** 11 個單元測試

---

## Phase 2: 功能完善

**優先級：HIGH | 預估時程：2 週**

### 2.1 批次發送 API ✅ 已完成

**狀態：已實作** - 批次通知發送端點

**實作位置：** `src/triggers/http.rs`

**端點：** `POST /api/v1/notifications/batch`

**主要功能：**
- 單次請求發送最多 100 筆通知
- 支援所有目標類型 (user, users, broadcast, channel, channels)
- `stop_on_error` 選項 - 遇到錯誤時停止處理
- `deduplicate` 選項 - 跳過重複的 target+event_type 組合
- 每筆通知獨立結果，批次統計摘要

**請求格式：**

```json
{
  "notifications": [
    {
      "target": { "type": "user", "value": "user-1" },
      "event_type": "order.shipped",
      "payload": { "order_id": "ORD-001" },
      "priority": "High"
    },
    {
      "target": { "type": "channel", "value": "orders" },
      "event_type": "stock.low",
      "payload": { "product_id": "SKU-123" }
    }
  ],
  "options": {
    "stop_on_error": false,
    "deduplicate": true
  }
}
```

**回應格式：**

```json
{
  "batch_id": "batch-550e8400-e29b-41d4-a716-446655440000",
  "results": [
    {
      "index": 0,
      "notification_id": "...",
      "delivered_to": 2,
      "failed": 0,
      "success": true
    },
    {
      "index": 1,
      "notification_id": "...",
      "delivered_to": 45,
      "failed": 1,
      "success": true
    }
  ],
  "summary": {
    "total": 2,
    "succeeded": 2,
    "failed": 0,
    "total_delivered": 47
  }
}
```

**限制：**
- 最大 100 筆/批次
- 總 payload 大小 ≤ 1MB
- 支援 partial success（部分成功）

**測試覆蓋：** 13 個單元測試

---

### 2.2 頻道資訊端點 ✅ 已完成

**狀態：已實作** - 頻道與使用者訂閱查詢端點

**實作位置：** `src/api/handlers.rs`, `src/connection_manager/registry.rs`

**主要功能：**
- 列出所有頻道與訂閱數
- 查詢特定頻道詳情
- 查詢使用者的訂閱列表（跨連線彙整）
- 404 錯誤回應（頻道不存在或使用者未連線）

**GET /api/v1/channels**

```json
{
  "channels": [
    { "name": "orders", "subscriber_count": 45 },
    { "name": "system-alerts", "subscriber_count": 150 },
    { "name": "admin", "subscriber_count": 5 }
  ],
  "total_channels": 3
}
```

**GET /api/v1/channels/{name}**

```json
{
  "name": "orders",
  "subscriber_count": 45
}
```

**GET /api/v1/users/{user_id}/subscriptions**

```json
{
  "user_id": "user-123",
  "connection_count": 2,
  "subscriptions": ["orders", "system-alerts"]
}
```

**測試覆蓋：** 10 個單元測試

---

### 2.3 客戶端 ACK 協議 ✅ 已完成

**狀態：已實作** - 通知送達確認與追蹤系統

**實作位置：** `src/notification/ack.rs`, `src/websocket/message.rs`, `src/websocket/handler.rs`

**協議設計：**

```
Server → Client: notification (id: "abc-123")
Client → Server: { "type": "Ack", "payload": { "notification_id": "abc-123" } }
Server → Client: { "type": "acked", "notification_id": "abc-123" }
Server: 標記 notification 為已確認，計算延遲
```

**主要功能：**
- `AckTracker` - 追蹤待確認通知，使用 DashMap 儲存
- `track()` - 當通知發送成功時追蹤 (由 Dispatcher 自動調用)
- `acknowledge()` - 處理客戶端 ACK，驗證 user_id 匹配
- `cleanup_expired()` - 清理超時未確認的通知
- 延遲計算 - 從發送到 ACK 的平均延遲

**客戶端訊息格式：**

```json
{
  "type": "Ack",
  "payload": {
    "notification_id": "abc-123"
  }
}
```

**伺服器確認訊息：**

```json
{
  "type": "acked",
  "notification_id": "abc-123"
}
```

**配置選項：**

```toml
[ack]
enabled = true
timeout_seconds = 30
cleanup_interval_seconds = 60
```

**環境變數：**

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `ACK_ENABLED` | 啟用 ACK 追蹤 | `false` |
| `ACK_TIMEOUT_SECONDS` | ACK 超時時間 (秒) | `30` |
| `ACK_CLEANUP_INTERVAL_SECONDS` | 清理間隔 (秒) | `60` |

**統計端點擴展 (GET /stats)：**

```json
{
  "ack": {
    "enabled": true,
    "total_tracked": 10250,
    "total_acked": 10100,
    "total_expired": 50,
    "pending_count": 100,
    "ack_rate": 0.9854,
    "avg_latency_ms": 45
  }
}
```

**測試覆蓋：** 10 個單元測試
- [x] 追蹤通知 (enabled/disabled)
- [x] 確認成功
- [x] 確認失敗 (user_id 不匹配)
- [x] 確認未知通知
- [x] 多用戶隔離
- [x] 過期清理
- [x] ACK rate 計算
- [x] 平均延遲計算

---

## Phase 3: 可觀測性與擴展

**優先級：MEDIUM | 預估時程：2 週**

### 3.1 Prometheus 指標匯出 ✅ 已完成

**狀態：已實作** - 完整的 Prometheus 指標匯出系統

**實作位置：** `src/metrics/mod.rs`, `src/api/handlers.rs`

**端點：** `GET /metrics`

**主要功能：**
- 連線指標（總連線數、唯一用戶數、每用戶連線分布）
- 訊息指標（發送數量、投遞成功/失敗、按目標類型分類）
- 延遲指標（訊息投遞延遲、ACK 延遲）
- Redis 指標（連線狀態、熔斷器狀態）
- 佇列指標（佇列大小、入隊/重播/過期數量）
- 限流指標（允許/拒絕請求數）
- WebSocket 指標（連線開/關、訊息類型、連線持續時間）
- 批次 API 指標（請求數、批次大小）
- ACK 指標（追蹤數、確認數、過期數）

**指標清單：**

```prometheus
# 連線指標
ara_connections_total                    # 當前連線總數 (gauge)
ara_users_connected                      # 唯一用戶數 (gauge)
ara_connections_per_user                 # 每用戶連線分布 (histogram)
ara_channel_subscriptions{channel}       # 每頻道訂閱數 (gauge)
ara_channels_active                      # 活躍頻道數 (gauge)

# 訊息指標
ara_messages_sent_total{target}          # 發送訊息數 (counter, labels: user/users/broadcast/channel/channels)
ara_messages_delivered_total             # 投遞成功數 (counter)
ara_messages_failed_total                # 投遞失敗數 (counter)
ara_message_delivery_latency_seconds     # 投遞延遲 (histogram)

# Redis 指標
ara_redis_connection_status              # 連線狀態 (gauge, 1=connected, 0=disconnected)
ara_redis_circuit_breaker_state          # 熔斷器狀態 (gauge, 0=closed, 1=open, 2=half-open)
ara_redis_reconnections_total            # 重連次數 (counter)
ara_redis_messages_received_total        # 接收訊息數 (counter)

# 佇列指標
ara_queue_size_total                     # 佇列總大小 (gauge)
ara_queue_users_total                    # 有佇列的用戶數 (gauge)
ara_queue_enqueued_total                 # 入隊訊息數 (counter)
ara_queue_replayed_total                 # 重播訊息數 (counter)
ara_queue_expired_total                  # 過期訊息數 (counter)
ara_queue_dropped_total                  # 丟棄訊息數 (counter)

# 限流指標
ara_ratelimit_allowed_total{type}        # 允許請求數 (counter, labels: http/ws)
ara_ratelimit_denied_total{type}         # 拒絕請求數 (counter, labels: http/ws)

# ACK 指標
ara_ack_tracked_total                    # 追蹤通知數 (counter)
ara_ack_received_total                   # 確認數 (counter)
ara_ack_expired_total                    # 過期數 (counter)
ara_ack_pending                          # 待確認數 (gauge)
ara_ack_latency_seconds                  # ACK 延遲 (histogram)

# WebSocket 指標
ara_ws_connections_opened_total          # 開啟連線數 (counter)
ara_ws_connections_closed_total          # 關閉連線數 (counter)
ara_ws_messages_received_total{type}     # 接收訊息數 (counter, labels: subscribe/unsubscribe/ping/ack)
ara_ws_connection_duration_seconds       # 連線持續時間 (histogram)

# 批次 API 指標
ara_batch_requests_total                 # 批次請求數 (counter)
ara_batch_size                           # 批次大小分布 (histogram)

# HTTP 指標
ara_http_requests_total{method,path,status}  # HTTP 請求數 (counter)
ara_http_request_latency_seconds{method,path} # HTTP 延遲 (histogram)
```

**Cargo 依賴：**

```toml
[dependencies]
prometheus = "0.13"
lazy_static = "1.5"
```

**整合元件：**
- NotificationDispatcher - 訊息發送/投遞指標
- WebSocket Handler - 連線開關、訊息類型、持續時間
- UserMessageQueue - 入隊、重播、過期、丟棄
- RateLimiter 中介層 - 允許/拒絕請求
- AckTracker - 追蹤、確認、過期

**測試覆蓋：** 8 個單元測試

---

### 3.2 負載測試套件 ✅ 已完成

**狀態：已實作** - 完整的 K6 負載測試套件

**實作位置：** `tests/load/`

**測試腳本：**

| 腳本 | 用途 | 說明 |
|------|------|------|
| `websocket.js` | WebSocket 連線測試 | 測試 WS 連線建立、訂閱、訊息接收 |
| `http-api.js` | HTTP API 測試 | 測試通知發送 API 吞吐量與延遲 |
| `batch-api.js` | 批次 API 測試 | 測試批次發送端點效能 |
| `e2e-load.js` | 端對端測試 | 結合 WS 與 HTTP 的完整流程測試 |

**預定義 Profile：**

| Profile | 連線數 | 請求/秒 | 持續時間 | 用途 |
|---------|--------|---------|----------|------|
| smoke | 10 | 10 | 30s | 快速驗證 |
| baseline | 100 | 50 | 2m | 基準效能 |
| medium | 500 | 100 | 3m | 尖峰時段模擬 |
| high | 1000 | 200 | 5m | 高流量測試 |
| stress | 2000 | 500 | 5m | 找出極限 |
| soak | 500 | 50 | 30m | 長時間穩定性 |
| spike | 100 | 1000 | 1m | 流量突增測試 |

**效能閾值：**

| 指標 | 目標值 | 說明 |
|------|--------|------|
| `connection_success_rate` | > 95% | WebSocket 連線成功率 |
| `request_success_rate` | > 99% | HTTP 請求成功率 |
| `message_latency_ms` | p95 < 100ms | 訊息延遲 |
| `e2e_latency_ms` | p95 < 200ms | 端對端延遲 |

**執行方式：**

```bash
# 安裝 K6
brew install k6  # macOS
choco install k6 # Windows

# 執行測試
k6 run --env HOST=localhost:8081 --env JWT_TOKEN=<token> tests/load/websocket.js

# 使用 Profile
k6 run --env PROFILE=high tests/load/e2e-load.js

# 使用執行腳本
./tests/load/run-tests.sh websocket baseline  # Linux/macOS
tests\load\run-tests.bat websocket baseline   # Windows
```

**輔助工具：**
- `config.js` - 共享配置與 Profile 定義
- `utils/jwt-generator.js` - JWT 生成工具
- `run-tests.sh` / `run-tests.bat` - 執行腳本

**文檔：** `tests/load/README.md`

---

### 3.3 分散式追蹤 (OpenTelemetry) ✅ 已完成

**狀態：已實作** - 完整的 OpenTelemetry 分散式追蹤整合

**實作位置：** `src/telemetry/mod.rs`

**主要功能：**
- OTLP gRPC exporter 支援（相容 Jaeger, Tempo, Zipkin）
- 與 `tracing` crate 無縫整合
- 可配置的取樣比率
- 自動 span 建立與傳播
- 完整的 span 屬性記錄

**追蹤範圍：**

```
HTTP Request
    └── span: http.send_notification (target_user_id, event_type, priority)
    └── span: http.broadcast (event_type, priority, audience)
    └── span: http.batch_send (batch_size, stop_on_error, deduplicate)
         └── span: dispatcher.dispatch (notification_id, event_type, target_type)
              └── span: dispatcher.send_to_user / send_to_channel / broadcast

WebSocket Connection
    └── span: ws.connection (user_id)
         └── span: ws.message (connection_id, user_id, message_type)
              └── span: ws.subscribe (connection_id, channel_count)
              └── span: ws.unsubscribe (connection_id, channel_count)
              └── span: ws.ack (connection_id, user_id, notification_id)
```

**環境變數：**

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `OTEL_ENABLED` | 啟用 OTEL 匯出 | `false` |
| `OTEL_ENDPOINT` | OTLP gRPC 端點 | `http://localhost:4317` |
| `OTEL_SERVICE_NAME` | 服務名稱 | `ara-notification-service` |
| `OTEL_SAMPLING_RATIO` | 取樣比率 (0.0-1.0) | `1.0` |

**Cargo 依賴：**

```toml
opentelemetry = "0.27"
opentelemetry_sdk = { version = "0.27", features = ["rt-tokio"] }
opentelemetry-otlp = { version = "0.27", features = ["tonic"] }
opentelemetry-semantic-conventions = "0.27"
tracing-opentelemetry = "0.28"
```

**測試覆蓋：** 3 個單元測試
- [x] 預設配置
- [x] 屬性建立
- [x] TelemetryGuard 生命週期

---

## Phase 4: 進階功能

**優先級：LOW | 預估時程：2 週**

### 4.1 通知模板系統 ✅ 已完成

**狀態：已實作** - 完整的模板 CRUD 與變數替換系統

**實作位置：** `src/template/mod.rs`, `src/api/handlers.rs`, `src/triggers/http.rs`

**端點：**

```
POST   /api/v1/templates          # 建立模板
GET    /api/v1/templates          # 列出模板
GET    /api/v1/templates/{id}     # 取得模板
PUT    /api/v1/templates/{id}     # 更新模板
DELETE /api/v1/templates/{id}     # 刪除模板
```

**模板格式：**

```json
{
  "id": "order-shipped",
  "name": "訂單出貨通知",
  "event_type": "order.shipped",
  "payload_template": {
    "title": "您的訂單已出貨",
    "body": "訂單 {{order_id}} 已由 {{carrier}} 配送，追蹤碼：{{tracking_number}}",
    "action_url": "/orders/{{order_id}}"
  },
  "default_priority": "High",
  "default_ttl": 86400
}
```

**主要功能：**
- `TemplateStore` - 記憶體模板儲存，使用 DashMap 實作
- `substitute_variables()` - 變數替換引擎，支援 `{{variable}}` 語法
- 支援巢狀 JSON 物件與陣列中的變數替換
- 模板 ID 驗證 (1-64 字元，英數字、dash、underscore)
- 自動時間戳記 (created_at, updated_at)

**使用方式 - 直接內容：**

```json
POST /api/v1/notifications/send
{
  "target_user_id": "user-123",
  "event_type": "order.shipped",
  "payload": {
    "order_id": "ORD-456",
    "carrier": "黑貓宅急便"
  }
}
```

**使用方式 - 模板：**

```json
POST /api/v1/notifications/send
{
  "target_user_id": "user-123",
  "template_id": "order-shipped",
  "variables": {
    "order_id": "ORD-456",
    "carrier": "黑貓宅急便",
    "tracking_number": "TW123456789"
  }
}
```

**批次 API 同樣支援模板：**

```json
POST /api/v1/notifications/batch
{
  "notifications": [
    {
      "target": { "type": "user", "value": "user-1" },
      "template_id": "order-shipped",
      "variables": { "order_id": "ORD-001", "carrier": "FedEx" }
    },
    {
      "target": { "type": "channel", "value": "orders" },
      "event_type": "stock.low",
      "payload": { "product_id": "SKU-123" }
    }
  ]
}
```

**測試覆蓋：** 15 個單元測試
- [x] 模板 CRUD 操作
- [x] 重複 ID 驗證
- [x] ID 格式驗證
- [x] 簡單變數替換
- [x] 多重變數替換
- [x] 巢狀物件替換
- [x] 陣列內替換
- [x] 數值變數替換
- [x] 批次 API 模板解析

---

### 4.2 多租戶支援 ✅ 已完成

**狀態：已實作** - 完整的多租戶隔離與統計系統

**實作位置：** `src/tenant/mod.rs`, `src/auth/claims.rs`, `src/connection_manager/registry.rs`

**主要功能：**
- JWT `tenant_id` 欄位擴展（可選，預設為 "default"）
- `TenantContext` - 頻道命名空間隔離（格式：`{tenant_id}:{channel_name}`）
- `TenantManager` - 租戶狀態管理、連線限制、統計追蹤
- `TenantStats` - 每租戶統計（活躍連線、總連線、訊息發送/投遞）
- 每租戶連線限制覆寫機制
- API 端點（列出租戶、查詢租戶統計）

**JWT 擴展：**

```json
{
  "sub": "user-123",
  "tenant_id": "acme-corp",
  "exp": 1735200000,
  "roles": ["user"]
}
```

**隔離策略：**

```rust
// src/tenant/mod.rs
pub struct TenantContext {
    pub tenant_id: String,
    pub is_default: bool,
}

impl TenantContext {
    /// 頻道命名空間：{tenant_id}:{channel_name}
    pub fn namespace_channel(&self, channel: &str) -> String;

    /// 解析命名空間頻道名稱
    pub fn extract_channel_name(&self, namespaced_channel: &str) -> Option<String>;
}

pub struct TenantManager {
    /// 取得租戶連線限制（支援覆寫）
    pub fn get_limits(&self, tenant_id: &str) -> ConnectionLimits;

    /// 建立租戶上下文
    pub fn create_context(&self, tenant_id: &str) -> TenantContext;

    /// 取得租戶統計
    pub fn get_stats(&self, tenant_id: &str) -> TenantStatsSnapshot;

    /// 列出活躍租戶
    pub fn list_active_tenants(&self) -> Vec<TenantInfo>;
}
```

**配置選項：**

```toml
[tenant]
enabled = true  # 啟用多租戶模式

[tenant.default_limits]
max_connections = 1000
max_connections_per_user = 5
max_subscriptions_per_connection = 50

[tenant.tenant_overrides.premium]
max_connections = 5000
max_connections_per_user = 10
max_subscriptions_per_connection = 100
```

**環境變數：**

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `TENANT_ENABLED` | 啟用多租戶模式 | `false` |
| `TENANT_DEFAULT_LIMITS_MAX_CONNECTIONS` | 預設租戶最大連線 | `1000` |
| `TENANT_DEFAULT_LIMITS_MAX_CONNECTIONS_PER_USER` | 預設每用戶最大連線 | `5` |
| `TENANT_DEFAULT_LIMITS_MAX_SUBSCRIPTIONS_PER_CONNECTION` | 預設每連線最大訂閱 | `50` |

**API 端點：**

**GET /api/v1/tenants** - 列出活躍租戶

```json
{
  "enabled": true,
  "tenants": [
    {
      "tenant_id": "acme-corp",
      "stats": {
        "active_connections": 150,
        "total_connections": 1250,
        "messages_sent": 5000,
        "messages_delivered": 48500
      }
    }
  ],
  "total": 1
}
```

**GET /api/v1/tenants/{tenant_id}** - 查詢租戶統計

```json
{
  "tenant_id": "acme-corp",
  "stats": {
    "active_connections": 150,
    "total_connections": 1250,
    "messages_sent": 5000,
    "messages_delivered": 48500
  },
  "connection_stats": {
    "total_connections": 150,
    "unique_users": 75
  }
}
```

**測試覆蓋：** 12 個單元測試
- [x] 租戶上下文建立（預設/自訂）
- [x] 頻道命名空間（預設/自訂租戶）
- [x] 頻道名稱解析
- [x] 租戶管理器（啟用/停用）
- [x] 租戶限制覆寫
- [x] 租戶統計追蹤
- [x] 活躍租戶列表

---

### 4.3 SSE 備援 ✅ 已完成

**狀態：已實作** - Server-Sent Events 作為 WebSocket 的備援方案

**實作位置：** `src/sse/mod.rs`, `src/sse/handler.rs`

**端點：** `GET /sse?token=JWT`

或使用 Authorization Header：
```
GET /sse
Authorization: Bearer <JWT_TOKEN>
```

**主要功能：**
- JWT 認證（查詢參數或 Authorization header）
- 與 WebSocket 共用 ConnectionManager（相同連線管理）
- 單向通知串流（伺服器到客戶端）
- 心跳保活（使用 SSE keep-alive）
- 離線佇列訊息自動重播
- 連線指標追蹤（共用 WebSocket 指標）

**事件格式：**

```
event: connected
data: {"type":"connected","connection_id":"550e8400-e29b-41d4-a716-446655440000"}

event: notification
data: {"type":"notification","id":"...","event_type":"order.created",...}

event: heartbeat
data: {"type":"heartbeat"}

event: error
data: {"code":"...","message":"..."}
```

**事件類型：**

| 事件 | 說明 |
|------|------|
| `connected` | 連線建立確認，包含 connection_id |
| `notification` | 通知事件，包含完整通知內容 |
| `heartbeat` | 心跳保活事件 |
| `message` | 其他訊息類型 |
| `error` | 錯誤事件 |

**適用場景：**
- 瀏覽器不支援 WebSocket
- 防火牆阻擋 WebSocket 升級
- 單向通知（不需客戶端發送訊息）
- 簡化客戶端實作（EventSource API）

**限制：**
- 僅支援伺服器到客戶端的單向通訊
- 不支援頻道訂閱/取消訂閱（連線時訂閱所有可用頻道）
- 不支援客戶端 ACK（因無法接收客戶端訊息）

**測試覆蓋：** 5 個單元測試
- [x] SSE 事件序列化
- [x] Token 從查詢參數提取
- [x] Token 從 Header 提取
- [x] 查詢參數優先順序
- [x] 無 Token 處理

---

## 技術債務清單

| 項目 | 優先級 | 工作量 | 說明 |
|------|--------|--------|------|
| 心跳抖動 | LOW | 小 | 加入隨機偏移避免 CPU 尖峰 |
| Redis 指數退避 | MEDIUM | 中 | 改進重連策略 |
| 統計原子性 | MEDIUM | 小 | 使用 atomic batch 或 Mutex |
| 序列化錯誤計數 | LOW | 小 | 記錄 pre-serialization fallback 次數 |
| 活動時間 fallback 日誌 | LOW | 極小 | DateTime 解析失敗時記錄 |

---

## 里程碑與時程

```
2025 Q1
═══════════════════════════════════════════════════════════════════════════════

Week 1-2: Phase 1 - 關鍵基礎設施
├── [1.1] 訊息佇列系統
├── [1.2] 請求限流
└── [1.3] Redis 高可用

Week 3-4: Phase 2 - 功能完善
├── [2.1] 批次發送 API
├── [2.2] 頻道資訊端點
└── [2.3] 客戶端 ACK 協議

Week 5-6: Phase 3 - 可觀測性
├── [3.1] Prometheus 指標
├── [3.2] 負載測試套件
└── [3.3] OpenTelemetry 整合

Week 7-8: Phase 4 - 進階功能
├── [4.1] 通知模板系統
├── [4.2] 多租戶支援
└── [4.3] SSE 備援

═══════════════════════════════════════════════════════════════════════════════

版本規劃：
  v0.2.0 - Phase 1 完成 (訊息佇列 + 限流 + Redis HA)
  v0.3.0 - Phase 2 完成 (批次 API + 頻道資訊 + ACK)
  v0.4.0 - Phase 3 完成 (Prometheus + 負載測試 + OTEL)
  v1.0.0 - Phase 4 完成 (模板 + 多租戶 + SSE) - 正式版
```

---

## 附錄：評估檢查清單

### 功能上線前檢查

- [ ] 單元測試覆蓋率 > 80%
- [ ] 整合測試通過
- [ ] 負載測試達標（10K 連線 < 100ms 延遲）
- [ ] 文檔更新（API.md, ARCHITECTURE.md）
- [ ] CHANGELOG 更新
- [ ] 向下相容性確認

### 安全審查檢查

- [ ] 輸入驗證完整
- [ ] 錯誤訊息無敏感資訊洩漏
- [ ] 限流機制正常運作
- [ ] JWT 驗證無繞過
- [ ] API Key 無硬編碼

---

*最後更新：2025-12-27*
