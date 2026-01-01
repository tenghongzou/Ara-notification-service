# Observability

This document describes the monitoring, tracing, and logging configuration for Ara Notification Service.

---

## Prometheus Monitoring

### Metrics Endpoint

```http
GET /metrics
```

Returns Prometheus format metrics data.

### Core Metrics

#### Connection Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `ara_connections_total` | Gauge | Current total connections |
| `ara_users_connected` | Gauge | Connected users count |
| `ara_channels_active` | Gauge | Active channels count |
| `ara_channel_subscriptions` | Gauge | Total channel subscriptions |

#### Message Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `ara_messages_sent_total` | Counter | Total messages sent (by target_type) |
| `ara_messages_delivered_total` | Counter | Successfully delivered count |
| `ara_messages_failed_total` | Counter | Failed delivery count |
| `ara_message_delivery_latency_seconds` | Histogram | Message delivery latency |

#### Queue Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `ara_queue_messages_total` | Gauge | Total queued messages |
| `ara_queue_messages_per_user` | Gauge | Queued messages per user |
| `ara_queue_messages_expired_total` | Counter | Total expired messages |

#### ACK Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `ara_ack_pending_total` | Gauge | Pending acknowledgments |
| `ara_ack_received_total` | Counter | Acknowledged notifications |
| `ara_ack_timeout_total` | Counter | Timed out acknowledgments |
| `ara_ack_latency_seconds` | Histogram | ACK response time |

#### Rate Limit Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `ara_ratelimit_requests_total` | Counter | Total requests |
| `ara_ratelimit_rejected_total` | Counter | Rejected requests |
| `ara_ratelimit_tokens_available` | Gauge | Available tokens |

#### Redis Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `ara_redis_connection_status` | Gauge | Connection status (1=connected, 0=disconnected) |
| `ara_redis_circuit_breaker_state` | Gauge | Circuit breaker state (0=closed, 1=open, 2=half-open) |
| `ara_redis_reconnect_attempts_total` | Counter | Reconnection attempts |

### Prometheus Configuration Example

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'ara-notification'
    static_configs:
      - targets: ['notification:8081']
    metrics_path: '/metrics'
    scrape_interval: 15s
```

### Grafana Dashboard

Recommended panel configuration:

```json
{
  "panels": [
    {
      "title": "Active Connections",
      "type": "stat",
      "targets": [
        { "expr": "ara_connections_total" }
      ]
    },
    {
      "title": "Messages Rate",
      "type": "graph",
      "targets": [
        { "expr": "rate(ara_messages_sent_total[5m])" }
      ]
    },
    {
      "title": "Delivery Latency",
      "type": "heatmap",
      "targets": [
        { "expr": "ara_message_delivery_latency_seconds_bucket" }
      ]
    },
    {
      "title": "Redis Health",
      "type": "stat",
      "targets": [
        { "expr": "ara_redis_connection_status" }
      ]
    }
  ]
}
```

---

## OpenTelemetry Tracing

### Configuration

```bash
OTEL_ENABLED=true
OTEL_ENDPOINT=http://otel-collector:4317   # OTLP gRPC endpoint
OTEL_SERVICE_NAME=ara-notification-service
OTEL_SAMPLING_RATIO=1.0                     # Sampling ratio (0.0-1.0)
```

### Supported Collectors

| Collector | Description |
|-----------|-------------|
| **Jaeger** | Distributed tracing system |
| **Tempo** | Grafana tracing backend |
| **Zipkin** | Distributed tracing system |
| **OTLP Collector** | OpenTelemetry standard collector |

### Trace Spans

Service automatically generates the following traces:

| Span Name | Description |
|-----------|-------------|
| `http.request` | HTTP request handling |
| `websocket.connection` | WebSocket connection lifecycle |
| `notification.send` | Notification send flow |
| `notification.dispatch` | Notification dispatch |
| `redis.publish` | Redis publish operation |
| `queue.enqueue` | Queue enqueue |
| `queue.replay` | Queue replay |

### OpenTelemetry Collector Configuration

```yaml
# otel-collector-config.yaml
receivers:
  otlp:
    protocols:
      grpc:
        endpoint: 0.0.0.0:4317

processors:
  batch:
    timeout: 1s
    send_batch_size: 1024

exporters:
  jaeger:
    endpoint: jaeger:14250
    tls:
      insecure: true

service:
  pipelines:
    traces:
      receivers: [otlp]
      processors: [batch]
      exporters: [jaeger]
```

### Jaeger Docker Compose

```yaml
version: '3.8'

services:
  jaeger:
    image: jaegertracing/all-in-one:latest
    ports:
      - "16686:16686"  # UI
      - "14250:14250"  # gRPC
    environment:
      - COLLECTOR_OTLP_ENABLED=true

  otel-collector:
    image: otel/opentelemetry-collector:latest
    command: ["--config=/etc/otel-collector-config.yaml"]
    volumes:
      - ./otel-collector-config.yaml:/etc/otel-collector-config.yaml
    ports:
      - "4317:4317"
    depends_on:
      - jaeger
```

---

## Structured Logging

### Log Levels

```bash
# Set log level
RUST_LOG=info                    # Basic info
RUST_LOG=debug                   # Detailed debug
RUST_LOG=trace                   # Most detailed
RUST_LOG=warn                    # Warnings and errors only
RUST_LOG=error                   # Errors only

# Module-level settings
RUST_LOG=warn,ara_notification_service=debug
RUST_LOG=info,ara_notification_service::websocket=trace
```

### Log Formats

#### Standard Format (Development)

```
2024-01-01T12:00:00.000Z  INFO ara_notification_service::websocket: New connection established user_id="user-123" connection_id="uuid"
```

#### JSON Format (Production)

```bash
RUST_LOG_FORMAT=json
```

```json
{
  "timestamp": "2024-01-01T12:00:00.000Z",
  "level": "INFO",
  "target": "ara_notification_service::websocket",
  "message": "New connection established",
  "user_id": "user-123",
  "connection_id": "uuid"
}
```

### Log Content

#### Connection Logs

```
INFO  New connection established user_id="user-123" tenant_id="default"
INFO  Connection closed user_id="user-123" duration_seconds=3600
WARN  Connection limit exceeded user_id="user-123" current=5 max=5
```

#### Message Logs

```
INFO  Notification sent notification_id="uuid" target_type="user" delivered=3
DEBUG Notification payload notification_id="uuid" event_type="order.created"
ERROR Failed to deliver notification_id="uuid" error="Connection closed"
```

#### Redis Logs

```
WARN  Redis connection lost, attempting reconnect attempt=1
INFO  Redis connection restored after_seconds=5
ERROR Redis circuit breaker opened failures=5
```

### Log Collection

#### Fluent Bit Configuration

```ini
[INPUT]
    Name              tail
    Path              /var/log/notification/*.log
    Parser            json

[OUTPUT]
    Name              es
    Match             *
    Host              elasticsearch
    Port              9200
    Index             ara-notification
```

#### Loki Configuration

```yaml
# promtail.yaml
scrape_configs:
  - job_name: ara-notification
    static_configs:
      - targets:
          - localhost
        labels:
          job: notification
          __path__: /var/log/notification/*.log
```

---

## Health Checks

### Endpoints

```http
GET /health
GET /health?detailed=true
```

### Response Format

**Simple Response:**

```json
{
  "status": "healthy"
}
```

**Detailed Response:**

```json
{
  "status": "healthy",
  "version": "1.0.0",
  "uptime_seconds": 86400,
  "components": {
    "redis": {
      "status": "connected",
      "latency_ms": 1
    },
    "websocket": {
      "status": "ready",
      "connections": 1234
    },
    "queue": {
      "status": "enabled",
      "pending_messages": 456
    },
    "rate_limiter": {
      "status": "enabled",
      "rejected_last_minute": 12
    }
  },
  "stats": {
    "connections_total": 1234,
    "users_connected": 567,
    "channels_active": 89,
    "messages_sent_today": 12345
  }
}
```

### Health Status

| Status | HTTP Code | Description |
|--------|-----------|-------------|
| `healthy` | 200 | All components normal |
| `degraded` | 200 | Some components abnormal, service available |
| `unhealthy` | 503 | Critical components abnormal, service unavailable |

---

## Alerting Rules

### Prometheus Alerting Example

```yaml
# alerts.yml
groups:
  - name: ara-notification
    rules:
      - alert: HighConnectionCount
        expr: ara_connections_total > 9000
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High connection count"
          description: "Connection count is {{ $value }}"

      - alert: RedisDisconnected
        expr: ara_redis_connection_status == 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Redis disconnected"

      - alert: HighMessageFailureRate
        expr: rate(ara_messages_failed_total[5m]) / rate(ara_messages_sent_total[5m]) > 0.01
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High message failure rate"
          description: "Failure rate is {{ $value | humanizePercentage }}"

      - alert: CircuitBreakerOpen
        expr: ara_redis_circuit_breaker_state == 1
        for: 0m
        labels:
          severity: critical
        annotations:
          summary: "Redis circuit breaker is open"

      - alert: QueueBacklog
        expr: ara_queue_messages_total > 10000
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "Large queue backlog"
```

---

## Dashboard Templates

### Overview Dashboard

Recommended panels:

1. **Connection Overview**
   - Total connections (Stat)
   - Connected users (Stat)
   - Connection trend (Graph)

2. **Message Traffic**
   - Send rate (Graph)
   - Delivery success rate (Gauge)
   - Latency distribution (Heatmap)

3. **System Health**
   - Redis status (Stat)
   - Circuit breaker status (Stat)
   - Error rate (Graph)

4. **Resource Usage**
   - Queue depth (Graph)
   - Rate limit rejection rate (Graph)
   - Memory usage (Graph)

---

## Related Documentation

- [System Architecture](./01-architecture.md)
- [Installation & Deployment](./02-installation.md)
- [Advanced Features](./05-advanced-features.md)

