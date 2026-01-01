# Installation & Deployment

This document describes how to install, configure, and deploy Ara Notification Service.

---

## Requirements

### Required Software

| Software | Minimum Version | Recommended | Description |
|----------|----------------|-------------|-------------|
| **Rust** | 1.75 | Latest stable | Compilation and execution |
| **Redis** | 6.0 | 7.0+ | Pub/Sub, distributed state |

### Optional Software

| Software | Purpose |
|----------|---------|
| **Docker** | Containerized deployment |
| **PostgreSQL** | Persistent queue, ACK records |
| **K6** | Load testing |

---

## Local Development

### 1. Navigate to Project

```bash
# Enter service directory
cd /srv/Ara-infra/services/notification
```

### 2. Environment Configuration

```bash
# Copy example file
cp .env.example .env

# Edit .env
vim .env
```

**Minimum Configuration:**

```bash
# Required: JWT signing secret (at least 32 characters)
JWT_SECRET=your-super-secure-secret-key-at-least-32-chars

# Redis connection
REDIS_URL=redis://localhost:6379

# Log level
RUST_LOG=info
```

### 3. Start Redis

```bash
# Using Docker
docker run -d --name redis -p 6379:6379 redis:7-alpine

# Or local installation
redis-server
```

### 4. Build and Run

```bash
# Debug build
cargo build

# Run service
cargo run

# Or one step
cargo run --release
```

### 5. Verify Installation

```bash
# Health check
curl http://localhost:8081/health

# Expected response
{"status":"healthy","components":{"redis":"connected"}}
```

---

## Docker Deployment

### Using Pre-built Image

```bash
# Build image
docker build -t ara-notification-service .

# Run container
docker run -d \
  --name notification \
  -p 8081:8081 \
  -e JWT_SECRET=your-secret-key-at-least-32-chars \
  -e REDIS_URL=redis://redis:6379 \
  -e RUN_MODE=production \
  ara-notification-service
```

### Docker Compose Integration

Service is integrated into Ara-infra main project's `docker-compose.yml`:

```bash
# From Ara-infra root
cd /srv/Ara-infra

# Start notification service
docker-compose up -d notification

# View logs
docker-compose logs -f notification

# Restart service
docker-compose restart notification
```

### Dockerfile Details

```dockerfile
# Multi-stage build
FROM rust:1.75-alpine AS builder

WORKDIR /app
COPY . .

# Build release version
RUN cargo build --release

# Final image
FROM alpine:3.19

# Install runtime dependencies
RUN apk add --no-cache ca-certificates

COPY --from=builder /app/target/release/ara-notification-service /usr/local/bin/

EXPOSE 8081

CMD ["ara-notification-service"]
```

---

## Configuration Reference

### Core Configuration

| Variable | Description | Default | Required |
|----------|-------------|---------|----------|
| `SERVER_HOST` | Listen address | `0.0.0.0` | No |
| `SERVER_PORT` | Listen port | `8081` | No |
| `RUN_MODE` | Run mode | `development` | No |
| `JWT_SECRET` | JWT signing secret | - | **Yes** |
| `JWT_ISSUER` | JWT issuer validation | - | No |
| `JWT_AUDIENCE` | JWT audience validation | - | No |
| `REDIS_URL` | Redis connection URL | `redis://localhost:6379` | No |
| `API_KEY` | HTTP API authentication key | - | Recommended for production |
| `CORS_ORIGINS` | Allowed origins | - (allow all) | Recommended for production |
| `RUST_LOG` | Log level | `info` | No |

### WebSocket Configuration

| Variable | Description | Default |
|----------|-------------|---------|
| `WEBSOCKET_HEARTBEAT_INTERVAL` | Heartbeat interval (seconds) | `30` |
| `WEBSOCKET_CONNECTION_TIMEOUT` | Connection timeout (seconds) | `120` |
| `WEBSOCKET_CLEANUP_INTERVAL` | Cleanup task interval (seconds) | `60` |
| `WEBSOCKET_MAX_CONNECTIONS` | Maximum total connections | `10000` |
| `WEBSOCKET_MAX_CONNECTIONS_PER_USER` | Max connections per user | `5` |
| `WEBSOCKET_MAX_SUBSCRIPTIONS_PER_CONNECTION` | Max channels per connection | `50` |

### Redis High Availability

| Variable | Description | Default |
|----------|-------------|---------|
| `REDIS_CIRCUIT_BREAKER_FAILURE_THRESHOLD` | Circuit breaker open threshold | `5` |
| `REDIS_CIRCUIT_BREAKER_SUCCESS_THRESHOLD` | Circuit breaker close threshold | `2` |
| `REDIS_CIRCUIT_BREAKER_RESET_TIMEOUT_SECONDS` | Circuit breaker reset timeout | `30` |
| `REDIS_BACKOFF_INITIAL_DELAY_MS` | Backoff initial delay | `100` |
| `REDIS_BACKOFF_MAX_DELAY_MS` | Backoff max delay | `30000` |

### Feature Flags

| Variable | Description | Default |
|----------|-------------|---------|
| `QUEUE_ENABLED` | Offline message queue | `false` |
| `RATELIMIT_ENABLED` | Request rate limiting | `false` |
| `ACK_ENABLED` | ACK tracking | `false` |
| `TENANT_ENABLED` | Multi-tenant mode | `false` |
| `CLUSTER_ENABLED` | Cluster mode | `false` |
| `OTEL_ENABLED` | OpenTelemetry tracing | `false` |

---

## Production Configuration

### Recommended Configuration Example

```bash
# Production .env
RUN_MODE=production

# Security
JWT_SECRET=your-production-secret-key-minimum-32-characters
API_KEY=your-api-key-for-http-endpoints
CORS_ORIGINS=https://app.example.com,https://admin.example.com

# Connection settings
REDIS_URL=redis://redis.internal:6379

# Connection limits
WEBSOCKET_MAX_CONNECTIONS=50000
WEBSOCKET_MAX_CONNECTIONS_PER_USER=10

# Enable features
QUEUE_ENABLED=true
QUEUE_MAX_SIZE_PER_USER=200
QUEUE_MESSAGE_TTL_SECONDS=7200

RATELIMIT_ENABLED=true
RATELIMIT_HTTP_REQUESTS_PER_SECOND=500
RATELIMIT_HTTP_BURST_SIZE=1000

ACK_ENABLED=true
ACK_TIMEOUT_SECONDS=60

# Observability
OTEL_ENABLED=true
OTEL_ENDPOINT=http://otel-collector:4317
OTEL_SERVICE_NAME=ara-notification-production

RUST_LOG=warn,ara_notification_service=info
```

### Resource Recommendations

| Scale | Concurrent Connections | CPU | Memory | Redis |
|-------|----------------------|-----|--------|-------|
| Small | < 1,000 | 1 core | 512 MB | 1 GB |
| Medium | < 10,000 | 2 cores | 1 GB | 2 GB |
| Large | < 50,000 | 4 cores | 2 GB | 4 GB |
| Extra Large | 50,000+ | 8+ cores | 4+ GB | 8+ GB + Cluster |

---

## Health Checks

### Endpoints

```bash
# Basic health check
GET /health

# Detailed health info
GET /health?detailed=true
```

### Response Format

```json
{
  "status": "healthy",
  "version": "1.0.0",
  "uptime_seconds": 3600,
  "components": {
    "redis": "connected",
    "websocket": "ready",
    "queue": "enabled",
    "rate_limiter": "enabled"
  },
  "stats": {
    "connections": 1234,
    "users_connected": 567,
    "channels_active": 89
  }
}
```

### Kubernetes Probes

```yaml
# deployment.yaml
spec:
  containers:
    - name: notification
      livenessProbe:
        httpGet:
          path: /health
          port: 8081
        initialDelaySeconds: 5
        periodSeconds: 10
      readinessProbe:
        httpGet:
          path: /health
          port: 8081
        initialDelaySeconds: 3
        periodSeconds: 5
```

---

## Database Migrations

If using PostgreSQL as queue or ACK backend:

```bash
# Create database
createdb ara_notification

# Run migrations
psql -d ara_notification -f migrations/001_create_message_queue.sql
psql -d ara_notification -f migrations/002_create_pending_acks.sql
psql -d ara_notification -f migrations/003_create_ack_stats.sql
```

**Migration File Description:**

| File | Purpose |
|------|---------|
| `001_create_message_queue.sql` | Offline message queue table |
| `002_create_pending_acks.sql` | Pending acknowledgment table |
| `003_create_ack_stats.sql` | ACK statistics table |

---

## Troubleshooting

### Common Issues

#### 1. JWT Validation Failed

```
Error: Invalid token
```

**Solutions:**
- Ensure `JWT_SECRET` matches the backend signing secret
- Check if token is expired
- Verify `JWT_ISSUER` and `JWT_AUDIENCE` settings

#### 2. Redis Connection Failed

```
Error: Redis connection refused
```

**Solutions:**
- Confirm Redis service is running
- Check `REDIS_URL` format is correct
- Verify network connectivity

#### 3. Connection Limit Exceeded

```
Error: Connection limit exceeded
```

**Solutions:**
- Adjust `WEBSOCKET_MAX_CONNECTIONS`
- Check for connection leaks
- Consider enabling cluster mode

#### 4. High Memory Usage

**Solutions:**
- Reduce `QUEUE_MAX_SIZE_PER_USER`
- Shorten `QUEUE_MESSAGE_TTL_SECONDS`
- Enable Redis or PostgreSQL backend

---

## Related Documentation

- [System Architecture](./01-architecture.md)
- [API Reference](./03-api-reference.md)
- [Advanced Features](./05-advanced-features.md)
- [Observability](./06-observability.md)

