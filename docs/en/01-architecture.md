# System Architecture

This document describes the system architecture and design concepts of Ara Notification Service.

---

## Tech Stack

| Component | Technology | Description |
|-----------|-----------|-------------|
| Language | **Rust 1.75+** | Memory safety, high performance |
| Runtime | **Tokio** | Async runtime, multi-threaded |
| Web Framework | **Axum 0.8** | Lightweight, modular |
| WebSocket | **tokio-tungstenite** | Async WebSocket |
| Authentication | **jsonwebtoken** | JWT validation |
| Configuration | **config-rs** | Multi-source config |
| Logging | **tracing** | Structured logging |
| Metrics | **prometheus** | Prometheus metrics |
| Tracing | **opentelemetry** | Distributed tracing |

---

## System Architecture Diagram

```
                                    ┌─────────────────────────────────────────┐
                                    │           Load Balancer                 │
                                    └──────────────────┬──────────────────────┘
                                                       │
                    ┌──────────────────────────────────┼──────────────────────────────────┐
                    │                                  │                                  │
                    ▼                                  ▼                                  ▼
         ┌──────────────────┐               ┌──────────────────┐               ┌──────────────────┐
         │  Notification    │               │  Notification    │               │  Notification    │
         │  Service Node 1  │               │  Service Node 2  │               │  Service Node 3  │
         │                  │               │                  │               │                  │
         │  ┌────────────┐  │               │  ┌────────────┐  │               │  ┌────────────┐  │
         │  │ WebSocket  │  │               │  │ WebSocket  │  │               │  │ WebSocket  │  │
         │  │  Handler   │  │               │  │  Handler   │  │               │  │  Handler   │  │
         │  └────────────┘  │               │  └────────────┘  │               │  └────────────┘  │
         │  ┌────────────┐  │               │  ┌────────────┐  │               │  ┌────────────┐  │
         │  │    SSE     │  │               │  │    SSE     │  │               │  │    SSE     │  │
         │  │  Handler   │  │               │  │  Handler   │  │               │  │  Handler   │  │
         │  └────────────┘  │               │  └────────────┘  │               │  └────────────┘  │
         │  ┌────────────┐  │               │  ┌────────────┐  │               │  ┌────────────┐  │
         │  │ Connection │  │               │  │ Connection │  │               │  │ Connection │  │
         │  │  Manager   │  │               │  │  Manager   │  │               │  │  Manager   │  │
         │  └────────────┘  │               │  └────────────┘  │               │  └────────────┘  │
         └────────┬─────────┘               └────────┬─────────┘               └────────┬─────────┘
                  │                                  │                                  │
                  └──────────────────────────────────┼──────────────────────────────────┘
                                                     │
                                                     ▼
                              ┌─────────────────────────────────────────────┐
                              │              Redis Cluster                  │
                              │                                             │
                              │   ┌─────────────┐    ┌─────────────┐       │
                              │   │  Pub/Sub    │    │   Session   │       │
                              │   │  Messages   │    │    Store    │       │
                              │   └─────────────┘    └─────────────┘       │
                              └─────────────────────────────────────────────┘
                                                     ▲
                                                     │
                              ┌─────────────────────────────────────────────┐
                              │             Symfony Backend                 │
                              │                                             │
                              │   - Publish notifications via Redis         │
                              │   - Or call HTTP API                        │
                              └─────────────────────────────────────────────┘
```

---

## Module Structure

```
src/
├── lib.rs                    # Library entry, module exports
├── main.rs                   # Main entry
├── config/                   # Configuration management
│   ├── mod.rs
│   └── settings.rs           # Settings struct
├── server/                   # HTTP server
│   ├── mod.rs
│   ├── app.rs                # Route composition (single entry point)
│   └── state.rs              # AppState
├── auth/                     # Authentication
│   ├── mod.rs
│   ├── jwt.rs                # JWT validation
│   └── claims.rs             # JWT claims structure
├── websocket/                # WebSocket handling
│   ├── mod.rs
│   ├── handler.rs            # Connection handler
│   └── messages.rs           # Message types
├── sse/                      # SSE handling
│   └── handler.rs
├── notification/             # Notification core
│   ├── mod.rs
│   ├── dispatcher.rs         # Distribution logic
│   ├── event.rs              # Event definition
│   └── priority.rs           # Priority enum
├── connection_manager/       # Connection management
│   ├── mod.rs
│   ├── manager.rs            # Main logic
│   └── handle.rs             # Connection handle
├── template/                 # Template system
│   ├── mod.rs
│   ├── registry.rs           # Template registry
│   └── renderer.rs           # Variable substitution
├── queue/                    # Offline message queue
│   ├── mod.rs
│   ├── memory.rs             # Memory backend
│   ├── redis.rs              # Redis backend
│   └── postgres.rs           # PostgreSQL backend
├── ratelimit/                # Rate limiting
│   ├── mod.rs
│   ├── local.rs              # Local rate limiter
│   └── redis.rs              # Distributed rate limiter
├── redis/                    # Redis client
│   ├── mod.rs
│   ├── client.rs             # Connection management
│   └── circuit_breaker.rs    # Circuit breaker
├── triggers/                 # Notification triggers
│   ├── mod.rs
│   ├── http.rs               # HTTP API trigger
│   └── pubsub.rs             # Redis Pub/Sub trigger
├── tenant/                   # Multi-tenancy
│   └── mod.rs
├── cluster/                  # Cluster mode
│   ├── mod.rs
│   └── session_store.rs      # Distributed sessions
├── api/                      # REST API
│   ├── mod.rs
│   ├── health.rs             # Health and stats handlers
│   ├── connection.rs         # Channel/subscription query handlers
│   ├── template.rs           # Template CRUD handlers
│   ├── tenant.rs             # Tenant query handlers
│   ├── cluster.rs            # Cluster status handlers
│   └── metrics.rs            # Prometheus metrics handler
├── metrics/                  # Prometheus metrics
│   └── mod.rs
├── telemetry/                # OpenTelemetry
│   └── mod.rs
├── tasks/                    # Background tasks
│   ├── mod.rs
│   ├── heartbeat.rs          # Heartbeat task
│   └── cleanup.rs            # Cleanup task
├── shutdown/                 # Graceful shutdown
│   └── mod.rs
└── error/                    # Error handling
    └── mod.rs
```

> Single route entrypoint: `src/server/app.rs::create_app()` (composes `/health`, `/metrics`, and `/api/v1/*` routes).

---

## Core Components

### ConnectionManager

Core component responsible for managing all WebSocket/SSE connections, using DashMap for high-concurrency read/write access.

```rust
pub struct ConnectionManager {
    // Primary index: connection_id -> ConnectionHandle
    connections: DashMap<ConnectionId, Arc<ConnectionHandle>>,

    // Secondary index: user_id -> Set of connection_ids
    user_connections: DashMap<UserId, HashSet<ConnectionId>>,

    // Tertiary index: channel_name -> Set of connection_ids
    channel_subscriptions: DashMap<ChannelName, HashSet<ConnectionId>>,

    // Configuration
    config: ConnectionConfig,

    // Metrics
    metrics: ConnectionMetrics,
}
```

**Key Methods:**

| Method | Description |
|--------|-------------|
| `register()` | Register new connection |
| `unregister()` | Remove connection |
| `get_user_connections()` | Get all connections for a user |
| `subscribe_channel()` | Subscribe to channel |
| `unsubscribe_channel()` | Unsubscribe from channel |
| `get_channel_subscribers()` | Get all subscribers for a channel |
| `broadcast_all()` | Broadcast to all connections |

### NotificationDispatcher

Notification distribution engine responsible for routing notifications to correct targets.

```rust
pub struct NotificationDispatcher {
    connection_manager: Arc<ConnectionManager>,
    queue: Option<Arc<dyn MessageQueue>>,
    ack_tracker: Option<Arc<AckTracker>>,
    template_registry: Arc<TemplateRegistry>,
    metrics: DispatcherMetrics,
}
```

**Distribution Flow:**

```
Receive Notification
       │
       ▼
┌──────────────────┐
│  Template Check  │──── Has template_id ────▶ Render Template
└────────┬─────────┘                                   │
         │◀────────────────────────────────────────────┘
         ▼
┌──────────────────┐
│   Target Type    │
└────────┬─────────┘
         │
    ┌────┴────┬─────────────┐
    │         │             │
    ▼         ▼             ▼
  User     Channel      Broadcast
    │         │             │
    ▼         ▼             ▼
Get User    Get Channel   Get All
Connections Subscribers   Connections
    │         │             │
    └────┬────┴─────────────┘
         │
         ▼
┌──────────────────┐
│   Send Message   │
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│   User Online?   │
└────────┬─────────┘
         │
    ┌────┴────┐
    │         │
   Yes        No
    │         │
    ▼         ▼
  Send     Queue?
Directly   Enabled?
              │
         ┌────┴────┐
         │         │
        Yes        No
         │         │
         ▼         ▼
   Enqueue to   Discard
   Queue
```

### ConnectionHandle

Represents a single WebSocket/SSE connection.

```rust
pub struct ConnectionHandle {
    pub id: ConnectionId,
    pub user_id: UserId,
    pub tenant_id: Option<TenantId>,
    pub connection_type: ConnectionType,
    pub sender: Sender<OutgoingMessage>,
    pub subscriptions: RwLock<HashSet<ChannelName>>,
    pub connected_at: Instant,
    pub last_activity: AtomicU64,
    pub metadata: ConnectionMetadata,
}
```

---

## Design Patterns

### Circuit Breaker

Protects Redis connection from cascading failures:

```
         Normal State (Closed)
                 │
                 │ failure_count >= threshold
                 ▼
          Open State
                 │
                 │ wait reset_timeout
                 ▼
       Half-Open State
                 │
     ┌───────────┴───────────┐
     │                       │
  success                  failure
     │                       │
     ▼                       ▼
  Closed                   Open
```

**Configuration:**

```bash
REDIS_CIRCUIT_BREAKER_FAILURE_THRESHOLD=5    # Failures to open
REDIS_CIRCUIT_BREAKER_SUCCESS_THRESHOLD=2    # Successes to close
REDIS_CIRCUIT_BREAKER_RESET_TIMEOUT_SECONDS=30
```

### Token Bucket Rate Limiting

Controls request rate to protect system resources:

```
┌─────────────────────────────────────┐
│           Token Bucket              │
│                                     │
│  Capacity: 100 tokens              │
│  Refill Rate: 10 tokens/sec        │
│                                     │
│  ┌───┬───┬───┬───┬───┬───┐        │
│  │ ● │ ● │ ● │ ○ │ ○ │ ○ │ ...    │
│  └───┴───┴───┴───┴───┴───┘        │
│   ▲                                 │
│   │ Request consumes 1 token       │
└───┴─────────────────────────────────┘
```

### Exponential Backoff

Reconnection strategy to prevent thundering herd:

```rust
delay = min(
    initial_delay * (2 ^ attempt) + jitter,
    max_delay
)
```

**Configuration:**

```bash
REDIS_BACKOFF_INITIAL_DELAY_MS=100
REDIS_BACKOFF_MAX_DELAY_MS=30000
```

---

## Data Flows

### WebSocket Connection Flow

```
Client                    Service                    Redis
   │                         │                         │
   │──── WS Connect ────────▶│                         │
   │     (JWT Token)         │                         │
   │                         │                         │
   │                    Validate JWT                   │
   │                         │                         │
   │                    Register Connection            │
   │                         │                         │
   │◀─── Connected ─────────│                         │
   │     (connection_id)     │                         │
   │                         │                         │
   │──── Subscribe ─────────▶│                         │
   │     (channels)          │                         │
   │                         │────── SUBSCRIBE ───────▶│
   │                         │                         │
   │◀─── Subscribed ─────────│                         │
   │                         │                         │
   │                         │◀───── MESSAGE ─────────│
   │◀─── Notification ───────│                         │
   │                         │                         │
   │──── Ack ───────────────▶│                         │
   │                         │                         │
```

### Notification Send Flow

```
Symfony                  Service                   Client
   │                        │                         │
   │──── HTTP POST ────────▶│                         │
   │     or Redis Pub       │                         │
   │                        │                         │
   │                   Parse Request                  │
   │                        │                         │
   │                   Lookup Connections             │
   │                        │                         │
   │                        │──── Send Message ──────▶│
   │                        │                         │
   │                   Update Metrics                 │
   │                        │                         │
   │◀─── Response ─────────│                         │
   │     (notification_id)  │                         │
```

---

## Performance Optimizations

### DashMap Sharding

DashMap internally uses sharding to reduce lock contention:

```
┌────────────────────────────────────────┐
│              DashMap                    │
│                                        │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐  │
│  │ Shard 0 │ │ Shard 1 │ │ Shard N │  │
│  │  Lock   │ │  Lock   │ │  Lock   │  │
│  │  Data   │ │  Data   │ │  Data   │  │
│  └─────────┘ └─────────┘ └─────────┘  │
│                                        │
│  Key hash determines shard             │
└────────────────────────────────────────┘
```

### SmallVec Optimization

Reduces heap allocations for small collections:

```rust
// Most users have only 1-2 connections
// SmallVec stores small data on stack
type UserConnections = SmallVec<[ConnectionId; 4]>;
```

### Message Batching

Batch process multiple notifications:

```rust
// Internal batch processing
async fn dispatch_batch(&self, notifications: Vec<Notification>) {
    // Group by target to reduce lookups
    let grouped = group_by_target(notifications);

    for (target, batch) in grouped {
        self.dispatch_to_target(target, batch).await;
    }
}
```

---

## Related Documentation

- [Installation & Deployment](./02-installation.md)
- [API Reference](./03-api-reference.md)
- [Development Guide](./04-development-guide.md)
