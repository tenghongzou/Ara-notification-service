-- ACK statistics table
CREATE TABLE IF NOT EXISTS ack_stats (
    tenant_id VARCHAR(255) PRIMARY KEY,
    total_tracked BIGINT NOT NULL DEFAULT 0,
    total_acked BIGINT NOT NULL DEFAULT 0,
    total_expired BIGINT NOT NULL DEFAULT 0,
    total_latency_ms BIGINT NOT NULL DEFAULT 0,
    last_updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Upsert function for atomic stats updates
CREATE OR REPLACE FUNCTION upsert_ack_stats(
    p_tenant_id VARCHAR(255),
    p_tracked BIGINT DEFAULT 0,
    p_acked BIGINT DEFAULT 0,
    p_expired BIGINT DEFAULT 0,
    p_latency_ms BIGINT DEFAULT 0
) RETURNS VOID AS $$
BEGIN
    INSERT INTO ack_stats (tenant_id, total_tracked, total_acked, total_expired, total_latency_ms, last_updated_at)
    VALUES (p_tenant_id, p_tracked, p_acked, p_expired, p_latency_ms, NOW())
    ON CONFLICT (tenant_id) DO UPDATE SET
        total_tracked = ack_stats.total_tracked + EXCLUDED.total_tracked,
        total_acked = ack_stats.total_acked + EXCLUDED.total_acked,
        total_expired = ack_stats.total_expired + EXCLUDED.total_expired,
        total_latency_ms = ack_stats.total_latency_ms + EXCLUDED.total_latency_ms,
        last_updated_at = NOW();
END;
$$ LANGUAGE plpgsql;

-- Queue stats table for message queue statistics
CREATE TABLE IF NOT EXISTS queue_stats (
    tenant_id VARCHAR(255) PRIMARY KEY,
    total_enqueued BIGINT NOT NULL DEFAULT 0,
    total_drained BIGINT NOT NULL DEFAULT 0,
    total_expired BIGINT NOT NULL DEFAULT 0,
    last_updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Upsert function for queue stats
CREATE OR REPLACE FUNCTION upsert_queue_stats(
    p_tenant_id VARCHAR(255),
    p_enqueued BIGINT DEFAULT 0,
    p_drained BIGINT DEFAULT 0,
    p_expired BIGINT DEFAULT 0
) RETURNS VOID AS $$
BEGIN
    INSERT INTO queue_stats (tenant_id, total_enqueued, total_drained, total_expired, last_updated_at)
    VALUES (p_tenant_id, p_enqueued, p_drained, p_expired, NOW())
    ON CONFLICT (tenant_id) DO UPDATE SET
        total_enqueued = queue_stats.total_enqueued + EXCLUDED.total_enqueued,
        total_drained = queue_stats.total_drained + EXCLUDED.total_drained,
        total_expired = queue_stats.total_expired + EXCLUDED.total_expired,
        last_updated_at = NOW();
END;
$$ LANGUAGE plpgsql;
