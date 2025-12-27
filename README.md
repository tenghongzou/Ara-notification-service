# Ara Notification Service

高效能即時通知服務，使用 Rust 建構，提供 WebSocket 即時推播功能。

## 特性

- **WebSocket 即時推播** - 低延遲、高效能的即時通訊
- **JWT 認證** - 安全的身份驗證機制，支援角色權限
- **多種訊息模式** - 點對點、廣播、頻道訂閱
- **雙觸發機制** - HTTP REST API 與 Redis Pub/Sub
- **多裝置支援** - 同一使用者可在多裝置同時接收通知
- **離線訊息佇列** - 離線使用者重連時自動重播訊息
- **通知模板系統** - 可重用模板與 `{{variable}}` 變數替換
- **請求限流** - Token Bucket 演算法保護系統資源
- **心跳檢測** - 自動偵測並清理閒置連線
- **Prometheus 監控** - 完整的指標匯出 (`/metrics`)
- **OpenTelemetry 追蹤** - 分散式追蹤整合 (Jaeger, Tempo, Zipkin)
- **K6 負載測試** - 完整測試套件含 7 種負載場景
- **完全無狀態** - 純記憶體運作，適合水平擴展
- **安全防護** - API Key 認證、連線限制、CORS 控制、錯誤遮蔽

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
│  │  connections │ user_index │ channel_index              │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────┬───────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    WebSocket 客戶端                          │
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

# 執行測試
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

### WebSocket 連線

```
ws://localhost:8081/ws?token=<JWT>
```

或使用 Authorization Header：
```
ws://localhost:8081/ws
Authorization: Bearer <JWT>
```

### HTTP REST API

受保護的 API 需要 `X-API-Key` Header：

```bash
curl -X POST http://localhost:8081/api/v1/notifications/send \
  -H "X-API-Key: your-api-key" \
  -H "Content-Type: application/json" \
  -d '{"target_user_id": "user123", "event_type": "test", "payload": {}}'
```

| 方法 | 路徑 | 認證 | 說明 |
|------|------|------|------|
| POST | `/api/v1/notifications/send` | API Key | 點對點通知 |
| POST | `/api/v1/notifications/send-to-users` | API Key | 多使用者通知 |
| POST | `/api/v1/notifications/broadcast` | API Key | 廣播通知 |
| POST | `/api/v1/notifications/channel` | API Key | 頻道通知 |
| POST | `/api/v1/notifications/channels` | API Key | 多頻道通知 |
| POST | `/api/v1/notifications/batch` | API Key | 批次發送（最多 100 筆） |
| GET | `/api/v1/channels` | API Key | 頻道列表與訂閱數 |
| GET | `/api/v1/channels/{name}` | API Key | 頻道詳情 |
| GET | `/api/v1/users/{user_id}/subscriptions` | API Key | 使用者訂閱列表 |
| GET | `/stats` | API Key | 連線統計 |
| GET | `/health` | 無 | 健康檢查 |
| WS | `/ws` | JWT | WebSocket 連線 |

### Redis Pub/Sub 頻道

```
notification:user:{user_id}     # 點對點
notification:broadcast          # 廣播
notification:channel:{name}     # 頻道
```

詳細 API 規格請參閱 [docs/API.md](docs/API.md)

## 配置

### 環境變數

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `SERVER_HOST` | 服務監聽位址 | `0.0.0.0` |
| `SERVER_PORT` | 服務監聽埠 | `8081` |
| `JWT_SECRET` | JWT 簽名密鑰 (HS256) | (必填) |
| `JWT_ISSUER` | JWT 簽發者驗證 | (選填) |
| `JWT_AUDIENCE` | JWT 受眾驗證 | (選填) |
| `REDIS_URL` | Redis 連線 URL | `redis://localhost:6379` |
| `RUST_LOG` | 日誌等級 | `info` |

### WebSocket 配置

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `WEBSOCKET_HEARTBEAT_INTERVAL` | 心跳間隔 (秒) | `30` |
| `WEBSOCKET_CONNECTION_TIMEOUT` | 連線超時 (秒) | `120` |
| `WEBSOCKET_CLEANUP_INTERVAL` | 清理任務間隔 (秒) | `60` |
| `WEBSOCKET_MAX_CONNECTIONS` | 最大總連線數 (0=無限) | `10000` |
| `WEBSOCKET_MAX_CONNECTIONS_PER_USER` | 每使用者最大連線數 (0=無限) | `5` |
| `WEBSOCKET_MAX_SUBSCRIPTIONS_PER_CONNECTION` | 每連線最大頻道訂閱數 (0=無限) | `50` |

### 安全配置

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `RUN_MODE` | 執行模式 (`development` / `production`) | `development` |
| `API_KEY` | HTTP API 認證金鑰 | (選填，建議生產環境必填) |
| `CORS_ORIGINS` | 允許的來源 (逗號分隔) | (空=允許全部) |

#### 安全機制說明

| 機制 | 說明 |
|------|------|
| **API Key 認證** | HTTP API 端點需要 `X-API-Key` Header，開發模式若未設定則跳過驗證 |
| **連線限制** | 限制總連線數與每使用者連線數，防止資源耗盡攻擊 |
| **頻道訂閱限制** | 限制每連線可訂閱的頻道數量 |
| **CORS 控制** | 生產環境限制允許的來源，開發模式警告並允許全部 |
| **請求大小限制** | API 請求 body 限制為 64KB |
| **錯誤遮蔽** | 生產模式下隱藏內部錯誤詳情，僅回傳通用錯誤訊息 |
| **JWT 驗證** | WebSocket 連線需要有效的 JWT Token |

### 訊息佇列配置

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `QUEUE_ENABLED` | 是否啟用離線訊息佇列 | `false` |
| `QUEUE_MAX_SIZE_PER_USER` | 每使用者最大佇列訊息數 | `100` |
| `QUEUE_MESSAGE_TTL_SECONDS` | 訊息存活時間（秒） | `3600` (1小時) |
| `QUEUE_CLEANUP_INTERVAL_SECONDS` | 清理過期訊息間隔（秒） | `300` (5分鐘) |

**離線訊息佇列說明**：當使用者離線時，發送給該使用者的通知會被暫存在記憶體佇列中。使用者重新連線時，所有未過期的訊息會按順序自動重播。佇列採用 FIFO 策略，當佇列滿時會丟棄最舊的訊息。

### 限流配置

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `RATELIMIT_ENABLED` | 是否啟用請求限流 | `false` |
| `RATELIMIT_HTTP_REQUESTS_PER_SECOND` | HTTP 請求限制（每秒） | `100` |
| `RATELIMIT_HTTP_BURST_SIZE` | HTTP 請求突發容量 | `200` |
| `RATELIMIT_WS_CONNECTIONS_PER_MINUTE` | WebSocket 連線限制（每分鐘/每 IP） | `10` |
| `RATELIMIT_WS_MESSAGES_PER_SECOND` | WebSocket 訊息限制（每秒/每連線） | `50` |

**限流機制說明**：採用 Token Bucket 演算法，支援請求突發的同時保護系統資源。限流適用於 HTTP API（按 API Key 或 IP）與 WebSocket 連線（按 IP）。超過限制時回傳 `429 Too Many Requests` 並包含 `Retry-After` 標頭。

### Redis 高可用配置

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `REDIS_CIRCUIT_BREAKER_FAILURE_THRESHOLD` | 熔斷器開啟所需的連續失敗次數 | `5` |
| `REDIS_CIRCUIT_BREAKER_SUCCESS_THRESHOLD` | 熔斷器關閉所需的成功次數（半開狀態） | `2` |
| `REDIS_CIRCUIT_BREAKER_RESET_TIMEOUT_SECONDS` | 熔斷器重置超時（秒） | `30` |
| `REDIS_BACKOFF_INITIAL_DELAY_MS` | 指數退避初始延遲（毫秒） | `100` |
| `REDIS_BACKOFF_MAX_DELAY_MS` | 指數退避最大延遲（毫秒） | `30000` |

**Redis 高可用說明**：當 Redis 連線失敗時，系統會使用熔斷器模式保護資源。連續失敗達到閾值後，熔斷器開啟並停止嘗試連線。等待重置超時後，進入半開狀態嘗試重新連線。重連採用指數退避策略（含 10% 抖動），避免雪崩效應。健康狀態可透過 `/health` 和 `/stats` 端點監控。

### OpenTelemetry 配置

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `OTEL_ENABLED` | 是否啟用 OpenTelemetry 追蹤 | `false` |
| `OTEL_ENDPOINT` | OTLP gRPC 端點 | `http://localhost:4317` |
| `OTEL_SERVICE_NAME` | 服務名稱 | `ara-notification-service` |
| `OTEL_SAMPLING_RATIO` | 取樣比率 (0.0-1.0) | `1.0` |

**OpenTelemetry 說明**：啟用後，服務會將追蹤資料透過 OTLP gRPC 協定匯出到指定端點（如 Jaeger、Tempo、Zipkin）。追蹤範圍涵蓋 HTTP 請求、WebSocket 連線與訊息處理、通知派發等關鍵路徑。生產環境建議調整取樣比率以控制資料量。

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

### JavaScript Client

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
    }
};
```

## 專案結構

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

## 文件

- [API 規格](docs/API.md) - 完整的 API 文件
- [系統架構](docs/ARCHITECTURE.md) - 詳細架構說明
- [開發路線圖](docs/ROADMAP.md) - 進階功能開發計畫
- [貢獻指南](CONTRIBUTING.md) - 貢獻程式碼指引
- [變更記錄](CHANGELOG.md) - 版本變更歷史

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
