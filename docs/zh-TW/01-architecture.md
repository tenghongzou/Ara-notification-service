# 系統架構

本文件說明 Ara Notification Service 的技術架構、模組設計與核心設計模式。

---

## 技術棧

### 核心框架

| 技術 | 版本 | 用途 |
|------|------|------|
| **Rust** | 1.75+ | 系統程式語言 |
| **Tokio** | 1.x | 非同步執行環境 |
| **Axum** | 0.8 | Web 框架 (HTTP/WebSocket) |
| **Tower** | 0.5 | 中介層抽象 |

### 資料儲存

| 技術 | 用途 |
|------|------|
| **Redis** | Pub/Sub 訊息、分散式狀態、限流 |
| **PostgreSQL** | 持久化佇列、ACK 記錄 (選用) |
| **DashMap** | 記憶體內並發資料結構 |

### 可觀測性

| 技術 | 用途 |
|------|------|
| **Prometheus** | 指標收集與匯出 |
| **OpenTelemetry** | 分散式追蹤 |
| **tracing** | 結構化日誌 |

---

## 系統架構圖

```
┌─────────────────────────────────────────────────────────────┐
│                      觸發來源                                │
│  ┌─────────────────┐              ┌─────────────────┐       │
│  │  Symfony/其他   │              │  其他微服務      │       │
│  │    Backend      │              │                 │       │
│  └────────┬────────┘              └────────┬────────┘       │
│           │                                │                │
│           ▼                                ▼                │
│  ┌─────────────────┐              ┌─────────────────┐       │
│  │  HTTP REST API  │              │ Redis Pub/Sub   │       │
│  │  (X-API-Key)    │              │                 │       │
│  └────────┬────────┘              └────────┬────────┘       │
└───────────┼────────────────────────────────┼────────────────┘
            │                                │
            ▼                                ▼
┌─────────────────────────────────────────────────────────────┐
│                 Notification Service                         │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐ │
│  │                    Triggers Layer                       │ │
│  │   HTTP Handlers          Redis Subscriber               │ │
│  └────────────────────────────┬───────────────────────────┘ │
│                               │                              │
│                               ▼                              │
│  ┌────────────────────────────────────────────────────────┐ │
│  │              NotificationDispatcher                     │ │
│  │    ┌──────────┬──────────┬──────────┬────────────┐     │ │
│  │    │ User(s)  │Broadcast │Channel(s)│ Template   │     │ │
│  │    └────┬─────┴────┬─────┴────┬─────┴─────┬──────┘     │ │
│  └─────────┼──────────┼──────────┼───────────┼────────────┘ │
│            │          │          │           │               │
│            ▼          ▼          ▼           ▼               │
│  ┌────────────────────────────────────────────────────────┐ │
│  │              ConnectionManager (DashMap)                │ │
│  │  ┌────────────┬─────────────┬──────────────┬─────────┐ │ │
│  │  │connections │ user_index  │channel_index │ tenant  │ │ │
│  │  └────────────┴─────────────┴──────────────┴─────────┘ │ │
│  └────────────────────────────────────────────────────────┘ │
│                               │                              │
│                               ▼                              │
│  ┌────────────────────────────────────────────────────────┐ │
│  │              WebSocket / SSE Handlers                   │ │
│  │   ┌─────────────────┐    ┌─────────────────┐           │ │
│  │   │ WebSocket (雙向) │    │   SSE (單向)    │           │ │
│  │   └─────────────────┘    └─────────────────┘           │ │
│  └────────────────────────────────────────────────────────┘ │
└─────────────────────────────┬───────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    客戶端                                    │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐    │
│  │ 前端 App │  │ 管理後台 │  │ 行動裝置 │  │ 其他客戶 │    │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘    │
└─────────────────────────────────────────────────────────────┘
```

---

## 模組架構

```
src/
├── main.rs                     # 進入點、優雅關閉
├── lib.rs                      # 模組匯出
│
├── config/                     # 配置模組
│   └── settings.rs             # Settings 結構、驗證規則
│
├── server/                     # 伺服器建構
│   ├── app.rs                  # Axum Router、中介層
│   ├── middleware.rs           # API Key 認證、限流
│   └── state.rs                # AppState 共享狀態
│
├── auth/                       # JWT 認證
│   ├── jwt.rs                  # JwtValidator
│   └── claims.rs               # Claims (含 tenant_id)
│
├── websocket/                  # WebSocket 處理
│   ├── handler.rs              # 連線處理、訊息路由
│   └── message.rs              # ClientMessage, ServerMessage
│
├── sse/                        # SSE 處理
│   └── handler.rs              # SSE 連線、事件串流
│
├── notification/               # 通知核心
│   ├── types.rs                # NotificationEvent, Priority
│   ├── dispatcher.rs           # NotificationDispatcher
│   ├── ack.rs                  # AckTracker
│   └── ack_*_backend.rs        # ACK 後端實作
│
├── connection_manager/         # 連線管理
│   ├── manager.rs              # ConnectionManager
│   ├── types.rs                # ConnectionHandle
│   └── stats.rs                # 連線統計
│
├── template/                   # 通知模板
│   ├── store.rs                # TemplateStore
│   ├── types.rs                # Template 結構
│   └── substitution.rs         # 變數替換引擎
│
├── queue/                      # 離線訊息佇列
│   ├── user_queue.rs           # UserMessageQueue
│   ├── backend.rs              # 後端抽象
│   └── *_backend.rs            # Memory/Redis/Postgres 實作
│
├── ratelimit/                  # 請求限流
│   ├── limiter.rs              # RateLimiter
│   ├── token_bucket.rs         # Token Bucket 演算法
│   └── distributed.rs          # 分散式限流
│
├── redis/                      # Redis 高可用
│   ├── circuit_breaker.rs      # 熔斷器
│   ├── health.rs               # 健康監控
│   └── backoff.rs              # 指數退避
│
├── triggers/                   # 觸發器
│   ├── http/                   # HTTP REST API
│   │   ├── handlers.rs         # 端點處理器
│   │   ├── models.rs           # 請求/回應類型
│   │   └── batch.rs            # 批次發送
│   └── redis.rs                # Redis Pub/Sub 訂閱
│
├── tenant/                     # 多租戶
│   └── mod.rs                  # TenantManager
│
├── cluster/                    # 叢集模式
│   ├── router.rs               # ClusterRouter
│   ├── types.rs                # ClusterConfig
│   └── *_store.rs              # Session Store 實作
│
├── api/                        # REST API
│   ├── routes.rs               # 路由定義
│   └── handlers.rs             # 端點處理器
│
├── metrics/                    # Prometheus 指標
│   └── helpers.rs              # 指標定義
│
├── telemetry/                  # OpenTelemetry
│   └── mod.rs                  # 追蹤初始化
│
├── tasks/                      # 背景任務
│   └── heartbeat.rs            # 心跳與清理
│
├── shutdown/                   # 優雅關閉
│   └── mod.rs                  # Shutdown 處理
│
└── error/                      # 錯誤處理
    └── mod.rs                  # AppError 定義
```

---

## 核心元件

### 1. ConnectionManager

三索引設計，使用 DashMap 實現並發安全的 O(1) 查詢：

```rust
pub struct ConnectionManager {
    // 主索引：連線 ID -> 連線資訊
    connections: DashMap<Uuid, ConnectionHandle>,

    // 使用者索引：使用者 ID -> 連線 ID 列表
    // SmallVec 優化：1-4 個連線不需堆積分配
    user_index: DashMap<String, SmallVec<[Uuid; 4]>>,

    // 頻道索引：頻道名稱 -> 連線 ID 集合
    channel_index: DashMap<String, HashSet<Uuid>>,

    // 租戶索引：租戶 ID -> 連線 ID 集合
    tenant_index: DashMap<String, HashSet<Uuid>>,
}
```

### 2. NotificationDispatcher

通知派發中樞，支援多種目標類型：

```rust
pub enum NotificationTarget {
    User(String),              // 單一使用者
    Users(Vec<String>),        // 多使用者
    Broadcast,                 // 所有連線
    Channel(String),           // 單一頻道
    Channels(Vec<String>),     // 多頻道
}
```

### 3. ConnectionHandle

單一連線的狀態容器：

```rust
pub struct ConnectionHandle {
    pub id: Uuid,                           // 連線唯一 ID
    pub user_id: String,                    // 使用者 ID
    pub tenant_id: String,                  // 租戶 ID
    pub sender: mpsc::Sender<OutboundMessage>,  // 訊息發送通道
    pub subscriptions: RwLock<HashSet<String>>, // 訂閱頻道
    pub connected_at: DateTime<Utc>,        // 連線時間
}
```

### 4. OutboundMessage

優化的輸出訊息類型：

```rust
pub enum OutboundMessage {
    // 原始訊息，每次序列化
    Raw(ServerMessage),

    // 預序列化訊息，避免重複序列化（用於廣播）
    PreSerialized(Arc<str>),
}
```

---

## 設計模式

### 熔斷器模式 (Circuit Breaker)

Redis 連線使用熔斷器保護，防止連鎖失敗：

```
    ┌─────────────────────────────────────────┐
    │           Circuit Breaker               │
    │                                         │
    │   ┌──────────┐    failure    ┌───────┐ │
    │   │  Closed  │──────────────▶│ Open  │ │
    │   │ (正常)   │               │(中斷) │ │
    │   └────▲─────┘               └───┬───┘ │
    │        │                         │     │
    │   success                  timeout     │
    │        │       ┌──────────┐      │     │
    │        └───────│Half-Open │◀─────┘     │
    │                │ (測試中) │            │
    │                └──────────┘            │
    └─────────────────────────────────────────┘
```

**狀態說明：**
- **Closed**：正常運作，請求直接通過
- **Open**：熔斷開啟，請求立即失敗，不嘗試連線
- **Half-Open**：允許少量請求測試，成功則恢復，失敗則回到 Open

### Token Bucket 限流

```
    ┌────────────────────────────────────────┐
    │           Token Bucket                  │
    │                                         │
    │    ┌─────────────────────────────┐     │
    │    │  ○ ○ ○ ○ ○ ○ ○ ○ ○ ○       │     │
    │    │        (tokens)             │     │
    │    └─────────────┬───────────────┘     │
    │                  │                      │
    │         取用 token (請求)               │
    │                  │                      │
    │                  ▼                      │
    │    bucket 為空 → 拒絕請求               │
    │    有 token → 允許請求                  │
    │                                         │
    │    ← 定時補充 tokens (refill rate)     │
    └────────────────────────────────────────┘
```

### 指數退避 (Exponential Backoff)

重連策略，避免瞬間大量請求：

```
重連間隔 = min(initial_delay * 2^attempt + jitter, max_delay)

attempt 0: 100ms + jitter
attempt 1: 200ms + jitter
attempt 2: 400ms + jitter
attempt 3: 800ms + jitter
...
最大延遲: 30000ms
```

---

## 資料流

### WebSocket 連線流程

```
客戶端                    伺服器
   │                        │
   │ ──── WS Upgrade ─────▶ │
   │ (token in query/header)│
   │                        │
   │                        ├── JWT 驗證
   │                        ├── 連線限制檢查
   │                        ├── 註冊到 ConnectionManager
   │                        │
   │ ◀── Connected ──────── │
   │                        │
   │ ──── Subscribe ──────▶ │
   │      {channels: [...]} ├── 更新 channel_index
   │                        │
   │ ◀── Subscribed ─────── │
   │                        │
   │ ◀── Notification ───── │ ◀── 來自 Dispatcher
   │                        │
   │ ──── Ack ────────────▶ │
   │      {notification_id} ├── 更新 AckTracker
   │                        │
   │ ◀── Acked ──────────── │
   │                        │
```

### 通知發送流程

```
觸發來源                  Dispatcher              ConnectionManager
   │                        │                        │
   │ ── SendNotification ─▶ │                        │
   │                        │                        │
   │                        ├── 解析目標類型          │
   │                        │   (User/Broadcast/Channel)
   │                        │                        │
   │                        │ ── 查詢連線 ──────────▶ │
   │                        │                        │
   │                        │ ◀── 連線列表 ───────── │
   │                        │                        │
   │                        ├── 建立 NotificationEvent
   │                        │                        │
   │                        ├── 遍歷連線發送          │
   │                        │   (透過 mpsc sender)   │
   │                        │                        │
   │                        ├── 離線使用者 → Queue   │
   │                        │                        │
   │ ◀── Response ──────── │                        │
   │   {delivered, queued}  │                        │
```

---

## 安全設計

### 認證層級

| 層級 | 方式 | 用途 |
|------|------|------|
| 客戶端連線 | JWT Token | WebSocket/SSE 連線驗證 |
| HTTP API | X-API-Key Header | REST API 呼叫保護 |
| 內部通訊 | Redis Pub/Sub | 服務間無認證（內網信任） |

### 連線限制

```rust
pub struct ConnectionLimits {
    pub max_connections: usize,           // 總連線上限
    pub max_connections_per_user: usize,  // 每使用者上限
    pub max_subscriptions_per_connection: usize, // 每連線頻道上限
}
```

### 錯誤遮蔽

生產模式下隱藏內部錯誤詳情：

```rust
// 開發模式
{"error": "Database connection failed: connection refused"}

// 生產模式
{"error": "Internal server error"}
```

---

## 相關文件

- [安裝與部署](./02-installation.md)
- [API 參考](./03-api-reference.md)
- [開發指南](./04-development-guide.md)
- [進階功能](./05-advanced-features.md)

