//! PostgreSQL-based ACK tracking backend.
//!
//! This module provides a persistent implementation of the `AckTrackerBackend` trait
//! using PostgreSQL for storage. ACK tracking state survives service restarts.

use async_trait::async_trait;
use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::metrics::{ACK_EXPIRED_TOTAL, ACK_LATENCY, ACK_RECEIVED_TOTAL, ACK_TRACKED_TOTAL};
use crate::notification::ack::AckConfig;

use super::ack_backend::{AckBackendError, AckBackendStats, AckTrackerBackend, PendingAckInfo};

/// PostgreSQL-based ACK tracking backend.
///
/// Uses PostgreSQL tables for storing pending ACK info and statistics.
///
/// Table structure:
/// - `pending_acks` - Pending ACK tracking with expiration
/// - `ack_stats` - Per-tenant statistics
pub struct PostgresAckBackend {
    /// PostgreSQL connection pool
    pool: PgPool,

    /// Configuration
    config: AckConfig,

    /// Tenant ID for multi-tenant isolation
    tenant_id: String,
}

impl PostgresAckBackend {
    /// Create a new PostgreSQL ACK backend.
    pub fn new(config: AckConfig, pool: PgPool) -> Self {
        Self {
            pool,
            config,
            tenant_id: "default".to_string(),
        }
    }

    /// Create a new PostgreSQL ACK backend with a specific tenant ID.
    pub fn with_tenant(config: AckConfig, pool: PgPool, tenant_id: String) -> Self {
        Self {
            pool,
            config,
            tenant_id,
        }
    }
}

#[async_trait]
impl AckTrackerBackend for PostgresAckBackend {
    fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    fn timeout_seconds(&self) -> u64 {
        self.config.timeout_seconds
    }

    fn cleanup_interval_seconds(&self) -> u64 {
        self.config.cleanup_interval_seconds
    }

    async fn track(&self, notification_id: Uuid, user_id: &str, connection_id: Uuid) {
        if !self.config.enabled {
            return;
        }

        let expires_at = Utc::now() + Duration::seconds(self.config.timeout_seconds as i64);

        // Insert pending ACK record
        let result = sqlx::query(
            r#"
            INSERT INTO pending_acks (notification_id, tenant_id, user_id, connection_id, sent_at, expires_at)
            VALUES ($1, $2, $3, $4, NOW(), $5)
            ON CONFLICT (notification_id) DO NOTHING
            "#
        )
        .bind(notification_id)
        .bind(&self.tenant_id)
        .bind(user_id)
        .bind(connection_id)
        .bind(expires_at)
        .execute(&self.pool)
        .await;

        if let Err(e) = result {
            tracing::warn!(
                error = %e,
                notification_id = %notification_id,
                "Failed to track pending ACK in PostgreSQL"
            );
            return;
        }

        // Update stats
        let _ = sqlx::query("SELECT upsert_ack_stats($1, 1, 0, 0, 0)")
            .bind(&self.tenant_id)
            .execute(&self.pool)
            .await;

        ACK_TRACKED_TOTAL.inc();

        tracing::trace!(
            notification_id = %notification_id,
            user_id = %user_id,
            connection_id = %connection_id,
            "Tracking notification for ACK in PostgreSQL"
        );
    }

    async fn acknowledge(&self, notification_id: Uuid, user_id: &str) -> bool {
        if !self.config.enabled {
            return false;
        }

        // Get pending ACK info and delete in one query
        let pending: Option<(chrono::DateTime<Utc>, String)> = sqlx::query_as(
            r#"
            DELETE FROM pending_acks
            WHERE notification_id = $1 AND user_id = $2
            RETURNING sent_at, user_id
            "#
        )
        .bind(notification_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten();

        match pending {
            Some((sent_at, _)) => {
                // Calculate latency
                let latency_ms = Utc::now()
                    .signed_duration_since(sent_at)
                    .num_milliseconds()
                    .max(0) as u64;

                // Update stats
                let _ = sqlx::query("SELECT upsert_ack_stats($1, 0, 1, 0, $2)")
                    .bind(&self.tenant_id)
                    .bind(latency_ms as i64)
                    .execute(&self.pool)
                    .await;

                ACK_RECEIVED_TOTAL.inc();
                ACK_LATENCY.observe(latency_ms as f64 / 1000.0);

                tracing::debug!(
                    notification_id = %notification_id,
                    user_id = %user_id,
                    latency_ms = latency_ms,
                    "Notification acknowledged (PostgreSQL)"
                );

                true
            }
            None => {
                tracing::debug!(
                    notification_id = %notification_id,
                    user_id = %user_id,
                    "ACK received for unknown notification"
                );
                false
            }
        }
    }

    async fn get_pending(&self, notification_id: Uuid) -> Result<Option<PendingAckInfo>, AckBackendError> {
        let pending: Option<(Uuid, String, Uuid, chrono::DateTime<Utc>)> = sqlx::query_as(
            r#"
            SELECT notification_id, user_id, connection_id, sent_at
            FROM pending_acks
            WHERE notification_id = $1
            "#
        )
        .bind(notification_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(AckBackendError::Postgres)?;

        Ok(pending.map(|(notification_id, user_id, connection_id, sent_at)| {
            PendingAckInfo {
                notification_id,
                user_id,
                connection_id,
                sent_at,
            }
        }))
    }

    async fn cleanup_expired(&self) -> usize {
        if !self.config.enabled {
            return 0;
        }

        // Delete expired pending ACKs and count
        let result = sqlx::query(
            "DELETE FROM pending_acks WHERE expires_at <= NOW()"
        )
        .execute(&self.pool)
        .await;

        let count = match result {
            Ok(r) => r.rows_affected() as usize,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Failed to cleanup expired ACKs from PostgreSQL"
                );
                return 0;
            }
        };

        if count > 0 {
            // Update stats
            let _ = sqlx::query("SELECT upsert_ack_stats($1, 0, 0, $2, 0)")
                .bind(&self.tenant_id)
                .bind(count as i64)
                .execute(&self.pool)
                .await;

            ACK_EXPIRED_TOTAL.inc_by(count as u64);

            tracing::debug!(
                expired = count,
                "Cleaned up expired pending ACKs from PostgreSQL"
            );
        }

        count
    }

    async fn pending_count(&self) -> usize {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pending_acks WHERE tenant_id = $1"
        )
        .bind(&self.tenant_id)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(Some(0))
        .unwrap_or(0);

        count as usize
    }

    async fn stats(&self) -> AckBackendStats {
        // Get stats from database
        let stats: Option<(i64, i64, i64, i64)> = sqlx::query_as(
            r#"
            SELECT total_tracked, total_acked, total_expired, total_latency_ms
            FROM ack_stats WHERE tenant_id = $1
            "#
        )
        .bind(&self.tenant_id)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten();

        let pending_count = self.pending_count().await as u64;

        match stats {
            Some((total_tracked, total_acked, total_expired, total_latency_ms)) => {
                AckBackendStats {
                    backend_type: "postgres".to_string(),
                    enabled: self.config.enabled,
                    total_tracked: total_tracked as u64,
                    total_acked: total_acked as u64,
                    total_expired: total_expired as u64,
                    pending_count,
                    ack_rate: AckBackendStats::calculate_ack_rate(
                        total_acked as u64,
                        total_expired as u64,
                    ),
                    avg_latency_ms: AckBackendStats::calculate_avg_latency(
                        total_latency_ms as u64,
                        total_acked as u64,
                    ),
                }
            }
            None => AckBackendStats {
                backend_type: "postgres".to_string(),
                enabled: self.config.enabled,
                total_tracked: 0,
                total_acked: 0,
                total_expired: 0,
                pending_count,
                ack_rate: 1.0,
                avg_latency_ms: 0,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> AckConfig {
        AckConfig {
            enabled: true,
            timeout_seconds: 30,
            cleanup_interval_seconds: 60,
        }
    }

    #[test]
    fn test_config() {
        let config = create_test_config();
        assert!(config.enabled);
        assert_eq!(config.timeout_seconds, 30);
        assert_eq!(config.cleanup_interval_seconds, 60);
    }

    #[test]
    fn test_backend_type() {
        // Would need a mock pool for full testing
        let config = create_test_config();
        assert!(config.enabled);
    }
}
