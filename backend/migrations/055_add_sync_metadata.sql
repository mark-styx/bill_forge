-- Sprint 14 Feature #8: Mobile App Backend - Offline Sync Support
-- Adds modified_at columns for delta sync and creates indexes

-- Add modified_at column to invoices
ALTER TABLE invoices ADD COLUMN IF NOT EXISTS modified_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

-- Add modified_at column to vendors
ALTER TABLE vendors ADD COLUMN IF NOT EXISTS modified_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

-- Add modified_at column to approval_requests
ALTER TABLE approval_requests ADD COLUMN IF NOT EXISTS modified_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

-- Create indexes for efficient delta sync queries
CREATE INDEX IF NOT EXISTS idx_invoices_modified ON invoices(tenant_id, modified_at DESC);
CREATE INDEX IF NOT EXISTS idx_vendors_modified ON vendors(tenant_id, modified_at DESC);
CREATE INDEX IF NOT EXISTS idx_approval_requests_modified ON approval_requests(tenant_id, modified_at DESC);

-- Create auto-update trigger function for modified_at
CREATE OR REPLACE FUNCTION update_modified_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.modified_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Apply triggers to tables
DROP TRIGGER IF EXISTS invoices_modified_at ON invoices;
CREATE TRIGGER invoices_modified_at
    BEFORE UPDATE ON invoices
    FOR EACH ROW
    EXECUTE FUNCTION update_modified_at();

DROP TRIGGER IF EXISTS vendors_modified_at ON vendors;
CREATE TRIGGER vendors_modified_at
    BEFORE UPDATE ON vendors
    FOR EACH ROW
    EXECUTE FUNCTION update_modified_at();

DROP TRIGGER IF EXISTS approval_requests_modified_at ON approval_requests;
CREATE TRIGGER approval_requests_modified_at
    BEFORE UPDATE ON approval_requests
    FOR EACH ROW
    EXECUTE FUNCTION update_modified_at();

-- Comments for documentation
COMMENT ON COLUMN invoices.modified_at IS 'Last modification timestamp (auto-updated) for delta sync';
COMMENT ON COLUMN vendors.modified_at IS 'Last modification timestamp (auto-updated) for delta sync';
COMMENT ON COLUMN approval_requests.modified_at IS 'Last modification timestamp (auto-updated) for delta sync';
