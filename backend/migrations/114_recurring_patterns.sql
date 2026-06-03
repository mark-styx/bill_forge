-- Migration: Add recurring_patterns table
-- Feature: #313 - Recurring-invoice detection and auto-approval policies
--
-- Stores detected vendor cadence patterns (rent, SaaS, utilities) derived
-- from historical approved invoices. AP managers can enable per-pattern
-- auto-approval with configurable amount tolerance and arrival window.

CREATE TABLE recurring_patterns (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    vendor_id UUID NOT NULL,

    -- Detected cadence (median interval in days: monthly=30, quarterly=90, etc.)
    cadence_days INT NOT NULL,
    -- Trailing median of total_amount_cents from the sample invoices
    trailing_median_cents BIGINT NOT NULL,
    -- Number of invoices used to compute cadence/median
    sample_count INT NOT NULL DEFAULT 0,
    -- Date of the most recent invoice in the pattern sample
    last_invoice_date DATE,
    -- SHA-256 of the normalized line-item JSON from the latest sample invoice
    last_line_items_hash TEXT,

    -- Auto-approval policy knobs (off by default)
    auto_approve_enabled BOOL NOT NULL DEFAULT false,
    -- Allowed deviation from trailing median, in percentage (e.g. 5.0 = +/-5%)
    amount_tolerance_pct NUMERIC(5,2) NOT NULL DEFAULT 5.0,
    -- Allowed deviation from expected arrival date, in days
    window_tolerance_days INT NOT NULL DEFAULT 3,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY (vendor_id) REFERENCES vendors(id) ON DELETE CASCADE
);

-- One pattern per vendor per tenant for v1
CREATE UNIQUE INDEX idx_recurring_patterns_tenant_vendor
    ON recurring_patterns(tenant_id, vendor_id);

CREATE INDEX idx_recurring_patterns_tenant_id
    ON recurring_patterns(tenant_id);

COMMENT ON TABLE recurring_patterns IS 'Auto-detected recurring vendor patterns with per-pattern auto-approval policies';
