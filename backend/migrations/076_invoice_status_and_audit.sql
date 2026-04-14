-- 076: Add invoice status column and tenant-scoped audit log for state machine transitions.
--
-- The `status` column is the canonical state machine field.  It defaults to 'received'
-- which is the entry state for newly-captured invoices.
-- The `invoice_audit_log` table records every status transition with full tenant isolation.

-- Add status column to invoices (if not already present)
ALTER TABLE invoices ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'received';

-- Index for status-based queries
CREATE INDEX IF NOT EXISTS idx_invoices_status ON invoices(tenant_id, status);

-- Tenant-scoped audit log for invoice state machine transitions
CREATE TABLE IF NOT EXISTS invoice_audit_log (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL,
    invoice_id  UUID NOT NULL,
    actor_id    UUID,                    -- NULL for system-initiated transitions
    from_status TEXT,                    -- NULL for initial creation
    to_status   TEXT NOT NULL,
    event_type  TEXT NOT NULL,
    metadata    JSONB NOT NULL DEFAULT '{}',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Composite index for the most common query pattern:
-- "get all audit entries for an invoice, newest first, scoped to tenant"
CREATE INDEX IF NOT EXISTS idx_invoice_audit_log_tenant_invoice
    ON invoice_audit_log (tenant_id, invoice_id, created_at DESC);

-- RLS: tenants can only read their own audit rows.
-- (RLS is enforced at the application level via tenant-scoped queries;
--  this policy provides defense-in-depth if RLS is enabled on the table.)
ALTER TABLE invoice_audit_log ENABLE ROW LEVEL SECURITY;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_policies WHERE policyname = 'invoice_audit_log_tenant_isolation'
    ) THEN
        CREATE POLICY invoice_audit_log_tenant_isolation ON invoice_audit_log
            FOR ALL
            USING (tenant_id = current_setting('app.current_tenant_id')::uuid);
    END IF;
END $$;
