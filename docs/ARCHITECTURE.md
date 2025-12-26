# 系統架構設計

本文檔詳細說明 Ara Notification Service 的內部架構設計、核心元件、資料流程與併發機制。

## 目錄

- [概覽](#概覽)
- [核心元件](#核心元件)
- [資料流程](#資料流程)
- [背景任務](#背景任務)
- [併發設計](#併發設計)
- [配置載入](#配置載入)
- [限制與擴展點](#限制與擴展點)

---

## 概覽

### 技術棧

| 層級 | 技術 | 版本 | 用途 |
|------|------|------|------|
| Web 框架 | Axum | 0.8 | HTTP/WebSocket 路由與中介層 |
| 非同步執行 | Tokio | 1.x | 多執行緒非同步 Runtime |
| 序列化 | Serde | 1.0 | JSON 序列化/反序列化 |
| JWT 驗證 | jsonwebtoken | 9.x | JWT Token 驗證 |
| Redis 客戶端 | redis-rs | 0.27 | Pub/Sub 訂閱 |
| 併發集合 | DashMap | 6.x | 無鎖並行 HashMap |
| 日誌追蹤 | tracing | 0.1 | 結構化日誌 |
| 配置管理 | config-rs | 0.14 | 多來源配置合併 |

### 設計原則

1. **完全無狀態** - 所有連線狀態僅存於記憶體，服務重啟後狀態清空
2. **水平擴展** - 多實例部署時搭配 Redis Pub/Sub 實現跨節點通知
3. **零持久化** - 不儲存訊息歷史，專注於即時推播
4. **背壓控制** - MPSC Channel 緩衝機制防止慢速客戶端拖累系統

### 高層架構圖

```
                              ┌─────────────────────────────────────┐
                              │           觸發來源                   │
                              │                                     │
                              │  ┌──────────────┐  ┌──────────────┐│
                              │  │ HTTP REST API│  │Redis Pub/Sub ││
                              │  └──────┬───────┘  └──────┬───────┘│
                              └─────────┼─────────────────┼────────┘
                                        │                 │
                                        ▼                 ▼
┌───────────────────────────────────────────────────────────────────────┐
│                        Notification Service                            │
│  ┌──────────────────────────────────────────────────────────────────┐ │
│  │                          AppState                                 │ │
│  │  ┌────────────┐ ┌─────────────────┐ ┌─────────────────────────┐  │ │
│  │  │ Settings   │ │ JwtValidator    │ │ NotificationDispatcher  │  │ │
│  │  │ (Arc)      │ │ (Arc)           │ │ (Arc)                   │  │ │
│  │  └────────────┘ └─────────────────┘ └───────────┬─────────────┘  │ │
│  │                                                  │                │ │
│  │                                                  ▼                │ │
│  │  ┌──────────────────────────────────────────────────────────────┐│ │
│  │  │                  ConnectionManager (Arc)                      ││ │
│  │  │  ┌─────────────────┬─────────────────┬─────────────────────┐ ││ │
│  │  │  │   connections   │   user_index    │   channel_index     │ ││ │
│  │  │  │  DashMap<Uuid,  │  DashMap<String,│  DashMap<String,    │ ││ │
│  │  │  │  ConnectionHdl> │  HashSet<Uuid>> │  HashSet<Uuid>>     │ ││ │
│  │  │  └────────┬────────┴────────┬────────┴──────────┬──────────┘ ││ │
│  │  └───────────┼─────────────────┼───────────────────┼────────────┘│ │
│  └──────────────┼─────────────────┼───────────────────┼─────────────┘ │
│                 │                 │                   │               │
│                 ▼                 ▼                   ▼               │
│  ┌──────────────────────────────────────────────────────────────────┐ │
│  │                     ConnectionHandle (每連線)                      │ │
│  │  ┌────────┬──────────┬───────────────┬─────────────────────────┐ │ │
│  │  │   id   │ user_id  │ mpsc::Sender  │ RwLock<last_activity>   │ │ │
│  │  │ (Uuid) │ (String) │ <ServerMsg>   │ RwLock<subscriptions>   │ │ │
│  │  └────────┴──────────┴───────┬───────┴─────────────────────────┘ │ │
│  └──────────────────────────────┼───────────────────────────────────┘ │
│                                 │                                     │
└─────────────────────────────────┼─────────────────────────────────────┘
                                  │
                                  ▼ (mpsc channel)
                    ┌─────────────────────────────┐
                    │    WebSocket 客戶端          │
                    └─────────────────────────────┘
```

---

## 核心元件

### AppState

應用程式共享狀態容器，使用 `Arc` 實現跨執行緒共享。

**檔案位置：** `src/server/state.rs`

```rust
pub struct AppState {
    pub settings: Arc<Settings>,           // 配置（唯讀）
    pub jwt_validator: Arc<JwtValidator>,  // JWT 驗證器
    pub connection_manager: Arc<ConnectionManager>, // 連線管理
    pub dispatcher: Arc<NotificationDispatcher>,    // 通知分發
}
```

**職責：**
- 作為 Axum 的 State Extractor
- 統一管理所有共享資源的生命週期
- 啟動時一次性初始化

### ConnectionManager

連線管理器，維護三個 DashMap 索引以支援不同查詢模式。

**檔案位置：** `src/connection_manager/registry.rs`

```rust
pub struct ConnectionManager {
    connections: DashMap<Uuid, Arc<ConnectionHandle>>,      // 主索引
    user_index: DashMap<String, HashSet<Uuid>>,            // 使用者 → 連線
    channel_index: DashMap<String, HashSet<Uuid>>,         // 頻道 → 連線
}
```

**三索引設計優勢：**

| 索引 | Key | Value | 用途 |
|------|-----|-------|------|
| `connections` | `Uuid` | `Arc<ConnectionHandle>` | O(1) 查詢單一連線 |
| `user_index` | `user_id` | `HashSet<Uuid>` | O(1) 查詢使用者所有連線（多裝置支援） |
| `channel_index` | `channel_name` | `HashSet<Uuid>` | O(1) 查詢頻道所有訂閱者 |

**核心方法：**

| 方法 | 說明 |
|------|------|
| `register()` | 註冊新連線，更新 connections + user_index |
| `unregister()` | 移除連線，清理所有索引 |
| `subscribe_to_channel()` | 訂閱頻道，更新 channel_index + 連線的 subscriptions |
| `unsubscribe_from_channel()` | 取消訂閱 |
| `get_user_connections()` | 取得使用者所有連線 |
| `get_channel_connections()` | 取得頻道所有訂閱者 |
| `cleanup_stale_connections()` | 清理閒置連線 |

### ConnectionHandle

單一 WebSocket 連線的抽象，包含傳送通道與元資料。

**檔案位置：** `src/connection_manager/registry.rs`

```rust
pub struct ConnectionHandle {
    pub id: Uuid,                              // 連線 UUID
    pub user_id: String,                       // 使用者 ID（從 JWT 解析）
    pub sender: mpsc::Sender<ServerMessage>,   // 訊息發送通道（32 緩衝）
    pub connected_at: DateTime<Utc>,           // 連線時間
    pub last_activity: RwLock<DateTime<Utc>>,  // 最後活動時間
    pub subscriptions: RwLock<HashSet<String>>,// 已訂閱頻道
}
```

**MPSC Channel 設計：**
- 緩衝大小：32 訊息
- 滿載行為：發送失敗，標記連線異常
- 接收端：WebSocket handler 的發送迴圈

### NotificationDispatcher

通知分發器，根據 NotificationTarget 路由訊息至對應連線。

**檔案位置：** `src/notification/dispatcher.rs`

```rust
pub struct NotificationDispatcher {
    connection_manager: Arc<ConnectionManager>,
    stats: DispatcherStats,  // AtomicU64 統計計數器
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

**驗證項目：**
- 簽名有效性
- 過期時間（exp）
- 簽發者（iss，選填）
- 受眾（aud，選填）

**JWT Claims 結構：**

```rust
pub struct Claims {
    pub sub: String,              // 使用者 ID
    pub exp: usize,               // 過期時間 (Unix timestamp)
    pub iat: Option<usize>,       // 簽發時間
    pub iss: Option<String>,      // 簽發者
    pub aud: Option<String>,      // 受眾
    pub roles: Option<Vec<String>>, // 角色（選填）
}
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
  │                         │◄── Claims ────────────────│
  │                         │                            │
  │◄─── 101 Switching ──────│                            │
  │                         │                            │
  │                         │── mpsc::channel(32) ──────►│
  │                         │                            │
  │                         │── register(user_id, tx) ──►│
  │                         │◄── ConnectionHandle ──────│
  │                         │                            │
  │                         │ spawn sender_loop          │
  │                         │ spawn receiver_loop        │
  │                         │                            │
```

**連線處理流程：**
1. 解析 JWT Token（query string 或 Authorization header）
2. 驗證 Token 有效性
3. 建立 mpsc channel (buffer=32)
4. 註冊至 ConnectionManager
5. 啟動發送迴圈（讀取 mpsc::Receiver，寫入 WebSocket）
6. 啟動接收迴圈（讀取 WebSocket，處理客戶端訊息）

### 頻道訂閱流程

```
Client                    Handler               ConnectionManager
  │                         │                         │
  │── {"type":"Subscribe",  │                         │
  │    "payload":{          │                         │
  │      "channels":[...]}} │                         │
  │                         │                         │
  │                         │── subscribe_to_channel ─►│
  │                         │   (for each channel)    │
  │                         │                         │
  │                         │◄─────────────────────────│
  │                         │                         │
  │◄── {"type":"subscribed",│                         │
  │     "channels":[...]}  ─│                         │
  │                         │                         │
```

**訂閱操作：**
1. 更新 ConnectionHandle 的 `subscriptions` (RwLock)
2. 更新 ConnectionManager 的 `channel_index` (DashMap)
3. 回傳確認訊息

### HTTP 觸發通知傳遞

```
HTTP Client              API Handler            Dispatcher         ConnectionManager
    │                        │                      │                     │
    │── POST /send ─────────►│                      │                     │
    │   {target_user_id,     │                      │                     │
    │    event_type,         │                      │                     │
    │    payload}            │                      │                     │
    │                        │                      │                     │
    │                        │── dispatch(User, ────►│                     │
    │                        │          Event)      │                     │
    │                        │                      │── get_user_conns ──►│
    │                        │                      │◄── Vec<Handle> ─────│
    │                        │                      │                     │
    │                        │                      │── handle.send() ───►│
    │                        │                      │   (for each conn)   │
    │                        │                      │                     │
    │                        │◄── DeliveryResult ───│                     │
    │◄── 200 OK ─────────────│                      │                     │
    │   {delivered_to: N}    │                      │                     │
    │                        │                      │                     │
```

### Redis 觸發通知傳遞

```
Redis                RedisSubscriber              Dispatcher
  │                        │                          │
  │── PUBLISH notification:│                          │
  │   user:123 {...} ─────►│                          │
  │                        │                          │
  │                        │── parse JSON ────────────│
  │                        │── build NotificationEvent│
  │                        │── parse_target() ────────│
  │                        │                          │
  │                        │── dispatch(target, ─────►│
  │                        │          event)          │
  │                        │                          │
  │                        │◄── DeliveryResult ───────│
  │                        │                          │
```

---

## 背景任務

### HeartbeatTask

心跳與清理任務，使用 `tokio::select!` 監聽多個計時器。

**檔案位置：** `src/tasks/heartbeat.rs`

```rust
pub struct HeartbeatTask {
    config: WebSocketConfig,
    connection_manager: Arc<ConnectionManager>,
    shutdown: broadcast::Receiver<()>,  // Graceful shutdown 信號
}
```

**雙計時器設計：**

| 計時器 | 預設間隔 | 功能 |
|--------|----------|------|
| `heartbeat_timer` | 30 秒 | 發送 `ServerMessage::Heartbeat` 至所有連線 |
| `cleanup_timer` | 60 秒 | 清理超時閒置連線（預設 120 秒無活動） |

**運作流程：**

```rust
loop {
    tokio::select! {
        _ = shutdown.recv() => break,
        _ = heartbeat_timer.tick() => send_heartbeats(),
        _ = cleanup_timer.tick() => cleanup_stale_connections(),
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
    shutdown: broadcast::Sender<()>,
}
```

**特性：**
- 支援 glob 模式訂閱（`notification:user:*`）
- 斷線自動重連（5 秒間隔）
- Graceful shutdown 支援

**預設訂閱頻道：**

```
notification:user:*      # 所有使用者通知
notification:broadcast   # 廣播通知
notification:channel:*   # 所有頻道通知
```

---

## 併發設計

### 資料結構選型

| 元件 | 同步原語 | 選型原因 |
|------|----------|----------|
| ConnectionManager 三索引 | `DashMap` | 分片鎖設計，高併發讀寫不阻塞 |
| ConnectionHandle.sender | `mpsc::Sender` | 異步通道，背壓控制 |
| ConnectionHandle.last_activity | `RwLock` | 讀多寫少（每次活動更新） |
| ConnectionHandle.subscriptions | `RwLock` | 訂閱變更不頻繁 |
| DispatcherStats | `AtomicU64` | 無鎖統計計數 |
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

每個 WebSocket 連線使用獨立的 mpsc channel：

```rust
let (tx, rx) = mpsc::channel::<ServerMessage>(32);
```

**背壓機制：**
1. 發送者（dispatcher）透過 `tx.send().await` 非阻塞發送
2. 緩衝區滿時 `send()` 返回錯誤
3. 發送失敗計入統計，連線標記為異常
4. 下次清理任務將移除異常連線

### Graceful Shutdown

使用 `broadcast::channel` 實現多任務協調關閉：

```rust
// main.rs
let (shutdown_tx, _) = broadcast::channel::<()>(1);

// 分發給各任務
let heartbeat_rx = shutdown_tx.subscribe();
let redis_rx = shutdown_tx.subscribe();

// 收到 SIGTERM/Ctrl+C 時
shutdown_tx.send(())?;  // 所有接收者收到通知

// 等待任務結束
tokio::join!(heartbeat_handle, redis_handle);
```

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

### 環境變數映射

| 環境變數 | 配置路徑 | 預設值 |
|----------|----------|--------|
| `SERVER_HOST` | `server.host` | `0.0.0.0` |
| `SERVER_PORT` | `server.port` | `8081` |
| `JWT_SECRET` | `jwt.secret` | (必填) |
| `JWT_ISSUER` | `jwt.issuer` | (選填) |
| `JWT_AUDIENCE` | `jwt.audience` | (選填) |
| `REDIS_URL` | `redis.url` | `redis://localhost:6379` |
| `REDIS_CHANNELS` | `redis.channels` | (逗號分隔) |
| `WEBSOCKET_HEARTBEAT_INTERVAL` | `websocket.heartbeat_interval` | `30` |
| `WEBSOCKET_CONNECTION_TIMEOUT` | `websocket.connection_timeout` | `120` |
| `WEBSOCKET_CLEANUP_INTERVAL` | `websocket.cleanup_interval` | `60` |
| `API_KEY` | `api.key` | (選填) |

### 配置結構

**檔案位置：** `src/config/settings.rs`

```rust
pub struct Settings {
    pub server: ServerConfig,
    pub jwt: JwtConfig,
    pub redis: RedisConfig,
    pub api: ApiConfig,
    pub websocket: WebSocketConfig,
}

pub struct WebSocketConfig {
    pub heartbeat_interval: u64,    // 心跳間隔（秒）
    pub connection_timeout: u64,    // 連線超時（秒）
    pub cleanup_interval: u64,      // 清理間隔（秒）
}
```

---

## 限制與擴展點

### 當前限制

| 限制 | 說明 | 影響 |
|------|------|------|
| 無訊息持久化 | 訊息僅即時傳遞，不儲存歷史 | 離線使用者無法收到錯過的通知 |
| 無離線佇列 | 使用者離線時訊息直接丟棄 | 需外部系統處理離線推播（如 Push Notification） |
| 單節點狀態 | 連線狀態不跨節點共享 | 水平擴展需配合 Load Balancer Sticky Session |
| 無認證限流 | 未實作連線數/請求數限制 | 需在 API Gateway 層處理 |
| 無訊息確認 | 無 ACK 機制確認客戶端收到 | 不保證訊息送達（at-most-once） |

### 未來擴展點

#### 1. 訊息持久化 Trait

```rust
#[async_trait]
pub trait NotificationStore {
    async fn save(&self, event: &NotificationEvent) -> Result<()>;
    async fn get_unread(&self, user_id: &str) -> Result<Vec<NotificationEvent>>;
    async fn mark_read(&self, notification_id: Uuid) -> Result<()>;
}

// 實作選項
pub struct PostgresStore { ... }
pub struct RedisStore { ... }
pub struct InMemoryStore { ... }  // 現有行為
```

#### 2. Prometheus Metrics

```rust
// 可擴展的 metrics endpoint
lazy_static! {
    static ref CONNECTIONS_GAUGE: IntGauge = ...;
    static ref MESSAGES_COUNTER: IntCounterVec = ...;
    static ref MESSAGE_LATENCY: Histogram = ...;
}
```

#### 3. 跨節點連線同步

使用 Redis 儲存連線狀態，實現真正的無狀態水平擴展：

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

#### 4. WebSocket 壓縮

```rust
// Axum WebSocket 支援 permessage-deflate
.layer(WebSocketUpgradeLayer::new().with_deflate())
```

#### 5. 訊息確認機制

```rust
pub enum ClientMessage {
    // ... existing
    Ack { notification_id: Uuid },
}

// 未確認訊息重試佇列
struct PendingMessage {
    event: NotificationEvent,
    sent_at: DateTime<Utc>,
    retry_count: u32,
}
```

---

## 附錄：服務啟動流程

**檔案位置：** `src/main.rs`

```
main()
  │
  ├── init_tracing()              # 初始化日誌
  │
  ├── Settings::new()             # 載入配置
  │
  ├── AppState::new()             # 建立共享狀態
  │     ├── JwtValidator::new()
  │     ├── ConnectionManager::new()
  │     └── NotificationDispatcher::new()
  │
  ├── RedisSubscriber::new()      # 建立 Redis 訂閱器
  │
  ├── tokio::spawn(redis.start()) # 背景：Redis 訂閱
  │
  ├── HeartbeatTask::new()
  │
  ├── tokio::spawn(heartbeat.run()) # 背景：心跳任務
  │
  ├── create_app(state)           # 建立 Axum Router
  │
  ├── TcpListener::bind()
  │
  ├── axum::serve()               # 啟動 HTTP 伺服器
  │     └── with_graceful_shutdown()
  │
  └── tokio::join!(...)           # 等待背景任務結束
```

---

## 附錄：目錄結構

```
src/
├── main.rs                     # 進入點、服務啟動
├── lib.rs                      # 模組匯出
├── config/                     # 配置模組
│   └── settings.rs             # Settings, JwtConfig, RedisConfig, WebSocketConfig
├── server/                     # 伺服器建構
│   ├── app.rs                  # Axum 路由與中介層
│   └── state.rs                # AppState 共享狀態
├── auth/                       # JWT 認證
│   ├── jwt.rs                  # JwtValidator
│   └── claims.rs               # Claims 結構
├── websocket/                  # WebSocket 處理
│   ├── handler.rs              # 連線處理、訊息路由
│   └── message.rs              # ClientMessage, ServerMessage
├── notification/               # 通知核心邏輯
│   ├── types.rs                # NotificationEvent, Priority, Audience, NotificationTarget
│   └── dispatcher.rs           # NotificationDispatcher, DeliveryResult
├── connection_manager/         # 連線管理
│   └── registry.rs             # ConnectionManager, ConnectionHandle
├── triggers/                   # 觸發器
│   ├── http.rs                 # HTTP REST API handlers
│   └── redis.rs                # RedisSubscriber
├── tasks/                      # 背景任務
│   └── heartbeat.rs            # HeartbeatTask (心跳 + 清理)
├── api/                        # REST API
│   ├── routes.rs               # 路由定義
│   └── handlers.rs             # health, stats handlers
└── error/                      # 錯誤處理
    └── mod.rs                  # AppError 定義
```
