-- Pending ACK tracking table
CREATE TABLE IF NOT EXISTS pending_acks (
    notification_id UUID PRIMARY KEY,
    tenant_id VARCHAR(255) NOT NULL DEFAULT 'default',
    user_id VARCHAR(255) NOT NULL,
    connection_id UUID NOT NULL,
    sent_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for user lookup
CREATE INDEX IF NOT EXISTS idx_pending_acks_user
    ON pending_acks(tenant_id, user_id);

-- Index for expired ACK cleanup
CREATE INDEX IF NOT EXISTS idx_pending_acks_expires
    ON pending_acks(expires_at);
