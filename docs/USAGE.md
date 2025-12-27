# 使用說明

本文檔提供 Ara Notification Service 的完整使用指南，涵蓋從基本連線到進階功能的所有操作。

## 目錄

- [快速開始](#快速開始)
- [WebSocket 連線](#websocket-連線)
- [SSE 連線](#sse-連線)
- [發送通知](#發送通知)
- [頻道訂閱](#頻道訂閱)
- [通知模板](#通知模板)
- [批次發送](#批次發送)
- [離線訊息佇列](#離線訊息佇列)
- [客戶端 ACK 確認](#客戶端-ack-確認)
- [多租戶使用](#多租戶使用)
- [監控與統計](#監控與統計)
- [錯誤處理](#錯誤處理)
- [最佳實踐](#最佳實踐)
- [常見問題](#常見問題)

---

## 快速開始

### 1. 環境準備

```bash
# 設定必要的環境變數
export JWT_SECRET="your-secure-secret-key-at-least-32-characters"
export REDIS_URL="redis://localhost:6379"
export API_KEY="your-api-key-for-http-endpoints"

# 啟動服務
cargo run
```

### 2. 產生 JWT Token

使用您的後端服務產生 JWT Token：

```php
// PHP 範例 (使用 firebase/php-jwt)
use Firebase\JWT\JWT;

$payload = [
    'sub' => 'user-123',           // 使用者 ID (必填)
    'exp' => time() + 3600,        // 過期時間 (必填)
    'iat' => time(),               // 簽發時間
    'roles' => ['user', 'admin'],  // 角色 (選填)
    'tenant_id' => 'acme-corp',    // 租戶 ID (選填，預設 "default")
];

$token = JWT::encode($payload, $_ENV['JWT_SECRET'], 'HS256');
```

```javascript
// Node.js 範例 (使用 jsonwebtoken)
const jwt = require('jsonwebtoken');

const token = jwt.sign(
  {
    sub: 'user-123',
    roles: ['user', 'admin'],
    tenant_id: 'acme-corp',
  },
  process.env.JWT_SECRET,
  { expiresIn: '1h' }
);
```

### 3. 建立 WebSocket 連線

```javascript
const ws = new WebSocket(`ws://localhost:8081/ws?token=${token}`);

ws.onopen = () => console.log('Connected!');
ws.onmessage = (event) => console.log('Message:', JSON.parse(event.data));
ws.onerror = (error) => console.error('Error:', error);
ws.onclose = () => console.log('Disconnected');
```

### 4. 發送第一個通知

```bash
curl -X POST http://localhost:8081/api/v1/notifications/send \
  -H "X-API-Key: your-api-key-for-http-endpoints" \
  -H "Content-Type: application/json" \
  -d '{
    "target_user_id": "user-123",
    "event_type": "welcome",
    "payload": {"message": "Welcome to Ara!"}
  }'
```

---

## WebSocket 連線

### 連線方式

**方式一：Query Parameter**
```
ws://localhost:8081/ws?token=<JWT_TOKEN>
```

**方式二：Authorization Header**
```javascript
// 瀏覽器原生 WebSocket 不支援自訂 Header
// 使用 Sec-WebSocket-Protocol 或 query parameter

// Node.js 或使用 ws 套件時可用 header
const WebSocket = require('ws');
const ws = new WebSocket('ws://localhost:8081/ws', {
  headers: { Authorization: `Bearer ${token}` }
});
```

### 完整客戶端範例

```javascript
class NotificationClient {
  constructor(serverUrl, token) {
    this.serverUrl = serverUrl;
    this.token = token;
    this.ws = null;
    this.reconnectAttempts = 0;
    this.maxReconnectAttempts = 5;
    this.listeners = new Map();
  }

  connect() {
    this.ws = new WebSocket(`${this.serverUrl}/ws?token=${this.token}`);

    this.ws.onopen = () => {
      console.log('Connected to notification service');
      this.reconnectAttempts = 0;
      this.emit('connected');
    };

    this.ws.onmessage = (event) => {
      const message = JSON.parse(event.data);
      this.handleMessage(message);
    };

    this.ws.onerror = (error) => {
      console.error('WebSocket error:', error);
      this.emit('error', error);
    };

    this.ws.onclose = (event) => {
      console.log('Connection closed:', event.code, event.reason);
      this.emit('disconnected', event);
      this.attemptReconnect();
    };
  }

  handleMessage(message) {
    switch (message.type) {
      case 'notification':
        console.log('Received notification:', message.event_type);
        this.emit('notification', message);
        // 發送 ACK 確認
        this.ack(message.id);
        break;

      case 'subscribed':
        console.log('Subscribed to channels:', message.payload);
        this.emit('subscribed', message.payload);
        break;

      case 'unsubscribed':
        console.log('Unsubscribed from channels:', message.payload);
        this.emit('unsubscribed', message.payload);
        break;

      case 'acked':
        console.log('ACK confirmed:', message.notification_id);
        this.emit('acked', message.notification_id);
        break;

      case 'pong':
        console.log('Pong received');
        break;

      case 'heartbeat':
        // 伺服器心跳，保持連線活躍
        break;

      case 'error':
        console.error('Server error:', message.code, message.message);
        this.emit('error', { code: message.code, message: message.message });
        break;
    }
  }

  // 訂閱頻道
  subscribe(channels) {
    this.send({
      type: 'Subscribe',
      payload: { channels: Array.isArray(channels) ? channels : [channels] }
    });
  }

  // 取消訂閱
  unsubscribe(channels) {
    this.send({
      type: 'Unsubscribe',
      payload: { channels: Array.isArray(channels) ? channels : [channels] }
    });
  }

  // 發送心跳
  ping() {
    this.send({ type: 'Ping' });
  }

  // 確認通知收到
  ack(notificationId) {
    this.send({
      type: 'Ack',
      payload: { notification_id: notificationId }
    });
  }

  send(data) {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(data));
    }
  }

  attemptReconnect() {
    if (this.reconnectAttempts < this.maxReconnectAttempts) {
      this.reconnectAttempts++;
      const delay = Math.min(1000 * Math.pow(2, this.reconnectAttempts), 30000);
      console.log(`Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts})`);
      setTimeout(() => this.connect(), delay);
    }
  }

  // 事件監聽
  on(event, callback) {
    if (!this.listeners.has(event)) {
      this.listeners.set(event, []);
    }
    this.listeners.get(event).push(callback);
  }

  emit(event, data) {
    const callbacks = this.listeners.get(event) || [];
    callbacks.forEach(cb => cb(data));
  }

  disconnect() {
    if (this.ws) {
      this.ws.close(1000, 'Client disconnect');
    }
  }
}

// 使用範例
const client = new NotificationClient('ws://localhost:8081', token);

client.on('connected', () => {
  client.subscribe(['orders', 'system-alerts']);
});

client.on('notification', (notification) => {
  console.log('New notification:', notification.event_type);
  console.log('Payload:', notification.payload);

  // 根據事件類型處理
  switch (notification.event_type) {
    case 'order.created':
      showOrderNotification(notification.payload);
      break;
    case 'system.maintenance':
      showMaintenanceAlert(notification.payload);
      break;
  }
});

client.connect();
```

### Vue.js 整合範例

```vue
<template>
  <div>
    <div v-for="notification in notifications" :key="notification.id">
      {{ notification.payload.message }}
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted, onUnmounted } from 'vue';

const notifications = ref([]);
let ws = null;

const connect = (token) => {
  ws = new WebSocket(`ws://localhost:8081/ws?token=${token}`);

  ws.onmessage = (event) => {
    const message = JSON.parse(event.data);
    if (message.type === 'notification') {
      notifications.value.unshift(message);
      // 發送 ACK
      ws.send(JSON.stringify({
        type: 'Ack',
        payload: { notification_id: message.id }
      }));
    }
  };

  ws.onopen = () => {
    // 訂閱頻道
    ws.send(JSON.stringify({
      type: 'Subscribe',
      payload: { channels: ['orders'] }
    }));
  };
};

onMounted(() => {
  const token = localStorage.getItem('jwt_token');
  if (token) connect(token);
});

onUnmounted(() => {
  if (ws) ws.close();
});
</script>
```

### React 整合範例

```jsx
import { useEffect, useState, useCallback, useRef } from 'react';

function useNotifications(token, channels = []) {
  const [notifications, setNotifications] = useState([]);
  const [connected, setConnected] = useState(false);
  const wsRef = useRef(null);

  const connect = useCallback(() => {
    if (!token) return;

    const ws = new WebSocket(`ws://localhost:8081/ws?token=${token}`);
    wsRef.current = ws;

    ws.onopen = () => {
      setConnected(true);
      if (channels.length > 0) {
        ws.send(JSON.stringify({
          type: 'Subscribe',
          payload: { channels }
        }));
      }
    };

    ws.onmessage = (event) => {
      const message = JSON.parse(event.data);
      if (message.type === 'notification') {
        setNotifications(prev => [message, ...prev]);
        // 發送 ACK
        ws.send(JSON.stringify({
          type: 'Ack',
          payload: { notification_id: message.id }
        }));
      }
    };

    ws.onclose = () => {
      setConnected(false);
      // 自動重連
      setTimeout(connect, 3000);
    };

    return ws;
  }, [token, channels]);

  useEffect(() => {
    const ws = connect();
    return () => ws?.close();
  }, [connect]);

  const subscribe = useCallback((newChannels) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify({
        type: 'Subscribe',
        payload: { channels: newChannels }
      }));
    }
  }, []);

  return { notifications, connected, subscribe };
}

// 使用
function App() {
  const { notifications, connected, subscribe } = useNotifications(
    'your-jwt-token',
    ['orders', 'alerts']
  );

  return (
    <div>
      <p>Status: {connected ? 'Connected' : 'Disconnected'}</p>
      {notifications.map(n => (
        <div key={n.id}>{n.payload.message}</div>
      ))}
    </div>
  );
}
```

---

## SSE 連線

SSE (Server-Sent Events) 提供單向通知推播，適用於不需要雙向通訊或防火牆阻擋 WebSocket 的場景。

### 連線方式

```javascript
// 使用 EventSource API
const token = 'your-jwt-token';
const eventSource = new EventSource(`http://localhost:8081/sse?token=${token}`);

// 連線建立
eventSource.addEventListener('connected', (event) => {
  const data = JSON.parse(event.data);
  console.log('Connected with ID:', data.connection_id);
});

// 接收通知
eventSource.addEventListener('notification', (event) => {
  const notification = JSON.parse(event.data);
  console.log('Notification received:', notification.event_type);
  console.log('Payload:', notification.payload);
});

// 心跳 (保持連線)
eventSource.addEventListener('heartbeat', () => {
  console.log('Heartbeat received');
});

// 錯誤處理
eventSource.addEventListener('error', (event) => {
  if (event.data) {
    const error = JSON.parse(event.data);
    console.error('Error:', error.code, error.message);
  }
});

// 連線錯誤
eventSource.onerror = (error) => {
  console.error('Connection error:', error);
  // EventSource 會自動嘗試重連
};

// 關閉連線
// eventSource.close();
```

### SSE vs WebSocket 選擇指南

| 場景 | 建議 | 原因 |
|------|------|------|
| 即時雙向通訊 | WebSocket | 支援客戶端發送訊息 |
| 僅接收通知 | SSE | 更簡單、自動重連 |
| 需要頻道訂閱 | WebSocket | SSE 不支援動態訂閱 |
| 企業防火牆環境 | SSE | 使用標準 HTTP，較少被阻擋 |
| 需要 ACK 確認 | WebSocket | SSE 無法發送 ACK |
| 低延遲要求 | WebSocket | 連線建立後延遲更低 |

---

## 發送通知

### HTTP API 發送

#### 點對點通知

```bash
curl -X POST http://localhost:8081/api/v1/notifications/send \
  -H "X-API-Key: your-api-key" \
  -H "Content-Type: application/json" \
  -d '{
    "target_user_id": "user-123",
    "event_type": "order.shipped",
    "payload": {
      "order_id": "ORD-456",
      "tracking_number": "TW123456789",
      "estimated_delivery": "2025-12-30"
    },
    "priority": "High",
    "ttl": 86400,
    "correlation_id": "order-ship-001"
  }'
```

**回應：**
```json
{
  "success": true,
  "notification_id": "550e8400-e29b-41d4-a716-446655440000",
  "delivered_to": 2,
  "failed": 0,
  "timestamp": "2025-12-27T10:30:00Z"
}
```

#### 多使用者通知

```bash
curl -X POST http://localhost:8081/api/v1/notifications/send-to-users \
  -H "X-API-Key: your-api-key" \
  -H "Content-Type: application/json" \
  -d '{
    "target_user_ids": ["user-1", "user-2", "user-3"],
    "event_type": "team.announcement",
    "payload": {
      "title": "Team Meeting",
      "message": "Meeting at 3 PM today",
      "meeting_url": "https://meet.example.com/abc"
    }
  }'
```

#### 廣播通知

```bash
curl -X POST http://localhost:8081/api/v1/notifications/broadcast \
  -H "X-API-Key: your-api-key" \
  -H "Content-Type: application/json" \
  -d '{
    "event_type": "system.maintenance",
    "payload": {
      "message": "System will be under maintenance at 2 AM",
      "duration_minutes": 30
    },
    "priority": "Critical"
  }'
```

#### 頻道通知

```bash
curl -X POST http://localhost:8081/api/v1/notifications/channel \
  -H "X-API-Key: your-api-key" \
  -H "Content-Type: application/json" \
  -d '{
    "channel": "orders",
    "event_type": "order.status_changed",
    "payload": {
      "order_id": "ORD-789",
      "old_status": "pending",
      "new_status": "processing"
    }
  }'
```

### PHP/Symfony 整合

```php
<?php

namespace App\Service;

use Symfony\Contracts\HttpClient\HttpClientInterface;

class NotificationService
{
    public function __construct(
        private HttpClientInterface $httpClient,
        private string $notificationUrl,
        private string $apiKey,
    ) {}

    public function sendToUser(
        string $userId,
        string $eventType,
        array $payload,
        string $priority = 'Normal',
        ?int $ttl = null,
    ): array {
        return $this->send('/api/v1/notifications/send', [
            'target_user_id' => $userId,
            'event_type' => $eventType,
            'payload' => $payload,
            'priority' => $priority,
            'ttl' => $ttl,
        ]);
    }

    public function sendToUsers(array $userIds, string $eventType, array $payload): array
    {
        return $this->send('/api/v1/notifications/send-to-users', [
            'target_user_ids' => $userIds,
            'event_type' => $eventType,
            'payload' => $payload,
        ]);
    }

    public function broadcast(string $eventType, array $payload, string $priority = 'Normal'): array
    {
        return $this->send('/api/v1/notifications/broadcast', [
            'event_type' => $eventType,
            'payload' => $payload,
            'priority' => $priority,
        ]);
    }

    public function sendToChannel(string $channel, string $eventType, array $payload): array
    {
        return $this->send('/api/v1/notifications/channel', [
            'channel' => $channel,
            'event_type' => $eventType,
            'payload' => $payload,
        ]);
    }

    public function sendWithTemplate(
        string $userId,
        string $templateId,
        array $variables,
    ): array {
        return $this->send('/api/v1/notifications/send', [
            'target_user_id' => $userId,
            'template_id' => $templateId,
            'variables' => $variables,
        ]);
    }

    private function send(string $endpoint, array $data): array
    {
        $response = $this->httpClient->request('POST', $this->notificationUrl . $endpoint, [
            'headers' => [
                'X-API-Key' => $this->apiKey,
                'Content-Type' => 'application/json',
            ],
            'json' => $data,
        ]);

        return $response->toArray();
    }
}
```

**使用範例：**

```php
// 在 Controller 或 Service 中
class OrderController
{
    public function shipOrder(Order $order, NotificationService $notificationService): Response
    {
        // ... 處理訂單出貨邏輯 ...

        // 發送通知給訂購者
        $notificationService->sendToUser(
            userId: $order->getCustomerId(),
            eventType: 'order.shipped',
            payload: [
                'order_id' => $order->getId(),
                'tracking_number' => $order->getTrackingNumber(),
                'carrier' => $order->getCarrier(),
            ],
            priority: 'High',
            ttl: 86400, // 24 小時
        );

        // 或使用模板
        $notificationService->sendWithTemplate(
            userId: $order->getCustomerId(),
            templateId: 'order-shipped',
            variables: [
                'order_id' => $order->getId(),
                'tracking_number' => $order->getTrackingNumber(),
            ],
        );

        return new Response('Order shipped');
    }
}
```

### Redis Pub/Sub 發送

適用於微服務架構，無需 HTTP 呼叫：

```bash
# 單一使用者
redis-cli PUBLISH "notification:user:user-123" '{
  "type": "user",
  "target": "user-123",
  "event": {
    "event_type": "message.new",
    "payload": {"from": "user-456", "content": "Hello!"},
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
    "priority": "High"
  }
}'

# 頻道
redis-cli PUBLISH "notification:channel:orders" '{
  "type": "channel",
  "target": "orders",
  "event": {
    "event_type": "order.new",
    "payload": {"order_id": "123"},
    "priority": "Normal"
  }
}'
```

**PHP 使用 Redis：**

```php
use Predis\Client;

class RedisNotificationPublisher
{
    public function __construct(private Client $redis) {}

    public function notifyUser(string $userId, string $eventType, array $payload): void
    {
        $message = json_encode([
            'type' => 'user',
            'target' => $userId,
            'event' => [
                'event_type' => $eventType,
                'payload' => $payload,
                'priority' => 'Normal',
            ],
        ]);

        $this->redis->publish("notification:user:{$userId}", $message);
    }

    public function broadcast(string $eventType, array $payload): void
    {
        $message = json_encode([
            'type' => 'broadcast',
            'target' => null,
            'event' => [
                'event_type' => $eventType,
                'payload' => $payload,
            ],
        ]);

        $this->redis->publish('notification:broadcast', $message);
    }
}
```

---

## 頻道訂閱

頻道允許使用者訂閱特定主題，只接收相關通知。

### 頻道命名規則

- 長度：1-64 字元
- 允許字元：英數字、dash (`-`)、underscore (`_`)、dot (`.`)
- 範例：`orders`、`user-123-messages`、`system.alerts`、`room_456`

### 客戶端訂閱

```javascript
// 訂閱多個頻道
ws.send(JSON.stringify({
  type: 'Subscribe',
  payload: { channels: ['orders', 'inventory', 'shipping'] }
}));

// 伺服器回應
// {"type":"subscribed","payload":["orders","inventory","shipping"]}

// 取消訂閱
ws.send(JSON.stringify({
  type: 'Unsubscribe',
  payload: { channels: ['inventory'] }
}));

// 伺服器回應
// {"type":"unsubscribed","payload":["inventory"]}
```

### 訂閱限制

| 限制 | 預設值 | 環境變數 |
|------|--------|----------|
| 每連線最大訂閱數 | 50 | `WEBSOCKET_MAX_SUBSCRIPTIONS_PER_CONNECTION` |

### 查詢頻道資訊

```bash
# 列出所有頻道
curl http://localhost:8081/api/v1/channels \
  -H "X-API-Key: your-api-key"

# 回應
{
  "channels": [
    { "name": "orders", "subscriber_count": 45 },
    { "name": "system-alerts", "subscriber_count": 150 }
  ],
  "total_channels": 2
}

# 查詢特定頻道
curl http://localhost:8081/api/v1/channels/orders \
  -H "X-API-Key: your-api-key"

# 回應
{
  "name": "orders",
  "subscriber_count": 45
}

# 查詢使用者訂閱
curl http://localhost:8081/api/v1/users/user-123/subscriptions \
  -H "X-API-Key: your-api-key"

# 回應
{
  "user_id": "user-123",
  "connection_count": 2,
  "subscriptions": ["orders", "system-alerts"]
}
```

### 頻道使用模式

**模式一：資源訂閱**
```javascript
// 使用者訂閱自己的訂單頻道
ws.send(JSON.stringify({
  type: 'Subscribe',
  payload: { channels: [`user-${userId}-orders`] }
}));
```

**模式二：房間訂閱**
```javascript
// 聊天室
ws.send(JSON.stringify({
  type: 'Subscribe',
  payload: { channels: [`room-${roomId}`] }
}));
```

**模式三：主題訂閱**
```javascript
// 訂閱感興趣的主題
ws.send(JSON.stringify({
  type: 'Subscribe',
  payload: { channels: ['news.tech', 'news.sports', 'weather.taipei'] }
}));
```

---

## 通知模板

模板系統允許預定義通知內容，使用變數動態替換。

### 建立模板

```bash
curl -X POST http://localhost:8081/api/v1/templates \
  -H "X-API-Key: your-api-key" \
  -H "Content-Type: application/json" \
  -d '{
    "id": "order-shipped",
    "name": "Order Shipped Notification",
    "event_type": "order.shipped",
    "payload_template": {
      "title": "Your order {{order_id}} has shipped!",
      "body": "Track your package: {{tracking_number}}",
      "data": {
        "order_id": "{{order_id}}",
        "carrier": "{{carrier}}",
        "tracking_url": "https://track.example.com/{{tracking_number}}"
      }
    },
    "default_priority": "High",
    "default_ttl": 86400,
    "description": "Sent when an order ships"
  }'
```

### 使用模板發送

```bash
curl -X POST http://localhost:8081/api/v1/notifications/send \
  -H "X-API-Key: your-api-key" \
  -H "Content-Type: application/json" \
  -d '{
    "target_user_id": "user-123",
    "template_id": "order-shipped",
    "variables": {
      "order_id": "ORD-456",
      "tracking_number": "TW123456789",
      "carrier": "FedEx"
    }
  }'
```

**渲染結果：**
```json
{
  "title": "Your order ORD-456 has shipped!",
  "body": "Track your package: TW123456789",
  "data": {
    "order_id": "ORD-456",
    "carrier": "FedEx",
    "tracking_url": "https://track.example.com/TW123456789"
  }
}
```

### 管理模板

```bash
# 列出所有模板
curl http://localhost:8081/api/v1/templates \
  -H "X-API-Key: your-api-key"

# 取得特定模板
curl http://localhost:8081/api/v1/templates/order-shipped \
  -H "X-API-Key: your-api-key"

# 更新模板
curl -X PUT http://localhost:8081/api/v1/templates/order-shipped \
  -H "X-API-Key: your-api-key" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Order Shipped (Updated)",
    "default_priority": "Normal"
  }'

# 刪除模板
curl -X DELETE http://localhost:8081/api/v1/templates/order-shipped \
  -H "X-API-Key: your-api-key"
```

### 變數替換規則

- 語法：`{{variable_name}}`
- 支援巢狀物件：模板中的任何字串都會替換
- 數值轉字串：數值變數會自動轉為字串
- 陣列/物件：轉為 JSON 字串

```json
// 模板
{
  "message": "You have {{count}} items",
  "items": ["{{item1}}", "{{item2}}"]
}

// 變數
{
  "count": 5,
  "item1": "Apple",
  "item2": "Orange"
}

// 結果
{
  "message": "You have 5 items",
  "items": ["Apple", "Orange"]
}
```

---

## 批次發送

批次 API 允許單次請求發送多筆通知，提高效率。

### 基本用法

```bash
curl -X POST http://localhost:8081/api/v1/notifications/batch \
  -H "X-API-Key: your-api-key" \
  -H "Content-Type: application/json" \
  -d '{
    "notifications": [
      {
        "target": { "type": "user", "value": "user-1" },
        "event_type": "order.created",
        "payload": { "order_id": "ORD-001" }
      },
      {
        "target": { "type": "user", "value": "user-2" },
        "event_type": "order.created",
        "payload": { "order_id": "ORD-002" }
      },
      {
        "target": { "type": "channel", "value": "inventory" },
        "event_type": "stock.updated",
        "payload": { "product_id": "SKU-123" }
      }
    ],
    "options": {
      "deduplicate": true
    }
  }'
```

### Target 類型

| type | value | 說明 |
|------|-------|------|
| `user` | `"user-123"` | 單一使用者 |
| `users` | `["user-1", "user-2"]` | 多個使用者 |
| `broadcast` | 無 | 所有連線 |
| `channel` | `"orders"` | 單一頻道 |
| `channels` | `["orders", "inventory"]` | 多個頻道 |

### 批次選項

| 選項 | 預設 | 說明 |
|------|------|------|
| `stop_on_error` | `false` | 遇錯停止 |
| `deduplicate` | `false` | 跳過重複的 target+event_type |

### 批次限制

| 限制 | 值 |
|------|------|
| 最大通知數 | 100 筆 |
| 最大請求大小 | 1 MB |

### PHP 批次發送範例

```php
class NotificationService
{
    public function sendBatch(array $notifications, array $options = []): array
    {
        $response = $this->httpClient->request('POST',
            $this->notificationUrl . '/api/v1/notifications/batch', [
            'headers' => [
                'X-API-Key' => $this->apiKey,
            ],
            'json' => [
                'notifications' => $notifications,
                'options' => $options,
            ],
        ]);

        return $response->toArray();
    }
}

// 使用
$result = $notificationService->sendBatch([
    [
        'target' => ['type' => 'user', 'value' => 'user-1'],
        'event_type' => 'order.shipped',
        'payload' => ['order_id' => 'ORD-001'],
    ],
    [
        'target' => ['type' => 'user', 'value' => 'user-2'],
        'event_type' => 'order.shipped',
        'payload' => ['order_id' => 'ORD-002'],
    ],
], ['deduplicate' => true]);

echo "Sent: {$result['summary']['succeeded']}";
echo "Failed: {$result['summary']['failed']}";
```

---

## 離線訊息佇列

當使用者離線時，訊息會暫存，使用者重連後自動重播。

### 啟用佇列

```bash
export QUEUE_ENABLED=true
export QUEUE_MAX_SIZE_PER_USER=100      # 每使用者最大訊息數
export QUEUE_MESSAGE_TTL_SECONDS=3600   # 訊息存活時間（1小時）
```

### 運作流程

```
1. 發送通知給離線使用者
   └─► 訊息進入 UserMessageQueue

2. 使用者重新連線 (WebSocket 或 SSE)
   └─► 自動從佇列取出並發送所有未過期訊息

3. 訊息溢出處理
   └─► 超過 max_size 時，丟棄最舊的訊息 (FIFO)

4. 背景清理
   └─► 定期清理過期訊息
```

### 注意事項

- 佇列存於記憶體，服務重啟會遺失
- 廣播訊息不會入隊（無法追蹤離線使用者）
- 頻道訊息不會入隊（訂閱狀態會遺失）

---

## 客戶端 ACK 確認

ACK 機制用於追蹤通知是否成功送達客戶端。

### 啟用 ACK

```bash
export ACK_ENABLED=true
export ACK_TIMEOUT_SECONDS=30           # ACK 超時時間
export ACK_CLEANUP_INTERVAL_SECONDS=60  # 清理間隔
```

### 客戶端發送 ACK

```javascript
ws.onmessage = (event) => {
  const message = JSON.parse(event.data);

  if (message.type === 'notification') {
    // 處理通知...

    // 發送 ACK 確認
    ws.send(JSON.stringify({
      type: 'Ack',
      payload: { notification_id: message.id }
    }));
  }

  if (message.type === 'acked') {
    console.log('ACK confirmed for:', message.notification_id);
  }
};
```

### ACK 統計

```bash
curl http://localhost:8081/stats -H "X-API-Key: your-api-key"
```

**回應：**
```json
{
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

---

## 多租戶使用

多租戶模式提供完整的資料隔離。

### 啟用多租戶

```bash
export TENANT_ENABLED=true
export TENANT_DEFAULT_MAX_CONNECTIONS=1000
export TENANT_DEFAULT_MAX_CONNECTIONS_PER_USER=5
```

### JWT 中的租戶 ID

```php
$payload = [
    'sub' => 'user-123',
    'exp' => time() + 3600,
    'tenant_id' => 'acme-corp',  // 租戶識別碼
];
```

### 頻道隔離

啟用多租戶後，頻道會自動加上租戶前綴：

```
租戶 "acme-corp" 訂閱 "orders"
  → 內部頻道: "acme-corp:orders"

租戶 "globex" 訂閱 "orders"
  → 內部頻道: "globex:orders"

兩者互不干擾
```

### 查詢租戶統計

```bash
# 列出所有租戶
curl http://localhost:8081/api/v1/tenants \
  -H "X-API-Key: your-api-key"

# 回應
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
    }
  ],
  "total": 1
}

# 特定租戶詳情
curl http://localhost:8081/api/v1/tenants/acme-corp \
  -H "X-API-Key: your-api-key"
```

---

## 監控與統計

### 健康檢查

```bash
curl http://localhost:8081/health

# 回應
{
  "status": "healthy",
  "version": "1.0.0"
}
```

### 連線統計

```bash
curl http://localhost:8081/stats -H "X-API-Key: your-api-key"

# 回應
{
  "connections": {
    "total_connections": 150,
    "unique_users": 120,
    "channels": {
      "orders": 45,
      "system-alerts": 150
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
    "circuit_breaker_state": "closed"
  }
}
```

### Prometheus 指標

```bash
curl http://localhost:8081/metrics

# 輸出 Prometheus 文字格式
# HELP ara_connections_total Total WebSocket connections
# TYPE ara_connections_total gauge
ara_connections_total 150

# HELP ara_messages_sent_total Total messages sent
# TYPE ara_messages_sent_total counter
ara_messages_sent_total{target="user"} 5000
ara_messages_sent_total{target="broadcast"} 1000
...
```

### Grafana 儀表板設定

**連線數面板：**
```promql
ara_connections_total
```

**訊息傳遞率：**
```promql
rate(ara_messages_delivered_total[5m])
```

**ACK 成功率：**
```promql
ara_ack_received_total / ara_ack_tracked_total
```

**Redis 狀態：**
```promql
ara_redis_connection_status
```

---

## 錯誤處理

### HTTP API 錯誤

| 狀態碼 | 說明 | 處理方式 |
|--------|------|----------|
| 400 | 請求格式錯誤 | 檢查 JSON 格式 |
| 401 | 未授權 | 檢查 API Key |
| 413 | 請求過大 | 減少 payload 大小 |
| 429 | 請求過多 | 實作退避重試 |
| 500 | 伺服器錯誤 | 檢查服務日誌 |

### WebSocket 錯誤碼

| 錯誤碼 | 說明 |
|--------|------|
| `INVALID_MESSAGE` | 訊息格式錯誤 |
| `CONNECTION_LIMIT` | 連線數超限 |
| `SUBSCRIPTION_ERROR` | 訂閱失敗 |
| `INVALID_ACK` | ACK 無效 |

### 錯誤處理範例

```javascript
ws.onmessage = (event) => {
  const message = JSON.parse(event.data);

  if (message.type === 'error') {
    console.error(`Error [${message.code}]: ${message.message}`);

    switch (message.code) {
      case 'CONNECTION_LIMIT':
        // 連線數超限，可能需要關閉其他連線
        break;
      case 'SUBSCRIPTION_ERROR':
        // 訂閱失敗，可能超過限制
        break;
      case 'INVALID_ACK':
        // ACK 失敗，通知可能已過期
        break;
    }
  }
};

ws.onclose = (event) => {
  if (event.code === 1008) {
    // 認證失敗，Token 可能過期
    refreshToken().then(newToken => {
      // 使用新 Token 重連
    });
  }
};
```

---

## 最佳實踐

### 1. Token 管理

```javascript
// 在 Token 過期前主動更新
function scheduleTokenRefresh(token) {
  const payload = JSON.parse(atob(token.split('.')[1]));
  const expiresIn = payload.exp * 1000 - Date.now();
  const refreshTime = expiresIn - 60000; // 提前 1 分鐘更新

  setTimeout(async () => {
    const newToken = await refreshToken();
    // 重新連線使用新 Token
  }, refreshTime);
}
```

### 2. 重連策略

```javascript
class ReconnectingWebSocket {
  constructor(url, options = {}) {
    this.url = url;
    this.maxRetries = options.maxRetries || 10;
    this.baseDelay = options.baseDelay || 1000;
    this.maxDelay = options.maxDelay || 30000;
  }

  connect() {
    this.ws = new WebSocket(this.url);
    this.ws.onclose = () => this.reconnect();
  }

  reconnect() {
    if (this.retries >= this.maxRetries) {
      console.error('Max retries reached');
      return;
    }

    const delay = Math.min(
      this.baseDelay * Math.pow(2, this.retries),
      this.maxDelay
    );

    // 加入隨機抖動避免雷群效應
    const jitter = delay * 0.1 * Math.random();

    setTimeout(() => {
      this.retries++;
      this.connect();
    }, delay + jitter);
  }
}
```

### 3. 訊息處理

```javascript
// 使用事件佇列避免阻塞
class NotificationQueue {
  constructor() {
    this.queue = [];
    this.processing = false;
  }

  enqueue(notification) {
    this.queue.push(notification);
    this.processNext();
  }

  async processNext() {
    if (this.processing || this.queue.length === 0) return;

    this.processing = true;
    const notification = this.queue.shift();

    try {
      await this.handleNotification(notification);
    } finally {
      this.processing = false;
      this.processNext();
    }
  }

  async handleNotification(notification) {
    // 處理通知...
  }
}
```

### 4. 效能優化

- **減少訂閱數量**：只訂閱必要的頻道
- **使用批次 API**：合併多個通知請求
- **合理設定 TTL**：避免過期訊息堆積
- **啟用 ACK 追蹤**：監控送達率

---

## 常見問題

### Q: 連線建立失敗，回傳 401

**可能原因：**
1. JWT Token 格式錯誤
2. Token 已過期
3. JWT_SECRET 不匹配

**解決方式：**
```javascript
// 檢查 Token
const parts = token.split('.');
if (parts.length !== 3) {
  console.error('Invalid token format');
}

// 檢查過期時間
const payload = JSON.parse(atob(parts[1]));
if (payload.exp * 1000 < Date.now()) {
  console.error('Token expired');
}
```

### Q: 訊息發送成功但客戶端沒收到

**可能原因：**
1. 使用者未連線
2. 未訂閱對應頻道
3. 多租戶模式下租戶 ID 不匹配

**診斷步驟：**
```bash
# 檢查使用者連線狀態
curl http://localhost:8081/api/v1/users/user-123/subscriptions \
  -H "X-API-Key: your-api-key"

# 檢查頻道訂閱者
curl http://localhost:8081/api/v1/channels/orders \
  -H "X-API-Key: your-api-key"
```

### Q: 連線經常斷開

**可能原因：**
1. 網路不穩定
2. 心跳超時
3. 伺服器資源不足

**解決方式：**
```bash
# 增加超時時間
export WEBSOCKET_CONNECTION_TIMEOUT=180
export WEBSOCKET_HEARTBEAT_INTERVAL=30
```

### Q: 訊息延遲高

**可能原因：**
1. 伺服器負載高
2. Redis 連線問題
3. 客戶端處理阻塞

**診斷步驟：**
```bash
# 檢查統計
curl http://localhost:8081/stats -H "X-API-Key: your-api-key"

# 檢查 Redis 狀態
redis-cli ping
```

### Q: 收到重複通知

**可能原因：**
1. 客戶端重連後重新訂閱
2. 離線佇列重播
3. 發送端重複呼叫

**解決方式：**
```javascript
// 客戶端去重
const processedIds = new Set();

function handleNotification(notification) {
  if (processedIds.has(notification.id)) {
    return; // 已處理過
  }
  processedIds.add(notification.id);
  // 處理通知...
}
```

---

## 附錄：完整 API 參考

詳細的 API 規格請參閱 [API.md](API.md)。
