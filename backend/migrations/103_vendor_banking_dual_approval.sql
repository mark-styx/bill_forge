-- 098: Dual-approval + screening for vendor banking changes (refs #266)
--
-- Extends vendor_banking_verifications with first/second approver columns,
-- screening results JSONB, and a DB-level CHECK enforcing two distinct approvers.

-- ---------------------------------------------------------------------------
-- 1. Dual-approval columns
-- ---------------------------------------------------------------------------
ALTER TABLE vendor_banking_verifications ADD COLUMN IF NOT EXISTS first_approver_id UUID NULL;
ALTER TABLE vendor_banking_verifications ADD COLUMN IF NOT EXISTS first_approved_at TIMESTAMPTZ NULL;
ALTER TABLE vendor_banking_verifications ADD COLUMN IF NOT EXISTS second_approver_id UUID NULL;
ALTER TABLE vendor_banking_verifications ADD COLUMN IF NOT EXISTS second_approved_at TIMESTAMPTZ NULL;

-- ---------------------------------------------------------------------------
-- 2. Screening results JSONB
-- ---------------------------------------------------------------------------
ALTER TABLE vendor_banking_verifications ADD COLUMN IF NOT EXISTS screening_results JSONB NOT NULL DEFAULT '{}'::jsonb;

-- ---------------------------------------------------------------------------
-- 3. Extend status CHECK to include 'pending_second_approval'
-- ---------------------------------------------------------------------------
ALTER TABLE vendor_banking_verifications DROP CONSTRAINT IF EXISTS vendor_banking_verifications_status_check;
ALTER TABLE vendor_banking_verifications ADD CONSTRAINT vendor_banking_verifications_status_check
    CHECK (status IN ('pending', 'pending_second_approval', 'verified', 'rejected'));

-- ---------------------------------------------------------------------------
-- 4. Separation-of-duties: second approver must differ from first
-- ---------------------------------------------------------------------------
ALTER TABLE vendor_banking_verifications DROP CONSTRAINT IF EXISTS distinct_approvers_check;
ALTER TABLE vendor_banking_verifications ADD CONSTRAINT distinct_approvers_check
    CHECK (second_approver_id IS NULL OR second_approver_id <> first_approver_id);

COMMENT ON COLUMN vendor_banking_verifications.first_approver_id IS 'UUID of the first approver (screening + first sign-off)';
COMMENT ON COLUMN vendor_banking_verifications.second_approver_id IS 'UUID of the second approver (clears payment_hold)';
COMMENT ON COLUMN vendor_banking_verifications.screening_results IS 'JSONB with OFAC, AVS, Plaid screening results';
