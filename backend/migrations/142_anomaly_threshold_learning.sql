-- Migration 142: Per-tenant anomaly/duplicate threshold learning (refs #397)
--
-- Closes the 'self-optimizing' gap for the anomaly/duplicate detectors. The
-- detectors previously hardcoded zscore/iqr/amount/date thresholds and the
-- acknowledged flag only acted as workflow state. This migration adds:
--   - duplicate-detector thresholds to anomaly_rules so they can be persisted
--     per tenant alongside the existing zscore/iqr/volume thresholds.
--   - a false_positive flag on invoice_anomalies so an acknowledgement can be
--     distinguished from a "true positive, I saw it" acknowledgement.
--   - a threshold_calibration_history table so each recalibration of a per-
--     tenant threshold is auditable.

ALTER TABLE anomaly_rules
    ADD COLUMN IF NOT EXISTS amount_tolerance DECIMAL(5, 4) DEFAULT 0.02,
    ADD COLUMN IF NOT EXISTS date_tolerance_days INTEGER DEFAULT 14;

ALTER TABLE invoice_anomalies
    ADD COLUMN IF NOT EXISTS false_positive BOOLEAN NOT NULL DEFAULT FALSE;

CREATE INDEX IF NOT EXISTS idx_invoice_anomalies_false_positive
    ON invoice_anomalies(tenant_id, false_positive)
    WHERE false_positive = TRUE;

CREATE TABLE IF NOT EXISTS threshold_calibration_history (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,

    -- Which detector threshold was moved. One of:
    --   'zscore', 'iqr', 'amount_tolerance', 'date_tolerance_days'.
    detector_type TEXT NOT NULL,

    old_value DECIMAL(8, 4),
    new_value DECIMAL(8, 4) NOT NULL,

    -- Observed false-positive rate (0.0-1.0) over the calibration window.
    fp_rate DECIMAL(5, 4),
    -- Number of anomaly rows the rate was computed over.
    sample_size INTEGER NOT NULL DEFAULT 0,

    recalibrated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_threshold_calibration_history_tenant
    ON threshold_calibration_history(tenant_id, recalibrated_at DESC);

COMMENT ON TABLE threshold_calibration_history IS 'Audit log of anomaly/duplicate detector threshold adjustments driven by acknowledged false-positive feedback (issue #397).';
COMMENT ON COLUMN invoice_anomalies.false_positive IS 'Set TRUE when an acknowledgement explicitly marks the anomaly as a false positive; consumed by the threshold-recalibration loop.';
