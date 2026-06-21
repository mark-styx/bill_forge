-- Migration 152: Vendor compliance tracking fields (refs #441)
--
-- Adds the columns the Vendor Risk + Compliance Watchlist (#441) needs to
-- monitor every active vendor across the three soft-hit dimensions that
-- complement the existing OFAC/sanctions hard-block path:
--   * W-9/W-8 expiry           (warn 30 days before expiry, escalate during
--                              1099 season once already expired)
--   * COI (certificate of insurance) expiry
--   * 1099-eligibility threshold ($600 YTD reporting threshold with no W-9)
--
-- All columns are nullable / default-zero so this migration is back-compat
-- with every existing vendor row. Tenant isolation continues to come from
-- the `vendors` table's existing RLS policy (no new policy needed — these
-- columns live on the already-isolated row).

-- w9_on_file + is_1099_eligible exist on the domain Vendor struct but were
-- never persisted in the schema (vendor_repo.rs hardcoded them to false).
-- Persisting them here is required by the rescan job so it can read whether
-- the vendor has a W-9 on file when evaluating the 1099 threshold soft hit.
ALTER TABLE vendors ADD COLUMN IF NOT EXISTS w9_on_file BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE vendors ADD COLUMN IF NOT EXISTS is_1099_eligible BOOLEAN NOT NULL DEFAULT FALSE;

ALTER TABLE vendors ADD COLUMN IF NOT EXISTS w9_expires_on DATE;
ALTER TABLE vendors ADD COLUMN IF NOT EXISTS w8_received_date DATE;
ALTER TABLE vendors ADD COLUMN IF NOT EXISTS w8_expires_on DATE;
ALTER TABLE vendors ADD COLUMN IF NOT EXISTS coi_received_date DATE;
ALTER TABLE vendors ADD COLUMN IF NOT EXISTS coi_expires_on DATE;
ALTER TABLE vendors ADD COLUMN IF NOT EXISTS ytd_paid_cents BIGINT NOT NULL DEFAULT 0;

-- Partial indexes used by the VendorRiskRescan job's range sweeps. Restricting
-- to rows that have an expiry date set keeps each index small on tenants where
-- the compliance feature has not been populated yet.
CREATE INDEX IF NOT EXISTS idx_vendors_w9_expires_on
    ON vendors (tenant_id, w9_expires_on)
    WHERE w9_expires_on IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_vendors_coi_expires_on
    ON vendors (tenant_id, coi_expires_on)
    WHERE coi_expires_on IS NOT NULL;

COMMENT ON COLUMN vendors.w9_expires_on IS
    'Date the on-file W-9 expires. NULL when no W-9 is on file. Used by VendorRiskRescan to emit w9_expiring/w9_expired alerts.';
COMMENT ON COLUMN vendors.w8_received_date IS
    'Date the on-file W-8 (W-8BEN / W-8BEN-E) was received.';
COMMENT ON COLUMN vendors.w8_expires_on IS
    'Date the on-file W-8 expires (IRS W-8BEN forms expire 3 calendar years after signing).';
COMMENT ON COLUMN vendors.coi_received_date IS
    'Date the latest certificate of insurance was received from the vendor.';
COMMENT ON COLUMN vendors.coi_expires_on IS
    'Date the certificate of insurance expires. NULL when no COI is on file. Used by VendorRiskRescan to emit coi_expiring/coi_expired alerts.';
COMMENT ON COLUMN vendors.ytd_paid_cents IS
    'Year-to-date paid amount in cents, used to detect crossing the $600 1099 reporting threshold without a W-9 on file.';


-- ---------------------------------------------------------------------------
-- Extend the vendor_risk_alerts.alert_type CHECK to admit the new soft-hit
-- kinds emitted by the rescan job. Critical-vs-soft is encoded in the
-- `severity` column (sanctions_hit remains the only kind that flips
-- payment_hold = true in the worker).
-- ---------------------------------------------------------------------------
ALTER TABLE vendor_risk_alerts DROP CONSTRAINT IF EXISTS vendor_risk_alerts_alert_type_check;
ALTER TABLE vendor_risk_alerts ADD CONSTRAINT vendor_risk_alerts_alert_type_check
    CHECK (alert_type IN (
        'sanctions_hit',
        'pep_hit',
        'banking_change',
        'address_drift',
        'tax_id_reverify_failed',
        'beneficial_owner_change',
        'w9_expiring',
        'w9_expired',
        'w8_expiring',
        'w8_expired',
        'coi_expiring',
        'coi_expired',
        'threshold_1099_no_w9'
    ));
