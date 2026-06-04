-- Bucket-level OCR calibration: per-(tenant, provider, field, confidence-bucket) stats.
-- Maps raw confidence ranges to observed correctness rates for true calibration.
CREATE TABLE IF NOT EXISTS ocr_field_calibration_bucket (
    tenant_id    UUID       NOT NULL,
    provider     TEXT       NOT NULL,
    field_name   TEXT       NOT NULL,
    bucket       SMALLINT   NOT NULL CHECK (bucket >= 0 AND bucket <= 9),
    extractions  BIGINT     NOT NULL DEFAULT 0,
    corrections  BIGINT     NOT NULL DEFAULT 0,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, provider, field_name, bucket)
);

-- Enable RLS using the same current_tenant pattern as migration 102.
ALTER TABLE ocr_field_calibration_bucket ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS rls_tenant_ocr_field_calibration_bucket ON ocr_field_calibration_bucket;
CREATE POLICY rls_tenant_ocr_field_calibration_bucket ON ocr_field_calibration_bucket
    USING (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::uuid);
