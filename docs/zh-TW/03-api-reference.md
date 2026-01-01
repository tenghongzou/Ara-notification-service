# API 參考

本文件詳細說明 Ara Notification Service 的所有 API 端點、WebSocket 協定與 Redis Pub/Sub 整合。

---

## 概覽

### 基礎資訊

| 項目 | 值 |
|------|-----|
| 基礎 URL | `http://localhost:8081` |
| WebSocket | `ws://localhost:8081/ws` |
| SSE | `http://localhost:8081/sse` |
| 認證 (HTTP API) | `X-API-Key` Header |
| 認證 (WebSocket/SSE) | JWT Token |

### 端點分類

| 類別 | 認證 | 說明 |
|------|------|------|
| 公開端點 | 無 | 健康檢查、指標 |
| 受保護 API | X-API-Key | REST API 發送通知 |
| 即時連線 | JWT Token | WebSocket、SSE |

---

## 公開端點

### 健康檢查

```http
GET /health
```

**回應：**

```json
{
  "status": "healthy",
  "version": "1.0.0",
  "components": {
    "redis": "connected"
  }
}
```

### Prometheus 指標

```http
GET /metrics
```

**回應：** Prometheus 格式的指標資料

---

## HTTP REST API

所有 REST API 需要 `X-API-Key` Header：

```http
X-API-Key: your-api-key
Content-Type: application/json
```

### 發送通知 (點對點)

```http
POST /api/v1/notifications/send
```

**請求：**

```json
{
  "target_user_id": "user-123",
  "event_type": "order.created",
  "payload": {
    "order_id": "ORD-001",
    "amount": 99.99
  },
  "priority": "High",
  "ttl": 3600
}
```

**使用模板：**

```json
{
  "target_user_id": "user-123",
  "template_id": "order-shipped",
  "variables": {
    "order_id": "ORD-001",
    "tracking_number": "TRK-12345"
  }
}
```

**回應：**

```json
{
  "notification_id": "550e8400-e29b-41d4-a716-446655440000",
  "delivered_to": 2,
  "queued": false
}
```

### 發送給多使用者

```http
POST /api/v1/notifications/send-to-users
```

**請求：**

```json
{
  "target_user_ids": ["user-1", "user-2", "user-3"],
  "event_type": "group.notification",
  "payload": {
    "message": "New group update"
  },
  "priority": "Normal"
}
```

**回應：**

```json
{
  "notification_id": "...",
  "results": {
    "user-1": { "delivered": true, "connections": 2 },
    "user-2": { "delivered": true, "connections": 1 },
    "user-3": { "delivered": false, "queued": true }
  }
}
```

### 廣播通知

```http
POST /api/v1/notifications/broadcast
```

**請求：**

```json
{
  "event_type": "system.maintenance",
  "payload": {
    "message": "Scheduled maintenance in 30 minutes",
    "scheduled_at": "2024-01-01T12:00:00Z"
  },
  "priority": "Critical"
}
```

**回應：**

```json
{
  "notification_id": "...",
  "delivered_to": 1234
}
```

### 頻道通知

```http
POST /api/v1/notifications/channel
```

**請求：**

```json
{
  "channel": "orders",
  "event_type": "order.status_changed",
  "payload": {
    "order_id": "ORD-001",
    "status": "shipped"
  }
}
```

**回應：**

```json
{
  "notification_id": "...",
  "channel": "orders",
  "delivered_to": 45
}
```

### 多頻道通知

```http
POST /api/v1/notifications/channels
```

**請求：**

```json
{
  "channels": ["orders", "inventory", "shipping"],
  "event_type": "stock.updated",
  "payload": {
    "product_id": "PROD-001",
    "quantity": 50
  }
}
```

**回應：**

```json
{
  "notification_id": "...",
  "channels": {
    "orders": { "delivered_to": 45 },
    "inventory": { "delivered_to": 12 },
    "shipping": { "delivered_to": 8 }
  },
  "total_delivered": 65
}
```

### 批次發送

```http
POST /api/v1/notifications/batch
```

**請求：**

```json
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

**限制：** 最多 100 筆通知

**回應：**

```json
{
  "results": [
    { "index": 0, "success": true, "notification_id": "..." },
    { "index": 1, "success": true, "notification_id": "..." }
  ],
  "successful": 2,
  "failed": 0
}
```

---

## 頻道管理

### 列出頻道

```http
GET /api/v1/channels
```

**回應：**

```json
{
  "channels": [
    { "name": "orders", "subscribers": 45 },
    { "name": "alerts", "subscribers": 120 },
    { "name": "system", "subscribers": 200 }
  ]
}
```

### 頻道詳情

```http
GET /api/v1/channels/{name}
```

**回應：**

```json
{
  "name": "orders",
  "subscribers": 45,
  "created_at": "2024-01-01T00:00:00Z"
}
```

### 使用者訂閱列表

```http
GET /api/v1/users/{user_id}/subscriptions
```

**回應：**

```json
{
  "user_id": "user-123",
  "subscriptions": ["orders", "alerts"],
  "connections": 2
}
```

---

## 模板管理

### 建立模板

```http
POST /api/v1/templates
```

**請求：**

```json
{
  "id": "order-shipped",
  "name": "Order Shipped",
  "event_type": "order.shipped",
  "payload_template": {
    "title": "Order {{order_id}} Shipped",
    "body": "Your order is on the way! Tracking: {{tracking_number}}",
    "action_url": "/orders/{{order_id}}"
  },
  "priority": "High",
  "ttl": 86400
}
```

### 列出模板

```http
GET /api/v1/templates
```

### 取得模板

```http
GET /api/v1/templates/{id}
```

### 更新模板

```http
PUT /api/v1/templates/{id}
```

### 刪除模板

```http
DELETE /api/v1/templates/{id}
```

---

## 統計資訊

### 連線統計

```http
GET /api/v1/stats
```

**回應：**

```json
{
  "connections": {
    "total": 1234,
    "websocket": 1100,
    "sse": 134
  },
  "users": {
    "connected": 567,
    "with_multiple_connections": 89
  },
  "channels": {
    "active": 45,
    "total_subscriptions": 2345
  },
  "messages": {
    "sent_today": 12345,
    "delivered_today": 12300,
    "failed_today": 45
  }
}
```

---

## WebSocket 協定

### 連線

```
ws://localhost:8081/ws?token=<JWT>
```

或使用 Header：

```
ws://localhost:8081/ws
Authorization: Bearer <JWT>
```

### 客戶端訊息

#### 訂閱頻道

```json
{
  "type": "Subscribe",
  "payload": {
    "channels": ["orders", "alerts"]
  }
}
```

#### 取消訂閱

```json
{
  "type": "Unsubscribe",
  "payload": {
    "channels": ["orders"]
  }
}
```

#### Ping

```json
{
  "type": "Ping"
}
```

#### ACK 確認

```json
{
  "type": "Ack",
  "payload": {
    "notification_id": "550e8400-e29b-41d4-a716-446655440000"
  }
}
```

### 伺服器訊息

#### 通知

```json
{
  "type": "notification",
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "occurred_at": "2024-01-01T12:00:00Z",
  "event_type": "order.created",
  "payload": {
    "order_id": "ORD-001",
    "amount": 99.99
  },
  "metadata": {
    "source": "order-service",
    "priority": "High",
    "ttl": 3600
  }
}
```

#### 訂閱確認

```json
{
  "type": "subscribed",
  "payload": ["orders", "alerts"]
}
```

#### 取消訂閱確認

```json
{
  "type": "unsubscribed",
  "payload": ["orders"]
}
```

#### Pong

```json
{
  "type": "pong"
}
```

#### 心跳

```json
{
  "type": "heartbeat"
}
```

#### ACK 確認

```json
{
  "type": "acked",
  "notification_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

#### 錯誤

```json
{
  "type": "error",
  "code": "VALIDATION_ERROR",
  "message": "Invalid channel name"
}
```

#### 關閉通知

```json
{
  "type": "shutdown",
  "reason": "Server maintenance",
  "reconnect_after_seconds": 60
}
```

---

## SSE 協定

### 連線

```http
GET /sse?token=<JWT>
```

或使用 Header：

```http
GET /sse
Authorization: Bearer <JWT>
```

### 事件類型

#### connected

連線成功事件：

```
event: connected
data: {"connection_id":"uuid","user_id":"user-123"}
```

#### notification

通知事件：

```
event: notification
data: {"id":"uuid","event_type":"order.created","payload":{...}}
```

#### heartbeat

心跳事件：

```
event: heartbeat
data: {"timestamp":"2024-01-01T12:00:00Z"}
```

---

## Redis Pub/Sub

### 頻道格式

| 頻道模式 | 說明 |
|----------|------|
| `notification:user:{user_id}` | 點對點訊息 |
| `notification:broadcast` | 廣播訊息 |
| `notification:channel:{name}` | 頻道訊息 |

### 訊息格式

**點對點：**

```json
{
  "type": "user",
  "target": "user-123",
  "event": {
    "event_type": "order.created",
    "payload": {
      "order_id": "ORD-001"
    },
    "priority": "High",
    "ttl": 3600
  }
}
```

**廣播：**

```json
{
  "type": "broadcast",
  "event": {
    "event_type": "system.announcement",
    "payload": {
      "message": "System update"
    }
  }
}
```

**頻道：**

```json
{
  "type": "channel",
  "target": "orders",
  "event": {
    "event_type": "order.status_changed",
    "payload": {
      "order_id": "ORD-001",
      "status": "shipped"
    }
  }
}
```

### Symfony 整合範例

```php
// 透過 Redis Pub/Sub 發送
$redis->publish('notification:user:' . $userId, json_encode([
    'type' => 'user',
    'target' => $userId,
    'event' => [
        'event_type' => 'order.created',
        'payload' => ['order_id' => $orderId],
        'priority' => 'High',
    ],
]));

// 廣播
$redis->publish('notification:broadcast', json_encode([
    'type' => 'broadcast',
    'event' => [
        'event_type' => 'system.maintenance',
        'payload' => ['message' => 'Maintenance in 30 minutes'],
    ],
]));

// 頻道
$redis->publish('notification:channel:orders', json_encode([
    'type' => 'channel',
    'target' => 'orders',
    'event' => [
        'event_type' => 'order.status_changed',
        'payload' => ['order_id' => $orderId, 'status' => 'shipped'],
    ],
]));
```

---

## 錯誤處理

### HTTP 錯誤碼

| 狀態碼 | 說明 |
|--------|------|
| 400 | 請求格式錯誤 |
| 401 | 認證失敗 (無效 API Key) |
| 403 | 權限不足 |
| 404 | 資源不存在 |
| 422 | 驗證錯誤 |
| 429 | 請求過於頻繁 (限流) |
| 500 | 伺服器內部錯誤 |

### 錯誤回應格式

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "Invalid event_type format",
    "details": {
      "field": "event_type",
      "constraint": "must not be empty"
    }
  }
}
```

---

## 優先級說明

| 優先級 | 值 | 說明 |
|--------|-----|------|
| Low | `"Low"` | 低優先級，可延遲處理 |
| Normal | `"Normal"` | 一般優先級 (預設) |
| High | `"High"` | 高優先級，優先處理 |
| Critical | `"Critical"` | 緊急，最高優先級 |

---

## 相關文件

- [系統架構](./01-architecture.md)
- [安裝與部署](./02-installation.md)
- [開發指南](./04-development-guide.md)
- [進階功能](./05-advanced-features.md)

