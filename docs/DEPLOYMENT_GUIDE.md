# Ara 通知服務部署指南

本文件提供 Ara Notification Service 的完整部署說明，包含 Ara-infra 整合、生產環境配置與監控設定。

## 目錄

- [快速開始](#快速開始)
- [Ara-infra 整合部署](#ara-infra-整合部署)
- [獨立部署](#獨立部署)
- [環境變數配置](#環境變數配置)
- [分布式叢集模式](#分布式叢集模式)
- [持久化後端設定](#持久化後端設定)
- [生產環境最佳實踐](#生產環境最佳實踐)
- [監控與可觀測性](#監控與可觀測性)
- [故障排除](#故障排除)

---

## 快速開始

### 系統需求

| 組件 | 最低版本 | 建議版本 |
|------|---------|---------|
| Docker | 20.10+ | 24.0+ |
| Docker Compose | 2.0+ | 2.21+ |
| Redis | 6.2+ | 7.0+ |
| PostgreSQL (選用) | 14+ | 17+ |

### 5 分鐘快速啟動

```bash
# 1. 進入 Ara-infra 根目錄
cd /path/to/Ara-infra

# 2. 設定環境變數
export JWT_SECRET="your-secure-jwt-secret-at-least-32-chars"

# 3. 啟動所有服務
docker-compose up -d

# 4. 驗證服務健康狀態
curl http://localhost:8081/health
# 預期回應: {"status":"healthy","version":"1.0.0",...}

# 5. 檢視連線統計
curl http://localhost:8081/stats
```

---

## Ara-infra 整合部署

### 專案結構

```
Ara-infra/
├── docker-compose.yml          # 主要部署配置
├── docker/
│   └── services/
│       └── notification/
│           └── Dockerfile      # 通知服務容器定義
├── services/
│   └── notification/           # 通知服務原始碼
│       ├── src/
│       ├── Cargo.toml
│       └── ...
├── backend/                    # Symfony 後端
└── administration/             # SvelteKit 管理介面
```

### docker-compose.yml 配置

通知服務已整合至 Ara-infra 的 docker-compose.yml：

```yaml
services:
  # ... 其他服務 ...

  notification:
    build:
      context: ./docker/services/notification
      dockerfile: Dockerfile
    container_name: ara_notification
    ports:
      - "8081:8081"
    environment:
      # 基本配置
      RUST_LOG: info
      SERVER_HOST: "0.0.0.0"
      SERVER_PORT: "8081"

      # JWT 認證 (必填)
      JWT_SECRET: "${JWT_SECRET:-your-jwt-secret-here}"

      # Redis 連線
      REDIS_URL: "redis://redis:6379"
      REDIS_CHANNELS: "notification:user:*,notification:broadcast,notification:channel:*"

      # WebSocket 配置
      WEBSOCKET_HEARTBEAT_INTERVAL: "30"
      WEBSOCKET_CONNECTION_TIMEOUT: "120"
      WEBSOCKET_CLEANUP_INTERVAL: "60"
    depends_on:
      - redis
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8081/health"]
      interval: 30s
      timeout: 3s
      retries: 3
      start_period: 10s
```

### 啟動服務

```bash
# 啟動所有服務
docker-compose up -d

# 僅啟動通知服務 (及其依賴)
docker-compose up -d notification

# 查看日誌
docker-compose logs -f notification

# 重新建構並啟動
docker-compose up -d --build notification
```

### 與 Symfony 後端整合

在 Symfony 應用程式中發送通知：

```php
// src/Service/NotificationService.php
<?php

namespace App\Service;

use Symfony\Contracts\HttpClient\HttpClientInterface;

class NotificationService
{
    public function __construct(
        private HttpClientInterface $httpClient,
        private string $notificationApiKey,
    ) {}

    /**
     * 發送點對點通知
     */
    public function sendToUser(
        string $userId,
        string $eventType,
        array $payload,
        string $priority = 'Normal'
    ): void {
        $this->httpClient->request('POST', 'http://notification:8081/api/v1/notifications/send', [
            'headers' => [
                'X-API-Key' => $this->notificationApiKey,
                'Content-Type' => 'application/json',
            ],
            'json' => [
                'target_user_id' => $userId,
                'event_type' => $eventType,
                'payload' => $payload,
                'priority' => $priority,
            ],
        ]);
    }

    /**
     * 使用模板發送通知
     */
    public function sendWithTemplate(
        string $userId,
        string $templateId,
        array $variables
    ): void {
        $this->httpClient->request('POST', 'http://notification:8081/api/v1/notifications/send', [
            'headers' => [
                'X-API-Key' => $this->notificationApiKey,
            ],
            'json' => [
                'target_user_id' => $userId,
                'template_id' => $templateId,
                'variables' => $variables,
            ],
        ]);
    }

    /**
     * 發送頻道通知
     */
    public function sendToChannel(string $channel, string $eventType, array $payload): void
    {
        $this->httpClient->request('POST', 'http://notification:8081/api/v1/notifications/channel', [
            'headers' => [
                'X-API-Key' => $this->notificationApiKey,
            ],
            'json' => [
                'channel' => $channel,
                'event_type' => $eventType,
                'payload' => $payload,
            ],
        ]);
    }

    /**
     * 廣播通知給所有連線使用者
     */
    public function broadcast(string $eventType, array $payload): void
    {
        $this->httpClient->request('POST', 'http://notification:8081/api/v1/notifications/broadcast', [
            'headers' => [
                'X-API-Key' => $this->notificationApiKey,
            ],
            'json' => [
                'event_type' => $eventType,
                'payload' => $payload,
            ],
        ]);
    }
}
```

### 透過 Redis Pub/Sub 發送

```php
// 使用 Redis Pub/Sub 直接發送 (適合高頻率場景)
$redis = new \Redis();
$redis->connect('redis', 6379);

// 點對點通知
$redis->publish('notification:user:user123', json_encode([
    'type' => 'user',
    'target' => 'user123',
    'event' => [
        'event_type' => 'order.created',
        'payload' => ['order_id' => 'ORD-001', 'amount' => 99.99],
        'priority' => 'High',
    ],
]));

// 頻道通知
$redis->publish('notification:channel:orders', json_encode([
    'type' => 'channel',
    'target' => 'orders',
    'event' => [
        'event_type' => 'order.status_changed',
        'payload' => ['order_id' => 'ORD-001', 'status' => 'shipped'],
    ],
]));

// 廣播
$redis->publish('notification:broadcast', json_encode([
    'type' => 'broadcast',
    'event' => [
        'event_type' => 'system.maintenance',
        'payload' => ['message' => '系統將於 10 分鐘後進行維護'],
    ],
]));
```

---

## 獨立部署

### 本地開發

```bash
# 進入通知服務目錄
cd services/notification

# 設定環境變數
export JWT_SECRET="dev-secret-key-at-least-32-characters"
export REDIS_URL="redis://localhost:6379"
export RUST_LOG="debug"

# 建構並執行
cargo build --release
./target/release/ara-notification-service
```

### Docker 獨立建構

```bash
# 建構映像檔
cd services/notification
docker build -t ara-notification-service:latest .

# 執行容器
docker run -d \
  --name notification \
  -p 8081:8081 \
  -e JWT_SECRET="your-secure-secret" \
  -e REDIS_URL="redis://host.docker.internal:6379" \
  -e RUST_LOG="info" \
  ara-notification-service:latest
```

---

## 環境變數配置

### 核心配置

| 變數 | 必填 | 預設值 | 說明 |
|------|------|--------|------|
| `JWT_SECRET` | **是** | - | JWT 簽名密鑰 (建議 32+ 字元) |
| `SERVER_HOST` | 否 | `0.0.0.0` | 服務監聽位址 |
| `SERVER_PORT` | 否 | `8081` | 服務監聽埠 |
| `REDIS_URL` | 否 | `redis://localhost:6379` | Redis 連線 URL |
| `API_KEY` | 否 | - | HTTP API 認證金鑰 |
| `RUN_MODE` | 否 | `development` | 執行模式 (`development`/`production`) |
| `RUST_LOG` | 否 | `info` | 日誌等級 (`error`/`warn`/`info`/`debug`/`trace`) |

### WebSocket 配置

| 變數 | 預設值 | 說明 |
|------|--------|------|
| `WEBSOCKET_HEARTBEAT_INTERVAL` | `30` | 心跳間隔 (秒) |
| `WEBSOCKET_CONNECTION_TIMEOUT` | `120` | 閒置超時 (秒) |
| `WEBSOCKET_CLEANUP_INTERVAL` | `60` | 清理任務間隔 (秒) |
| `WEBSOCKET_MAX_CONNECTIONS` | `10000` | 最大總連線數 |
| `WEBSOCKET_MAX_CONNECTIONS_PER_USER` | `5` | 每使用者最大連線數 |
| `WEBSOCKET_MAX_SUBSCRIPTIONS_PER_CONNECTION` | `50` | 每連線最大頻道訂閱數 |

### 離線訊息佇列

| 變數 | 預設值 | 說明 |
|------|--------|------|
| `QUEUE_ENABLED` | `false` | 是否啟用離線訊息佇列 |
| `QUEUE_BACKEND` | `memory` | 後端類型 (`memory`/`redis`/`postgres`) |
| `QUEUE_MAX_SIZE_PER_USER` | `100` | 每使用者最大佇列訊息數 |
| `QUEUE_MESSAGE_TTL_SECONDS` | `3600` | 訊息存活時間 (秒) |

### ACK 確認追蹤

| 變數 | 預設值 | 說明 |
|------|--------|------|
| `ACK_ENABLED` | `false` | 是否啟用 ACK 追蹤 |
| `ACK_BACKEND` | `memory` | 後端類型 (`memory`/`redis`/`postgres`) |
| `ACK_TIMEOUT_SECONDS` | `30` | ACK 超時時間 (秒) |

### 多租戶配置

| 變數 | 預設值 | 說明 |
|------|--------|------|
| `TENANT_ENABLED` | `false` | 是否啟用多租戶模式 |
| `TENANT_DEFAULT_MAX_CONNECTIONS` | `1000` | 預設租戶最大連線數 |
| `TENANT_DEFAULT_MAX_CONNECTIONS_PER_USER` | `5` | 預設租戶每用戶連線數 |

### 限流配置

| 變數 | 預設值 | 說明 |
|------|--------|------|
| `RATELIMIT_ENABLED` | `false` | 是否啟用請求限流 |
| `RATELIMIT_BACKEND` | `memory` | 後端類型 (`memory`/`redis`) |
| `RATELIMIT_HTTP_REQUESTS_PER_SECOND` | `100` | HTTP 請求限制 (每秒) |
| `RATELIMIT_HTTP_BURST_SIZE` | `200` | HTTP 請求突發容量 |
| `RATELIMIT_WS_CONNECTIONS_PER_MINUTE` | `10` | WebSocket 連線限制 (每分鐘/每 IP) |

---

## 分布式叢集模式

### 啟用叢集模式

叢集模式允許多個通知服務實例協同工作，透過 Redis 同步會話資訊和路由訊息。

```yaml
# docker-compose.cluster.yml
services:
  notification-1:
    build:
      context: ./docker/services/notification
    environment:
      # 叢集配置
      CLUSTER_ENABLED: "true"
      CLUSTER_SERVER_ID: "notification-1"
      CLUSTER_SESSION_PREFIX: "ara:cluster:sessions"
      CLUSTER_SESSION_TTL_SECONDS: "60"
      CLUSTER_ROUTING_CHANNEL: "ara:cluster:route"

      # 其他配置
      JWT_SECRET: "${JWT_SECRET}"
      REDIS_URL: "redis://redis:6379"
    ports:
      - "8081:8081"

  notification-2:
    build:
      context: ./docker/services/notification
    environment:
      CLUSTER_ENABLED: "true"
      CLUSTER_SERVER_ID: "notification-2"
      CLUSTER_SESSION_PREFIX: "ara:cluster:sessions"
      CLUSTER_SESSION_TTL_SECONDS: "60"
      CLUSTER_ROUTING_CHANNEL: "ara:cluster:route"
      JWT_SECRET: "${JWT_SECRET}"
      REDIS_URL: "redis://redis:6379"
    ports:
      - "8082:8081"

  # 使用負載均衡器
  nginx:
    image: nginx:alpine
    ports:
      - "80:80"
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf
    depends_on:
      - notification-1
      - notification-2
```

### 叢集環境變數

| 變數 | 預設值 | 說明 |
|------|--------|------|
| `CLUSTER_ENABLED` | `false` | 是否啟用叢集模式 |
| `CLUSTER_SERVER_ID` | 自動生成 | 伺服器唯一識別碼 |
| `CLUSTER_SESSION_PREFIX` | `ara:cluster:sessions` | Redis 會話資料前綴 |
| `CLUSTER_SESSION_TTL_SECONDS` | `60` | 會話 TTL (應大於心跳間隔) |
| `CLUSTER_ROUTING_CHANNEL` | `ara:cluster:route` | 跨伺服器訊息路由頻道 |

### Nginx 負載均衡配置

```nginx
# nginx.conf
upstream notification_cluster {
    # 使用 IP Hash 確保同一使用者的連線固定到同一伺服器
    ip_hash;
    server notification-1:8081;
    server notification-2:8081;
}

server {
    listen 80;

    # WebSocket 連線
    location /ws {
        proxy_pass http://notification_cluster;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_read_timeout 86400;
    }

    # SSE 連線
    location /sse {
        proxy_pass http://notification_cluster;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_buffering off;
        proxy_cache off;
        proxy_read_timeout 86400;
    }

    # HTTP API
    location /api/ {
        proxy_pass http://notification_cluster;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }

    # 健康檢查 & 監控
    location ~ ^/(health|stats|metrics)$ {
        proxy_pass http://notification_cluster;
    }
}
```

### 叢集 API 端點

| 端點 | 說明 |
|------|------|
| `GET /api/v1/cluster/status` | 叢集狀態資訊 |
| `GET /api/v1/cluster/users/{user_id}` | 使用者所在伺服器列表 |

---

## 持久化後端設定

### Redis 後端

適用於需要跨重啟保留資料的場景：

```yaml
environment:
  # 離線訊息佇列使用 Redis
  QUEUE_ENABLED: "true"
  QUEUE_BACKEND: "redis"

  # ACK 追蹤使用 Redis
  ACK_ENABLED: "true"
  ACK_BACKEND: "redis"

  # 限流使用 Redis (分布式場景)
  RATELIMIT_ENABLED: "true"
  RATELIMIT_BACKEND: "redis"
```

### PostgreSQL 後端

適用於需要長期儲存和查詢的場景：

```yaml
environment:
  # 資料庫連線
  DATABASE_URL: "postgresql://user:password@postgres:5432/notification"

  # 離線訊息佇列使用 PostgreSQL
  QUEUE_ENABLED: "true"
  QUEUE_BACKEND: "postgres"

  # ACK 追蹤使用 PostgreSQL
  ACK_ENABLED: "true"
  ACK_BACKEND: "postgres"
```

需要的資料表 (自動建立)：

```sql
-- 離線訊息佇列
CREATE TABLE IF NOT EXISTS notification_queue (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    tenant_id VARCHAR(255) DEFAULT 'default',
    message JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_queue_user_tenant ON notification_queue(user_id, tenant_id);
CREATE INDEX idx_queue_expires ON notification_queue(expires_at);

-- ACK 追蹤
CREATE TABLE IF NOT EXISTS notification_acks (
    notification_id UUID PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    tenant_id VARCHAR(255) DEFAULT 'default',
    connection_id UUID NOT NULL,
    sent_at TIMESTAMPTZ NOT NULL,
    acked_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_acks_user ON notification_acks(user_id);
CREATE INDEX idx_acks_expires ON notification_acks(expires_at);
```

---

## 生產環境最佳實踐

### 安全性設定

```yaml
environment:
  # 強制使用安全的 JWT 密鑰
  JWT_SECRET: "${JWT_SECRET}"  # 從環境變數注入，不要硬編碼

  # 啟用 API Key 認證
  API_KEY: "${NOTIFICATION_API_KEY}"

  # 設定 CORS (限制允許的來源)
  CORS_ORIGINS: "https://app.example.com,https://admin.example.com"

  # 生產模式 (隱藏內部錯誤詳情)
  RUN_MODE: "production"
  RUST_LOG: "warn"
```

### 資源限制

```yaml
notification:
  deploy:
    resources:
      limits:
        cpus: '2'
        memory: 2G
      reservations:
        cpus: '0.5'
        memory: 512M
    restart_policy:
      condition: on-failure
      delay: 5s
      max_attempts: 3
```

### 連線限制

```yaml
environment:
  # 合理的連線限制
  WEBSOCKET_MAX_CONNECTIONS: "50000"
  WEBSOCKET_MAX_CONNECTIONS_PER_USER: "10"
  WEBSOCKET_MAX_SUBSCRIPTIONS_PER_CONNECTION: "100"

  # 啟用限流
  RATELIMIT_ENABLED: "true"
  RATELIMIT_HTTP_REQUESTS_PER_SECOND: "1000"
  RATELIMIT_WS_CONNECTIONS_PER_MINUTE: "30"
```

### Redis 高可用

```yaml
environment:
  # 熔斷器配置
  REDIS_CIRCUIT_BREAKER_FAILURE_THRESHOLD: "5"
  REDIS_CIRCUIT_BREAKER_SUCCESS_THRESHOLD: "2"
  REDIS_CIRCUIT_BREAKER_RESET_TIMEOUT_SECONDS: "30"

  # 退避重連
  REDIS_BACKOFF_INITIAL_DELAY_MS: "100"
  REDIS_BACKOFF_MAX_DELAY_MS: "30000"
```

---

## 監控與可觀測性

### Prometheus 監控

通知服務在 `/metrics` 端點提供 Prometheus 格式的指標：

```bash
curl http://localhost:8081/metrics
```

主要指標：

| 指標 | 類型 | 說明 |
|------|------|------|
| `ara_connections_total` | Gauge | 當前活躍連線數 |
| `ara_connections_by_transport` | Gauge | 按傳輸類型 (ws/sse) 分類的連線數 |
| `ara_messages_sent_total` | Counter | 發送的訊息總數 |
| `ara_messages_delivered_total` | Counter | 成功送達的訊息數 |
| `ara_ws_messages_received_total` | Counter | 接收的 WebSocket 訊息數 |
| `ara_ack_tracked_total` | Counter | 追蹤的 ACK 總數 |
| `ara_ack_received_total` | Counter | 收到的 ACK 總數 |
| `ara_queue_messages` | Gauge | 佇列中的訊息數 |
| `ara_ratelimit_requests_total` | Counter | 限流請求總數 |
| `ara_redis_operations_total` | Counter | Redis 操作總數 |
| `ara_cluster_connections_total` | Gauge | 叢集總連線數 |

### Prometheus 配置

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'notification'
    static_configs:
      - targets: ['notification:8081']
    metrics_path: /metrics
    scrape_interval: 15s
```

### Grafana 儀表板

建議監控的面板：

1. **連線狀態**
   - 活躍連線數趨勢
   - 按傳輸類型分佈
   - 每使用者連線分佈

2. **訊息吞吐量**
   - 每秒發送/送達訊息數
   - 按類型分類 (user/broadcast/channel)
   - 失敗率

3. **系統健康**
   - Redis 連線狀態
   - 熔斷器狀態
   - 錯誤率

### OpenTelemetry 追蹤

```yaml
environment:
  OTEL_ENABLED: "true"
  OTEL_ENDPOINT: "http://jaeger:4317"
  OTEL_SERVICE_NAME: "ara-notification-service"
  OTEL_SAMPLING_RATIO: "0.1"  # 生產環境建議 10% 取樣
```

---

## 故障排除

### 常見問題

#### 1. WebSocket 連線失敗

**症狀：** 客戶端無法建立 WebSocket 連線

**檢查步驟：**
```bash
# 檢查服務健康狀態
curl http://localhost:8081/health

# 檢查 JWT Token 有效性
curl -v "ws://localhost:8081/ws?token=YOUR_TOKEN"

# 檢查服務日誌
docker-compose logs -f notification
```

**常見原因：**
- JWT Token 過期或無效
- JWT_SECRET 不匹配
- 防火牆阻擋 WebSocket 升級

#### 2. Redis 連線問題

**症狀：** 日誌顯示 Redis 連線錯誤

**檢查步驟：**
```bash
# 測試 Redis 連線
docker-compose exec redis redis-cli ping

# 檢查 Redis URL 配置
docker-compose exec notification env | grep REDIS
```

#### 3. 訊息未送達

**檢查步驟：**
```bash
# 檢查連線統計
curl http://localhost:8081/stats

# 確認使用者已連線
curl "http://localhost:8081/api/v1/users/USER_ID/subscriptions" \
  -H "X-API-Key: YOUR_API_KEY"
```

#### 4. 高記憶體使用

**可能原因：**
- 大量離線訊息堆積
- 未配置訊息 TTL
- 連線未正確清理

**解決方案：**
```yaml
environment:
  QUEUE_MESSAGE_TTL_SECONDS: "3600"
  QUEUE_MAX_SIZE_PER_USER: "100"
  WEBSOCKET_CLEANUP_INTERVAL: "30"
```

### 日誌等級

```bash
# 除錯模式 (詳細日誌)
RUST_LOG=debug docker-compose up notification

# 追蹤模式 (最詳細)
RUST_LOG=trace docker-compose up notification

# 只顯示錯誤
RUST_LOG=error docker-compose up notification
```

### 健康檢查端點

| 端點 | 說明 |
|------|------|
| `GET /health` | 服務健康狀態 |
| `GET /stats` | 連線和訊息統計 |
| `GET /metrics` | Prometheus 指標 |

---

## 附錄：完整 docker-compose 範例

```yaml
version: "3.9"

services:
  notification:
    build:
      context: ./docker/services/notification
      dockerfile: Dockerfile
    container_name: ara_notification
    ports:
      - "8081:8081"
    environment:
      # 基本配置
      RUST_LOG: "info"
      RUN_MODE: "production"
      SERVER_HOST: "0.0.0.0"
      SERVER_PORT: "8081"

      # 認證
      JWT_SECRET: "${JWT_SECRET}"
      API_KEY: "${NOTIFICATION_API_KEY}"

      # Redis
      REDIS_URL: "redis://redis:6379"
      REDIS_CHANNELS: "notification:user:*,notification:broadcast,notification:channel:*"

      # WebSocket
      WEBSOCKET_HEARTBEAT_INTERVAL: "30"
      WEBSOCKET_CONNECTION_TIMEOUT: "120"
      WEBSOCKET_MAX_CONNECTIONS: "10000"
      WEBSOCKET_MAX_CONNECTIONS_PER_USER: "5"

      # 離線訊息佇列
      QUEUE_ENABLED: "true"
      QUEUE_BACKEND: "redis"
      QUEUE_MAX_SIZE_PER_USER: "100"
      QUEUE_MESSAGE_TTL_SECONDS: "3600"

      # ACK 追蹤
      ACK_ENABLED: "true"
      ACK_BACKEND: "redis"
      ACK_TIMEOUT_SECONDS: "30"

      # 限流
      RATELIMIT_ENABLED: "true"
      RATELIMIT_HTTP_REQUESTS_PER_SECOND: "100"

      # 多租戶
      TENANT_ENABLED: "false"

      # 叢集模式 (單節點部署時關閉)
      CLUSTER_ENABLED: "false"

      # 監控
      OTEL_ENABLED: "false"

    depends_on:
      redis:
        condition: service_healthy
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8081/health"]
      interval: 30s
      timeout: 3s
      retries: 3
      start_period: 10s
    deploy:
      resources:
        limits:
          cpus: '2'
          memory: 1G
        reservations:
          cpus: '0.25'
          memory: 256M

  redis:
    image: redis:7.0-alpine
    container_name: ara_redis
    command: redis-server --appendonly yes
    volumes:
      - redis_data:/data
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 10s
      timeout: 3s
      retries: 3

volumes:
  redis_data:
```

---

## 相關文件

- [API 規格](API.md) - 完整 REST API 文件
- [系統架構](ARCHITECTURE.md) - 詳細架構說明
- [使用範例](USAGE.md) - 客戶端整合範例
- [開發路線圖](ROADMAP.md) - 功能開發計畫
