-- Migration 091: Approval SLA tracking
--
-- Adds persisted SLA metadata to approval requests so reporting and workers can
-- reason about near-breach/breached items without recomputing policy every time.

ALTER TABLE approval_requests
    ADD COLUMN IF NOT EXISTS sla_hours INTEGER NOT NULL DEFAULT 24,
    ADD COLUMN IF NOT EXISTS sla_started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN IF NOT EXISTS near_breach_notified_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS breached_notified_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS escalated_at TIMESTAMPTZ;

UPDATE approval_requests
SET
    sla_started_at = created_at,
    sla_hours = CASE
        WHEN expires_at IS NOT NULL THEN GREATEST(1, CEIL(EXTRACT(EPOCH FROM (expires_at - created_at)) / 3600.0)::INTEGER)
        ELSE COALESCE(sla_hours, 24)
    END
WHERE created_at IS NOT NULL
  AND (sla_hours = 24 OR expires_at IS NOT NULL);

CREATE INDEX IF NOT EXISTS idx_approval_requests_sla_pending
    ON approval_requests(tenant_id, status, sla_started_at, sla_hours)
    WHERE status = 'pending';

CREATE INDEX IF NOT EXISTS idx_approval_requests_sla_notifications
    ON approval_requests(tenant_id, status, near_breach_notified_at, breached_notified_at)
    WHERE status = 'pending';
