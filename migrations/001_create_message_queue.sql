-- Message queue table for offline message storage
CREATE TABLE IF NOT EXISTS message_queue (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id VARCHAR(255) NOT NULL DEFAULT 'default',
    user_id VARCHAR(255) NOT NULL,
    event_data JSONB NOT NULL,
    queued_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    attempts INTEGER NOT NULL DEFAULT 0,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for efficient user message lookup
CREATE INDEX IF NOT EXISTS idx_message_queue_user
    ON message_queue(tenant_id, user_id);

-- Index for expired message cleanup
CREATE INDEX IF NOT EXISTS idx_message_queue_expires
    ON message_queue(expires_at);

-- Index for ordering messages by queue time
CREATE INDEX IF NOT EXISTS idx_message_queue_queued_at
    ON message_queue(tenant_id, user_id, queued_at);
