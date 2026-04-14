-- Vendor Management module: add routing_rules JSONB and payment_terms_days columns.
-- These columns support the approval engine's vendor-specific routing configuration.

-- Add payment_terms_days (standard Net-30 default)
ALTER TABLE vendors ADD COLUMN IF NOT EXISTS payment_terms_days INT NOT NULL DEFAULT 30;

-- Add routing_rules JSONB column (shape: {approver_email, auto_approve_threshold_cents, requires_dual_approval})
ALTER TABLE vendors ADD COLUMN IF NOT EXISTS routing_rules JSONB NOT NULL DEFAULT '{}';

-- Add CHECK constraint on status (idempotent)
DO $$ BEGIN
    ALTER TABLE vendors ADD CONSTRAINT vendors_status_check
        CHECK (status IN ('active', 'inactive', 'on_hold'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- Unique index on (tenant_id, tax_id) for vendors with a tax_id
CREATE UNIQUE INDEX IF NOT EXISTS idx_vendors_tenant_tax_id
    ON vendors(tenant_id, tax_id) WHERE tax_id IS NOT NULL;

-- Index for status-scoped tenant queries
CREATE INDEX IF NOT EXISTS idx_vendors_tenant_status
    ON vendors(tenant_id, status);
