-- 097: Vendor banking-change verification workflow (refs #243)
--
-- Adds encrypted banking columns to vendors, a payment_hold freeze flag,
-- and a vendor_banking_verifications table for the out-of-band callback
-- verification flow. Prevents BEC fraud by blocking payments and ERP sync
-- until a human verifies banking-detail changes.

-- ---------------------------------------------------------------------------
-- 1. Add banking columns to vendors
-- ---------------------------------------------------------------------------
ALTER TABLE vendors ADD COLUMN IF NOT EXISTS bank_name TEXT;
ALTER TABLE vendors ADD COLUMN IF NOT EXISTS bank_account_last_four VARCHAR(4);
ALTER TABLE vendors ADD COLUMN IF NOT EXISTS bank_account_encrypted TEXT;
ALTER TABLE vendors ADD COLUMN IF NOT EXISTS bank_routing_encrypted TEXT;
ALTER TABLE vendors ADD COLUMN IF NOT EXISTS bank_account_type TEXT;
ALTER TABLE vendors ADD COLUMN IF NOT EXISTS bank_account_updated_at TIMESTAMPTZ;

-- ---------------------------------------------------------------------------
-- 2. Add payment hold columns
-- ---------------------------------------------------------------------------
ALTER TABLE vendors ADD COLUMN IF NOT EXISTS payment_hold BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE vendors ADD COLUMN IF NOT EXISTS payment_hold_reason TEXT;

-- ---------------------------------------------------------------------------
-- 3. Vendor banking verifications table
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS vendor_banking_verifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    vendor_id UUID NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
    previous_account_last_four VARCHAR(4),
    new_account_last_four VARCHAR(4),
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'verified', 'rejected')),
    requested_by UUID NOT NULL,
    requested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    verified_by UUID,
    verified_at TIMESTAMPTZ,
    callback_method TEXT
        CHECK (callback_method IS NULL OR callback_method IN ('phone', 'in_person', 'known_email')),
    callback_contact TEXT,
    verifier_notes TEXT
);

-- ---------------------------------------------------------------------------
-- 4. Indexes
-- ---------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_banking_ver_tenant_vendor_status
    ON vendor_banking_verifications(tenant_id, vendor_id, status);
CREATE INDEX IF NOT EXISTS idx_banking_ver_tenant_status
    ON vendor_banking_verifications(tenant_id, status);

-- ---------------------------------------------------------------------------
-- 5. RLS (follows pattern from migration 080)
-- ---------------------------------------------------------------------------
ALTER TABLE vendor_banking_verifications ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS rls_tenant_vendor_banking_verifications ON vendor_banking_verifications;
CREATE POLICY rls_tenant_vendor_banking_verifications ON vendor_banking_verifications
    USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

COMMENT ON TABLE vendor_banking_verifications IS 'Tracks out-of-band callback verification for vendor banking-detail changes to prevent BEC fraud';
