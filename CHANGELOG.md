# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2025-12-27

### Added
- **SSE Fallback Endpoint** - Server-Sent Events as WebSocket alternative
  - Endpoint: `GET /sse?token=JWT` or with Authorization header
  - One-way notification streaming (server to client only)
  - JWT authentication (same as WebSocket)
  - Shares ConnectionManager with WebSocket connections
  - Automatic replay of queued messages on connect
  - SSE keep-alive for connection health
  - Event types: `connected`, `notification`, `heartbeat`, `error`
  - 5 unit tests

- **Multi-Tenant Support** - Complete tenant isolation and statistics
  - JWT `tenant_id` claim support (optional, defaults to "default")
  - `TenantContext` for channel namespacing (format: `{tenant_id}:{channel_name}`)
  - `TenantManager` for tenant state management, limits, and statistics
  - `TenantStats` with atomic counters for per-tenant metrics
  - Per-tenant connection limit overrides
  - Tenant API endpoints (`GET /api/v1/tenants`, `GET /api/v1/tenants/{tenant_id}`)
  - Configuration via `[tenant]` section in TOML or environment variables
  - 12 unit tests covering tenant context, namespacing, and statistics

- **Notification Template System** - Reusable templates with variable substitution
  - Template CRUD API endpoints (`POST/GET/PUT/DELETE /api/v1/templates`)
  - In-memory template storage with DashMap for thread-safe access
  - Variable substitution engine supporting `{{variable}}` syntax
  - Nested JSON object and array substitution support
  - Template ID validation (1-64 chars, alphanumeric/dash/underscore)
  - Default priority and TTL from templates
  - All notification endpoints support `template_id` + `variables` as alternative to `event_type` + `payload`
  - Batch API template support with per-item template resolution
  - 15 unit tests covering CRUD, validation, and substitution

- **Offline Message Queue** - Per-user message queue for offline delivery
  - Messages queued when user has no active connections
  - Automatic replay on reconnection
  - Configurable queue size per user (default: 100 messages)
  - Configurable message TTL (default: 1 hour)
  - FIFO overflow handling (drops oldest messages)
  - Automatic cleanup of expired messages

- **Rate Limiting** - Token Bucket algorithm for request throttling
  - Per API key / IP rate limiting for HTTP requests (default: 100 req/sec)
  - Per IP rate limiting for WebSocket connections (default: 10 conn/min)
  - Configurable burst capacity for handling traffic spikes
  - Proper 429 responses with Retry-After and X-RateLimit-* headers
  - Automatic cleanup of stale rate limit buckets
  - 9 unit tests covering all scenarios

- **Redis High Availability** - Resilient Redis connection with circuit breaker
  - Circuit breaker pattern (Closed/Open/HalfOpen states)
  - Exponential backoff with 10% jitter for reconnection
  - Configurable failure/success thresholds
  - Redis health status exposed in `/health` and `/stats` endpoints
  - Graceful degradation when Redis is unavailable
  - 11 unit tests covering circuit breaker and backoff

- **Batch Send API** - Send multiple notifications in a single request
  - Endpoint: `POST /api/v1/notifications/batch`
  - Supports all target types (user, users, broadcast, channel, channels)
  - Maximum 100 notifications per batch
  - 1MB payload limit for batch requests
  - Optional `stop_on_error` mode
  - Optional `deduplicate` mode (skips duplicate target+event_type combinations)
  - Per-notification results with batch summary
  - 13 unit tests covering parsing and deduplication

- **Channel Info API** - Query channel and subscription information
  - `GET /api/v1/channels` - List all channels with subscriber counts
  - `GET /api/v1/channels/{name}` - Get specific channel details
  - `GET /api/v1/users/{user_id}/subscriptions` - Get user's channel subscriptions
  - Proper 404 responses for non-existent channels or disconnected users
  - 10 unit tests covering channel info methods

- **Client ACK Protocol** - Notification delivery confirmation system
  - Client can acknowledge received notifications with `{ "type": "Ack", "payload": { "notification_id": "..." } }`
  - Server responds with `{ "type": "acked", "notification_id": "..." }` on valid ACK
  - User verification - ACK only valid from the user who received the notification
  - Automatic tracking when notifications are delivered (via Dispatcher integration)
  - Configurable ACK timeout (default: 30 seconds)
  - ACK statistics in `/stats` endpoint (ack_rate, avg_latency_ms, pending_count)
  - Expired ACK cleanup background task
  - 10 unit tests covering ACK tracking and statistics

- **Prometheus Metrics Export** - Comprehensive metrics for monitoring
  - Endpoint: `GET /metrics` (Prometheus text format)
  - Connection metrics: total connections, unique users, per-user distribution, channel subscriptions
  - Message metrics: sent by target type, delivered, failed, delivery latency
  - Redis metrics: connection status, circuit breaker state, reconnection count
  - Queue metrics: size, users with queue, enqueued, replayed, expired, dropped
  - Rate limiting metrics: allowed/denied by type (HTTP/WebSocket)
  - ACK metrics: tracked, received, expired, pending, latency histogram
  - WebSocket metrics: connections opened/closed, message types, connection duration
  - Batch API metrics: request count, batch size distribution
  - HTTP metrics: request count by method/path/status, request latency
  - 8 unit tests covering metric recording

- **K6 Load Testing Suite** - Comprehensive performance testing
  - `websocket.js` - WebSocket connection, subscription, and message delivery tests
  - `http-api.js` - HTTP notification API throughput and latency tests
  - `batch-api.js` - Batch API endpoint performance tests
  - `e2e-load.js` - End-to-end load test combining WebSocket clients and HTTP senders
  - 7 predefined load profiles (smoke, baseline, medium, high, stress, soak, spike)
  - Custom metrics: connection success rate, message latency, e2e latency
  - Configurable thresholds for automated pass/fail determination
  - Helper utilities: JWT generator, shared configuration
  - Cross-platform run scripts (bash and Windows batch)
  - Comprehensive documentation in `tests/load/README.md`

- **OpenTelemetry Distributed Tracing** - Full observability with OTLP export
  - OTLP gRPC exporter for Jaeger, Tempo, Zipkin, and other backends
  - Seamless integration with `tracing` crate via `tracing-opentelemetry`
  - Configurable sampling ratio (0.0-1.0)
  - HTTP handlers instrumented with request context
  - WebSocket handlers instrumented with connection and message tracking
  - NotificationDispatcher instrumented with delivery tracing
  - Span attributes for user_id, connection_id, notification_id, event_type
  - Environment variable configuration (`OTEL_ENABLED`, `OTEL_ENDPOINT`, etc.)
  - 3 unit tests covering configuration and attributes

### Planned

See [ROADMAP.md](docs/ROADMAP.md) for Phase 4 advanced features (templates, multi-tenancy, SSE).

---

## [0.1.0] - 2025-12-27

### Added

#### Core Features
- **WebSocket Real-time Push** - Low-latency WebSocket connections with Axum 0.8
- **JWT Authentication** - HS256 token validation with configurable issuer/audience
- **Multi-target Messaging** - Support for User, Users, Broadcast, Channel, and Channels targets
- **Dual Trigger System** - HTTP REST API and Redis Pub/Sub for notification delivery
- **Multi-device Support** - Per-user connection registry supporting multiple devices
- **Channel Subscriptions** - Dynamic subscribe/unsubscribe with name validation

#### Connection Management
- **DashMap Triple Indexing** - O(1) lookups for connections, users, and channels
- **Connection Limits** - Configurable total (10K) and per-user (5) connection limits
- **Subscription Limits** - Maximum 50 channels per connection
- **Heartbeat Monitoring** - Configurable ping intervals (default: 30s)
- **Stale Connection Cleanup** - Automatic removal of inactive connections (default: 120s timeout)

#### Security
- **API Key Authentication** - X-API-Key header validation for REST endpoints
- **CORS Control** - Configurable allowed origins with development mode warnings
- **Request Body Limits** - 64KB maximum payload size
- **Error Masking** - Production mode hides internal error details
- **Input Validation** - Channel name validation (1-64 chars, alphanumeric/dash/underscore/dot)

#### Performance Optimizations
- **Lock-free Activity Tracking** - AtomicI64 for last_activity timestamps
- **Message Pre-serialization** - Arc<str> sharing for broadcasts to avoid repeated serialization
- **Bounded Parallelism** - FuturesUnordered with MAX_CONCURRENT_SENDS (100)
- **Async Unregister** - Only cleans up subscribed channels

#### Infrastructure
- **Graceful Shutdown** - Broadcast signal to all background tasks
- **Redis Auto-reconnect** - 5-second retry on connection loss
- **Structured Logging** - Tracing integration with JSON output support
- **Health Check Endpoint** - GET /health with version info
- **Statistics Endpoint** - GET /stats with connection and notification metrics

#### Configuration
- **Multi-source Configuration** - Support for config files, .env, and environment variables
- **WebSocket Tuning** - Configurable heartbeat, timeout, and cleanup intervals
- **Connection Limits** - Configurable max connections and subscriptions

### Documentation
- **README.md** - Project overview with quick start guide
- **CLAUDE.md** - Development guidelines for Claude Code
- **docs/API.md** - Complete API specification
- **docs/ARCHITECTURE.md** - System architecture documentation
- **docs/ROADMAP.md** - Development roadmap with phase planning
- **CONTRIBUTING.md** - Contribution guidelines

### Testing
- 18 unit tests across 8 modules
- JWT validation tests
- Notification type and builder tests
- Channel name validation tests
- Redis message parsing tests
- Heartbeat task tests

---

## Version History

| Version | Date | Highlights |
|---------|------|------------|
| 0.1.0 | 2025-12-27 | Initial release with core features |

---

## Upgrade Notes

### From 0.0.x to 0.1.0

This is the first stable release. No migration required.

**New Environment Variables:**

| Variable | Description | Default |
|----------|-------------|---------|
| `RUN_MODE` | development/production | development |
| `API_KEY` | HTTP API authentication key | (optional) |
| `CORS_ORIGINS` | Comma-separated allowed origins | (allow all) |
| `WEBSOCKET_MAX_CONNECTIONS` | Maximum total connections | 10000 |
| `WEBSOCKET_MAX_CONNECTIONS_PER_USER` | Max connections per user | 5 |
| `WEBSOCKET_MAX_SUBSCRIPTIONS_PER_CONNECTION` | Max channels per connection | 50 |

---

## Links

- [GitHub Repository](https://github.com/your-org/ara-infra)
- [API Documentation](docs/API.md)
- [Architecture Documentation](docs/ARCHITECTURE.md)
- [Development Roadmap](docs/ROADMAP.md)
