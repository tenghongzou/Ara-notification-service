# Ara Notification Service Technical Documentation

Welcome to the Ara Notification Service technical documentation. This documentation covers system architecture, installation, API reference, development guide, and advanced features.

---

## Documentation Index

### Core Documentation

| Document | Description |
|----------|-------------|
| [Architecture](./01-architecture.md) | Tech stack, module design, data flow, design patterns |
| [Installation & Deployment](./02-installation.md) | Requirements, local development, Docker deployment, production config |
| [API Reference](./03-api-reference.md) | REST API, WebSocket protocol, Redis Pub/Sub |
| [Development Guide](./04-development-guide.md) | Coding standards, module extension, testing guide |
| [Advanced Features](./05-advanced-features.md) | Offline queue, ACK tracking, multi-tenancy, cluster mode |
| [Observability](./06-observability.md) | Prometheus monitoring, OpenTelemetry tracing, logging |

---

## Quick Navigation

### I'm new, where do I start?

1. Read [Architecture](./01-architecture.md) to understand the overall design
2. Follow [Installation & Deployment](./02-installation.md) to set up development environment
3. Check [API Reference](./03-api-reference.md) to learn how to send notifications

### I want to integrate notification service into backend?

1. Read [API Reference](./03-api-reference.md) for HTTP API and Redis Pub/Sub
2. Reference integration examples in main project README
3. See [Advanced Features](./05-advanced-features.md) for template system

### I want to develop frontend real-time notifications?

1. Read WebSocket/SSE section in [API Reference](./03-api-reference.md)
2. Reference JavaScript client examples in main project README
3. Understand channel subscription and ACK confirmation mechanism

---

## Project Highlights

### Technical Features

- **Rust + Tokio**: High-performance async runtime, supports 10,000+ concurrent connections
- **Dual Protocol Support**: WebSocket bidirectional + SSE unidirectional (firewall-friendly)
- **Three-Index Design**: DashMap-based O(1) connection lookup
- **Circuit Breaker Pattern**: Redis fault isolation, service remains available
- **SmallVec Optimization**: Reduces memory allocation overhead

### Feature Modules

| Module | Description |
|--------|-------------|
| WebSocket Handler | Bidirectional real-time communication, heartbeat, auto-reconnect |
| SSE Handler | Unidirectional push, firewall-friendly |
| Connection Manager | Three-index connection management, multi-device support |
| Notification Dispatcher | Multiple sending patterns (user, broadcast, channel) |
| Template Store | Notification templates, variable substitution |
| Message Queue | Offline message queue, replay on reconnect |
| Rate Limiter | Token Bucket rate limiting |
| ACK Tracker | Delivery confirmation tracking |
| Tenant Manager | Multi-tenant isolation |
| Cluster Router | Distributed cluster routing |

### Message Patterns

| Pattern | Description | Use Case |
|---------|-------------|----------|
| User | Send to all devices of a specific user | Personal notifications, DMs |
| Users | Send to multiple users | Group notifications |
| Broadcast | Send to all connected users | System announcements |
| Channel | Send to channel subscribers | Order status updates |
| Channels | Send to multiple channels | Cross-category notifications |

---

## Related Links

- [繁體中文文件](../zh-TW/README.md)
- [Project README](../../README.md)
- [API Specification (Original)](../API.md)
- [Architecture Document (Original)](../ARCHITECTURE.md)
- [Changelog](../../CHANGELOG.md)

