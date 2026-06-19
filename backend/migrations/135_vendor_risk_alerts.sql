-- Migration 135: Continuous vendor risk alert store (refs #381)
--
-- Moves vendor screening from a one-time onboarding check to continuous
-- monitoring. This table is the per-tenant severity-graded alert surface that
-- the VendorRiskRescan worker job, the real-time banking-change hook in
-- vendors.rs, and the per-tenant vendor-risk dashboard all read and write.
--
-- Mirrors the NULLIF-hardened RLS pattern from migrations 092/121/133/134 so an
-- unset/empty app.current_tenant_id denies rows instead of raising a UUID cast
-- error, matching the rest of the tenant-isolation surface (#368).

-- ---------------------------------------------------------------------------
-- 1. vendor_risk_alerts table
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS vendor_risk_alerts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    vendor_id UUID NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
    -- Discriminates the producer + payload shape. Add new variants here.
    alert_type TEXT NOT NULL
        CHECK (alert_type IN (
            'sanctions_hit',
            'pep_hit',
            'banking_change',
            'address_drift',
            'tax_id_reverify_failed',
            'beneficial_owner_change'
        )),
    severity TEXT NOT NULL
        CHECK (severity IN ('critical', 'high', 'medium', 'low')),
    -- Stable hash of the meaningful payload fields, used by producers to make
    -- repeated scans of the same finding idempotent. NULL means "always insert".
    payload_hash TEXT NULL,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    status TEXT NOT NULL DEFAULT 'open'
        CHECK (status IN ('open', 'acknowledged')),
    acknowledged_by UUID NULL,
    acknowledged_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ---------------------------------------------------------------------------
-- 2. Indexes
-- ---------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_vendor_risk_alerts_tenant_status_severity
    ON vendor_risk_alerts (tenant_id, status, severity);

CREATE INDEX IF NOT EXISTS idx_vendor_risk_alerts_tenant_vendor
    ON vendor_risk_alerts (tenant_id, vendor_id);

-- Idempotency: producers look up "is there an OPEN alert with the same
-- (vendor_id, alert_type, payload_hash)?" before inserting.
CREATE INDEX IF NOT EXISTS idx_vendor_risk_alerts_open_dedupe
    ON vendor_risk_alerts (vendor_id, alert_type, payload_hash)
    WHERE status = 'open';

-- ---------------------------------------------------------------------------
-- 3. RLS (follows pattern from migrations 092/121/133/134)
-- ---------------------------------------------------------------------------
ALTER TABLE vendor_risk_alerts ENABLE ROW LEVEL SECURITY;
ALTER TABLE vendor_risk_alerts FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS rls_tenant_vendor_risk_alerts ON vendor_risk_alerts;
CREATE POLICY rls_tenant_vendor_risk_alerts ON vendor_risk_alerts
    USING (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid);

-- ---------------------------------------------------------------------------
-- 4. last_risk_rescan_at on vendors
-- ---------------------------------------------------------------------------
ALTER TABLE vendors ADD COLUMN IF NOT EXISTS last_risk_rescan_at TIMESTAMPTZ NULL;

COMMENT ON TABLE vendor_risk_alerts IS 'Per-tenant severity-graded vendor-risk alerts (sanctions/PEP/banking-change/address/tax-id/beneficial-ownership); tenant-isolated via RLS. Open critical alerts block payment release until acknowledged.';
COMMENT ON COLUMN vendors.last_risk_rescan_at IS 'Timestamp of the last VendorRiskRescan worker pass that re-ran sanctions/PEP/beneficial-ownership screening for this vendor.';
