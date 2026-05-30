-- 096: Month-end close automation tables
-- close_periods: defines fiscal periods with cutoff dates and lock state
-- close_accrual_entries: accrual journal entries generated during period close

CREATE TABLE IF NOT EXISTS close_periods (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    period_label TEXT NOT NULL,              -- e.g. '2026-05'
    period_start DATE NOT NULL,
    period_end DATE NOT NULL,
    cutoff_date DATE NOT NULL,
    status TEXT NOT NULL DEFAULT 'open'
        CHECK (status IN ('open', 'cutoff_passed', 'locked')),
    locked_at TIMESTAMPTZ,
    locked_by_user_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE(tenant_id, period_label)
);

CREATE INDEX IF NOT EXISTS idx_close_periods_tenant ON close_periods(tenant_id);
CREATE INDEX IF NOT EXISTS idx_close_periods_status ON close_periods(tenant_id, status);

CREATE TABLE IF NOT EXISTS close_accrual_entries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    close_period_id UUID NOT NULL REFERENCES close_periods(id) ON DELETE CASCADE,
    invoice_id UUID,                         -- nullable for future non-invoice accruals
    vendor_id UUID,
    gl_account TEXT NOT NULL DEFAULT '2100 - Accrued Expenses',
    amount_cents BIGINT NOT NULL,
    description TEXT,
    source TEXT NOT NULL DEFAULT 'unapproved_invoice'
        CHECK (source IN ('unapproved_invoice')),
    erp_journal_id TEXT,                     -- QBO journal entry id after posting
    erp_post_status TEXT NOT NULL DEFAULT 'pending'
        CHECK (erp_post_status IN ('pending', 'posted', 'failed')),
    erp_post_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_close_accrual_period ON close_accrual_entries(close_period_id);
CREATE INDEX IF NOT EXISTS idx_close_accrual_invoice ON close_accrual_entries(invoice_id);
CREATE INDEX IF NOT EXISTS idx_close_accrual_post_status ON close_accrual_entries(erp_post_status);

-- Track which period an invoice was posted to (used for lock enforcement)
ALTER TABLE invoices ADD COLUMN IF NOT EXISTS posted_to_period_id UUID REFERENCES close_periods(id);

COMMENT ON TABLE close_periods IS 'Fiscal period definitions with cutoff dates and lock state for month-end close';
COMMENT ON TABLE close_accrual_entries IS 'Accrual journal entries auto-generated during period close from unapproved invoices';
