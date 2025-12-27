# 系統架構設計

本文檔詳細說明 Ara Notification Service v1.0.0 的內部架構設計、核心元件、資料流程與併發機制。

## 目錄

- [概覽](#概覽)
- [核心元件](#核心元件)
- [安全機制](#安全機制)
- [資料流程](#資料流程)
- [進階功能](#進階功能)
- [背景任務](#背景任務)
- [併發設計](#併發設計)
- [配置載入](#配置載入)
- [可觀測性](#可觀測性)
- [限制與擴展點](#限制與擴展點)

---

## 概覽

### 技術棧

| 層級 | 技術 | 版本 | 用途 |
|------|------|------|------|
| Web 框架 | Axum | 0.8 | HTTP/WebSocket/SSE 路由與中介層 |
| 非同步執行 | Tokio | 1.x | 多執行緒非同步 Runtime |
| 序列化 | Serde | 1.0 | JSON 序列化/反序列化 |
| JWT 驗證 | jsonwebtoken | 9.x | JWT Token 驗證 |
| Redis 客戶端 | redis-rs | 0.27 | Pub/Sub 訂閱、高可用連線 |
| 併發集合 | DashMap | 6.x | 無鎖並行 HashMap |
| 日誌追蹤 | tracing | 0.1 | 結構化日誌 |
| 配置管理 | config-rs | 0.14 | 多來源配置合併 |
| 監控指標 | prometheus | 0.13 | Prometheus 指標匯出 |
| 分散式追蹤 | opentelemetry | 0.27 | OTLP gRPC 匯出 |

### 設計原則

1. **完全無狀態** - 所有連線狀態僅存於記憶體，服務重啟後狀態清空
2. **水平擴展** - 多實例部署時搭配 Redis Pub/Sub 實現跨節點通知
3. **離線支援** - 可選的離線訊息佇列確保重連後訊息不遺失
4. **背壓控制** - MPSC Channel 緩衝機制防止慢速客戶端拖累系統
5. **多租戶隔離** - 頻道命名空間確保租戶間資料隔離
6. **優雅降級** - Redis 熔斷器確保 Redis 故障時服務持續運作

### 高層架構圖

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              觸發來源                                        │
│   ┌────────────────┐          ┌────────────────┐          ┌──────────────┐  │
│   │ Symfony/其他   │          │  其他微服務     │          │ Redis CLI    │  │
│   │   Backend      │          │                │          │              │  │
│   └───────┬────────┘          └───────┬────────┘          └──────┬───────┘  │
│           │                           │                          │          │
│           ▼                           ▼                          ▼          │
│   ┌────────────────┐          ┌────────────────┐          ┌──────────────┐  │
│   │ HTTP REST API  │          │ HTTP Batch API │          │Redis Pub/Sub │  │
│   └───────┬────────┘          └───────┬────────┘          └──────┬───────┘  │
└───────────┼───────────────────────────┼──────────────────────────┼──────────┘
            │                           │                          │
            └───────────────────────────┼──────────────────────────┘
                                        │
                                        ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                        Notification Service                                  │
│                                                                              │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │                              AppState                                   │ │
│  │  ┌──────────────┐ ┌───────────────┐ ┌────────────────┐ ┌─────────────┐ │ │
│  │  │ JwtValidator │ │ TenantManager │ │ TemplateStore  │ │ RateLimiter │ │ │
│  │  └──────────────┘ └───────────────┘ └────────────────┘ └─────────────┘ │ │
│  │  ┌──────────────┐ ┌───────────────┐ ┌────────────────┐ ┌─────────────┐ │ │
│  │  │  AckTracker  │ │MessageQueue   │ │  RedisHealth   │ │  Settings   │ │ │
│  │  └──────────────┘ └───────────────┘ └────────────────┘ └─────────────┘ │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
│                                        │                                     │
│                                        ▼                                     │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │                      NotificationDispatcher                             │ │
│  │                                                                         │ │
│  │    User(s)  ──┬──  Broadcast  ──┬──  Channel(s)  ──┬──  Template      │ │
│  │               │                 │                  │       │           │ │
│  │               └─────────────────┼──────────────────┘       │           │ │
│  │                                 │                          ▼           │ │
│  │                                 │              TemplateStore.render()  │ │
│  │                                 │                                      │ │
│  └─────────────────────────────────┼──────────────────────────────────────┘ │
│                                    │                                         │
│                                    ▼                                         │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │                        ConnectionManager                                │ │
│  │  ┌─────────────────┬─────────────────┬──────────────┬───────────────┐  │ │
│  │  │   connections   │    user_index   │channel_index │ tenant_index  │  │ │
│  │  │ DashMap<Uuid,   │ DashMap<String, │DashMap<Str,  │DashMap<Str,   │  │ │
│  │  │ ConnectionHdl>  │ HashSet<Uuid>>  │HashSet<Uuid>>│HashSet<Uuid>> │  │ │
│  │  └────────┬────────┴────────┬────────┴───────┬──────┴───────────────┘  │ │
│  └───────────┼─────────────────┼────────────────┼─────────────────────────┘ │
│              │                 │                │                            │
│              │      ┌──────────┴────────────────┘                            │
│              │      │                                                        │
│              ▼      ▼                                                        │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │                    ConnectionHandle (每連線)                             │ │
│  │  ┌────────┬──────────┬───────────┬─────────────────┬──────────────┐    │ │
│  │  │   id   │ user_id  │ tenant_id │ mpsc::Sender    │  RwLock<>    │    │ │
│  │  │ (Uuid) │ (String) │ (String)  │ <OutboundMsg>   │subscriptions │    │ │
│  │  └────────┴──────────┴───────────┴────────┬────────┴──────────────┘    │ │
│  └───────────────────────────────────────────┼────────────────────────────┘ │
│                                              │                               │
│      ┌───────────────────────────────────────┴───────────────────────────┐  │
│      │                                                                   │  │
│      ▼                                                                   ▼  │
│  ┌───────────────────┐                                    ┌───────────────┐ │
│  │  WebSocket 連線    │                                    │  SSE 連線      │ │
│  │  (雙向通訊)        │                                    │  (單向推播)    │ │
│  └────────┬──────────┘                                    └───────┬───────┘ │
└───────────┼───────────────────────────────────────────────────────┼─────────┘
            │                                                       │
            ▼                                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                          客戶端                                              │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐      │
│  │ 前端 App │  │ 管理後台 │  │ 行動裝置 │  │ IOT 裝置 │  │ 其他服務 │      │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘  └──────────┘      │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 核心元件

### AppState

應用程式共享狀態容器，使用 `Arc` 實現跨執行緒共享。

**檔案位置：** `src/server/state.rs`

```rust
pub struct AppState {
    pub settings: Arc<Settings>,
    pub jwt_validator: Arc<JwtValidator>,
    pub connection_manager: Arc<ConnectionManager>,
    pub dispatcher: Arc<NotificationDispatcher>,
    pub template_store: Arc<TemplateStore>,
    pub tenant_manager: Arc<TenantManager>,
    pub message_queue: Arc<UserMessageQueue>,
    pub rate_limiter: Option<Arc<RateLimiter>>,
    pub redis_health: Option<Arc<RedisHealth>>,
    pub ack_tracker: Option<Arc<AckTracker>>,
}
```

**職責：**
- 作為 Axum 的 State Extractor
- 統一管理所有共享資源的生命週期
- 啟動時一次性初始化
- 提供依賴注入機制

### ConnectionManager

連線管理器，維護四個 DashMap 索引以支援不同查詢模式，並實作連線數限制。

**檔案位置：** `src/connection_manager/registry.rs`

```rust
pub struct ConnectionManager {
    connections: DashMap<Uuid, Arc<ConnectionHandle>>,      // 主索引
    user_index: DashMap<String, HashSet<Uuid>>,            // 使用者 → 連線
    channel_index: DashMap<String, HashSet<Uuid>>,         // 頻道 → 連線
    tenant_index: DashMap<String, HashSet<Uuid>>,          // 租戶 → 連線
    limits: ConnectionLimits,                               // 連線限制配置
}

pub struct ConnectionLimits {
    pub max_connections: usize,                // 最大總連線數（預設 10000）
    pub max_connections_per_user: usize,       // 每使用者最大連線數（預設 5）
    pub max_subscriptions_per_connection: usize, // 每連線最大訂閱數（預設 50）
}
```

**四索引設計優勢：**

| 索引 | Key | Value | 用途 |
|------|-----|-------|------|
| `connections` | `Uuid` | `Arc<ConnectionHandle>` | O(1) 查詢單一連線 |
| `user_index` | `user_id` | `HashSet<Uuid>` | O(1) 查詢使用者所有連線（多裝置支援） |
| `channel_index` | `channel_name` | `HashSet<Uuid>` | O(1) 查詢頻道所有訂閱者 |
| `tenant_index` | `tenant_id` | `HashSet<Uuid>` | O(1) 查詢租戶所有連線 |

**核心方法：**

| 方法 | 說明 |
|------|------|
| `register()` | 註冊新連線（含限制檢查），回傳 `Result<Arc<ConnectionHandle>, ConnectionError>` |
| `unregister()` | 移除連線，清理所有索引（async，僅清理已訂閱頻道） |
| `subscribe_to_channel()` | 訂閱頻道（含限制檢查），回傳 `Result<(), String>` |
| `unsubscribe_from_channel()` | 取消訂閱 |
| `get_user_connections()` | 取得使用者所有連線 |
| `get_channel_connections()` | 取得頻道所有訂閱者 |
| `get_tenant_connections()` | 取得租戶所有連線 |
| `cleanup_stale_connections()` | 清理閒置連線 |

### ConnectionHandle

單一連線的抽象，包含傳送通道與元資料。

**檔案位置：** `src/connection_manager/registry.rs`

```rust
pub struct ConnectionHandle {
    pub id: Uuid,                              // 連線 UUID
    pub user_id: String,                       // 使用者 ID（從 JWT 解析）
    pub tenant_id: String,                     // 租戶 ID（從 JWT 解析）
    pub roles: Vec<String>,                    // 使用者角色（從 JWT 解析）
    pub sender: mpsc::Sender<OutboundMessage>, // 訊息發送通道（32 緩衝）
    pub connected_at: DateTime<Utc>,           // 連線時間
    last_activity: AtomicI64,                  // 最後活動時間（Unix timestamp，無鎖）
    pub subscriptions: RwLock<HashSet<String>>,// 已訂閱頻道
}
```

**OutboundMessage 類型：**

```rust
pub enum OutboundMessage {
    Raw(ServerMessage),           // 原始訊息，需序列化
    Serialized(Arc<str>),         // 預序列化訊息（用於廣播）
}
```

**MPSC Channel 設計：**
- 緩衝大小：32 訊息
- 滿載行為：發送失敗，標記連線異常
- 接收端：WebSocket/SSE handler 的發送迴圈
- 支援預序列化：避免廣播時重複序列化

### NotificationDispatcher

通知分發器，根據 NotificationTarget 路由訊息至對應連線。

**檔案位置：** `src/notification/dispatcher.rs`

```rust
pub struct NotificationDispatcher {
    connection_manager: Arc<ConnectionManager>,
    stats: DispatcherStats,           // AtomicU64 統計計數器
    ack_tracker: Option<Arc<AckTracker>>,
    message_queue: Option<Arc<UserMessageQueue>>,
}
```

**分發邏輯：**

```rust
pub enum NotificationTarget {
    User(String),           // 單一使用者（所有裝置）
    Users(Vec<String>),     // 多使用者
    Broadcast,              // 廣播（所有連線）
    Channel(String),        // 單一頻道
    Channels(Vec<String>),  // 多頻道（自動去重）
}
```

**分發流程：**

1. 根據 target 類型取得目標連線
2. 若啟用 ACK 追蹤，為每個投遞註冊 ACK
3. 透過 ConnectionHandle.sender 發送訊息
4. 若使用者離線且啟用佇列，將訊息入隊
5. 統計計數更新
6. 回傳 DeliveryResult

**統計計數器（Atomic）：**
- `total_sent` - 總發送數
- `total_delivered` - 成功傳遞數
- `total_failed` - 失敗數
- `user_notifications` - 點對點通知數
- `broadcast_notifications` - 廣播通知數
- `channel_notifications` - 頻道通知數

### JwtValidator

JWT Token 驗證器，支援 HS256/HS384/HS512 演算法。

**檔案位置：** `src/auth/jwt.rs`

```rust
pub struct JwtValidator {
    decoding_key: DecodingKey,
    validation: Validation,
}
```

**JWT Claims 結構：**

```rust
pub struct Claims {
    pub sub: String,              // 使用者 ID
    pub exp: usize,               // 過期時間 (Unix timestamp)
    pub iat: Option<usize>,       // 簽發時間
    pub iss: Option<String>,      // 簽發者
    pub aud: Option<String>,      // 受眾
    pub roles: Vec<String>,       // 角色（預設空陣列）
    pub tenant_id: Option<String>, // 租戶 ID（預設 "default"）
}
```

---

## 安全機制

### 認證層

| 機制 | 適用範圍 | 實作位置 |
|------|----------|----------|
| JWT Token | WebSocket/SSE 連線 | `src/websocket/handler.rs`, `src/sse/handler.rs` |
| API Key | HTTP REST API | `src/server/middleware.rs` |

### 限流層

**Rate Limiter（Token Bucket 演算法）**

**檔案位置：** `src/ratelimit/mod.rs`

```rust
pub struct RateLimiter {
    http_buckets: DashMap<String, TokenBucket>,  // 以 API Key 或 IP 為 key
    ws_buckets: DashMap<IpAddr, TokenBucket>,    // 以 IP 為 key
    config: RateLimitConfig,
}
```

| 限制類型 | 預設值 | 環境變數 |
|----------|--------|----------|
| HTTP 請求/秒 | 100 | `RATELIMIT_HTTP_REQUESTS_PER_SECOND` |
| HTTP 突發容量 | 200 | `RATELIMIT_HTTP_BURST_SIZE` |
| WebSocket 連線/分鐘/IP | 10 | `RATELIMIT_WS_CONNECTIONS_PER_MINUTE` |

**連線限制：**

| 限制類型 | 預設值 | 環境變數 |
|----------|--------|----------|
| 總連線數 | 10,000 | `WEBSOCKET_MAX_CONNECTIONS` |
| 每用戶連線數 | 5 | `WEBSOCKET_MAX_CONNECTIONS_PER_USER` |
| 每連線訂閱數 | 50 | `WEBSOCKET_MAX_SUBSCRIPTIONS_PER_CONNECTION` |

### 安全架構圖

```
                                    ┌─────────────────────────────────┐
                                    │         外部請求                 │
                                    └───────────────┬─────────────────┘
                                                    │
                    ┌───────────────────────────────┼───────────────────────────────┐
                    │                               ▼                               │
                    │                     ┌─────────────────┐                       │
                    │                     │   CORS Layer    │ ◄── 來源檢查          │
                    │                     └────────┬────────┘                       │
                    │                              │                                │
                    │              ┌───────────────┴───────────────┐                │
                    │              │                               │                │
                    │              ▼                               ▼                │
                    │     ┌─────────────────┐             ┌─────────────────┐       │
                    │     │  /ws, /sse      │             │ /api/* (受保護) │       │
                    │     └────────┬────────┘             └────────┬────────┘       │
                    │              │                               │                │
                    │              ▼                               ▼                │
                    │     ┌─────────────────┐             ┌─────────────────┐       │
                    │     │  JWT 驗證        │             │ API Key 中介層  │       │
                    │     └────────┬────────┘             └────────┬────────┘       │
                    │              │                               │                │
                    │              ▼                               ▼                │
                    │     ┌─────────────────┐             ┌─────────────────┐       │
                    │     │ WS Rate Limiter │             │ HTTP Rate Limit │       │
                    │     └────────┬────────┘             └────────┬────────┘       │
                    │              │                               │                │
                    │              ▼                               ▼                │
                    │     ┌─────────────────┐             ┌─────────────────┐       │
                    │     │  連線限制檢查    │             │ Body 大小限制   │       │
                    │     └────────┬────────┘             └────────┬────────┘       │
                    │              │                               │                │
                    │              └───────────────┬───────────────┘                │
                    │                              │                                │
                    │                              ▼                                │
                    │                    ┌─────────────────┐                        │
                    │                    │   業務邏輯處理   │                        │
                    │                    └────────┬────────┘                        │
                    │                             │                                 │
                    │                             ▼                                 │
                    │                    ┌─────────────────┐                        │
                    │                    │   錯誤遮蔽      │ ◄── 生產模式隱藏詳情   │
                    │                    └─────────────────┘                        │
                    │                                                               │
                    └───────────────────────────────────────────────────────────────┘
```

---

## 資料流程

### WebSocket 連線建立

```
Client                    Server                   ConnectionManager
  │                         │                            │
  │──── GET /ws?token=JWT ──►│                            │
  │                         │                            │
  │                         │── validate JWT ───────────►│
  │                         │◄── Claims (with tenant) ──│
  │                         │                            │
  │                         │── check rate limit ───────►│ (if enabled)
  │                         │                            │
  │◄─── 101 Switching ──────│                            │
  │                         │                            │
  │                         │── mpsc::channel(32) ──────►│
  │                         │                            │
  │                         │── register(user_id,────────►│
  │                         │   tenant_id, roles, tx)    │
  │                         │◄── ConnectionHandle ──────│
  │                         │                            │
  │                         │── replay_queue() ─────────►│ (if enabled)
  │                         │                            │
  │                         │ spawn sender_loop          │
  │                         │ spawn receiver_loop        │
  │                         │                            │
```

### SSE 連線建立

```
Client                    Server                   ConnectionManager
  │                         │                            │
  │── GET /sse?token=JWT ──►│                            │
  │                         │                            │
  │                         │── validate JWT ───────────►│
  │                         │◄── Claims (with tenant) ──│
  │                         │                            │
  │◄─── 200 OK + SSE ───────│                            │
  │     Content-Type:       │                            │
  │     text/event-stream   │                            │
  │                         │                            │
  │                         │── register() ─────────────►│
  │                         │◄── ConnectionHandle ──────│
  │                         │                            │
  │◄── event: connected ────│                            │
  │    data: {connection_id}│                            │
  │                         │                            │
  │                         │── replay_queue() ─────────►│
  │                         │                            │
  │◄── event: notification ─│ (stream notifications)    │
```

### HTTP 觸發通知傳遞

```
HTTP Client              API Handler            Dispatcher         ConnectionManager
    │                        │                      │                     │
    │── POST /send ─────────►│                      │                     │
    │   {target_user_id,     │                      │                     │
    │    event_type,         │                      │                     │
    │    payload}            │                      │                     │
    │                        │                      │                     │
    │        OR              │                      │                     │
    │   {target_user_id,     │                      │                     │
    │    template_id,        │── render template ──►│                     │
    │    variables}          │◄── rendered event ──│                     │
    │                        │                      │                     │
    │                        │── dispatch(User, ────►│                     │
    │                        │          Event)      │                     │
    │                        │                      │── get_user_conns ──►│
    │                        │                      │◄── Vec<Handle> ─────│
    │                        │                      │                     │
    │                        │                      │── handle.send() ───►│
    │                        │                      │   (for each conn)   │
    │                        │                      │                     │
    │                        │                      │── track_ack() ─────►│ (if ACK enabled)
    │                        │                      │                     │
    │                        │                      │── enqueue() ───────►│ (if offline + queue)
    │                        │                      │                     │
    │                        │◄── DeliveryResult ───│                     │
    │◄── 200 OK ─────────────│                      │                     │
    │   {delivered_to: N}    │                      │                     │
```

### 批次發送流程

```
HTTP Client              Batch Handler         Dispatcher
    │                        │                      │
    │── POST /batch ────────►│                      │
    │   {notifications: [    │                      │
    │     {target, event},   │                      │
    │     ...                │                      │
    │   ],                   │                      │
    │   options: {           │                      │
    │     stop_on_error,     │                      │
    │     deduplicate        │                      │
    │   }}                   │                      │
    │                        │                      │
    │                        │── deduplicate() ────►│ (if enabled)
    │                        │                      │
    │                        │── for each notification:
    │                        │   dispatch() ───────►│
    │                        │◄── result ───────────│
    │                        │   (collect results)  │
    │                        │                      │
    │◄── 200 OK ─────────────│                      │
    │   {batch_id,           │                      │
    │    results: [...],     │                      │
    │    summary}            │                      │
```

---

## 進階功能

### 通知模板系統

**檔案位置：** `src/template/mod.rs`

```rust
pub struct TemplateStore {
    templates: DashMap<String, Template>,
}

pub struct Template {
    pub id: String,                    // 唯一識別碼 (1-64 字元)
    pub name: String,                  // 人類可讀名稱
    pub event_type: String,            // 事件類型
    pub payload_template: Value,       // JSON 模板 (支援 {{variable}})
    pub default_priority: Priority,    // 預設優先級
    pub default_ttl: Option<u32>,      // 預設 TTL
    pub description: Option<String>,   // 描述
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

**變數替換引擎：**

```rust
// 模板定義
{
    "title": "Order {{order_id}} shipped",
    "body": "Tracking: {{tracking_number}}",
    "data": {
        "count": "{{item_count}} items"
    }
}

// 變數
{
    "order_id": "ORD-123",
    "tracking_number": "TW123456",
    "item_count": 5
}

// 渲染結果
{
    "title": "Order ORD-123 shipped",
    "body": "Tracking: TW123456",
    "data": {
        "count": "5 items"
    }
}
```

### 多租戶支援

**檔案位置：** `src/tenant/mod.rs`

```rust
pub struct TenantManager {
    config: TenantConfig,
    stats: DashMap<String, TenantStats>,
}

pub struct TenantContext {
    pub tenant_id: String,
    pub is_default: bool,
}

pub struct TenantStats {
    pub active_connections: AtomicU64,
    pub total_connections: AtomicU64,
    pub messages_sent: AtomicU64,
    pub messages_delivered: AtomicU64,
}
```

**頻道命名空間：**

```
租戶 "acme-corp" 訂閱 "orders"
  → 內部頻道名稱: "acme-corp:orders"

租戶 "default" 訂閱 "orders"
  → 內部頻道名稱: "orders" (不加前綴)
```

### 離線訊息佇列

**檔案位置：** `src/queue/mod.rs`

```rust
pub struct UserMessageQueue {
    queues: DashMap<String, VecDeque<QueuedMessage>>,
    config: QueueConfig,
}

pub struct QueuedMessage {
    pub message: OutboundMessage,
    pub enqueued_at: DateTime<Utc>,
}

pub struct QueueConfig {
    pub enabled: bool,
    pub max_size_per_user: usize,    // 預設 100
    pub message_ttl: Duration,        // 預設 1 小時
    pub cleanup_interval: Duration,   // 預設 5 分鐘
}
```

**佇列行為：**
- 使用者離線時，訊息入隊
- 重連時自動重播所有未過期訊息
- FIFO 溢出策略（丟棄最舊訊息）
- 背景任務定期清理過期訊息

### 客戶端 ACK 追蹤

**檔案位置：** `src/notification/ack.rs`

```rust
pub struct AckTracker {
    pending: DashMap<Uuid, PendingAck>,
    config: AckConfig,
    stats: AckStats,
}

pub struct PendingAck {
    pub user_id: String,
    pub sent_at: DateTime<Utc>,
}

pub struct AckStats {
    pub total_tracked: AtomicU64,
    pub total_acked: AtomicU64,
    pub total_expired: AtomicU64,
    pub total_latency_ms: AtomicU64,
}
```

**ACK 流程：**

1. 發送通知時，註冊 pending ACK
2. 客戶端發送 `{ "type": "Ack", "payload": { "notification_id": "..." } }`
3. 驗證 user_id 匹配
4. 計算 ACK 延遲
5. 回傳 `{ "type": "acked", "notification_id": "..." }`
6. 背景任務清理過期 ACK

### Redis 高可用

**檔案位置：** `src/redis/mod.rs`

```rust
pub struct RedisHealth {
    circuit_breaker: CircuitBreaker,
    backoff: ExponentialBackoff,
    stats: RedisStats,
}

pub struct CircuitBreaker {
    state: AtomicU8,              // Closed=0, Open=1, HalfOpen=2
    failure_count: AtomicU64,
    success_count: AtomicU64,
    last_failure_time: AtomicI64,
    config: CircuitBreakerConfig,
}

pub struct ExponentialBackoff {
    current_delay_ms: AtomicU64,
    config: BackoffConfig,
}
```

**熔斷器狀態轉換：**

```
         ┌──────────────────────────────────────────┐
         │                                          │
         ▼                                          │
    ┌─────────┐     failure_threshold    ┌─────────┐
    │ Closed  │─────────────────────────►│  Open   │
    │         │                          │         │
    └────┬────┘                          └────┬────┘
         │                                    │
         │                      reset_timeout │
         │                                    │
         │      success_threshold            ▼
         │◄────────────────────────────┌───────────┐
         │                             │ HalfOpen  │
         │        failure              │           │
         └─────────────────────────────└───────────┘
```

---

## 背景任務

### HeartbeatTask

心跳與清理任務，使用 `tokio::select!` 監聯多個計時器。

**檔案位置：** `src/tasks/heartbeat.rs`

```rust
pub struct HeartbeatTask {
    config: WebSocketConfig,
    connection_manager: Arc<ConnectionManager>,
    message_queue: Option<Arc<UserMessageQueue>>,
    ack_tracker: Option<Arc<AckTracker>>,
    shutdown: broadcast::Receiver<()>,
}
```

**運作流程：**

```rust
loop {
    tokio::select! {
        _ = shutdown.recv() => break,
        _ = heartbeat_timer.tick() => {
            send_heartbeats();
        }
        _ = cleanup_timer.tick() => {
            cleanup_stale_connections();
            message_queue.cleanup_expired();  // if enabled
            ack_tracker.cleanup_expired();    // if enabled
        }
    }
}
```

### RedisSubscriber

Redis Pub/Sub 訂閱器，支援模式訂閱與自動重連。

**檔案位置：** `src/triggers/redis.rs`

```rust
pub struct RedisSubscriber {
    config: RedisConfig,
    dispatcher: Arc<NotificationDispatcher>,
    redis_health: Option<Arc<RedisHealth>>,
    shutdown: broadcast::Sender<()>,
}
```

**特性：**
- 支援 glob 模式訂閱（`notification:user:*`）
- 熔斷器整合（Open 狀態時暫停訂閱）
- 指數退避重連
- Graceful shutdown 支援

---

## 併發設計

### 資料結構選型

| 元件 | 同步原語 | 選型原因 |
|------|----------|----------|
| ConnectionManager 四索引 | `DashMap` | 分片鎖設計，高併發讀寫不阻塞 |
| ConnectionHandle.sender | `mpsc::Sender` | 異步通道，背壓控制 |
| ConnectionHandle.last_activity | `AtomicI64` | 無鎖更新，高頻寫入場景 |
| ConnectionHandle.subscriptions | `RwLock` | 訂閱變更不頻繁 |
| DispatcherStats | `AtomicU64` | 無鎖統計計數 |
| TenantStats | `AtomicU64` | 無鎖統計計數 |
| AckStats | `AtomicU64` | 無鎖統計計數 |
| CircuitBreaker.state | `AtomicU8` | 無鎖狀態轉換 |
| Graceful shutdown | `broadcast::channel` | 多接收者通知 |

### DashMap 分片鎖

DashMap 內部將資料分散至多個分片，每個分片有獨立的 RwLock：

```
┌─────────────────────────────────────────────────┐
│                    DashMap                       │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐        │
│  │ Shard 0  │ │ Shard 1  │ │ Shard N  │  ...   │
│  │ (RwLock) │ │ (RwLock) │ │ (RwLock) │        │
│  └──────────┘ └──────────┘ └──────────┘        │
└─────────────────────────────────────────────────┘
```

**優勢：**
- 不同 key 的操作可完全並行
- 讀操作互不阻塞
- 避免全域鎖瓶頸

### MPSC Channel 背壓

每個 WebSocket/SSE 連線使用獨立的 mpsc channel：

```rust
let (tx, rx) = mpsc::channel::<OutboundMessage>(32);
```

**背壓機制：**
1. 發送者透過 `tx.try_send()` 非阻塞發送
2. 緩衝區滿時發送失敗
3. 發送失敗計入統計，連線標記為異常
4. 下次清理任務將移除異常連線

---

## 配置載入

### 優先順序（由低到高）

```
1. 程式碼預設值
     ↓
2. config/default.toml（若存在）
     ↓
3. config/{RUN_MODE}.toml（如 development.toml）
     ↓
4. .env 檔案
     ↓
5. 環境變數
```

後載入的配置會覆蓋先前的值。

### 配置結構

**檔案位置：** `src/config/settings.rs`

```rust
pub struct Settings {
    pub server: ServerConfig,
    pub jwt: JwtConfig,
    pub redis: RedisConfig,
    pub api: ApiConfig,
    pub websocket: WebSocketConfig,
    pub queue: QueueConfig,
    pub ratelimit: RateLimitConfig,
    pub ack: AckConfig,
    pub tenant: TenantConfig,
    pub telemetry: TelemetryConfig,
}
```

---

## 可觀測性

### Prometheus 指標

**檔案位置：** `src/metrics/mod.rs`

| 指標類別 | 範例指標 | 說明 |
|----------|----------|------|
| 連線 | `ara_connections_total` | 當前連線數 |
| 使用者 | `ara_users_connected` | 唯一使用者數 |
| 訊息 | `ara_messages_sent_total{target=...}` | 發送訊息數 |
| 投遞 | `ara_messages_delivered_total` | 成功投遞數 |
| Redis | `ara_redis_connection_status` | Redis 狀態 |
| 佇列 | `ara_queue_size_total` | 佇列大小 |
| 限流 | `ara_ratelimit_denied_total{type=...}` | 被拒請求數 |
| ACK | `ara_ack_pending` | 待確認數 |
| WebSocket | `ara_ws_connection_duration_seconds` | 連線持續時間 |
| 批次 | `ara_batch_size` | 批次大小分布 |

### OpenTelemetry 追蹤

**檔案位置：** `src/telemetry/mod.rs`

```rust
pub struct TelemetryConfig {
    pub enabled: bool,
    pub endpoint: String,            // OTLP gRPC endpoint
    pub service_name: String,
    pub sampling_ratio: f64,         // 0.0 - 1.0
}
```

**追蹤屬性：**
- `user_id` - 使用者 ID
- `tenant_id` - 租戶 ID
- `connection_id` - 連線 ID
- `notification_id` - 通知 ID
- `event_type` - 事件類型
- `target_type` - 目標類型

---

## 限制與擴展點

### 當前限制

| 限制 | 說明 | 解決方案 |
|------|------|----------|
| 無訊息持久化 | 服務重啟後離線佇列清空 | 未來可整合 Redis/PostgreSQL 持久化 |
| 單節點連線狀態 | 連線狀態不跨節點共享 | 使用 Redis 同步連線狀態 |
| 記憶體佇列 | 離線佇列存於記憶體 | 未來可使用 Redis Stream |

### 已實作安全機制

| 機制 | 說明 |
|------|------|
| JWT 認證 | WebSocket/SSE 連線身份驗證 |
| API Key 認證 | HTTP API 端點保護 |
| Rate Limiting | Token Bucket 限流 |
| 連線限制 | 總連線數與每用戶連線數限制 |
| 訂閱限制 | 每連線最多 50 個頻道 |
| Body 大小限制 | 一般 64KB，批次 1MB |
| CORS 控制 | 可配置的來源白名單 |
| 錯誤遮蔽 | 生產模式隱藏內部錯誤 |
| 多租戶隔離 | 頻道命名空間隔離 |

### 未來擴展點

#### 1. 訊息持久化 Trait

```rust
#[async_trait]
pub trait MessageStore {
    async fn save(&self, user_id: &str, msg: &OutboundMessage) -> Result<()>;
    async fn get_pending(&self, user_id: &str) -> Result<Vec<OutboundMessage>>;
    async fn mark_delivered(&self, ids: &[Uuid]) -> Result<()>;
}

// 實作選項
pub struct RedisMessageStore { ... }
pub struct PostgresMessageStore { ... }
```

#### 2. 跨節點連線同步

```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│  Node 1     │    │  Node 2     │    │  Node 3     │
│  (conns A)  │    │  (conns B)  │    │  (conns C)  │
└──────┬──────┘    └──────┬──────┘    └──────┬──────┘
       │                  │                  │
       └──────────────────┼──────────────────┘
                          │
                    ┌─────▼─────┐
                    │   Redis   │
                    │ (狀態同步) │
                    └───────────┘
```

#### 3. WebSocket 壓縮

```rust
// Axum WebSocket 支援 permessage-deflate
.layer(WebSocketUpgradeLayer::new().with_deflate())
```

---

## 附錄：目錄結構

```
src/
├── main.rs                     # 進入點、服務啟動
├── lib.rs                      # 模組匯出
├── config/                     # 配置模組
│   └── settings.rs             # Settings 結構與載入
├── server/                     # 伺服器建構
│   ├── app.rs                  # Axum 路由與中介層
│   ├── middleware.rs           # 認證、限流中介層
│   └── state.rs                # AppState 共享狀態
├── auth/                       # JWT 認證
│   ├── jwt.rs                  # JwtValidator
│   └── claims.rs               # Claims 結構 (含 tenant_id)
├── websocket/                  # WebSocket 處理
│   ├── handler.rs              # 連線處理、訊息路由
│   └── message.rs              # ClientMessage, ServerMessage
├── sse/                        # SSE 處理
│   ├── mod.rs                  # 模組匯出
│   └── handler.rs              # SSE 連線處理
├── notification/               # 通知核心邏輯
│   ├── types.rs                # NotificationEvent, Priority, Audience
│   ├── dispatcher.rs           # NotificationDispatcher
│   └── ack.rs                  # AckTracker 確認追蹤
├── connection_manager/         # 連線管理
│   └── registry.rs             # ConnectionManager 四索引設計
├── template/                   # 通知模板
│   └── mod.rs                  # TemplateStore, 變數替換
├── tenant/                     # 多租戶支援
│   └── mod.rs                  # TenantManager, TenantContext
├── queue/                      # 離線訊息佇列
│   └── mod.rs                  # UserMessageQueue
├── ratelimit/                  # 請求限流
│   └── mod.rs                  # RateLimiter (Token Bucket)
├── redis/                      # Redis 高可用
│   └── mod.rs                  # CircuitBreaker, RedisHealth
├── triggers/                   # 觸發器
│   ├── http.rs                 # HTTP REST API handlers
│   └── redis.rs                # RedisSubscriber
├── tasks/                      # 背景任務
│   └── heartbeat.rs            # HeartbeatTask
├── api/                        # REST API
│   ├── routes.rs               # 路由定義
│   └── handlers.rs             # health, stats, metrics, template, tenant handlers
├── metrics/                    # Prometheus 指標
│   └── mod.rs                  # 指標定義與匯出
├── telemetry/                  # OpenTelemetry
│   └── mod.rs                  # 追蹤初始化
└── error/                      # 錯誤處理
    └── mod.rs                  # AppError 定義
```
