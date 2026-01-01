# API Reference

This document provides detailed documentation for all API endpoints, WebSocket protocol, and Redis Pub/Sub integration in Ara Notification Service.

---

## Overview

### Basic Information

| Item | Value |
|------|-------|
| Base URL | `http://localhost:8081` |
| WebSocket | `ws://localhost:8081/ws` |
| SSE | `http://localhost:8081/sse` |
| Authentication (HTTP API) | `X-API-Key` Header |
| Authentication (WebSocket/SSE) | JWT Token |

### Endpoint Categories

| Category | Authentication | Description |
|----------|---------------|-------------|
| Public Endpoints | None | Health check, metrics |
| Protected API | X-API-Key | REST API for sending notifications |
| Real-time Connections | JWT Token | WebSocket, SSE |

---

## Public Endpoints

### Health Check

```http
GET /health
```

**Response:**

```json
{
  "status": "healthy",
  "version": "1.0.0",
  "components": {
    "redis": "connected"
  }
}
```

### Prometheus Metrics

```http
GET /metrics
```

**Response:** Prometheus format metrics data

---

## HTTP REST API

All REST APIs require the `X-API-Key` Header:

```http
X-API-Key: your-api-key
Content-Type: application/json
```

### Send Notification (Point-to-Point)

```http
POST /api/v1/notifications/send
```

**Request:**

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

**Using Template:**

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

**Response:**

```json
{
  "notification_id": "550e8400-e29b-41d4-a716-446655440000",
  "delivered_to": 2,
  "queued": false
}
```

### Send to Multiple Users

```http
POST /api/v1/notifications/send-to-users
```

**Request:**

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

**Response:**

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

### Broadcast Notification

```http
POST /api/v1/notifications/broadcast
```

**Request:**

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

**Response:**

```json
{
  "notification_id": "...",
  "delivered_to": 1234
}
```

### Channel Notification

```http
POST /api/v1/notifications/channel
```

**Request:**

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

**Response:**

```json
{
  "notification_id": "...",
  "channel": "orders",
  "delivered_to": 45
}
```

### Multi-Channel Notification

```http
POST /api/v1/notifications/channels
```

**Request:**

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

**Response:**

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

### Batch Send

```http
POST /api/v1/notifications/batch
```

**Request:**

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

**Limit:** Maximum 100 notifications

**Response:**

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

## Channel Management

### List Channels

```http
GET /api/v1/channels
```

**Response:**

```json
{
  "channels": [
    { "name": "orders", "subscribers": 45 },
    { "name": "alerts", "subscribers": 120 },
    { "name": "system", "subscribers": 200 }
  ]
}
```

### Channel Details

```http
GET /api/v1/channels/{name}
```

**Response:**

```json
{
  "name": "orders",
  "subscribers": 45,
  "created_at": "2024-01-01T00:00:00Z"
}
```

### User Subscription List

```http
GET /api/v1/users/{user_id}/subscriptions
```

**Response:**

```json
{
  "user_id": "user-123",
  "subscriptions": ["orders", "alerts"],
  "connections": 2
}
```

---

## Template Management

### Create Template

```http
POST /api/v1/templates
```

**Request:**

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

### List Templates

```http
GET /api/v1/templates
```

### Get Template

```http
GET /api/v1/templates/{id}
```

### Update Template

```http
PUT /api/v1/templates/{id}
```

### Delete Template

```http
DELETE /api/v1/templates/{id}
```

---

## Statistics

### Connection Statistics

```http
GET /api/v1/stats
```

**Response:**

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

## WebSocket Protocol

### Connection

```
ws://localhost:8081/ws?token=<JWT>
```

Or using Header:

```
ws://localhost:8081/ws
Authorization: Bearer <JWT>
```

### Client Messages

#### Subscribe to Channel

```json
{
  "type": "Subscribe",
  "payload": {
    "channels": ["orders", "alerts"]
  }
}
```

#### Unsubscribe

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

#### ACK Confirmation

```json
{
  "type": "Ack",
  "payload": {
    "notification_id": "550e8400-e29b-41d4-a716-446655440000"
  }
}
```

### Server Messages

#### Notification

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

#### Subscription Confirmation

```json
{
  "type": "subscribed",
  "payload": ["orders", "alerts"]
}
```

#### Unsubscribe Confirmation

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

#### Heartbeat

```json
{
  "type": "heartbeat"
}
```

#### ACK Confirmation

```json
{
  "type": "acked",
  "notification_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

#### Error

```json
{
  "type": "error",
  "code": "VALIDATION_ERROR",
  "message": "Invalid channel name"
}
```

#### Shutdown Notice

```json
{
  "type": "shutdown",
  "reason": "Server maintenance",
  "reconnect_after_seconds": 60
}
```

---

## SSE Protocol

### Connection

```http
GET /sse?token=<JWT>
```

Or using Header:

```http
GET /sse
Authorization: Bearer <JWT>
```

### Event Types

#### connected

Connection success event:

```
event: connected
data: {"connection_id":"uuid","user_id":"user-123"}
```

#### notification

Notification event:

```
event: notification
data: {"id":"uuid","event_type":"order.created","payload":{...}}
```

#### heartbeat

Heartbeat event:

```
event: heartbeat
data: {"timestamp":"2024-01-01T12:00:00Z"}
```

---

## Redis Pub/Sub

### Channel Format

| Channel Pattern | Description |
|----------------|-------------|
| `notification:user:{user_id}` | Point-to-point message |
| `notification:broadcast` | Broadcast message |
| `notification:channel:{name}` | Channel message |

### Message Format

**Point-to-Point:**

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

**Broadcast:**

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

**Channel:**

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

### Symfony Integration Example

```php
// Send via Redis Pub/Sub
$redis->publish('notification:user:' . $userId, json_encode([
    'type' => 'user',
    'target' => $userId,
    'event' => [
        'event_type' => 'order.created',
        'payload' => ['order_id' => $orderId],
        'priority' => 'High',
    ],
]));

// Broadcast
$redis->publish('notification:broadcast', json_encode([
    'type' => 'broadcast',
    'event' => [
        'event_type' => 'system.maintenance',
        'payload' => ['message' => 'Maintenance in 30 minutes'],
    ],
]));

// Channel
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

## Error Handling

### HTTP Error Codes

| Status Code | Description |
|-------------|-------------|
| 400 | Bad request format |
| 401 | Authentication failed (invalid API Key) |
| 403 | Insufficient permissions |
| 404 | Resource not found |
| 422 | Validation error |
| 429 | Too many requests (rate limited) |
| 500 | Internal server error |

### Error Response Format

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

## Priority Levels

| Priority | Value | Description |
|----------|-------|-------------|
| Low | `"Low"` | Low priority, can be delayed |
| Normal | `"Normal"` | Normal priority (default) |
| High | `"High"` | High priority, process first |
| Critical | `"Critical"` | Urgent, highest priority |

---

## Related Documentation

- [System Architecture](./01-architecture.md)
- [Installation & Deployment](./02-installation.md)
- [Development Guide](./04-development-guide.md)
- [Advanced Features](./05-advanced-features.md)

