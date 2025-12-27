# API 規格文檔

Ara Notification Service API 完整規格說明。

## 目錄

- [認證機制](#認證機制)
- [WebSocket API](#websocket-api)
- [SSE API](#sse-api)
- [HTTP REST API](#http-rest-api)
- [Redis Pub/Sub](#redis-pubsub)
- [資料結構](#資料結構)
- [錯誤處理](#錯誤處理)

---

## 認證機制

服務使用兩種認證方式：

| 認證方式 | 適用範圍 | 說明 |
|----------|----------|------|
| JWT Token | WebSocket 連線 | 用戶端身份驗證，包含用戶 ID 和角色 |
| API Key | HTTP REST API | 服務端對服務端認證 |

### API Key 認證

HTTP API 端點（除 `/health` 外）需要 `X-API-Key` Header：

```bash
curl -X POST http://localhost:8081/api/v1/notifications/send \
  -H "X-API-Key: your-api-key-here" \
  -H "Content-Type: application/json" \
  -d '{"target_user_id": "user-123", "event_type": "test", "payload": {}}'
```

**注意事項**：
- 開發模式（未設定 `API_KEY` 環境變數）：跳過 API Key 驗證
- 生產模式：必須設定 `API_KEY` 並在請求中提供正確的金鑰
- API Key 驗證失敗回傳 `401 Unauthorized`

### JWT Token 認證

WebSocket 連線需要有效的 JWT Token，詳見 [WebSocket API](#websocket-api) 章節。

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
  "roles": ["user", "admin"],
  "tenant_id": "acme-corp"
}
```

| 欄位 | 類型 | 必填 | 說明 |
|------|------|------|------|
| `sub` | string | Y | 用戶唯一識別碼 |
| `exp` | number | Y | 過期時間 (Unix timestamp) |
| `iat` | number | N | 簽發時間 (Unix timestamp) |
| `roles` | string[] | N | 用戶角色列表，預設空陣列 |
| `tenant_id` | string | N | 租戶識別碼，用於多租戶隔離，預設為 "default" |

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

#### Ack - 確認收到通知

確認收到特定通知，用於送達追蹤。

```json
{
  "type": "Ack",
  "payload": {
    "notification_id": "550e8400-e29b-41d4-a716-446655440000"
  }
}
```

| 欄位 | 類型 | 說明 |
|------|------|------|
| `notification_id` | UUID | 要確認的通知 ID |

**注意事項**：
- 只有當 ACK 追蹤啟用時才會處理（`ACK_ENABLED=true`）
- ACK 只能由收到通知的用戶發送
- 成功 ACK 會收到 `acked` 回應
- 無效 ACK（如錯誤的用戶、已過期通知）會收到 `error` 回應

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

#### acked - ACK 確認

伺服器回應客戶端的 ACK 請求，表示通知已成功確認。

```json
{
  "type": "acked",
  "notification_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

| 欄位 | 類型 | 說明 |
|------|------|------|
| `notification_id` | UUID | 已確認的通知 ID |

**注意事項**：
- 只有在 ACK 追蹤啟用且 ACK 有效時才會發送
- 無效 ACK 會收到 `error` 回應而非 `acked`

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
| `INVALID_ACK` | ACK 無效（通知不存在、已過期、或用戶不匹配） |

---

## SSE API

Server-Sent Events (SSE) 提供 WebSocket 的備援方案，適用於防火牆阻擋 WebSocket 或僅需單向通知的場景。

### 連線端點

```
GET /sse?token=<JWT_TOKEN>
```

或使用 Header 認證：

```
GET /sse
Authorization: Bearer <JWT_TOKEN>
```

### 認證

使用與 WebSocket 相同的 JWT Token 認證，Token 格式請參考 [JWT Token 格式](#jwt-token-格式)。

### 事件類型

SSE 連線會收到以下事件：

#### connected - 連線確認

連線建立後立即發送：

```
event: connected
data: {"type":"connected","connection_id":"550e8400-e29b-41d4-a716-446655440000"}
```

#### notification - 通知事件

接收通知時發送：

```
event: notification
data: {"type":"notification","id":"...","event_type":"order.created","payload":{...}}
```

#### heartbeat - 心跳保活

定期發送以保持連線：

```
event: heartbeat
data: {"type":"heartbeat"}
```

#### error - 錯誤事件

發生錯誤時發送：

```
event: error
data: {"code":"SERIALIZATION_ERROR","message":"..."}
```

### JavaScript 客戶端範例

```javascript
const token = 'your-jwt-token';
const eventSource = new EventSource(`/sse?token=${token}`);

eventSource.addEventListener('connected', (e) => {
  const data = JSON.parse(e.data);
  console.log('Connected:', data.connection_id);
});

eventSource.addEventListener('notification', (e) => {
  const notification = JSON.parse(e.data);
  console.log('Received:', notification.event_type, notification.payload);
});

eventSource.addEventListener('heartbeat', () => {
  console.log('Heartbeat received');
});

eventSource.onerror = (e) => {
  console.error('SSE error:', e);
  eventSource.close();
};
```

### 限制

| 項目 | 說明 |
|------|------|
| 通訊方向 | 僅支援伺服器到客戶端（單向） |
| 頻道訂閱 | 不支援動態訂閱/取消訂閱 |
| 客戶端 ACK | 不支援（無法接收客戶端訊息） |
| 連線限制 | 共用 WebSocket 連線限制 |

### 與 WebSocket 比較

| 功能 | WebSocket | SSE |
|------|-----------|-----|
| 通訊方向 | 雙向 | 單向（伺服器→客戶端） |
| 頻道訂閱 | ✓ | ✗ |
| 客戶端 ACK | ✓ | ✗ |
| 離線佇列重播 | ✓ | ✓ |
| 心跳 | ✓ | ✓ |
| 防火牆友好 | 可能被阻擋 | 較少被阻擋（使用 HTTP） |
| 瀏覽器支援 | 需要 WebSocket API | 使用 EventSource API |

---

## HTTP REST API

所有 API 端點（除 `/health` 外）需要 API Key 認證。

### 通用 Headers

| Header | 必填 | 說明 |
|--------|------|------|
| `X-API-Key` | Y* | API 認證金鑰（開發模式可選） |
| `Content-Type` | Y | `application/json` |

### 請求限制

| 限制 | 值 | 說明 |
|------|------|------|
| Request Body 大小 (一般端點) | 64 KB | 超過會回傳 413 Payload Too Large |
| Request Body 大小 (批次端點) | 1 MB | `/api/v1/notifications/batch` 專用 |
| 批次通知數量 | 100 筆/批次 | 批次 API 單次請求上限 |

---

### POST /api/v1/notifications/send

發送點對點通知給特定用戶的所有裝置。

**Request**

```bash
curl -X POST http://localhost:8081/api/v1/notifications/send \
  -H "X-API-Key: your-api-key" \
  -H "Content-Type: application/json" \
  -d '{
    "target_user_id": "user-123",
    "event_type": "order.shipped",
    "payload": {"order_id": "ORD-456", "tracking_number": "TRACK-789"},
    "priority": "High",
    "ttl": 3600
  }'
```

**Request Body**

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

### POST /api/v1/notifications/batch

批次發送多個通知，支援混合目標類型。

**Request**

```bash
curl -X POST http://localhost:8081/api/v1/notifications/batch \
  -H "X-API-Key: your-api-key" \
  -H "Content-Type: application/json" \
  -d '{
    "notifications": [
      {
        "target": { "type": "user", "value": "user-1" },
        "event_type": "order.shipped",
        "payload": { "order_id": "ORD-001" },
        "priority": "High"
      },
      {
        "target": { "type": "channel", "value": "orders" },
        "event_type": "stock.low",
        "payload": { "product_id": "SKU-123" }
      }
    ],
    "options": {
      "stop_on_error": false,
      "deduplicate": true
    }
  }'
```

**Request Body**

```json
{
  "notifications": [
    {
      "target": { "type": "user", "value": "user-1" },
      "event_type": "order.shipped",
      "payload": { "order_id": "ORD-001" },
      "priority": "High",
      "ttl": 3600,
      "correlation_id": "batch-item-1"
    },
    {
      "target": { "type": "users", "value": ["user-2", "user-3"] },
      "event_type": "team.update",
      "payload": { "action": "member_added" }
    },
    {
      "target": { "type": "broadcast" },
      "event_type": "system.maintenance",
      "payload": { "scheduled_at": "2025-12-27T02:00:00Z" },
      "priority": "Critical"
    },
    {
      "target": { "type": "channel", "value": "orders" },
      "event_type": "order.status_changed",
      "payload": { "order_id": "ORD-456", "status": "processing" }
    },
    {
      "target": { "type": "channels", "value": ["orders", "inventory"] },
      "event_type": "stock.update",
      "payload": { "product_id": "SKU-001", "quantity": 50 }
    }
  ],
  "options": {
    "stop_on_error": false,
    "deduplicate": true
  }
}
```

**Notification Item 欄位**

| 欄位 | 類型 | 必填 | 說明 |
|------|------|------|------|
| `target` | object | Y | 目標定義 (見下方 Target 格式) |
| `event_type` | string | Y | 事件類型 |
| `payload` | object | Y | 事件資料 |
| `priority` | enum | N | Low/Normal/High/Critical，預設 Normal |
| `ttl` | number | N | 生存時間 (秒) |
| `correlation_id` | string | N | 追蹤關聯 ID |

**Target 格式**

| type | value | 說明 |
|------|-------|------|
| `user` | `"user-123"` | 單一用戶 |
| `users` | `["user-1", "user-2"]` | 多個用戶 |
| `broadcast` | (無 value 欄位) | 所有連線 |
| `channel` | `"orders"` | 單一頻道 |
| `channels` | `["orders", "inventory"]` | 多個頻道 |

**Options 欄位**

| 欄位 | 類型 | 預設 | 說明 |
|------|------|------|------|
| `stop_on_error` | boolean | false | 遇到錯誤時停止處理剩餘通知 |
| `deduplicate` | boolean | false | 跳過重複的 target+event_type 組合 |

**Response**

```json
{
  "batch_id": "batch-550e8400-e29b-41d4-a716-446655440000",
  "results": [
    {
      "index": 0,
      "notification_id": "550e8400-e29b-41d4-a716-446655440001",
      "delivered_to": 2,
      "failed": 0,
      "success": true
    },
    {
      "index": 1,
      "notification_id": "550e8400-e29b-41d4-a716-446655440002",
      "delivered_to": 45,
      "failed": 1,
      "success": true
    }
  ],
  "summary": {
    "total": 2,
    "succeeded": 2,
    "failed": 0,
    "skipped": 0,
    "total_delivered": 47
  }
}
```

**Response 欄位**

| 欄位 | 類型 | 說明 |
|------|------|------|
| `batch_id` | string | 批次唯一識別碼 |
| `results` | array | 每筆通知的處理結果 |
| `results[].index` | number | 通知在請求陣列中的索引 |
| `results[].notification_id` | string | 通知 UUID |
| `results[].delivered_to` | number | 成功送達連線數 |
| `results[].failed` | number | 送達失敗連線數 |
| `results[].success` | boolean | 此筆通知是否成功 |
| `results[].error` | string? | 失敗時的錯誤訊息 |
| `results[].skipped` | boolean? | 是否因去重被跳過 |
| `summary.total` | number | 請求中的通知總數 |
| `summary.succeeded` | number | 成功處理的通知數 |
| `summary.failed` | number | 處理失敗的通知數 |
| `summary.skipped` | number | 因去重跳過的通知數 |
| `summary.total_delivered` | number | 總送達連線數 |

**限制**

| 限制 | 值 |
|------|------|
| 最大通知數量 | 100 筆/批次 |
| 最大請求大小 | 1 MB |

**錯誤情境**

超過批次大小限制時回傳 400 Bad Request：

```json
{
  "error": {
    "code": "BATCH_TOO_LARGE",
    "message": "Batch size 150 exceeds maximum allowed 100"
  }
}
```

---

### GET /api/v1/channels

列出所有活動頻道與其訂閱者數量。

**Request**

```bash
curl -X GET http://localhost:8081/api/v1/channels \
  -H "X-API-Key: your-api-key"
```

**Response**

```json
{
  "channels": [
    { "name": "orders", "subscriber_count": 45 },
    { "name": "system-alerts", "subscriber_count": 150 },
    { "name": "admin", "subscriber_count": 5 }
  ],
  "total_channels": 3
}
```

| 欄位 | 類型 | 說明 |
|------|------|------|
| `channels` | array | 頻道列表 |
| `channels[].name` | string | 頻道名稱 |
| `channels[].subscriber_count` | number | 訂閱者數量 |
| `total_channels` | number | 頻道總數 |

**注意事項**：
- 只返回有訂閱者的頻道
- 當最後一個訂閱者離開後，頻道會自動從列表中移除

---

### GET /api/v1/channels/{name}

取得特定頻道的詳細資訊。

**Request**

```bash
curl -X GET http://localhost:8081/api/v1/channels/orders \
  -H "X-API-Key: your-api-key"
```

**Response (成功)**

```json
{
  "name": "orders",
  "subscriber_count": 45
}
```

| 欄位 | 類型 | 說明 |
|------|------|------|
| `name` | string | 頻道名稱 |
| `subscriber_count` | number | 訂閱者數量 |

**Response (頻道不存在)**

```json
{
  "error": {
    "code": "CHANNEL_NOT_FOUND",
    "message": "Channel 'nonexistent' not found or has no subscribers"
  }
}
```

---

### GET /api/v1/users/{user_id}/subscriptions

取得特定使用者的頻道訂閱列表。

**Request**

```bash
curl -X GET http://localhost:8081/api/v1/users/user-123/subscriptions \
  -H "X-API-Key: your-api-key"
```

**Response (成功)**

```json
{
  "user_id": "user-123",
  "connection_count": 2,
  "subscriptions": ["orders", "system-alerts"]
}
```

| 欄位 | 類型 | 說明 |
|------|------|------|
| `user_id` | string | 使用者 ID |
| `connection_count` | number | 使用者當前的連線數 |
| `subscriptions` | array | 使用者訂閱的頻道列表（跨所有連線彙整，已去重） |

**Response (使用者未連線)**

```json
{
  "error": {
    "code": "USER_NOT_CONNECTED",
    "message": "User 'user-123' has no active connections"
  }
}
```

---

### GET /api/v1/tenants

列出所有活躍租戶及其統計資訊。

**Request**

```bash
curl -X GET http://localhost:8081/api/v1/tenants \
  -H "X-API-Key: your-api-key"
```

**Response**

```json
{
  "enabled": true,
  "tenants": [
    {
      "tenant_id": "acme-corp",
      "stats": {
        "active_connections": 150,
        "total_connections": 1250,
        "messages_sent": 5000,
        "messages_delivered": 48500
      }
    },
    {
      "tenant_id": "globex",
      "stats": {
        "active_connections": 80,
        "total_connections": 500,
        "messages_sent": 2000,
        "messages_delivered": 19000
      }
    }
  ],
  "total": 2
}
```

| 欄位 | 類型 | 說明 |
|------|------|------|
| `enabled` | boolean | 多租戶模式是否啟用 |
| `tenants` | array | 活躍租戶列表 |
| `tenants[].tenant_id` | string | 租戶識別碼 |
| `tenants[].stats` | object | 租戶統計資訊 |
| `total` | number | 活躍租戶總數 |

---

### GET /api/v1/tenants/{tenant_id}

取得特定租戶的詳細統計資訊。

**Request**

```bash
curl -X GET http://localhost:8081/api/v1/tenants/acme-corp \
  -H "X-API-Key: your-api-key"
```

**Response (成功)**

```json
{
  "tenant_id": "acme-corp",
  "stats": {
    "active_connections": 150,
    "total_connections": 1250,
    "messages_sent": 5000,
    "messages_delivered": 48500
  },
  "connection_stats": {
    "total_connections": 150,
    "unique_users": 75
  }
}
```

| 欄位 | 類型 | 說明 |
|------|------|------|
| `tenant_id` | string | 租戶識別碼 |
| `stats` | object | TenantManager 追蹤的統計 |
| `stats.active_connections` | number | 當前活躍連線數 |
| `stats.total_connections` | number | 歷史總連線數 |
| `stats.messages_sent` | number | 發送的訊息數 |
| `stats.messages_delivered` | number | 投遞的訊息數 |
| `connection_stats` | object | ConnectionManager 追蹤的統計 |
| `connection_stats.total_connections` | number | 當前連線數 |
| `connection_stats.unique_users` | number | 唯一用戶數 |

**Response (租戶不存在)**

```json
{
  "error": {
    "code": "TENANT_NOT_FOUND",
    "message": "Tenant 'unknown' has no recorded activity"
  }
}
```

---

### GET /health

健康檢查端點。

**Response**

```json
{
  "status": "healthy",
  "version": "1.0.0"
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
  },
  "redis": {
    "status": "healthy",
    "connected": true,
    "circuit_breaker_state": "closed",
    "circuit_breaker_failures": 0,
    "reconnection_attempts": 0,
    "total_reconnections": 1
  },
  "ack": {
    "enabled": true,
    "total_tracked": 10250,
    "total_acked": 10100,
    "total_expired": 50,
    "pending_count": 100,
    "ack_rate": 0.9854,
    "avg_latency_ms": 45
  }
}
```

| 欄位 | 類型 | 說明 |
|------|------|------|
| `ack` | object? | ACK 統計（僅在啟用時顯示） |
| `ack.enabled` | boolean | ACK 追蹤是否啟用 |
| `ack.total_tracked` | number | 追蹤的通知總數 |
| `ack.total_acked` | number | 已確認的通知數 |
| `ack.total_expired` | number | 過期未確認的通知數 |
| `ack.pending_count` | number | 當前待確認的通知數 |
| `ack.ack_rate` | number | ACK 成功率 (0-1) |
| `ack.avg_latency_ms` | number | 平均 ACK 延遲（毫秒） |

---

### GET /metrics

Prometheus 指標匯出端點。無需認證。

**Response**

Content-Type: `text/plain; version=0.0.4; charset=utf-8`

```prometheus
# HELP ara_connections_total Total number of active WebSocket connections
# TYPE ara_connections_total gauge
ara_connections_total 150

# HELP ara_users_connected Number of unique connected users
# TYPE ara_users_connected gauge
ara_users_connected 120

# HELP ara_messages_sent_total Total messages sent
# TYPE ara_messages_sent_total counter
ara_messages_sent_total{target="user"} 5000
ara_messages_sent_total{target="broadcast"} 1000
ara_messages_sent_total{target="channel"} 4250

# HELP ara_messages_delivered_total Total messages successfully delivered
# TYPE ara_messages_delivered_total counter
ara_messages_delivered_total 10248

# HELP ara_messages_failed_total Total message delivery failures
# TYPE ara_messages_failed_total counter
ara_messages_failed_total 2

# HELP ara_redis_connection_status Redis connection status (1=connected, 0=disconnected)
# TYPE ara_redis_connection_status gauge
ara_redis_connection_status 1

# HELP ara_queue_size_total Total messages in queue
# TYPE ara_queue_size_total gauge
ara_queue_size_total 50

# HELP ara_ratelimit_allowed_total Requests allowed by rate limiter
# TYPE ara_ratelimit_allowed_total counter
ara_ratelimit_allowed_total{type="http"} 15000
ara_ratelimit_allowed_total{type="ws"} 200

# HELP ara_ack_pending Current pending ACKs
# TYPE ara_ack_pending gauge
ara_ack_pending 100

# HELP ara_ws_connections_opened_total WebSocket connections opened
# TYPE ara_ws_connections_opened_total counter
ara_ws_connections_opened_total 500

# ... (更多指標)
```

**完整指標清單**

| 指標名稱 | 類型 | 標籤 | 說明 |
|---------|------|------|------|
| `ara_connections_total` | gauge | - | 當前連線總數 |
| `ara_users_connected` | gauge | - | 唯一用戶數 |
| `ara_connections_per_user` | histogram | - | 每用戶連線分布 |
| `ara_channel_subscriptions` | gauge | channel | 每頻道訂閱數 |
| `ara_channels_active` | gauge | - | 活躍頻道數 |
| `ara_messages_sent_total` | counter | target | 發送訊息數 (user/users/broadcast/channel/channels) |
| `ara_messages_delivered_total` | counter | - | 投遞成功數 |
| `ara_messages_failed_total` | counter | - | 投遞失敗數 |
| `ara_message_delivery_latency_seconds` | histogram | - | 投遞延遲 |
| `ara_redis_connection_status` | gauge | - | Redis 連線狀態 |
| `ara_redis_circuit_breaker_state` | gauge | - | 熔斷器狀態 (0/1/2) |
| `ara_redis_reconnections_total` | counter | - | 重連次數 |
| `ara_redis_messages_received_total` | counter | - | 接收訊息數 |
| `ara_queue_size_total` | gauge | - | 佇列總大小 |
| `ara_queue_users_total` | gauge | - | 有佇列的用戶數 |
| `ara_queue_enqueued_total` | counter | - | 入隊訊息數 |
| `ara_queue_replayed_total` | counter | - | 重播訊息數 |
| `ara_queue_expired_total` | counter | - | 過期訊息數 |
| `ara_queue_dropped_total` | counter | - | 丟棄訊息數 |
| `ara_ratelimit_allowed_total` | counter | type | 允許請求數 |
| `ara_ratelimit_denied_total` | counter | type | 拒絕請求數 |
| `ara_ack_tracked_total` | counter | - | 追蹤通知數 |
| `ara_ack_received_total` | counter | - | 確認數 |
| `ara_ack_expired_total` | counter | - | 過期數 |
| `ara_ack_pending` | gauge | - | 待確認數 |
| `ara_ack_latency_seconds` | histogram | - | ACK 延遲 |
| `ara_ws_connections_opened_total` | counter | - | 開啟連線數 |
| `ara_ws_connections_closed_total` | counter | - | 關閉連線數 |
| `ara_ws_messages_received_total` | counter | type | 接收訊息數 |
| `ara_ws_connection_duration_seconds` | histogram | - | 連線持續時間 |
| `ara_batch_requests_total` | counter | - | 批次請求數 |
| `ara_batch_size` | histogram | - | 批次大小分布 |

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
| 401 | 未授權 (缺少或無效的 API Key) |
| 413 | 請求 Body 過大（超過 64KB） |
| 422 | 驗證錯誤 |
| 500 | 伺服器內部錯誤 |

### HTTP 錯誤回應格式

```json
{
  "error": {
    "code": "UNAUTHORIZED",
    "message": "Invalid or missing API key"
  }
}
```

| 錯誤碼 | 說明 |
|--------|------|
| `UNAUTHORIZED` | API Key 驗證失敗 |
| `VALIDATION_ERROR` | 請求參數驗證失敗 |
| `INTERNAL_ERROR` | 伺服器內部錯誤（生產模式隱藏詳情） |
| `REDIS_ERROR` | Redis 連線錯誤（生產模式顯示通用訊息） |

### WebSocket 錯誤碼

| 錯誤碼 | 說明 |
|--------|------|
| `INVALID_MESSAGE` | 訊息格式錯誤或 JSON 解析失敗 |
| `UNSUPPORTED_FORMAT` | 不支援的訊息格式（如 Binary） |
| `CONNECTION_LIMIT` | 連線數超過限制（總數或每用戶） |
| `SUBSCRIPTION_ERROR` | 頻道訂閱失敗（如超過訂閱數量限制） |
| `INVALID_ACK` | ACK 無效（通知不存在、已過期、或用戶不匹配） |

### WebSocket 關閉碼

| 關閉碼 | 說明 |
|--------|------|
| 1000 | 正常關閉 |
| 1008 | 認證失敗 (Missing/Invalid JWT token) |

### 連線限制

| 限制 | 預設值 | 環境變數 |
|------|--------|----------|
| 最大總連線數 | 10,000 | `WEBSOCKET_MAX_CONNECTIONS` |
| 每用戶最大連線數 | 5 | `WEBSOCKET_MAX_CONNECTIONS_PER_USER` |
| 每連線最大訂閱數 | 50 | `WEBSOCKET_MAX_SUBSCRIPTIONS_PER_CONNECTION` |

超過限制時，WebSocket 連線會收到錯誤訊息並被關閉：

```json
{
  "type": "error",
  "code": "CONNECTION_LIMIT",
  "message": "User user-123 connection limit exceeded (5/5)"
}
```
