-- Persist per-(tenant, provider, field) accuracy stats for OCR confidence calibration.
-- Survives restarts (replaces in-memory-only OcrComparison metrics_store for accuracy).
CREATE TABLE IF NOT EXISTS ocr_field_calibration (
    tenant_id    UUID       NOT NULL,
    provider     TEXT       NOT NULL,
    field_name   TEXT       NOT NULL,
    extractions  BIGINT     NOT NULL DEFAULT 0,
    corrections  BIGINT     NOT NULL DEFAULT 0,
    last_updated TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, provider, field_name)
);

-- Enable RLS using the same current_tenant pattern as migration 092.
ALTER TABLE ocr_field_calibration ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS rls_tenant_ocr_field_calibration ON ocr_field_calibration;
CREATE POLICY rls_tenant_ocr_field_calibration ON ocr_field_calibration
    USING (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid);
