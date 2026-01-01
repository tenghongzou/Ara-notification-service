# 可觀測性

本文件說明 Ara Notification Service 的監控、追蹤與日誌配置。

---

## Prometheus 監控

### 指標端點

```http
GET /metrics
```

回傳 Prometheus 格式的指標資料。

### 核心指標

#### 連線指標

| 指標 | 類型 | 說明 |
|------|------|------|
| `ara_connections_total` | Gauge | 當前總連線數 |
| `ara_users_connected` | Gauge | 已連線使用者數 |
| `ara_channels_active` | Gauge | 活躍頻道數 |
| `ara_channel_subscriptions` | Gauge | 頻道訂閱總數 |

#### 訊息指標

| 指標 | 類型 | 說明 |
|------|------|------|
| `ara_messages_sent_total` | Counter | 發送訊息總數 (by target_type) |
| `ara_messages_delivered_total` | Counter | 成功送達總數 |
| `ara_messages_failed_total` | Counter | 發送失敗總數 |
| `ara_message_delivery_latency_seconds` | Histogram | 訊息送達延遲 |

#### 佇列指標

| 指標 | 類型 | 說明 |
|------|------|------|
| `ara_queue_messages_total` | Gauge | 佇列訊息總數 |
| `ara_queue_messages_per_user` | Gauge | 每使用者佇列訊息數 |
| `ara_queue_messages_expired_total` | Counter | 過期訊息總數 |

#### ACK 指標

| 指標 | 類型 | 說明 |
|------|------|------|
| `ara_ack_pending_total` | Gauge | 待確認通知數 |
| `ara_ack_received_total` | Counter | 已確認通知數 |
| `ara_ack_timeout_total` | Counter | 超時未確認數 |
| `ara_ack_latency_seconds` | Histogram | ACK 回應時間 |

#### 限流指標

| 指標 | 類型 | 說明 |
|------|------|------|
| `ara_ratelimit_requests_total` | Counter | 總請求數 |
| `ara_ratelimit_rejected_total` | Counter | 被拒絕請求數 |
| `ara_ratelimit_tokens_available` | Gauge | 可用令牌數 |

#### Redis 指標

| 指標 | 類型 | 說明 |
|------|------|------|
| `ara_redis_connection_status` | Gauge | 連線狀態 (1=connected, 0=disconnected) |
| `ara_redis_circuit_breaker_state` | Gauge | 熔斷器狀態 (0=closed, 1=open, 2=half-open) |
| `ara_redis_reconnect_attempts_total` | Counter | 重連嘗試次數 |

### Prometheus 配置範例

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

建議的面板配置：

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

## OpenTelemetry 追蹤

### 啟用配置

```bash
OTEL_ENABLED=true
OTEL_ENDPOINT=http://otel-collector:4317   # OTLP gRPC 端點
OTEL_SERVICE_NAME=ara-notification-service
OTEL_SAMPLING_RATIO=1.0                     # 取樣比率 (0.0-1.0)
```

### 支援的收集器

| 收集器 | 說明 |
|--------|------|
| **Jaeger** | 分散式追蹤系統 |
| **Tempo** | Grafana 追蹤後端 |
| **Zipkin** | 分散式追蹤系統 |
| **OTLP Collector** | OpenTelemetry 標準收集器 |

### 追蹤範圍

服務自動產生以下追蹤：

| Span 名稱 | 說明 |
|----------|------|
| `http.request` | HTTP 請求處理 |
| `websocket.connection` | WebSocket 連線生命週期 |
| `notification.send` | 通知發送流程 |
| `notification.dispatch` | 通知派發 |
| `redis.publish` | Redis 發布操作 |
| `queue.enqueue` | 佇列入隊 |
| `queue.replay` | 佇列重播 |

### OpenTelemetry Collector 配置

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

## 結構化日誌

### 日誌等級

```bash
# 設定日誌等級
RUST_LOG=info                    # 基本資訊
RUST_LOG=debug                   # 詳細除錯
RUST_LOG=trace                   # 最詳細
RUST_LOG=warn                    # 僅警告和錯誤
RUST_LOG=error                   # 僅錯誤

# 模組級別設定
RUST_LOG=warn,ara_notification_service=debug
RUST_LOG=info,ara_notification_service::websocket=trace
```

### 日誌格式

#### 標準格式 (開發)

```
2024-01-01T12:00:00.000Z  INFO ara_notification_service::websocket: New connection established user_id="user-123" connection_id="uuid"
```

#### JSON 格式 (生產)

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

### 日誌內容

#### 連線日誌

```
INFO  New connection established user_id="user-123" tenant_id="default"
INFO  Connection closed user_id="user-123" duration_seconds=3600
WARN  Connection limit exceeded user_id="user-123" current=5 max=5
```

#### 訊息日誌

```
INFO  Notification sent notification_id="uuid" target_type="user" delivered=3
DEBUG Notification payload notification_id="uuid" event_type="order.created"
ERROR Failed to deliver notification_id="uuid" error="Connection closed"
```

#### Redis 日誌

```
WARN  Redis connection lost, attempting reconnect attempt=1
INFO  Redis connection restored after_seconds=5
ERROR Redis circuit breaker opened failures=5
```

### 日誌收集

#### Fluent Bit 配置

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

#### Loki 配置

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

## 健康檢查

### 端點

```http
GET /health
GET /health?detailed=true
```

### 回應格式

**簡單回應：**

```json
{
  "status": "healthy"
}
```

**詳細回應：**

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

### 健康狀態

| 狀態 | HTTP 碼 | 說明 |
|------|---------|------|
| `healthy` | 200 | 所有元件正常 |
| `degraded` | 200 | 部分元件異常，服務可用 |
| `unhealthy` | 503 | 關鍵元件異常，服務不可用 |

---

## 告警規則

### Prometheus 告警範例

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

## 儀表板範本

### 概覽儀表板

建議的面板：

1. **連線概覽**
   - 總連線數 (Stat)
   - 連線使用者數 (Stat)
   - 連線趨勢 (Graph)

2. **訊息流量**
   - 發送速率 (Graph)
   - 送達成功率 (Gauge)
   - 延遲分布 (Heatmap)

3. **系統健康**
   - Redis 狀態 (Stat)
   - 熔斷器狀態 (Stat)
   - 錯誤率 (Graph)

4. **資源使用**
   - 佇列深度 (Graph)
   - 限流拒絕率 (Graph)
   - 記憶體使用 (Graph)

---

## 相關文件

- [系統架構](./01-architecture.md)
- [安裝與部署](./02-installation.md)
- [進階功能](./05-advanced-features.md)

