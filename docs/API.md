# API 規格文檔

Ara Notification Service API 完整規格說明。

## 目錄

- [WebSocket API](#websocket-api)
- [HTTP REST API](#http-rest-api)
- [Redis Pub/Sub](#redis-pubsub)
- [資料結構](#資料結構)
- [錯誤處理](#錯誤處理)

---

## WebSocket API

### 連線端點

```
ws://localhost:8081/ws?token=<JWT_TOKEN>
```

或使用 Header 認證：

```
ws://localhost:8081/ws
Authorization: Bearer <JWT_TOKEN>
```

### JWT Token 格式

JWT 使用 HS256 演算法簽名。

```json
{
  "sub": "user-123",
  "exp": 1735200000,
  "iat": 1735113600,
  "roles": ["user", "admin"]
}
```

| 欄位 | 類型 | 必填 | 說明 |
|------|------|------|------|
| `sub` | string | Y | 用戶唯一識別碼 |
| `exp` | number | Y | 過期時間 (Unix timestamp) |
| `iat` | number | N | 簽發時間 (Unix timestamp) |
| `roles` | string[] | N | 用戶角色列表，預設空陣列 |

---

### 客戶端訊息 (Client → Server)

#### Subscribe - 訂閱頻道

訂閱一個或多個頻道以接收該頻道的通知。

```json
{
  "type": "Subscribe",
  "payload": {
    "channels": ["orders", "system-alerts"]
  }
}
```

**頻道名稱規則**：
- 長度：1-64 字元
- 允許字元：英數字、dash (`-`)、underscore (`_`)、dot (`.`)

#### Unsubscribe - 取消訂閱

```json
{
  "type": "Unsubscribe",
  "payload": {
    "channels": ["orders"]
  }
}
```

#### Ping - 心跳檢測

```json
{
  "type": "Ping"
}
```

---

### 伺服器訊息 (Server → Client)

#### notification - 通知事件

```json
{
  "type": "notification",
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "occurred_at": "2025-12-26T10:30:00Z",
  "event_type": "order.created",
  "payload": {
    "order_id": "ORD-12345",
    "amount": 99.99
  },
  "metadata": {
    "source": "http-api",
    "priority": "Normal",
    "ttl": 3600,
    "audience": null,
    "correlation_id": "req-123"
  }
}
```

| 欄位 | 類型 | 說明 |
|------|------|------|
| `id` | UUID | 通知唯一識別碼 |
| `occurred_at` | ISO 8601 | 事件發生時間 |
| `event_type` | string | 事件類型標識 |
| `payload` | object | 事件資料內容 |
| `metadata.source` | string | 來源服務名稱 |
| `metadata.priority` | enum | Low, Normal, High, Critical |
| `metadata.ttl` | number? | 生存時間 (秒) |
| `metadata.audience` | object? | 目標受眾過濾 |
| `metadata.correlation_id` | string? | 追蹤關聯 ID |

#### subscribed - 訂閱確認

```json
{
  "type": "subscribed",
  "payload": ["orders", "system-alerts"]
}
```

#### unsubscribed - 取消訂閱確認

```json
{
  "type": "unsubscribed",
  "payload": ["orders"]
}
```

#### pong - 心跳回應

```json
{
  "type": "pong"
}
```

#### heartbeat - 伺服器心跳

伺服器定期發送（預設每 30 秒）以維持連線活性。

```json
{
  "type": "heartbeat"
}
```

#### error - 錯誤訊息

```json
{
  "type": "error",
  "code": "INVALID_MESSAGE",
  "message": "Failed to parse client message: expected ..."
}
```

| 錯誤碼 | 說明 |
|--------|------|
| `INVALID_MESSAGE` | 訊息格式錯誤或 JSON 解析失敗 |
| `UNSUPPORTED_FORMAT` | 不支援的訊息格式（如 Binary） |

---

## HTTP REST API

### POST /api/v1/notifications/send

發送點對點通知給特定用戶的所有裝置。

**Request**

```json
{
  "target_user_id": "user-123",
  "event_type": "order.shipped",
  "payload": {
    "order_id": "ORD-456",
    "tracking_number": "TRACK-789"
  },
  "priority": "High",
  "ttl": 3600,
  "correlation_id": "req-001"
}
```

| 欄位 | 類型 | 必填 | 說明 |
|------|------|------|------|
| `target_user_id` | string | Y | 目標用戶 ID |
| `event_type` | string | Y | 事件類型 |
| `payload` | object | Y | 事件資料 |
| `priority` | enum | N | Low/Normal/High/Critical，預設 Normal |
| `ttl` | number | N | 生存時間 (秒) |
| `correlation_id` | string | N | 追蹤關聯 ID |

**Response**

```json
{
  "success": true,
  "notification_id": "550e8400-e29b-41d4-a716-446655440000",
  "delivered_to": 2,
  "failed": 0,
  "timestamp": "2025-12-26T10:30:00Z"
}
```

---

### POST /api/v1/notifications/send-to-users

發送通知給多個指定用戶。

**Request**

```json
{
  "target_user_ids": ["user-1", "user-2", "user-3"],
  "event_type": "group.message",
  "payload": {
    "content": "Hello team!",
    "from": "admin"
  },
  "priority": "Normal",
  "ttl": 1800,
  "correlation_id": "group-msg-001"
}
```

| 欄位 | 類型 | 必填 | 說明 |
|------|------|------|------|
| `target_user_ids` | string[] | Y | 目標用戶 ID 列表 |
| `event_type` | string | Y | 事件類型 |
| `payload` | object | Y | 事件資料 |
| `priority` | enum | N | 預設 Normal |
| `ttl` | number | N | 生存時間 (秒) |
| `correlation_id` | string | N | 追蹤關聯 ID |

**Response**

```json
{
  "success": true,
  "notification_id": "550e8400-e29b-41d4-a716-446655440001",
  "delivered_to": 5,
  "failed": 1,
  "timestamp": "2025-12-26T10:30:00Z"
}
```

---

### POST /api/v1/notifications/broadcast

廣播通知給所有連線用戶。

**Request**

```json
{
  "event_type": "system.maintenance",
  "payload": {
    "scheduled_at": "2025-12-27T02:00:00Z",
    "duration_minutes": 30,
    "message": "系統將進行定期維護"
  },
  "priority": "Critical",
  "ttl": 7200,
  "audience": {
    "type": "Roles",
    "value": ["user"]
  },
  "correlation_id": "maint-001"
}
```

| 欄位 | 類型 | 必填 | 說明 |
|------|------|------|------|
| `event_type` | string | Y | 事件類型 |
| `payload` | object | Y | 事件資料 |
| `priority` | enum | N | 預設 Normal |
| `ttl` | number | N | 生存時間 (秒) |
| `audience` | object | N | 受眾過濾 |
| `correlation_id` | string | N | 追蹤關聯 ID |

**Response**

```json
{
  "success": true,
  "notification_id": "550e8400-e29b-41d4-a716-446655440002",
  "delivered_to": 150,
  "failed": 0,
  "timestamp": "2025-12-26T10:30:00Z"
}
```

---

### POST /api/v1/notifications/channel

發送通知到特定頻道的所有訂閱者。

**Request**

```json
{
  "channel": "orders",
  "event_type": "order.status_changed",
  "payload": {
    "order_id": "ORD-456",
    "old_status": "pending",
    "new_status": "processing"
  },
  "priority": "High",
  "ttl": 3600,
  "correlation_id": "order-update-001"
}
```

| 欄位 | 類型 | 必填 | 說明 |
|------|------|------|------|
| `channel` | string | Y | 目標頻道名稱 |
| `event_type` | string | Y | 事件類型 |
| `payload` | object | Y | 事件資料 |
| `priority` | enum | N | 預設 Normal |
| `ttl` | number | N | 生存時間 (秒) |
| `correlation_id` | string | N | 追蹤關聯 ID |

**Response**

```json
{
  "success": true,
  "notification_id": "550e8400-e29b-41d4-a716-446655440003",
  "delivered_to": 25,
  "failed": 0,
  "timestamp": "2025-12-26T10:30:00Z"
}
```

---

### POST /api/v1/notifications/channels

發送通知到多個頻道（自動去重跨頻道的重複連線）。

**Request**

```json
{
  "channels": ["orders", "inventory", "shipping"],
  "event_type": "stock.update",
  "payload": {
    "product_id": "SKU-001",
    "quantity": 50,
    "warehouse": "TW-TPE"
  },
  "priority": "Normal",
  "ttl": 1800,
  "correlation_id": "stock-update-001"
}
```

| 欄位 | 類型 | 必填 | 說明 |
|------|------|------|------|
| `channels` | string[] | Y | 目標頻道名稱列表 |
| `event_type` | string | Y | 事件類型 |
| `payload` | object | Y | 事件資料 |
| `priority` | enum | N | 預設 Normal |
| `ttl` | number | N | 生存時間 (秒) |
| `correlation_id` | string | N | 追蹤關聯 ID |

**Response**

```json
{
  "success": true,
  "notification_id": "550e8400-e29b-41d4-a716-446655440004",
  "delivered_to": 60,
  "failed": 0,
  "timestamp": "2025-12-26T10:30:00Z"
}
```

---

### GET /health

健康檢查端點。

**Response**

```json
{
  "status": "healthy",
  "version": "0.1.0"
}
```

---

### GET /stats

連線與通知統計資訊。

**Response**

```json
{
  "connections": {
    "total_connections": 150,
    "unique_users": 120,
    "channels": {
      "orders": 45,
      "system-alerts": 150,
      "admin": 5
    }
  },
  "notifications": {
    "total_sent": 10250,
    "total_delivered": 10248,
    "total_failed": 2,
    "user_notifications": 5000,
    "broadcast_notifications": 1000,
    "channel_notifications": 4250
  }
}
```

---

## Redis Pub/Sub

### 預設訂閱頻道

服務預設訂閱以下 Pattern：

```
notification:user:*        # 點對點通知
notification:broadcast     # 全域廣播
notification:channel:*     # 頻道通知
```

### 訊息格式

發送到 Redis 頻道的訊息必須符合以下 JSON 格式：

```json
{
  "type": "user|users|broadcast|channel|channels",
  "target": "string|[string]|null",
  "event": {
    "event_type": "string",
    "payload": {},
    "priority": "Low|Normal|High|Critical",
    "ttl": 3600,
    "correlation_id": "string"
  }
}
```

| 欄位 | 類型 | 說明 |
|------|------|------|
| `type` | enum | 目標類型 |
| `target` | string/array/null | 目標值（依 type 而定） |
| `event.event_type` | string | 事件類型 |
| `event.payload` | object | 事件資料 |
| `event.priority` | enum | 優先級（選填，預設 Normal） |
| `event.ttl` | number | 生存時間秒數（選填） |
| `event.correlation_id` | string | 追蹤 ID（選填） |

### Target 類型對照

| type | target 格式 | 說明 |
|------|-------------|------|
| `user` | `"user-123"` | 單一用戶 |
| `users` | `["user-1", "user-2"]` 或 `"user-1"` | 多個用戶 |
| `broadcast` | `null` | 所有連線 |
| `channel` | `"orders"` | 單一頻道 |
| `channels` | `["orders", "inventory"]` 或 `"orders"` | 多個頻道 |

### Redis CLI 範例

```bash
# 單一用戶
redis-cli PUBLISH "notification:user:user-123" '{
  "type": "user",
  "target": "user-123",
  "event": {
    "event_type": "message.new",
    "payload": {"from": "user-456", "content": "Hello!"},
    "priority": "Normal"
  }
}'

# 多個用戶
redis-cli PUBLISH "notification:broadcast" '{
  "type": "users",
  "target": ["user-1", "user-2", "user-3"],
  "event": {
    "event_type": "team.update",
    "payload": {"action": "member_added"},
    "priority": "Normal"
  }
}'

# 廣播
redis-cli PUBLISH "notification:broadcast" '{
  "type": "broadcast",
  "target": null,
  "event": {
    "event_type": "system.announcement",
    "payload": {"message": "Welcome!"},
    "priority": "Critical"
  }
}'

# 單一頻道
redis-cli PUBLISH "notification:channel:orders" '{
  "type": "channel",
  "target": "orders",
  "event": {
    "event_type": "order.new",
    "payload": {"order_id": "123"},
    "priority": "High",
    "ttl": 3600,
    "correlation_id": "order-123"
  }
}'

# 多個頻道
redis-cli PUBLISH "notification:channel:orders" '{
  "type": "channels",
  "target": ["orders", "inventory"],
  "event": {
    "event_type": "stock.low",
    "payload": {"product_id": "SKU-001", "quantity": 5},
    "priority": "High"
  }
}'
```

---

## 資料結構

### NotificationEvent

```typescript
interface NotificationEvent {
  id: string;                    // UUID v4
  occurred_at: string;           // ISO 8601 datetime
  event_type: string;            // 事件類型標識
  payload: object;               // 事件資料
  metadata: {
    source: string;              // 來源服務
    priority: Priority;          // 優先級
    ttl: number | null;          // 生存時間 (秒)
    audience: Audience | null;   // 目標受眾
    correlation_id: string | null; // 追蹤 ID
  };
}
```

### Priority

| 值 | 權重 | 說明 |
|----|------|------|
| `Low` | 1 | 低優先級，可延遲處理 |
| `Normal` | 2 | 一般優先級（預設） |
| `High` | 3 | 高優先級，優先處理 |
| `Critical` | 4 | 緊急，立即處理 |

### Audience

```typescript
type Audience =
  | { type: "All" }
  | { type: "Roles", value: string[] }
  | { type: "Users", value: string[] }
  | { type: "Channels", value: string[] };
```

### DeliveryResult

HTTP 回應的送達結果：

```typescript
interface DeliveryResult {
  success: boolean;        // delivered_to > 0
  notification_id: string; // UUID
  delivered_to: number;    // 成功送達連線數
  failed: number;          // 送達失敗連線數
  timestamp: string;       // ISO 8601
}
```

---

## 錯誤處理

### HTTP 狀態碼

| 狀態碼 | 說明 |
|--------|------|
| 200 | 成功 |
| 400 | 請求格式錯誤 |
| 401 | 未授權 (JWT/API Key 無效) |
| 422 | 驗證錯誤 |
| 500 | 伺服器內部錯誤 |

### WebSocket 關閉碼

| 關閉碼 | 說明 |
|--------|------|
| 1000 | 正常關閉 |
| 1008 | 認證失敗 (Missing/Invalid token) |
