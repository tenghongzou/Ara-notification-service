# 進階功能

本文件說明 Ara Notification Service 的進階功能配置與使用方式。

---

## 離線訊息佇列

當使用者離線時，通知會被暫存到佇列中，待使用者重新連線時自動重播。

### 啟用配置

```bash
QUEUE_ENABLED=true
QUEUE_MAX_SIZE_PER_USER=100      # 每使用者最大佇列訊息數
QUEUE_MESSAGE_TTL_SECONDS=3600   # 訊息存活時間 (秒)
QUEUE_CLEANUP_INTERVAL_SECONDS=300  # 清理過期訊息間隔
```

### 後端選擇

| 後端 | 配置 | 特性 |
|------|------|------|
| Memory | `QUEUE_BACKEND=memory` | 預設，重啟後遺失 |
| Redis | `QUEUE_BACKEND=redis` | 持久化，分散式 |
| PostgreSQL | `QUEUE_BACKEND=postgres` | 持久化，可查詢 |

### 運作流程

```
發送通知
    │
    ▼
使用者在線？ ──是──▶ 直接發送
    │
    否
    │
    ▼
加入佇列 ──▶ 設定 TTL
    │
    ▼
使用者重連 ──▶ 重播佇列訊息
```

### Redis 後端配置

```bash
QUEUE_BACKEND=redis
QUEUE_REDIS_KEY_PREFIX=ara:queue:
```

### PostgreSQL 後端配置

```bash
QUEUE_BACKEND=postgres
DATABASE_URL=postgres://user:pass@localhost/ara_notification
```

---

## ACK 確認追蹤

追蹤通知是否已被客戶端確認接收。

### 啟用配置

```bash
ACK_ENABLED=true
ACK_TIMEOUT_SECONDS=30           # ACK 超時時間
ACK_CLEANUP_INTERVAL_SECONDS=60  # 清理過期 ACK 間隔
```

### 後端選擇

| 後端 | 配置 | 特性 |
|------|------|------|
| Memory | `ACK_BACKEND=memory` | 預設，重啟後遺失 |
| Redis | `ACK_BACKEND=redis` | 持久化，分散式 |
| PostgreSQL | `ACK_BACKEND=postgres` | 持久化，可分析 |

### 客戶端 ACK 流程

```javascript
// WebSocket 客戶端
ws.onmessage = (event) => {
    const msg = JSON.parse(event.data);
    if (msg.type === 'notification') {
        // 處理通知
        handleNotification(msg);

        // 發送 ACK 確認
        ws.send(JSON.stringify({
            type: 'Ack',
            payload: { notification_id: msg.id }
        }));
    }
};
```

### ACK 統計 API

```http
GET /api/v1/stats/acks

{
  "pending": 45,
  "acknowledged": 12345,
  "expired": 23,
  "average_ack_time_ms": 150
}
```

---

## 通知模板系統

預定義通知模板，支援變數替換。

### 建立模板

```http
POST /api/v1/templates

{
  "id": "order-shipped",
  "name": "Order Shipped Notification",
  "event_type": "order.shipped",
  "payload_template": {
    "title": "Order {{order_id}} Shipped!",
    "body": "Your order is on the way. Tracking: {{tracking_number}}",
    "action": {
      "type": "open_url",
      "url": "/orders/{{order_id}}/tracking"
    }
  },
  "priority": "High",
  "ttl": 86400
}
```

### 使用模板發送

```http
POST /api/v1/notifications/send

{
  "target_user_id": "user-123",
  "template_id": "order-shipped",
  "variables": {
    "order_id": "ORD-001",
    "tracking_number": "TRK-12345"
  }
}
```

### 變數語法

| 語法 | 說明 |
|------|------|
| `{{variable}}` | 基本變數替換 |
| `{{nested.field}}` | 巢狀物件存取 |

### 模板管理

```bash
# 列出所有模板
GET /api/v1/templates

# 取得單一模板
GET /api/v1/templates/{id}

# 更新模板
PUT /api/v1/templates/{id}

# 刪除模板
DELETE /api/v1/templates/{id}
```

---

## 請求限流

使用 Token Bucket 演算法保護系統資源。

### 啟用配置

```bash
RATELIMIT_ENABLED=true
RATELIMIT_HTTP_REQUESTS_PER_SECOND=100    # HTTP 請求限制
RATELIMIT_HTTP_BURST_SIZE=200              # HTTP 突發容量
RATELIMIT_WS_CONNECTIONS_PER_MINUTE=10     # WebSocket 連線限制
```

### 後端選擇

| 後端 | 配置 | 特性 |
|------|------|------|
| Local | `RATELIMIT_BACKEND=local` | 預設，單節點 |
| Redis | `RATELIMIT_BACKEND=redis` | 分散式限流 |

### 限流回應

當請求被限流時，回傳 HTTP 429：

```json
{
  "error": {
    "code": "RATE_LIMIT_EXCEEDED",
    "message": "Too many requests",
    "retry_after_seconds": 5
  }
}
```

### 限流策略

| 類型 | 識別方式 | 配置 |
|------|---------|------|
| HTTP API | API Key 或 IP | `RATELIMIT_HTTP_*` |
| WebSocket | IP | `RATELIMIT_WS_*` |

---

## 多租戶支援

隔離不同租戶的連線、頻道與統計。

### 啟用配置

```bash
TENANT_ENABLED=true
TENANT_DEFAULT_MAX_CONNECTIONS=1000        # 預設租戶連線上限
TENANT_DEFAULT_MAX_CONNECTIONS_PER_USER=5  # 預設每使用者連線
```

### JWT 租戶識別

租戶透過 JWT 的 `tenant_id` 聲明識別：

```json
{
  "sub": "user-123",
  "tenant_id": "acme-corp",
  "exp": 1704067200
}
```

### 頻道命名空間

啟用多租戶後，頻道自動加上租戶前綴：

```
原始頻道: orders
實際頻道: tenant-acme-corp:orders
```

### 租戶 API

```bash
# 列出所有租戶
GET /api/v1/tenants

# 租戶詳情
GET /api/v1/tenants/{tenant_id}

{
  "tenant_id": "acme-corp",
  "connections": 234,
  "users": 89,
  "channels": 12,
  "messages_today": 5678
}
```

### 租戶特定限制

```bash
# 特定租戶配置 (透過 API 或設定檔)
TENANT_ACME_CORP_MAX_CONNECTIONS=5000
TENANT_ACME_CORP_MAX_CONNECTIONS_PER_USER=10
```

---

## 叢集模式

多節點部署，支援跨節點使用者路由。

### 啟用配置

```bash
CLUSTER_ENABLED=true
CLUSTER_NODE_ID=node-1              # 唯一節點識別
CLUSTER_SESSION_STORE=redis         # 會話儲存後端
```

### 會話儲存後端

| 後端 | 配置 | 特性 |
|------|------|------|
| Local | `CLUSTER_SESSION_STORE=local` | 單節點，無叢集 |
| Redis | `CLUSTER_SESSION_STORE=redis` | 分散式會話 |

### 運作原理

```
                    ┌─────────────────────────┐
                    │      Redis 會話儲存     │
                    │  user-123 → node-1     │
                    │  user-456 → node-2     │
                    └───────────┬─────────────┘
                                │
        ┌───────────────────────┼───────────────────────┐
        │                       │                       │
        ▼                       ▼                       ▼
  ┌──────────┐           ┌──────────┐           ┌──────────┐
  │  Node 1  │           │  Node 2  │           │  Node 3  │
  │  user-123 ◀──────────│  發送通知  │──────────▶│         │
  │  user-789│           │  to user-123        │  user-456│
  └──────────┘           └──────────┘           └──────────┘
```

### 叢集 API

```bash
# 叢集狀態
GET /api/v1/cluster/status

{
  "node_id": "node-1",
  "nodes": ["node-1", "node-2", "node-3"],
  "healthy_nodes": 3,
  "total_connections": 15000
}

# 查詢使用者位置
GET /api/v1/cluster/users/{user_id}

{
  "user_id": "user-123",
  "node_id": "node-1",
  "connections": 2
}
```

---

## 批次發送

單次請求發送多筆通知。

### API

```http
POST /api/v1/notifications/batch

{
  "notifications": [
    {
      "target": { "type": "user", "id": "user-1" },
      "event_type": "message.new",
      "payload": { "text": "Hello" }
    },
    {
      "target": { "type": "channel", "name": "orders" },
      "event_type": "order.created",
      "payload": { "order_id": "123" }
    }
  ],
  "atomic": false
}
```

### 參數說明

| 參數 | 說明 |
|------|------|
| `notifications` | 通知陣列，最多 100 筆 |
| `atomic` | 是否原子操作 (全成功或全失敗) |

### 回應

```json
{
  "results": [
    { "index": 0, "success": true, "notification_id": "..." },
    { "index": 1, "success": false, "error": "Channel not found" }
  ],
  "successful": 1,
  "failed": 1
}
```

---

## 功能組合配置

### 基礎配置

```bash
# 最小配置
JWT_SECRET=your-secret
REDIS_URL=redis://localhost:6379
```

### 標準生產配置

```bash
RUN_MODE=production
JWT_SECRET=your-production-secret
REDIS_URL=redis://redis:6379
API_KEY=your-api-key

# 啟用核心功能
QUEUE_ENABLED=true
RATELIMIT_ENABLED=true
ACK_ENABLED=true
```

### 企業級配置

```bash
RUN_MODE=production

# 安全
JWT_SECRET=your-production-secret
API_KEY=your-api-key
CORS_ORIGINS=https://app.example.com

# 多租戶
TENANT_ENABLED=true
TENANT_DEFAULT_MAX_CONNECTIONS=5000

# 叢集
CLUSTER_ENABLED=true
CLUSTER_SESSION_STORE=redis

# 持久化
QUEUE_ENABLED=true
QUEUE_BACKEND=postgres
ACK_ENABLED=true
ACK_BACKEND=postgres
DATABASE_URL=postgres://user:pass@db/ara

# 限流 (分散式)
RATELIMIT_ENABLED=true
RATELIMIT_BACKEND=redis

# 可觀測性
OTEL_ENABLED=true
OTEL_ENDPOINT=http://otel-collector:4317
```

---

## 相關文件

- [系統架構](./01-architecture.md)
- [API 參考](./03-api-reference.md)
- [可觀測性](./06-observability.md)

