# 單機商業部署 + 多租戶 + 安全

本文件提供「單機商業部署 + 多租戶 + 安全」的建議 `.env` 範本與部署架構圖。

## 建議 .env 範本

> 請先複製 `.env.example`，再依下列範本調整。

```bash
# 執行模式
RUN_MODE=production

# 服務監聽
SERVER_HOST=0.0.0.0
SERVER_PORT=8081

# JWT 與 API 安全
JWT_SECRET=change-me-to-a-strong-secret-min-32-chars
JWT_ISSUER=your-company
JWT_AUDIENCE=your-product
API_KEY=change-me-to-a-strong-api-key

# 多租戶
TENANT_ENABLED=true
TENANT_DEFAULT_MAX_CONNECTIONS=1000
TENANT_DEFAULT_MAX_CONNECTIONS_PER_USER=5

# CORS 限制（僅允許商業前台/後台來源）
CORS_ORIGINS=https://app.example.com,https://admin.example.com

# Redis 連線（建議僅內網）
REDIS_URL=redis://redis.internal:6379

# WebSocket 連線限制
WEBSOCKET_MAX_CONNECTIONS=20000
WEBSOCKET_MAX_CONNECTIONS_PER_USER=10
WEBSOCKET_MAX_SUBSCRIPTIONS_PER_CONNECTION=50

# 限流（避免濫用）
RATELIMIT_ENABLED=true
RATELIMIT_HTTP_REQUESTS_PER_SECOND=500
RATELIMIT_HTTP_BURST_SIZE=1000

# 離線佇列（視需求啟用）
QUEUE_ENABLED=true
QUEUE_MAX_SIZE_PER_USER=200
QUEUE_MESSAGE_TTL_SECONDS=7200

# ACK 追蹤（視需求啟用）
ACK_ENABLED=true
ACK_TIMEOUT_SECONDS=60

# 可觀測性（視需求啟用）
OTEL_ENABLED=false

# 日誌
RUST_LOG=info
```

## 部署架構圖（單機 + 反向代理 + 內網 Redis）

```text
                       ┌───────────────────────────────┐
                       │            Internet           │
                       └───────────────┬───────────────┘
                                       │
                                       ▼
                         ┌───────────────────────────┐
                         │  Reverse Proxy (TLS/WSS)  │
                         │  Nginx/Traefik/etc.       │
                         └──────────────┬────────────┘
                                        │
                                        ▼
                         ┌───────────────────────────┐
                         │ Ara Notification Service  │
                         │  - JWT 驗證               │
                         │  - 多租戶隔離             │
                         │  - API Key 保護           │
                         └──────────────┬────────────┘
                                        │
                                        ▼
                         ┌───────────────────────────┐
                         │       Redis (內網)        │
                         │  Pub/Sub & 狀態           │
                         └───────────────────────────┘
```

## 多租戶 JWT 建議 Claim

```json
{
  "sub": "user-123",
  "tenant_id": "acme-corp",
  "exp": 1704067200,
  "iss": "your-company",
  "aud": "your-product"
}
```
