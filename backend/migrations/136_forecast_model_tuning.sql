-- Migration 136: Per-tenant forecast model tuning (refs #367)
--
-- Closes the 'learns from outcomes' gap for the forecasting surface by giving
-- ArimaForecaster a place to persist per-tenant parameter overrides that the
-- forecast_tuning worker computes from the realized-vs-predicted rows in
-- forecast_accuracy_log (created by migration 053).
--
-- The shape mirrors the per-tenant-config convention used by
-- routing_configuration (migration 051): one row per tenant, plain UUID
-- tenant_id, an index on tenant_id, and updated_at for change tracking.

CREATE TABLE IF NOT EXISTS forecast_model_tuning (
    tenant_id UUID PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,

    -- Overrides applied by ArimaForecaster when present; NULL keeps the
    -- historical defaults (0.5 autocorrelation threshold, 1.0 CI multiplier).
    seasonality_threshold_override DECIMAL(4, 3),
    ci_width_multiplier DECIMAL(5, 3),

    -- Most recent 30-day MAPE observed for this tenant. Stored for
    -- observability/audit; ArimaForecaster does not consume it directly.
    mape_30d DECIMAL(8, 4),

    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_forecast_model_tuning_tenant
    ON forecast_model_tuning(tenant_id);

COMMENT ON TABLE forecast_model_tuning IS 'Per-tenant ArimaForecaster parameter overrides learned from forecast_accuracy_log outcomes; one row per tenant. Populated by the ForecastTuning worker job.';
