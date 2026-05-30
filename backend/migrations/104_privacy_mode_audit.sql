-- 104: Per-document privacy_mode audit field for local-OCR privacy mode (refs #269)
--
-- Records which OCR privacy mode was in effect at processing time on both
-- the invoice_captures and invoices rows.  Nullable so historical rows are
-- unaffected; only newly-processed documents are stamped.

ALTER TABLE invoice_captures ADD COLUMN IF NOT EXISTS privacy_mode TEXT;
ALTER TABLE invoices       ADD COLUMN IF NOT EXISTS privacy_mode TEXT;

COMMENT ON COLUMN invoice_captures.privacy_mode IS 'OCR privacy mode in effect at capture time: "local_only" or "cloud_allowed". NULL for rows created before the column was added.';
COMMENT ON COLUMN invoices.privacy_mode       IS 'OCR privacy mode in effect at capture time: "local_only" or "cloud_allowed". NULL for rows created before the column was added.';
