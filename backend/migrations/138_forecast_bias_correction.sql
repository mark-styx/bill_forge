-- Migration 138: Per-tenant forecast level bias correction (refs #398)
--
-- Closes the gap that signed bias from `forecast_accuracy_log` only widened
-- confidence bands and loosened seasonality detection, never feeding back into
-- the projected level. Adds a learned multiplicative scalar applied to
-- ArimaForecaster::forecast()'s predicted_value so the closed loop actually
-- shifts forecast output (effective intercept), not just its interval.

ALTER TABLE forecast_model_tuning
    ADD COLUMN IF NOT EXISTS level_bias_correction DECIMAL(6, 4);

COMMENT ON COLUMN forecast_model_tuning.level_bias_correction IS
    'Per-tenant multiplicative correction applied to ArimaForecaster predicted_value. Learned from signed bias in forecast_accuracy_log; negative value shrinks systematically-overshooting forecasts.';
