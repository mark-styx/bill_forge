-- Early-payment discount optimizer: add discount tracking columns to invoices
-- and tenant-level cost-of-capital setting.

-- Discount columns on invoices (populated at invoice creation from vendor payment_terms)
ALTER TABLE invoices ADD COLUMN IF NOT EXISTS discount_percent NUMERIC(5,2);
ALTER TABLE invoices ADD COLUMN IF NOT EXISTS discount_days INT;
ALTER TABLE invoices ADD COLUMN IF NOT EXISTS discount_deadline DATE;
ALTER TABLE invoices ADD COLUMN IF NOT EXISTS discount_captured_at TIMESTAMPTZ;
ALTER TABLE invoices ADD COLUMN IF NOT EXISTS discount_missed_at TIMESTAMPTZ;

-- Tenant-level settings for discount optimizer
CREATE TABLE IF NOT EXISTS tenant_discount_settings (
    tenant_id UUID PRIMARY KEY,
    cost_of_capital_bps INT NOT NULL DEFAULT 800,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Partial index for the discount worklist query: invoices with a discount
-- that have not been captured or missed yet.
CREATE INDEX IF NOT EXISTS idx_invoices_discount_worklist
    ON invoices(tenant_id, discount_deadline)
    WHERE discount_captured_at IS NULL
      AND discount_missed_at IS NULL
      AND discount_percent IS NOT NULL;
