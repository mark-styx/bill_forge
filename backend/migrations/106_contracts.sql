-- Migration: Add contracts and contract_matches tables
-- Feature: #271 - Contract-aware matching for recurring non-PO spend

-- Stores vendor contracts with fixed monthly amounts, escalators, and terms.
-- Incoming non-PO invoices match against these contracts.
CREATE TABLE contracts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    vendor_id UUID NOT NULL,
    contract_number TEXT,
    description TEXT,
    monthly_amount NUMERIC(14,2) NOT NULL,
    currency TEXT NOT NULL DEFAULT 'USD',
    escalator_pct NUMERIC(8,4) NOT NULL DEFAULT 0,       -- annual % escalation (e.g. 3.0 = 3%)
    escalator_anniversary_month SMALLINT,                  -- 1-12, NULL = use start_date month
    start_date DATE NOT NULL,
    end_date DATE NOT NULL,
    tolerance_pct NUMERIC(8,4) NOT NULL DEFAULT 2.0,     -- 2% default band
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'expired', 'cancelled')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX idx_contracts_tenant_vendor_status
    ON contracts(tenant_id, vendor_id, status);

-- Records the result of matching an invoice against a contract.
CREATE TABLE contract_matches (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    invoice_id UUID NOT NULL UNIQUE,
    contract_id UUID NOT NULL,
    expected_amount NUMERIC(14,2),
    actual_amount NUMERIC(14,2),
    variance_pct NUMERIC(8,4),
    match_result TEXT NOT NULL CHECK (match_result IN ('in_band', 'out_of_band', 'expired', 'no_match')),
    matched_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    FOREIGN KEY (contract_id) REFERENCES contracts(id) ON DELETE CASCADE
);

CREATE INDEX idx_contract_matches_contract ON contract_matches(contract_id);

COMMENT ON TABLE contracts IS 'Vendor contracts with fixed monthly amounts and escalation terms for recurring non-PO spend';
COMMENT ON TABLE contract_matches IS 'Records invoice-to-contract matching results for audit and exception handling';
