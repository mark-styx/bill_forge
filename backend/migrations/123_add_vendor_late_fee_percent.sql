-- AP Command Center late-fee risk: add vendor late-fee percent column.
-- Percent of invoice amount used to compute late-fee risk for the AP Command Center.

ALTER TABLE vendors ADD COLUMN IF NOT EXISTS late_fee_percent NUMERIC(5,2) NOT NULL DEFAULT 0.0;

ALTER TABLE vendors ADD CONSTRAINT vendors_late_fee_percent_range
    CHECK (late_fee_percent >= 0 AND late_fee_percent <= 100);

COMMENT ON COLUMN vendors.late_fee_percent IS 'Percent applied to invoice amount to compute late-fee risk for the AP Command Center.';
