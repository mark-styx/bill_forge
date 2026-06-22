-- Migration 155: GL coding assistant line review state (refs #436)
--
-- Per-line categorization rows already store the suggested GL coding and
-- confidence. These flags make the delivery contract explicit: high-confidence
-- lines can be auto-filled, while ambiguous/default/split lines are queued for
-- coder review.

ALTER TABLE invoice_line_categorizations
    ADD COLUMN IF NOT EXISTS auto_fill BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS review_required BOOLEAN NOT NULL DEFAULT true,
    ADD COLUMN IF NOT EXISTS review_reason TEXT,
    ADD COLUMN IF NOT EXISTS applied_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_invoice_line_cat_tenant_review
    ON invoice_line_categorizations(tenant_id, review_required, created_at DESC);
