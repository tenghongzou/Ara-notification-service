# 安裝與部署

本文件說明如何安裝、配置與部署 Ara Notification Service。

---

## 環境需求

### 必要軟體

| 軟體 | 最低版本 | 建議版本 | 說明 |
|------|---------|---------|------|
| **Rust** | 1.75 | 最新 stable | 編譯與執行 |
| **Redis** | 6.0 | 7.0+ | Pub/Sub、分散式狀態 |

### 選用軟體

| 軟體 | 用途 |
|------|------|
| **Docker** | 容器化部署 |
| **PostgreSQL** | 持久化佇列、ACK 記錄 |
| **K6** | 負載測試 |

---

## 本地開發

### 1. 複製專案

```bash
# 進入服務目錄
cd /srv/Ara-infra/services/notification
```

### 2. 環境變數設定

```bash
# 複製範例檔
cp .env.example .env

# 編輯 .env
vim .env
```

**最小配置：**

```bash
# 必填：JWT 簽名密鑰（至少 32 字元）
JWT_SECRET=your-super-secure-secret-key-at-least-32-chars

# Redis 連線
REDIS_URL=redis://localhost:6379

# 日誌等級
RUST_LOG=info
```

### 3. 啟動 Redis

```bash
# 使用 Docker
docker run -d --name redis -p 6379:6379 redis:7-alpine

# 或本地安裝
redis-server
```

### 4. 建構與執行

```bash
# Debug 建構
cargo build

# 執行服務
cargo run

# 或一步完成
cargo run --release
```

### 5. 驗證安裝

```bash
# 健康檢查
curl http://localhost:8081/health

# 預期回應
{"status":"healthy","components":{"redis":"connected"}}
```

---

## Docker 部署

### 使用預建映像

```bash
# 建構映像
docker build -t ara-notification-service .

# 執行容器
docker run -d \
  --name notification \
  -p 8081:8081 \
  -e JWT_SECRET=your-secret-key-at-least-32-chars \
  -e REDIS_URL=redis://redis:6379 \
  -e RUN_MODE=production \
  ara-notification-service
```

### Docker Compose 整合

服務已整合至 Ara-infra 主專案的 `docker-compose.yml`：

```bash
# 從 Ara-infra 根目錄
cd /srv/Ara-infra

# 啟動通知服務
docker-compose up -d notification

# 查看日誌
docker-compose logs -f notification

# 重啟服務
docker-compose restart notification
```

### Dockerfile 說明

```dockerfile
# 多階段建構
FROM rust:1.75-alpine AS builder

WORKDIR /app
COPY . .

# 建構 Release 版本
RUN cargo build --release

# 最終映像
FROM alpine:3.19

# 安裝執行時依賴
RUN apk add --no-cache ca-certificates

COPY --from=builder /app/target/release/ara-notification-service /usr/local/bin/

EXPOSE 8081

CMD ["ara-notification-service"]
```

---

## 配置參考

### 核心配置

| 變數 | 說明 | 預設值 | 必填 |
|------|------|--------|------|
| `SERVER_HOST` | 監聽位址 | `0.0.0.0` | 否 |
| `SERVER_PORT` | 監聽埠 | `8081` | 否 |
| `RUN_MODE` | 執行模式 | `development` | 否 |
| `JWT_SECRET` | JWT 簽名密鑰 | - | **是** |
| `JWT_ISSUER` | JWT 簽發者驗證 | - | 否 |
| `JWT_AUDIENCE` | JWT 受眾驗證 | - | 否 |
| `REDIS_URL` | Redis 連線 URL | `redis://localhost:6379` | 否 |
| `API_KEY` | HTTP API 認證金鑰 | - | 生產環境建議 |
| `CORS_ORIGINS` | 允許的來源 | - (允許全部) | 生產環境建議 |
| `RUST_LOG` | 日誌等級 | `info` | 否 |

### WebSocket 配置

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `WEBSOCKET_HEARTBEAT_INTERVAL` | 心跳間隔 (秒) | `30` |
| `WEBSOCKET_CONNECTION_TIMEOUT` | 連線超時 (秒) | `120` |
| `WEBSOCKET_CLEANUP_INTERVAL` | 清理任務間隔 (秒) | `60` |
| `WEBSOCKET_MAX_CONNECTIONS` | 最大總連線數 | `10000` |
| `WEBSOCKET_MAX_CONNECTIONS_PER_USER` | 每使用者最大連線 | `5` |
| `WEBSOCKET_MAX_SUBSCRIPTIONS_PER_CONNECTION` | 每連線最大頻道數 | `50` |

### Redis 高可用配置

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `REDIS_CIRCUIT_BREAKER_FAILURE_THRESHOLD` | 熔斷器開啟閾值 | `5` |
| `REDIS_CIRCUIT_BREAKER_SUCCESS_THRESHOLD` | 熔斷器關閉閾值 | `2` |
| `REDIS_CIRCUIT_BREAKER_RESET_TIMEOUT_SECONDS` | 熔斷器重置超時 | `30` |
| `REDIS_BACKOFF_INITIAL_DELAY_MS` | 退避初始延遲 | `100` |
| `REDIS_BACKOFF_MAX_DELAY_MS` | 退避最大延遲 | `30000` |

### 功能開關

| 變數 | 說明 | 預設值 |
|------|------|--------|
| `QUEUE_ENABLED` | 離線訊息佇列 | `false` |
| `RATELIMIT_ENABLED` | 請求限流 | `false` |
| `ACK_ENABLED` | ACK 追蹤 | `false` |
| `TENANT_ENABLED` | 多租戶模式 | `false` |
| `CLUSTER_ENABLED` | 叢集模式 | `false` |
| `OTEL_ENABLED` | OpenTelemetry 追蹤 | `false` |

---

## 生產環境配置

### 建議配置範例

```bash
# 生產環境 .env
RUN_MODE=production

# 安全性
JWT_SECRET=your-production-secret-key-minimum-32-characters
API_KEY=your-api-key-for-http-endpoints
CORS_ORIGINS=https://app.example.com,https://admin.example.com

# 連線設定
REDIS_URL=redis://redis.internal:6379

# 連線限制
WEBSOCKET_MAX_CONNECTIONS=50000
WEBSOCKET_MAX_CONNECTIONS_PER_USER=10

# 啟用功能
QUEUE_ENABLED=true
QUEUE_MAX_SIZE_PER_USER=200
QUEUE_MESSAGE_TTL_SECONDS=7200

RATELIMIT_ENABLED=true
RATELIMIT_HTTP_REQUESTS_PER_SECOND=500
RATELIMIT_HTTP_BURST_SIZE=1000

ACK_ENABLED=true
ACK_TIMEOUT_SECONDS=60

# 可觀測性
OTEL_ENABLED=true
OTEL_ENDPOINT=http://otel-collector:4317
OTEL_SERVICE_NAME=ara-notification-production

RUST_LOG=warn,ara_notification_service=info
```

### 資源配置建議

| 規模 | 並發連線 | CPU | 記憶體 | Redis |
|------|---------|-----|-------|-------|
| 小型 | < 1,000 | 1 核 | 512 MB | 1 GB |
| 中型 | < 10,000 | 2 核 | 1 GB | 2 GB |
| 大型 | < 50,000 | 4 核 | 2 GB | 4 GB |
| 超大 | 50,000+ | 8+ 核 | 4+ GB | 8+ GB + 叢集 |

---

## 健康檢查

### 端點

```bash
# 基本健康檢查
GET /health

# 詳細健康資訊
GET /health?detailed=true
```

### 回應格式

```json
{
  "status": "healthy",
  "version": "1.0.0",
  "uptime_seconds": 3600,
  "components": {
    "redis": "connected",
    "websocket": "ready",
    "queue": "enabled",
    "rate_limiter": "enabled"
  },
  "stats": {
    "connections": 1234,
    "users_connected": 567,
    "channels_active": 89
  }
}
```

### Kubernetes 探針

```yaml
# deployment.yaml
spec:
  containers:
    - name: notification
      livenessProbe:
        httpGet:
          path: /health
          port: 8081
        initialDelaySeconds: 5
        periodSeconds: 10
      readinessProbe:
        httpGet:
          path: /health
          port: 8081
        initialDelaySeconds: 3
        periodSeconds: 5
```

---

## 資料庫遷移

如果使用 PostgreSQL 作為佇列或 ACK 後端：

```bash
# 建立資料庫
createdb ara_notification

# 執行遷移
psql -d ara_notification -f migrations/001_create_message_queue.sql
psql -d ara_notification -f migrations/002_create_pending_acks.sql
psql -d ara_notification -f migrations/003_create_ack_stats.sql
```

**遷移檔案說明：**

| 檔案 | 用途 |
|------|------|
| `001_create_message_queue.sql` | 離線訊息佇列表 |
| `002_create_pending_acks.sql` | 待確認通知表 |
| `003_create_ack_stats.sql` | ACK 統計表 |

---

## 故障排除

### 常見問題

#### 1. JWT 驗證失敗

```
Error: Invalid token
```

**解決方案：**
- 確認 `JWT_SECRET` 與後端簽發使用相同密鑰
- 檢查 token 是否過期
- 驗證 `JWT_ISSUER` 和 `JWT_AUDIENCE` 設定

#### 2. Redis 連線失敗

```
Error: Redis connection refused
```

**解決方案：**
- 確認 Redis 服務已啟動
- 檢查 `REDIS_URL` 格式正確
- 驗證網路連通性

#### 3. 連線數達到上限

```
Error: Connection limit exceeded
```

**解決方案：**
- 調整 `WEBSOCKET_MAX_CONNECTIONS`
- 檢查是否有連線洩漏
- 考慮啟用叢集模式

#### 4. 記憶體使用過高

**解決方案：**
- 減少 `QUEUE_MAX_SIZE_PER_USER`
- 縮短 `QUEUE_MESSAGE_TTL_SECONDS`
- 啟用 Redis 或 PostgreSQL 後端

---

## 相關文件

- [系統架構](./01-architecture.md)
- [API 參考](./03-api-reference.md)
- [進階功能](./05-advanced-features.md)
- [可觀測性](./06-observability.md)

