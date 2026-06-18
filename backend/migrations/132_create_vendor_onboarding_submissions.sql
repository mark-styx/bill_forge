-- 098: Vendor self-service onboarding submissions (refs #312)
--
-- Stores vendor-portal onboarding submissions with W-9/W-8BEN + banking +
-- remit-to contacts. Submissions land in a review queue with field-level
-- confidence scores and a diff against any existing vendor record.

-- ---------------------------------------------------------------------------
-- 1. vendor_onboarding_submissions table
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS vendor_onboarding_submissions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    vendor_id UUID NULL REFERENCES vendors(id) ON DELETE SET NULL,
    portal_token_jti TEXT,

    -- Submitted fields
    submitted_legal_name TEXT NOT NULL,
    submitted_dba TEXT,
    submitted_address JSONB,
    submitted_tax_form_type TEXT NOT NULL CHECK (submitted_tax_form_type IN ('w9', 'w8ben')),
    submitted_tax_document_id UUID NULL REFERENCES vendor_documents(id) ON DELETE SET NULL,
    submitted_banking JSONB,
    submitted_remit_contacts JSONB,

    -- Confidence & diff
    field_confidence JSONB NOT NULL DEFAULT '{}'::jsonb,
    diff JSONB NOT NULL DEFAULT '{}'::jsonb,

    -- Review workflow
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'approved', 'rejected')),
    reviewed_by UUID NULL REFERENCES users(id),
    reviewed_at TIMESTAMPTZ NULL,
    review_notes TEXT NULL,

    -- Timestamps
    submitted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ---------------------------------------------------------------------------
-- 2. Add portal token column to vendor_documents for vendor-portal uploads
-- ---------------------------------------------------------------------------
ALTER TABLE vendor_documents ADD COLUMN IF NOT EXISTS uploaded_by_portal_token TEXT NULL;

-- ---------------------------------------------------------------------------
-- 3. Indexes
-- ---------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_onboarding_sub_tenant_status_submitted
    ON vendor_onboarding_submissions(tenant_id, status, submitted_at DESC);
CREATE INDEX IF NOT EXISTS idx_onboarding_sub_tenant_vendor
    ON vendor_onboarding_submissions(tenant_id, vendor_id);

-- ---------------------------------------------------------------------------
-- 4. RLS (follows pattern from migration 080)
-- ---------------------------------------------------------------------------
ALTER TABLE vendor_onboarding_submissions ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS rls_tenant_vendor_onboarding_submissions ON vendor_onboarding_submissions;
CREATE POLICY rls_tenant_vendor_onboarding_submissions ON vendor_onboarding_submissions
    USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

COMMENT ON TABLE vendor_onboarding_submissions IS 'Vendor self-service onboarding submissions captured via tokenized portal link, with field-level confidence scoring and diff against existing records';
