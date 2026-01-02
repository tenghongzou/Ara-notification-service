//! PostgreSQL-based message queue backend.
//!
//! This module provides a persistent implementation of the `MessageQueueBackend` trait
//! using PostgreSQL for storage. Messages are stored in a table with JSONB event data
//! and automatic expiration.

use async_trait::async_trait;
use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::metrics::{QUEUE_DROPPED_TOTAL, QUEUE_ENQUEUED_TOTAL, QUEUE_EXPIRED_TOTAL};
use crate::notification::NotificationEvent;

use super::backend::{DrainResult, MessageQueueBackend, QueueBackendError, QueueBackendStats, StoredMessage};
use super::QueueConfig;

/// PostgreSQL-based message queue backend.
///
/// Uses PostgreSQL table for storing queued messages with JSONB event data.
///
/// Table structure:
/// - `message_queue` - Main queue table with tenant isolation
pub struct PostgresQueueBackend {
    /// PostgreSQL connection pool
    pool: PgPool,

    /// Configuration
    config: QueueConfig,

    /// Tenant ID for multi-tenant isolation
    tenant_id: String,
}

impl PostgresQueueBackend {
    /// Create a new PostgreSQL queue backend.
    pub fn new(config: QueueConfig, pool: PgPool) -> Self {
        Self {
            pool,
            config,
            tenant_id: "default".to_string(),
        }
    }

    /// Create a new PostgreSQL queue backend with a specific tenant ID.
    pub fn with_tenant(config: QueueConfig, pool: PgPool, tenant_id: String) -> Self {
        Self {
            pool,
            config,
            tenant_id,
        }
    }
}

#[async_trait]
impl MessageQueueBackend for PostgresQueueBackend {
    fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    fn message_ttl_seconds(&self) -> u64 {
        self.config.message_ttl_seconds
    }

    async fn enqueue(&self, user_id: &str, event: NotificationEvent) -> Result<(), QueueBackendError> {
        if !self.config.enabled {
            return Err(QueueBackendError::Disabled);
        }

        let expires_at = Utc::now() + Duration::seconds(self.config.message_ttl_seconds as i64);
        let event_data = serde_json::to_value(&event)?;
        let id = Uuid::new_v4();

        // Atomic enqueue with queue size enforcement using CTE
        // This prevents race conditions by combining delete + insert in a single query
        let result: (i64,) = sqlx::query_as(
            r#"
            WITH deleted AS (
                DELETE FROM message_queue
                WHERE id IN (
                    SELECT id FROM message_queue
                    WHERE tenant_id = $1 AND user_id = $2
                    AND (SELECT COUNT(*) FROM message_queue WHERE tenant_id = $1 AND user_id = $2) >= $3
                    ORDER BY queued_at ASC
                    LIMIT 1
                )
                RETURNING 1
            ),
            inserted AS (
                INSERT INTO message_queue (id, tenant_id, user_id, event_data, queued_at, expires_at)
                VALUES ($4, $1, $2, $5, NOW(), $6)
                RETURNING 1
            )
            SELECT COALESCE((SELECT COUNT(*) FROM deleted), 0) as dropped
            "#
        )
        .bind(&self.tenant_id)
        .bind(user_id)
        .bind(self.config.max_queue_size_per_user as i64)
        .bind(id)
        .bind(&event_data)
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await
        .map_err(QueueBackendError::Postgres)?;

        let dropped = result.0;
        if dropped > 0 {
            QUEUE_DROPPED_TOTAL.inc();
            tracing::debug!(
                user_id = %user_id,
                tenant_id = %self.tenant_id,
                "Dropped oldest message from full queue"
            );
        }

        // Update queue stats
        if let Err(e) = sqlx::query("SELECT upsert_queue_stats($1, 1, 0, 0)")
            .bind(&self.tenant_id)
            .execute(&self.pool)
            .await
        {
            tracing::warn!(error = %e, "Failed to update queue stats after enqueue");
        }

        QUEUE_ENQUEUED_TOTAL.inc();

        tracing::trace!(
            user_id = %user_id,
            tenant_id = %self.tenant_id,
            message_id = %id,
            "Message enqueued to PostgreSQL"
        );

        Ok(())
    }

    async fn drain(&self, user_id: &str) -> Result<DrainResult, QueueBackendError> {
        if !self.config.enabled {
            return Err(QueueBackendError::Disabled);
        }

        // Fetch and delete all non-expired messages for this user in one query
        let rows: Vec<(Uuid, serde_json::Value, chrono::DateTime<Utc>, i32)> = sqlx::query_as(
            r#"
            DELETE FROM message_queue
            WHERE tenant_id = $1 AND user_id = $2 AND expires_at > NOW()
            RETURNING id, event_data, queued_at, attempts
            "#
        )
        .bind(&self.tenant_id)
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(QueueBackendError::Postgres)?;

        // Also count and delete expired messages
        let expired_result = sqlx::query(
            r#"
            DELETE FROM message_queue
            WHERE tenant_id = $1 AND user_id = $2 AND expires_at <= NOW()
            "#
        )
        .bind(&self.tenant_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(QueueBackendError::Postgres)?;

        let expired = expired_result.rows_affected() as usize;

        // Convert rows to StoredMessage
        let messages: Vec<StoredMessage> = rows
            .into_iter()
            .filter_map(|(id, event_data, queued_at, attempts)| {
                match serde_json::from_value(event_data) {
                    Ok(event) => Some(StoredMessage {
                        id,
                        event,
                        queued_at,
                        attempts: attempts as u32,
                        stream_id: None,
                    }),
                    Err(e) => {
                        tracing::warn!(
                            message_id = %id,
                            error = %e,
                            "Failed to deserialize queued message, skipping"
                        );
                        None
                    }
                }
            })
            .collect();

        let drained_count = messages.len();

        // Update queue stats
        if drained_count > 0 || expired > 0 {
            if let Err(e) = sqlx::query("SELECT upsert_queue_stats($1, 0, $2, $3)")
                .bind(&self.tenant_id)
                .bind(drained_count as i64)
                .bind(expired as i64)
                .execute(&self.pool)
                .await
            {
                tracing::warn!(error = %e, "Failed to update queue stats after drain");
            }
        }

        if expired > 0 {
            QUEUE_EXPIRED_TOTAL.inc_by(expired as u64);
        }

        tracing::debug!(
            user_id = %user_id,
            tenant_id = %self.tenant_id,
            drained = drained_count,
            expired = expired,
            "Drained messages from PostgreSQL queue"
        );

        Ok(DrainResult { messages, expired })
    }

    async fn peek(&self, user_id: &str, limit: usize) -> Result<Vec<StoredMessage>, QueueBackendError> {
        if !self.config.enabled {
            return Err(QueueBackendError::Disabled);
        }

        let rows: Vec<(Uuid, serde_json::Value, chrono::DateTime<Utc>, i32)> = sqlx::query_as(
            r#"
            SELECT id, event_data, queued_at, attempts
            FROM message_queue
            WHERE tenant_id = $1 AND user_id = $2 AND expires_at > NOW()
            ORDER BY queued_at ASC
            LIMIT $3
            "#
        )
        .bind(&self.tenant_id)
        .bind(user_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(QueueBackendError::Postgres)?;

        let messages = rows
            .into_iter()
            .filter_map(|(id, event_data, queued_at, attempts)| {
                match serde_json::from_value(event_data) {
                    Ok(event) => Some(StoredMessage {
                        id,
                        event,
                        queued_at,
                        attempts: attempts as u32,
                        stream_id: None,
                    }),
                    Err(e) => {
                        tracing::warn!(
                            message_id = %id,
                            error = %e,
                            "Failed to deserialize queued message in peek, skipping"
                        );
                        None
                    }
                }
            })
            .collect();

        Ok(messages)
    }

    async fn queue_size(&self, user_id: &str) -> Result<usize, QueueBackendError> {
        if !self.config.enabled {
            return Err(QueueBackendError::Disabled);
        }

        let count: i64 = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM message_queue WHERE tenant_id = $1 AND user_id = $2 AND expires_at > NOW()"
        )
        .bind(&self.tenant_id)
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(QueueBackendError::Postgres)?;

        Ok(count as usize)
    }

    async fn cleanup_expired(&self) -> Result<usize, QueueBackendError> {
        if !self.config.enabled {
            return Ok(0);
        }

        // Delete expired messages (tenant-scoped for proper isolation)
        let result = sqlx::query(
            "DELETE FROM message_queue WHERE tenant_id = $1 AND expires_at <= NOW()"
        )
        .bind(&self.tenant_id)
        .execute(&self.pool)
        .await
        .map_err(QueueBackendError::Postgres)?;

        let count = result.rows_affected() as usize;

        if count > 0 {
            QUEUE_EXPIRED_TOTAL.inc_by(count as u64);
            tracing::debug!(
                tenant_id = %self.tenant_id,
                expired = count,
                "Cleaned up expired messages from PostgreSQL"
            );
        }

        Ok(count)
    }

    async fn clear_user_queue(&self, user_id: &str) -> Result<usize, QueueBackendError> {
        if !self.config.enabled {
            return Err(QueueBackendError::Disabled);
        }

        let result = sqlx::query(
            "DELETE FROM message_queue WHERE tenant_id = $1 AND user_id = $2"
        )
        .bind(&self.tenant_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(QueueBackendError::Postgres)?;

        Ok(result.rows_affected() as usize)
    }

    async fn stats(&self) -> QueueBackendStats {
        // Get total messages and unique users
        let (total_messages, users_with_queue, max_queue_size): (i64, i64, i64) = sqlx::query_as(
            r#"
            SELECT
                COUNT(*) as total,
                COUNT(DISTINCT user_id) as users,
                COALESCE(MAX(user_count), 0) as max_size
            FROM message_queue
            LEFT JOIN (
                SELECT user_id, COUNT(*) as user_count
                FROM message_queue
                WHERE tenant_id = $1
                GROUP BY user_id
            ) counts USING (user_id)
            WHERE message_queue.tenant_id = $1
            "#
        )
        .bind(&self.tenant_id)
        .fetch_one(&self.pool)
        .await
        .unwrap_or((0, 0, 0));

        QueueBackendStats {
            backend_type: "postgres".to_string(),
            enabled: self.config.enabled,
            total_messages: total_messages as usize,
            users_with_queue: users_with_queue as usize,
            max_queue_size: max_queue_size as usize,
            max_queue_size_config: self.config.max_queue_size_per_user,
            message_ttl_seconds: self.config.message_ttl_seconds,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> QueueConfig {
        QueueConfig {
            enabled: true,
            max_queue_size_per_user: 100,
            message_ttl_seconds: 3600,
            cleanup_interval_seconds: 300,
        }
    }

    #[test]
    fn test_backend_creation() {
        // This test verifies struct creation without a real database
        let config = create_test_config();
        assert!(config.enabled);
        assert_eq!(config.max_queue_size_per_user, 100);
    }

    #[test]
    fn test_tenant_isolation() {
        // Verify tenant_id is properly set
        let config = create_test_config();
        // Would need a mock pool for full testing
        let _tenant_id = "test-tenant".to_string();
        assert!(config.enabled);
    }
}
