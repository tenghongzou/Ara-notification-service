# Advanced Features

This document describes the advanced feature configuration and usage of Ara Notification Service.

---

## Offline Message Queue

When a user is offline, notifications are temporarily stored in a queue and automatically replayed when the user reconnects.

### Configuration

```bash
QUEUE_ENABLED=true
QUEUE_MAX_SIZE_PER_USER=100      # Max queued messages per user
QUEUE_MESSAGE_TTL_SECONDS=3600   # Message TTL (seconds)
QUEUE_CLEANUP_INTERVAL_SECONDS=300  # Expired message cleanup interval
```

### Backend Selection

| Backend | Configuration | Characteristics |
|---------|--------------|-----------------|
| Memory | `QUEUE_BACKEND=memory` | Default, lost on restart |
| Redis | `QUEUE_BACKEND=redis` | Persistent, distributed |
| PostgreSQL | `QUEUE_BACKEND=postgres` | Persistent, queryable |

### Workflow

```
Send Notification
       │
       ▼
User Online? ──Yes──▶ Send Directly
       │
       No
       │
       ▼
Add to Queue ──▶ Set TTL
       │
       ▼
User Reconnects ──▶ Replay Queue Messages
```

### Redis Backend Configuration

```bash
QUEUE_BACKEND=redis
QUEUE_REDIS_KEY_PREFIX=ara:queue:
```

### PostgreSQL Backend Configuration

```bash
QUEUE_BACKEND=postgres
DATABASE_URL=postgres://user:pass@localhost/ara_notification
```

---

## ACK Confirmation Tracking

Track whether notifications have been acknowledged by clients.

### Configuration

```bash
ACK_ENABLED=true
ACK_TIMEOUT_SECONDS=30           # ACK timeout
ACK_CLEANUP_INTERVAL_SECONDS=60  # Expired ACK cleanup interval
```

### Backend Selection

| Backend | Configuration | Characteristics |
|---------|--------------|-----------------|
| Memory | `ACK_BACKEND=memory` | Default, lost on restart |
| Redis | `ACK_BACKEND=redis` | Persistent, distributed |
| PostgreSQL | `ACK_BACKEND=postgres` | Persistent, analyzable |

### Client ACK Flow

```javascript
// WebSocket client
ws.onmessage = (event) => {
    const msg = JSON.parse(event.data);
    if (msg.type === 'notification') {
        // Handle notification
        handleNotification(msg);

        // Send ACK confirmation
        ws.send(JSON.stringify({
            type: 'Ack',
            payload: { notification_id: msg.id }
        }));
    }
};
```

### ACK Statistics API

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

## Template System

Predefined notification templates with variable substitution support.

### Create Template

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

### Send Using Template

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

### Variable Syntax

| Syntax | Description |
|--------|-------------|
| `{{variable}}` | Basic variable substitution |
| `{{nested.field}}` | Nested object access |

### Template Management

```bash
# List all templates
GET /api/v1/templates

# Get single template
GET /api/v1/templates/{id}

# Update template
PUT /api/v1/templates/{id}

# Delete template
DELETE /api/v1/templates/{id}
```

---

## Rate Limiting

Uses Token Bucket algorithm to protect system resources.

### Configuration

```bash
RATELIMIT_ENABLED=true
RATELIMIT_HTTP_REQUESTS_PER_SECOND=100    # HTTP request limit
RATELIMIT_HTTP_BURST_SIZE=200              # HTTP burst capacity
RATELIMIT_WS_CONNECTIONS_PER_MINUTE=10     # WebSocket connection limit
```

### Backend Selection

| Backend | Configuration | Characteristics |
|---------|--------------|-----------------|
| Local | `RATELIMIT_BACKEND=local` | Default, single node |
| Redis | `RATELIMIT_BACKEND=redis` | Distributed rate limiting |

### Rate Limit Response

When rate limited, returns HTTP 429:

```json
{
  "error": {
    "code": "RATE_LIMIT_EXCEEDED",
    "message": "Too many requests",
    "retry_after_seconds": 5
  }
}
```

### Rate Limit Strategy

| Type | Identification | Configuration |
|------|---------------|---------------|
| HTTP API | API Key or IP | `RATELIMIT_HTTP_*` |
| WebSocket | IP | `RATELIMIT_WS_*` |

---

## Multi-Tenancy Support

Isolate connections, channels, and statistics between different tenants.

### Configuration

```bash
TENANT_ENABLED=true
TENANT_DEFAULT_MAX_CONNECTIONS=1000        # Default tenant connection limit
TENANT_DEFAULT_MAX_CONNECTIONS_PER_USER=5  # Default per-user connections
```

### JWT Tenant Identification

Tenant is identified via JWT `tenant_id` claim:

```json
{
  "sub": "user-123",
  "tenant_id": "acme-corp",
  "exp": 1704067200
}
```

### Channel Namespacing

With multi-tenancy enabled, channels are automatically prefixed:

```
Original channel: orders
Actual channel: tenant-acme-corp:orders
```

### Tenant API

```bash
# List all tenants
GET /api/v1/tenants

# Tenant details
GET /api/v1/tenants/{tenant_id}

{
  "tenant_id": "acme-corp",
  "connections": 234,
  "users": 89,
  "channels": 12,
  "messages_today": 5678
}
```

### Tenant-Specific Limits

```bash
# Specific tenant config (via API or config file)
TENANT_ACME_CORP_MAX_CONNECTIONS=5000
TENANT_ACME_CORP_MAX_CONNECTIONS_PER_USER=10
```

---

## Cluster Mode

Multi-node deployment with cross-node user routing support.

### Configuration

```bash
CLUSTER_ENABLED=true
CLUSTER_NODE_ID=node-1              # Unique node identifier
CLUSTER_SESSION_STORE=redis         # Session storage backend
```

### Session Storage Backend

| Backend | Configuration | Characteristics |
|---------|--------------|-----------------|
| Local | `CLUSTER_SESSION_STORE=local` | Single node, no cluster |
| Redis | `CLUSTER_SESSION_STORE=redis` | Distributed sessions |

### How It Works

```
                    ┌─────────────────────────┐
                    │    Redis Session Store  │
                    │  user-123 → node-1     │
                    │  user-456 → node-2     │
                    └───────────┬─────────────┘
                                │
        ┌───────────────────────┼───────────────────────┐
        │                       │                       │
        ▼                       ▼                       ▼
  ┌──────────┐           ┌──────────┐           ┌──────────┐
  │  Node 1  │           │  Node 2  │           │  Node 3  │
  │  user-123 ◀──────────│  Send to  │──────────▶│         │
  │  user-789│           │  user-123 │           │  user-456│
  └──────────┘           └──────────┘           └──────────┘
```

### Cluster API

```bash
# Cluster status
GET /api/v1/cluster/status

{
  "node_id": "node-1",
  "nodes": ["node-1", "node-2", "node-3"],
  "healthy_nodes": 3,
  "total_connections": 15000
}

# Query user location
GET /api/v1/cluster/users/{user_id}

{
  "user_id": "user-123",
  "node_id": "node-1",
  "connections": 2
}
```

---

## Batch Sending

Send multiple notifications in a single request.

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

### Parameters

| Parameter | Description |
|-----------|-------------|
| `notifications` | Notification array, max 100 items |
| `atomic` | Atomic operation (all succeed or all fail) |

### Response

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

## Combined Configuration

### Basic Configuration

```bash
# Minimum configuration
JWT_SECRET=your-secret
REDIS_URL=redis://localhost:6379
```

### Standard Production Configuration

```bash
RUN_MODE=production
JWT_SECRET=your-production-secret
REDIS_URL=redis://redis:6379
API_KEY=your-api-key

# Enable core features
QUEUE_ENABLED=true
RATELIMIT_ENABLED=true
ACK_ENABLED=true
```

### Enterprise Configuration

```bash
RUN_MODE=production

# Security
JWT_SECRET=your-production-secret
API_KEY=your-api-key
CORS_ORIGINS=https://app.example.com

# Multi-tenancy
TENANT_ENABLED=true
TENANT_DEFAULT_MAX_CONNECTIONS=5000

# Cluster
CLUSTER_ENABLED=true
CLUSTER_SESSION_STORE=redis

# Persistence
QUEUE_ENABLED=true
QUEUE_BACKEND=postgres
ACK_ENABLED=true
ACK_BACKEND=postgres
DATABASE_URL=postgres://user:pass@db/ara

# Rate limiting (distributed)
RATELIMIT_ENABLED=true
RATELIMIT_BACKEND=redis

# Observability
OTEL_ENABLED=true
OTEL_ENDPOINT=http://otel-collector:4317
```

---

## Related Documentation

- [System Architecture](./01-architecture.md)
- [API Reference](./03-api-reference.md)
- [Observability](./06-observability.md)

