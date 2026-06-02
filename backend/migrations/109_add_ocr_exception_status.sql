-- 109: Add OCR exception resolution state to invoices.
--
-- Per-invoice exception resolution workflow: pending -> approved | rejected.
-- The OCR Exceptions page uses these columns to track which low-confidence
-- invoices have been reviewed and resolved directly from the queue.

ALTER TABLE invoices
    ADD COLUMN IF NOT EXISTS ocr_exception_status TEXT NOT NULL DEFAULT 'pending',
    ADD COLUMN IF NOT EXISTS ocr_exception_resolved_by UUID NULL,
    ADD COLUMN IF NOT EXISTS ocr_exception_resolved_at TIMESTAMPTZ NULL;

CREATE INDEX IF NOT EXISTS idx_invoices_ocr_exception_status
    ON invoices(tenant_id, ocr_exception_status)
    WHERE ocr_exception_status = 'pending';
