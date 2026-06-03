-- 118: vendor_domain_first_seen table for deterministic domain-age fraud check.
--
-- Tracks the first time a domain appeared in a tenant's vendor records.
-- Used by the fraud guard to detect newly-seen domains (< 30 days = high risk).

CREATE TABLE IF NOT EXISTS vendor_domain_first_seen (
    tenant_id UUID NOT NULL,
    domain   TEXT NOT NULL,
    first_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, domain)
);

COMMENT ON TABLE vendor_domain_first_seen IS 'Tracks when each domain was first seen in a tenant''s vendor records, powering the domain-age fraud signal.';
