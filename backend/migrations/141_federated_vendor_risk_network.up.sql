-- Migration 141: Federated Vendor Risk Network (refs #408)
--
-- Privacy-preserving cross-tenant vendor risk signal sharing.
--
-- Mirrors the cross-tenant aggregation pattern from migration 130
-- (benchmark peer insights): contributions live in the metadata DB, keyed
-- by salted SHA-256 of the vendor identifier (normalized name + tax_id +
-- bank-account fingerprint) and by HMAC-SHA256 of the contributing
-- tenant_id. A k-anonymity floor of 5 distinct contributing tenants is
-- enforced at the query layer (see federated_vendor_risk_aggregates view).
--
-- No raw vendor names, tax IDs, bank numbers, or tenant identifiers are
-- ever stored in this table. The `why this vendor is flagged` explanation
-- surfaced to callers is templated solely from signal_type + contributor_count.

-- ---------------------------------------------------------------------------
-- 1. Opt-in consent per tenant
-- ---------------------------------------------------------------------------
-- A tenant must have an active row (opted_out_at IS NULL) before any
-- contribution is written or aggregate read. Tracked separately from
-- benchmark_opt_in so risk-network consent is a distinct, auditable choice.

CREATE TABLE IF NOT EXISTS tenant_risk_network_consent (
    tenant_id     UUID PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    opted_in_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    opted_out_at  TIMESTAMPTZ NULL
);

CREATE INDEX IF NOT EXISTS idx_tenant_risk_network_consent_active
    ON tenant_risk_network_consent (tenant_id)
    WHERE opted_out_at IS NULL;

-- ---------------------------------------------------------------------------
-- 2. Hashed/anonymized risk signal contributions
-- ---------------------------------------------------------------------------
-- One row per signal contributed by an opted-in tenant.
--
-- vendor_hash:            SHA-256(network_salt || normalized_name || tax_id || bank_fingerprint)
-- contributing_tenant_hash: HMAC-SHA256(network_salt, tenant_id)
--
-- Keeping the tenant identity HMAC-hashed (not plain UUID) allows
-- COUNT(DISTINCT contributing_tenant_hash) for k-anonymity enforcement
-- without storing or exposing which tenants contributed.

CREATE TABLE IF NOT EXISTS federated_vendor_signals (
    id                        BIGSERIAL PRIMARY KEY,
    vendor_hash               TEXT NOT NULL,
    signal_type               TEXT NOT NULL
        CHECK (signal_type IN (
            'bank_account_change',
            'ofac_near_match',
            'fake_invoice_pattern',
            'dispute_rate_high'
        )),
    contributing_tenant_hash  TEXT NOT NULL,
    contributed_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    signal_weight             REAL NOT NULL DEFAULT 1.0
);

CREATE INDEX IF NOT EXISTS idx_federated_vendor_signals_vendor_type
    ON federated_vendor_signals (vendor_hash, signal_type);

CREATE INDEX IF NOT EXISTS idx_federated_vendor_signals_contributor
    ON federated_vendor_signals (contributing_tenant_hash, contributed_at);

-- ---------------------------------------------------------------------------
-- 3. Aggregates view (read surface)
-- ---------------------------------------------------------------------------
-- Groups by (vendor_hash, signal_type) and exposes signal_count,
-- contributor_count (= DISTINCT contributing_tenant_hash), and a summed
-- weighted_score. The k-anonymity floor is enforced by the query layer
-- (aggregate_for_vendor in vendor-mgmt) which filters contributor_count >= 5.

CREATE OR REPLACE VIEW federated_vendor_risk_aggregates AS
SELECT
    vendor_hash,
    signal_type,
    COUNT(*)::BIGINT                              AS signal_count,
    COUNT(DISTINCT contributing_tenant_hash)::BIGINT AS contributor_count,
    SUM(signal_weight)::DOUBLE PRECISION          AS weighted_score
FROM federated_vendor_signals
WHERE contributed_at >= NOW() - INTERVAL '90 days'
GROUP BY vendor_hash, signal_type;

COMMENT ON TABLE tenant_risk_network_consent IS
    'Opt-in consent for the Federated Vendor Risk Network (#408). A row with opted_out_at IS NULL grants permission to contribute hashed vendor signals and read k-anonymized aggregates.';
COMMENT ON TABLE federated_vendor_signals IS
    'Privacy-preserving cross-tenant vendor risk signal contributions. vendor_hash is SHA-256 over a salted canonical tuple; contributing_tenant_hash is HMAC-SHA256(network_salt, tenant_id). No raw tenant or vendor identifiers are stored.';
COMMENT ON VIEW federated_vendor_risk_aggregates IS
    'Aggregated network-wide vendor risk signals over a 90-day window. Callers must filter contributor_count >= 5 (k-anonymity floor) before exposing rows.';
