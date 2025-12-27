# Ara Notification Service

高效能即時通知服務，使用 Rust 建構，提供 WebSocket/SSE 即時推播功能。

[![Version](https://img.shields.io/badge/version-1.0.0-blue.svg)](CHANGELOG.md)
[![Rust](https://img.shields.io/badge/rust-1.75+-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

## 特性

### 核心功能
- **WebSocket 即時推播** - 低延遲、高效能的雙向即時通訊
- **SSE 備援** - Server-Sent Events 單向推播，防火牆友好
- **JWT 認證** - 安全的身份驗證機制，支援角色權限與多租戶
- **多種訊息模式** - 點對點、多使用者、廣播、頻道訂閱
- **雙觸發機制** - HTTP REST API 與 Redis Pub/Sub 並行支援
- **多裝置支援** - 同一使用者可在多裝置同時接收通知

### 進階功能
- **離線訊息佇列** - 離線使用者重連時自動重播訊息
- **通知模板系統** - 可重用模板與 `{{variable}}` 變數替換
- **批次發送 API** - 單次請求發送多達 100 筆通知
- **客戶端 ACK** - 通知送達確認機制與統計
- **請求限流** - Token Bucket 演算法保護系統資源

### 多租戶支援
- **租戶隔離** - JWT `tenant_id` 聲明自動隔離頻道
- **獨立統計** - 每租戶連線數、訊息數統計
- **彈性限制** - 可針對特定租戶設定連線上限

### 可觀測性
- **Prometheus 監控** - 完整的指標匯出 (`/metrics`)
- **OpenTelemetry 追蹤** - 分散式追蹤整合 (Jaeger, Tempo, Zipkin)
- **Redis 高可用** - 熔斷器模式與指數退避重連
- **K6 負載測試** - 完整測試套件含 7 種負載場景

### 安全防護
- **API Key 認證** - HTTP API 端點保護
- **連線限制** - 總連線數與每使用者連線數限制
- **CORS 控制** - 可配置的跨域請求策略
- **錯誤遮蔽** - 生產模式隱藏內部錯誤詳情

## 系統架構

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
│  └────────┬────────┘              └────────┬────────┘       │
└───────────┼────────────────────────────────┼────────────────┘
            │                                │
            ▼                                ▼
┌─────────────────────────────────────────────────────────────┐
│                 Notification Service                         │
│  ┌───────────────────────────────────────────────────────┐  │
│  │              NotificationDispatcher                    │  │
│  │    ┌──────────┬──────────┬──────────────────┐         │  │
│  │    │ User(s)  │Broadcast │ Channel(s)       │         │  │
│  │    └────┬─────┴────┬─────┴───────┬──────────┘         │  │
│  └─────────┼──────────┼─────────────┼────────────────────┘  │
│            │          │             │                        │
│            ▼          ▼             ▼                        │
│  ┌───────────────────────────────────────────────────────┐  │
│  │              ConnectionManager (DashMap)               │  │
│  │  connections │ user_index │ channel_index │ tenant    │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────┬───────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    WebSocket / SSE 客戶端                    │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐    │
│  │ 前端 App │  │ 管理後台 │  │ 行動裝置 │  │ 其他客戶 │    │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘    │
└─────────────────────────────────────────────────────────────┘
```

## 快速開始

### 環境需求

- Rust 1.75+
- Redis 7.0+
- Docker (選用)

### 本地開發

```bash
# 複製環境變數
cp .env.example .env

# 編輯 .env 設定 JWT_SECRET
# JWT_SECRET=your-secure-secret-key

# 建構專案
cargo build

# 執行服務
cargo run

# 執行測試 (121 個測試)
cargo test
```

### Docker 部署

```bash
# 建構映像
docker build -t ara-notification-service .

# 執行容器
docker run -p 8081:8081 \
    -e JWT_SECRET=your-secret \
    -e REDIS_URL=redis://redis:6379 \
    ara-notification-service
```

### docker-compose 整合

服務已整合至 Ara-infra 的 docker-compose.yml：

```bash
# 從 Ara-infra 根目錄
docker-compose up -d notification
```

## 訊息模式

| 模式 | 說明 | 使用場景 |
|------|------|----------|
| 點對點 (User) | 發送給特定使用者的所有裝置 | 個人通知、私訊 |
| 多使用者 (Users) | 發送給多個指定使用者 | 群組通知、團隊訊息 |
| 廣播 (Broadcast) | 發送給所有連線使用者 | 系統公告、維護通知 |
| 頻道 (Channel) | 發送給訂閱特定頻道的使用者 | 訂單狀態、資料更新 |
| 多頻道 (Channels) | 發送給多個頻道（自動去重） | 跨類別通知 |

## API 概覽

### 即時連線

```bash
# WebSocket (雙向)
ws://localhost:8081/ws?token=<JWT>

# SSE (單向，防火牆友好)
curl -N http://localhost:8081/sse?token=<JWT>
```

### HTTP REST API

受保護的 API 需要 `X-API-Key` Header：

```bash
curl -X POST http://localhost:8081/api/v1/notifications/send \
  -H "X-API-Key: your-api-key" \
  -H "Content-Type: application/json" \
  -d '{"target_user_id": "user123", "event_type": "test", "payload": {}}'
```

| 方法 | 路徑 | 說明 |
|------|------|------|
| POST | `/api/v1/notifications/send` | 點對點通知 |
| POST | `/api/v1/notifications/send-to-users` | 多使用者通知 |
| POST | `/api/v1/notifications/broadcast` | 廣播通知 |
| POST | `/api/v1/notifications/channel` | 頻道通知 |
| POST | `/api/v1/notifications/channels` | 多頻道通知 |
| POST | `/api/v1/notifications/batch` | 批次發送（最多 100 筆） |
| GET | `/api/v1/channels` | 頻道列表與訂閱數 |
| GET | `/api/v1/channels/{name}` | 頻道詳情 |
| GET | `/api/v1/users/{user_id}/subscriptions` | 使用者訂閱列表 |
| POST | `/api/v1/templates` | 建立通知模板 |
| GET | `/api/v1/templates` | 模板列表 |
| GET | `/api/v1/templates/{id}` | 模板詳情 |
| PUT | `/api/v1/templates/{id}` | 更新模板 |
| DELETE | `/api/v1/templates/{id}` | 刪除模板 |
| GET | `/api/v1/tenants` | 租戶列表 |
| GET | `/api/v1/tenants/{id}` | 租戶統計 |
| GET | `/health` | 健康檢查 |
| GET | `/stats` | 連線統計 |
| GET | `/metrics` | Prometheus 指標 |
| WS | `/ws` | WebSocket 連線 |
| GET | `/sse` | SSE 連線 |

### Redis Pub/Sub 頻道

```
notification:user:{user_id}     # 點對點
notification:broadcast          # 廣播
notification:channel:{name}     # 頻道
```

詳細 API 規格請參閱 [docs/API.md](docs/API.md)

## 配置

### 核心環境變數

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `SERVER_HOST` | 服務監聽位址 | `0.0.0.0` |
| `SERVER_PORT` | 服務監聽埠 | `8081` |
| `JWT_SECRET` | JWT 簽名密鑰 (HS256) | (必填) |
| `JWT_ISSUER` | JWT 簽發者驗證 | (選填) |
| `JWT_AUDIENCE` | JWT 受眾驗證 | (選填) |
| `REDIS_URL` | Redis 連線 URL | `redis://localhost:6379` |
| `API_KEY` | HTTP API 認證金鑰 | (選填) |
| `CORS_ORIGINS` | 允許的來源 (逗號分隔) | (空=允許全部) |
| `RUN_MODE` | 執行模式 | `development` |
| `RUST_LOG` | 日誌等級 | `info` |

### WebSocket 配置

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `WEBSOCKET_HEARTBEAT_INTERVAL` | 心跳間隔 (秒) | `30` |
| `WEBSOCKET_CONNECTION_TIMEOUT` | 連線超時 (秒) | `120` |
| `WEBSOCKET_CLEANUP_INTERVAL` | 清理任務間隔 (秒) | `60` |
| `WEBSOCKET_MAX_CONNECTIONS` | 最大總連線數 | `10000` |
| `WEBSOCKET_MAX_CONNECTIONS_PER_USER` | 每使用者最大連線數 | `5` |
| `WEBSOCKET_MAX_SUBSCRIPTIONS_PER_CONNECTION` | 每連線最大頻道訂閱數 | `50` |

### 離線訊息佇列

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `QUEUE_ENABLED` | 是否啟用離線訊息佇列 | `false` |
| `QUEUE_MAX_SIZE_PER_USER` | 每使用者最大佇列訊息數 | `100` |
| `QUEUE_MESSAGE_TTL_SECONDS` | 訊息存活時間（秒） | `3600` |
| `QUEUE_CLEANUP_INTERVAL_SECONDS` | 清理過期訊息間隔（秒） | `300` |

### 限流配置

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `RATELIMIT_ENABLED` | 是否啟用請求限流 | `false` |
| `RATELIMIT_HTTP_REQUESTS_PER_SECOND` | HTTP 請求限制（每秒） | `100` |
| `RATELIMIT_HTTP_BURST_SIZE` | HTTP 請求突發容量 | `200` |
| `RATELIMIT_WS_CONNECTIONS_PER_MINUTE` | WebSocket 連線限制（每分鐘/每 IP） | `10` |

### ACK 確認追蹤

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `ACK_ENABLED` | 是否啟用 ACK 追蹤 | `false` |
| `ACK_TIMEOUT_SECONDS` | ACK 超時時間（秒） | `30` |
| `ACK_CLEANUP_INTERVAL_SECONDS` | 清理過期 ACK 間隔（秒） | `60` |

### 多租戶配置

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `TENANT_ENABLED` | 是否啟用多租戶模式 | `false` |
| `TENANT_DEFAULT_MAX_CONNECTIONS` | 預設租戶最大連線數 | `1000` |
| `TENANT_DEFAULT_MAX_CONNECTIONS_PER_USER` | 預設租戶每用戶連線數 | `5` |

### Redis 高可用

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `REDIS_CIRCUIT_BREAKER_FAILURE_THRESHOLD` | 熔斷器開啟閾值 | `5` |
| `REDIS_CIRCUIT_BREAKER_SUCCESS_THRESHOLD` | 熔斷器關閉閾值 | `2` |
| `REDIS_CIRCUIT_BREAKER_RESET_TIMEOUT_SECONDS` | 熔斷器重置超時 | `30` |
| `REDIS_BACKOFF_INITIAL_DELAY_MS` | 退避初始延遲 | `100` |
| `REDIS_BACKOFF_MAX_DELAY_MS` | 退避最大延遲 | `30000` |

### OpenTelemetry 配置

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `OTEL_ENABLED` | 是否啟用 OpenTelemetry 追蹤 | `false` |
| `OTEL_ENDPOINT` | OTLP gRPC 端點 | `http://localhost:4317` |
| `OTEL_SERVICE_NAME` | 服務名稱 | `ara-notification-service` |
| `OTEL_SAMPLING_RATIO` | 取樣比率 (0.0-1.0) | `1.0` |

## 整合範例

### Symfony (HTTP)

```php
$this->httpClient->request('POST', 'http://notification:8081/api/v1/notifications/send', [
    'headers' => [
        'X-API-Key' => $_ENV['NOTIFICATION_API_KEY'],
    ],
    'json' => [
        'target_user_id' => $userId,
        'event_type' => 'order.created',
        'payload' => ['order_id' => $orderId, 'amount' => 99.99],
        'priority' => 'High',
        'ttl' => 3600,
    ],
]);
```

### Symfony (使用模板)

```php
$this->httpClient->request('POST', 'http://notification:8081/api/v1/notifications/send', [
    'headers' => [
        'X-API-Key' => $_ENV['NOTIFICATION_API_KEY'],
    ],
    'json' => [
        'target_user_id' => $userId,
        'template_id' => 'order-shipped',
        'variables' => [
            'order_id' => $orderId,
            'tracking_number' => $trackingNumber,
        ],
    ],
]);
```

### Symfony (Redis)

```php
$redis->publish('notification:user:' . $userId, json_encode([
    'type' => 'user',
    'target' => $userId,
    'event' => [
        'event_type' => 'order.created',
        'payload' => ['order_id' => $orderId],
        'priority' => 'High',
    ],
]));
```

### JavaScript Client (WebSocket)

```javascript
const ws = new WebSocket('ws://localhost:8081/ws?token=' + jwtToken);

ws.onopen = () => {
    // 訂閱頻道
    ws.send(JSON.stringify({
        type: 'Subscribe',
        payload: { channels: ['orders', 'system-alerts'] }
    }));
};

ws.onmessage = (event) => {
    const msg = JSON.parse(event.data);
    if (msg.type === 'notification') {
        console.log('收到通知:', msg.event_type, msg.payload);
        // 發送 ACK 確認
        ws.send(JSON.stringify({
            type: 'Ack',
            payload: { notification_id: msg.id }
        }));
    }
};
```

### JavaScript Client (SSE)

```javascript
const token = 'your-jwt-token';
const eventSource = new EventSource(`/sse?token=${token}`);

eventSource.addEventListener('connected', (e) => {
    console.log('Connected:', JSON.parse(e.data).connection_id);
});

eventSource.addEventListener('notification', (e) => {
    const notification = JSON.parse(e.data);
    console.log('Received:', notification.event_type, notification.payload);
});

eventSource.onerror = (e) => {
    console.error('SSE error:', e);
    eventSource.close();
};
```

## 專案結構

```
src/
├── main.rs                     # 進入點、服務啟動
├── lib.rs                      # 模組匯出
├── config/                     # 配置模組
│   └── settings.rs             # Settings 結構與載入
├── server/                     # 伺服器建構
│   ├── app.rs                  # Axum 路由與中介層
│   ├── middleware.rs           # API Key 認證、限流中介層
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
│   └── registry.rs             # ConnectionManager 三索引設計
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
│   └── handlers.rs             # health, stats, metrics handlers
├── metrics/                    # Prometheus 指標
│   └── mod.rs                  # 指標定義與匯出
├── telemetry/                  # OpenTelemetry
│   └── mod.rs                  # 追蹤初始化
└── error/                      # 錯誤處理
    └── mod.rs                  # AppError 定義
```

## 文件

- [API 規格](docs/API.md) - 完整的 API 文件
- [系統架構](docs/ARCHITECTURE.md) - 詳細架構說明
- [開發路線圖](docs/ROADMAP.md) - 進階功能開發計畫
- [變更記錄](CHANGELOG.md) - 版本變更歷史
- [貢獻指南](CONTRIBUTING.md) - 貢獻程式碼指引

## 開發

```bash
# 格式化程式碼
cargo fmt

# 執行 Linter
cargo clippy

# 執行測試
cargo test

# 執行特定測試
cargo test test_valid_token

# 建構 Release 版本
cargo build --release
```

### 負載測試

使用 K6 執行負載測試：

```bash
# 安裝 K6
# macOS: brew install k6
# Windows: choco install k6

# 執行測試（需要設定環境變數）
export JWT_TOKEN="your-jwt-token"
export API_KEY="your-api-key"

# 執行所有測試
./tests/load/run-tests.sh all baseline

# 執行特定測試
./tests/load/run-tests.sh websocket       # WebSocket 連線測試
./tests/load/run-tests.sh http-api high   # HTTP API 高負載測試
./tests/load/run-tests.sh e2e stress      # 端對端壓力測試
```

可用 Profile: `smoke`, `baseline`, `medium`, `high`, `stress`, `soak`, `spike`

詳細說明請參閱 [tests/load/README.md](tests/load/README.md)

## 授權

MIT License
