//! Multi-tenant support for the notification service.
//!
//! This module provides:
//! - Tenant identification from JWT claims
//! - Channel namespacing for tenant isolation
//! - Per-tenant connection limits
//! - Per-tenant statistics
//!
//! # Channel Namespacing
//!
//! When multi-tenancy is enabled, channels are automatically namespaced:
//! - User subscribes to "orders" → Internal channel becomes "tenant-acme:orders"
//! - This ensures complete isolation between tenants
//!
//! # Configuration
//!
//! Multi-tenancy can be enabled via environment variables:
//! - `TENANT_ENABLED=true` - Enable multi-tenant mode
//! - `TENANT_DEFAULT_MAX_CONNECTIONS=1000` - Default per-tenant connection limit
//! - `TENANT_DEFAULT_MAX_CONNECTIONS_PER_USER=5` - Default per-tenant per-user limit

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::auth::DEFAULT_TENANT_ID;
use crate::connection_manager::ConnectionLimits;

// ============================================================================
// Configuration
// ============================================================================

/// Multi-tenant configuration
#[derive(Debug, Clone, Deserialize)]
pub struct TenantConfig {
    /// Whether multi-tenancy is enabled
    #[serde(default)]
    pub enabled: bool,
    /// Default connection limits for tenants without specific overrides
    #[serde(default = "default_tenant_limits")]
    pub default_limits: TenantLimitsConfig,
    /// Per-tenant limit overrides (tenant_id -> limits)
    #[serde(default)]
    pub tenant_overrides: HashMap<String, TenantLimitsConfig>,
}

impl Default for TenantConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_limits: default_tenant_limits(),
            tenant_overrides: HashMap::new(),
        }
    }
}

/// Per-tenant connection limits configuration
#[derive(Debug, Clone, Deserialize)]
pub struct TenantLimitsConfig {
    /// Maximum total connections for this tenant
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
    /// Maximum connections per user for this tenant
    #[serde(default = "default_max_connections_per_user")]
    pub max_connections_per_user: usize,
    /// Maximum subscriptions per connection
    #[serde(default = "default_max_subscriptions")]
    pub max_subscriptions_per_connection: usize,
}

fn default_tenant_limits() -> TenantLimitsConfig {
    TenantLimitsConfig {
        max_connections: 1000,
        max_connections_per_user: 5,
        max_subscriptions_per_connection: 50,
    }
}

fn default_max_connections() -> usize {
    1000
}

fn default_max_connections_per_user() -> usize {
    5
}

fn default_max_subscriptions() -> usize {
    50
}

impl From<&TenantLimitsConfig> for ConnectionLimits {
    fn from(config: &TenantLimitsConfig) -> Self {
        ConnectionLimits {
            max_connections: config.max_connections,
            max_connections_per_user: config.max_connections_per_user,
            max_subscriptions_per_connection: config.max_subscriptions_per_connection,
        }
    }
}

// ============================================================================
// Tenant Context
// ============================================================================

/// Context for a tenant, used in request/connection handling
#[derive(Debug, Clone)]
pub struct TenantContext {
    /// The tenant ID
    pub tenant_id: String,
    /// Whether this is the default tenant
    pub is_default: bool,
}

impl TenantContext {
    /// Create a new tenant context
    pub fn new(tenant_id: impl Into<String>) -> Self {
        let tenant_id = tenant_id.into();
        let is_default = tenant_id == DEFAULT_TENANT_ID;
        Self {
            tenant_id,
            is_default,
        }
    }

    /// Create a context for the default tenant
    pub fn default_tenant() -> Self {
        Self::new(DEFAULT_TENANT_ID)
    }

    /// Namespace a channel name for this tenant
    ///
    /// When multi-tenancy is enabled, channels are prefixed with the tenant ID
    /// to ensure complete isolation between tenants.
    pub fn namespace_channel(&self, channel: &str) -> String {
        if self.is_default {
            channel.to_string()
        } else {
            format!("{}:{}", self.tenant_id, channel)
        }
    }

    /// Extract the original channel name from a namespaced channel
    ///
    /// Returns the original name if the channel belongs to this tenant,
    /// or None if it belongs to a different tenant.
    pub fn extract_channel_name(&self, namespaced_channel: &str) -> Option<String> {
        if self.is_default {
            // Default tenant: channel names are not namespaced
            if namespaced_channel.contains(':') {
                // This is a namespaced channel from another tenant
                None
            } else {
                Some(namespaced_channel.to_string())
            }
        } else {
            // Non-default tenant: check for matching prefix
            let prefix = format!("{}:", self.tenant_id);
            if let Some(name) = namespaced_channel.strip_prefix(&prefix) {
                Some(name.to_string())
            } else {
                None
            }
        }
    }
}

// ============================================================================
// Tenant Manager
// ============================================================================

/// Manages tenant-specific state and limits
pub struct TenantManager {
    /// Configuration
    config: TenantConfig,
    /// Per-tenant statistics
    stats: DashMap<String, TenantStats>,
}

impl TenantManager {
    /// Create a new tenant manager
    pub fn new(config: TenantConfig) -> Self {
        Self {
            config,
            stats: DashMap::new(),
        }
    }

    /// Check if multi-tenancy is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get connection limits for a tenant
    pub fn get_limits(&self, tenant_id: &str) -> ConnectionLimits {
        if !self.config.enabled {
            // When disabled, return global limits
            return ConnectionLimits::default();
        }

        // Check for tenant-specific overrides
        if let Some(override_config) = self.config.tenant_overrides.get(tenant_id) {
            return override_config.into();
        }

        // Use default tenant limits
        (&self.config.default_limits).into()
    }

    /// Create a tenant context from a tenant ID
    pub fn create_context(&self, tenant_id: &str) -> TenantContext {
        if self.config.enabled {
            TenantContext::new(tenant_id)
        } else {
            TenantContext::default_tenant()
        }
    }

    /// Get or create stats for a tenant
    pub fn get_stats(&self, tenant_id: &str) -> TenantStatsSnapshot {
        if let Some(stats) = self.stats.get(tenant_id) {
            stats.snapshot()
        } else {
            TenantStatsSnapshot::default()
        }
    }

    /// Record a new connection for a tenant
    pub fn record_connection(&self, tenant_id: &str) {
        self.stats
            .entry(tenant_id.to_string())
            .or_insert_with(TenantStats::new)
            .record_connection();
    }

    /// Record a disconnection for a tenant
    pub fn record_disconnection(&self, tenant_id: &str) {
        if let Some(stats) = self.stats.get(tenant_id) {
            stats.record_disconnection();
        }
    }

    /// Record a message sent for a tenant
    pub fn record_message_sent(&self, tenant_id: &str) {
        self.stats
            .entry(tenant_id.to_string())
            .or_insert_with(TenantStats::new)
            .record_message_sent();
    }

    /// Record a message delivered for a tenant
    pub fn record_message_delivered(&self, tenant_id: &str, count: usize) {
        self.stats
            .entry(tenant_id.to_string())
            .or_insert_with(TenantStats::new)
            .record_message_delivered(count);
    }

    /// List all tenants with active connections
    pub fn list_active_tenants(&self) -> Vec<TenantInfo> {
        self.stats
            .iter()
            .filter(|entry| entry.active_connections.load(Ordering::Relaxed) > 0)
            .map(|entry| TenantInfo {
                tenant_id: entry.key().clone(),
                stats: entry.snapshot(),
            })
            .collect()
    }

    /// Get all tenant stats
    pub fn all_stats(&self) -> HashMap<String, TenantStatsSnapshot> {
        self.stats
            .iter()
            .map(|entry| (entry.key().clone(), entry.snapshot()))
            .collect()
    }
}

impl Default for TenantManager {
    fn default() -> Self {
        Self::new(TenantConfig::default())
    }
}

// ============================================================================
// Tenant Statistics
// ============================================================================

/// Per-tenant statistics (thread-safe with atomic counters)
pub struct TenantStats {
    /// Current active connections
    pub active_connections: AtomicU64,
    /// Total connections ever made
    pub total_connections: AtomicU64,
    /// Total messages sent
    pub messages_sent: AtomicU64,
    /// Total messages delivered
    pub messages_delivered: AtomicU64,
}

impl TenantStats {
    pub fn new() -> Self {
        Self {
            active_connections: AtomicU64::new(0),
            total_connections: AtomicU64::new(0),
            messages_sent: AtomicU64::new(0),
            messages_delivered: AtomicU64::new(0),
        }
    }

    pub fn record_connection(&self) {
        self.active_connections.fetch_add(1, Ordering::Relaxed);
        self.total_connections.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_disconnection(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn record_message_sent(&self) {
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_message_delivered(&self, count: usize) {
        self.messages_delivered
            .fetch_add(count as u64, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> TenantStatsSnapshot {
        TenantStatsSnapshot {
            active_connections: self.active_connections.load(Ordering::Relaxed),
            total_connections: self.total_connections.load(Ordering::Relaxed),
            messages_sent: self.messages_sent.load(Ordering::Relaxed),
            messages_delivered: self.messages_delivered.load(Ordering::Relaxed),
        }
    }
}

impl Default for TenantStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of tenant statistics
#[derive(Debug, Clone, Default, Serialize)]
pub struct TenantStatsSnapshot {
    pub active_connections: u64,
    pub total_connections: u64,
    pub messages_sent: u64,
    pub messages_delivered: u64,
}

/// Information about an active tenant
#[derive(Debug, Clone, Serialize)]
pub struct TenantInfo {
    pub tenant_id: String,
    pub stats: TenantStatsSnapshot,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tenant_context_default() {
        let ctx = TenantContext::default_tenant();
        assert_eq!(ctx.tenant_id, DEFAULT_TENANT_ID);
        assert!(ctx.is_default);
    }

    #[test]
    fn test_tenant_context_custom() {
        let ctx = TenantContext::new("acme-corp");
        assert_eq!(ctx.tenant_id, "acme-corp");
        assert!(!ctx.is_default);
    }

    #[test]
    fn test_namespace_channel_default_tenant() {
        let ctx = TenantContext::default_tenant();
        assert_eq!(ctx.namespace_channel("orders"), "orders");
    }

    #[test]
    fn test_namespace_channel_custom_tenant() {
        let ctx = TenantContext::new("acme-corp");
        assert_eq!(ctx.namespace_channel("orders"), "acme-corp:orders");
    }

    #[test]
    fn test_extract_channel_name_default_tenant() {
        let ctx = TenantContext::default_tenant();
        assert_eq!(ctx.extract_channel_name("orders"), Some("orders".to_string()));
        // Namespaced channels from other tenants should be rejected
        assert_eq!(ctx.extract_channel_name("acme:orders"), None);
    }

    #[test]
    fn test_extract_channel_name_custom_tenant() {
        let ctx = TenantContext::new("acme-corp");
        assert_eq!(
            ctx.extract_channel_name("acme-corp:orders"),
            Some("orders".to_string())
        );
        // Non-matching prefix
        assert_eq!(ctx.extract_channel_name("other:orders"), None);
        // No prefix
        assert_eq!(ctx.extract_channel_name("orders"), None);
    }

    #[test]
    fn test_tenant_manager_disabled() {
        let manager = TenantManager::new(TenantConfig::default());
        assert!(!manager.is_enabled());

        // Context should always be default when disabled
        let ctx = manager.create_context("acme-corp");
        assert!(ctx.is_default);
    }

    #[test]
    fn test_tenant_manager_enabled() {
        let config = TenantConfig {
            enabled: true,
            ..Default::default()
        };
        let manager = TenantManager::new(config);
        assert!(manager.is_enabled());

        // Context should use provided tenant_id when enabled
        let ctx = manager.create_context("acme-corp");
        assert!(!ctx.is_default);
        assert_eq!(ctx.tenant_id, "acme-corp");
    }

    #[test]
    fn test_tenant_limits_override() {
        let mut overrides = HashMap::new();
        overrides.insert(
            "premium".to_string(),
            TenantLimitsConfig {
                max_connections: 5000,
                max_connections_per_user: 10,
                max_subscriptions_per_connection: 100,
            },
        );

        let config = TenantConfig {
            enabled: true,
            default_limits: default_tenant_limits(),
            tenant_overrides: overrides,
        };
        let manager = TenantManager::new(config);

        // Default tenant gets default limits
        let default_limits = manager.get_limits("regular-tenant");
        assert_eq!(default_limits.max_connections, 1000);

        // Premium tenant gets overridden limits
        let premium_limits = manager.get_limits("premium");
        assert_eq!(premium_limits.max_connections, 5000);
        assert_eq!(premium_limits.max_connections_per_user, 10);
    }

    #[test]
    fn test_tenant_stats_recording() {
        let manager = TenantManager::new(TenantConfig {
            enabled: true,
            ..Default::default()
        });

        // Record some activity
        manager.record_connection("acme");
        manager.record_connection("acme");
        manager.record_message_sent("acme");
        manager.record_message_delivered("acme", 5);
        manager.record_disconnection("acme");

        let stats = manager.get_stats("acme");
        assert_eq!(stats.active_connections, 1);
        assert_eq!(stats.total_connections, 2);
        assert_eq!(stats.messages_sent, 1);
        assert_eq!(stats.messages_delivered, 5);
    }

    #[test]
    fn test_list_active_tenants() {
        let manager = TenantManager::new(TenantConfig {
            enabled: true,
            ..Default::default()
        });

        manager.record_connection("acme");
        manager.record_connection("globex");
        manager.record_connection("initech");
        manager.record_disconnection("initech");

        let active = manager.list_active_tenants();
        assert_eq!(active.len(), 2);

        let tenant_ids: Vec<_> = active.iter().map(|t| t.tenant_id.as_str()).collect();
        assert!(tenant_ids.contains(&"acme"));
        assert!(tenant_ids.contains(&"globex"));
    }
}
