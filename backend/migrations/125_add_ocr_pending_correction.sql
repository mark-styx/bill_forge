-- Stores per-field raw confidence bucket at extraction time so the
-- correction handler can debit the right bucket later.
CREATE TABLE IF NOT EXISTS ocr_pending_correction (
    id           BIGSERIAL    PRIMARY KEY,
    tenant_id    UUID         NOT NULL,
    provider     TEXT         NOT NULL,
    invoice_id   UUID         NOT NULL,
    field_name   TEXT         NOT NULL,
    bucket       SMALLINT     NOT NULL CHECK (bucket >= 0 AND bucket <= 9),
    created_at   TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ocr_pending_correction_lookup
    ON ocr_pending_correction (tenant_id, provider, invoice_id, field_name);

-- Enable RLS using the same current_tenant pattern.
ALTER TABLE ocr_pending_correction ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS rls_tenant_ocr_pending_correction ON ocr_pending_correction;
CREATE POLICY rls_tenant_ocr_pending_correction ON ocr_pending_correction
    USING (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid);
