-- Correction rules derived from user feedback patterns.
-- When the ML model frequently suggests X but users pick Y,
-- this table records that mapping so future suggestions auto-correct.

CREATE TABLE IF NOT EXISTS category_correction_rules (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    tenant_id UUID NOT NULL,
    category_type VARCHAR(32) NOT NULL,  -- 'gl_code', 'department', 'cost_center'
    suggested_value TEXT NOT NULL,
    correct_value TEXT NOT NULL,
    frequency INT NOT NULL DEFAULT 1,
    active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, category_type, suggested_value, correct_value)
);

CREATE INDEX IF NOT EXISTS idx_ccr_lookup
    ON category_correction_rules(tenant_id, category_type, suggested_value)
    WHERE active = true;

-- Per-tenant confidence calibration computed from feedback history.
-- Stores the gap between confidence-when-correct vs confidence-when-wrong
-- so the ML model can adjust its raw scores.

CREATE TABLE IF NOT EXISTS categorization_confidence_calibration (
    tenant_id UUID NOT NULL PRIMARY KEY,
    avg_confidence_when_correct REAL NOT NULL DEFAULT 0.0,
    avg_confidence_when_wrong REAL NOT NULL DEFAULT 0.0,
    total_samples INT NOT NULL DEFAULT 0,
    calibration_offset REAL NOT NULL DEFAULT 0.0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
