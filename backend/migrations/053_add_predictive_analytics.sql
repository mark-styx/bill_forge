-- Predictive Analytics Tables
-- Sprint 14 Feature #2: Forecasting and Anomaly Detection

-- Table: spend_forecasts
-- Stores time-series forecasts for vendors, departments, and GL codes
CREATE TABLE spend_forecasts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,

    -- Entity being forecasted
    entity_id TEXT NOT NULL,
    entity_type TEXT NOT NULL CHECK (entity_type IN ('vendor', 'department', 'gl_code', 'tenant', 'approver')),

    -- Forecast details
    metric_name TEXT NOT NULL,
    horizon TEXT NOT NULL CHECK (horizon IN ('days_30', 'days_60', 'days_90')),
    predicted_value DECIMAL(18, 2) NOT NULL,
    confidence_lower DECIMAL(18, 2) NOT NULL,
    confidence_upper DECIMAL(18, 2) NOT NULL,
    confidence_level DECIMAL(3, 2) DEFAULT 0.95,

    -- Model metadata
    model_version TEXT NOT NULL,
    seasonality_detected BOOLEAN DEFAULT FALSE,

    -- Timestamps
    generated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    valid_until TIMESTAMPTZ NOT NULL,

    -- Audit
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Unique constraint: one forecast per entity/metric/horizon combo
    UNIQUE(tenant_id, entity_id, entity_type, metric_name, horizon, generated_at)
);

CREATE INDEX idx_spend_forecasts_tenant ON spend_forecasts(tenant_id);
CREATE INDEX idx_spend_forecasts_entity ON spend_forecasts(entity_id, entity_type);
CREATE INDEX idx_spend_forecasts_valid ON spend_forecasts(valid_until);

-- Table: invoice_anomalies
-- Detected anomalies in invoices, approvals, and vendor behavior
CREATE TABLE invoice_anomalies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,

    -- Anomaly details
    anomaly_type TEXT NOT NULL CHECK (anomaly_type IN (
        'invoice_amount_outlier',
        'duplicate_invoice',
        'vendor_volume_spike',
        'approval_time_anomaly',
        'budget_threshold',
        'vendor_concentration'
    )),
    severity TEXT NOT NULL CHECK (severity IN ('low', 'medium', 'high', 'critical')),

    -- Entity with anomaly
    entity_id TEXT NOT NULL,
    entity_type TEXT NOT NULL CHECK (entity_type IN ('vendor', 'department', 'gl_code', 'tenant', 'approver')),

    -- Anomaly metrics
    detected_value DECIMAL(18, 2) NOT NULL,
    expected_range_min DECIMAL(18, 2) NOT NULL,
    expected_range_max DECIMAL(18, 2) NOT NULL,
    deviation_score DECIMAL(8, 4) NOT NULL,

    -- Metadata (flexible JSON for different anomaly types)
    metadata JSONB,

    -- Timestamps
    detected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Acknowledgment tracking
    acknowledged BOOLEAN DEFAULT FALSE,
    acknowledged_at TIMESTAMPTZ,
    acknowledged_by UUID REFERENCES users(id),

    -- Resolution tracking
    resolved BOOLEAN DEFAULT FALSE,
    resolved_at TIMESTAMPTZ,
    resolved_by UUID REFERENCES users(id),
    resolution_notes TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_invoice_anomalies_tenant ON invoice_anomalies(tenant_id);
CREATE INDEX idx_invoice_anomalies_type ON invoice_anomalies(anomaly_type);
CREATE INDEX idx_invoice_anomalies_severity ON invoice_anomalies(severity);
CREATE INDEX idx_invoice_anomalies_entity ON invoice_anomalies(entity_id, entity_type);
CREATE INDEX idx_invoice_anomalies_detected ON invoice_anomalies(detected_at DESC);
CREATE INDEX idx_invoice_anomalies_unacknowledged ON invoice_anomalies(tenant_id, acknowledged) WHERE acknowledged = FALSE;

-- Table: forecast_accuracy_log
-- Track forecast accuracy over time for model improvement
CREATE TABLE forecast_accuracy_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,

    -- Forecast reference
    forecast_id UUID NOT NULL REFERENCES spend_forecasts(id) ON DELETE CASCADE,
    entity_id TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    metric_name TEXT NOT NULL,
    horizon TEXT NOT NULL,

    -- Forecast vs actual
    predicted_value DECIMAL(18, 2) NOT NULL,
    actual_value DECIMAL(18, 2) NOT NULL,

    -- Error metrics
    absolute_error DECIMAL(18, 2) NOT NULL,
    percentage_error DECIMAL(8, 4), -- NULL if actual is 0
    squared_error DECIMAL(18, 4) NOT NULL,

    -- Timestamps
    forecast_date TIMESTAMPTZ NOT NULL,
    actual_date TIMESTAMPTZ NOT NULL,
    calculated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_forecast_accuracy_tenant ON forecast_accuracy_log(tenant_id);
CREATE INDEX idx_forecast_accuracy_forecast ON forecast_accuracy_log(forecast_id);
CREATE INDEX idx_forecast_accuracy_calculated ON forecast_accuracy_log(calculated_at DESC);

-- Table: budget_alerts
-- Proactive alerts for budget thresholds and vendor concentration
CREATE TABLE budget_alerts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,

    -- Alert details
    alert_type TEXT NOT NULL CHECK (alert_type IN (
        'budget_threshold_approaching',
        'budget_threshold_exceeded',
        'vendor_concentration',
        'approval_bottleneck_predicted',
        'spend_spike_detected'
    )),
    severity TEXT NOT NULL CHECK (severity IN ('low', 'medium', 'high', 'critical')),

    -- Entity
    entity_id TEXT,
    entity_type TEXT,

    -- Alert content
    title TEXT NOT NULL,
    message TEXT NOT NULL,

    -- Threshold information
    threshold_value DECIMAL(18, 2),
    current_value DECIMAL(18, 2),
    threshold_percentage DECIMAL(5, 2),

    -- Actions
    recommended_action TEXT,

    -- Timestamps
    triggered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,

    -- Status
    dismissed BOOLEAN DEFAULT FALSE,
    dismissed_at TIMESTAMPTZ,
    dismissed_by UUID REFERENCES users(id),

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_budget_alerts_tenant ON budget_alerts(tenant_id);
CREATE INDEX idx_budget_alerts_type ON budget_alerts(alert_type);
CREATE INDEX idx_budget_alerts_triggered ON budget_alerts(triggered_at DESC);
CREATE INDEX idx_budget_alerts_active ON budget_alerts(tenant_id, dismissed) WHERE dismissed = FALSE;

-- Table: anomaly_rules
-- Configurable thresholds for anomaly detection
CREATE TABLE anomaly_rules (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,

    -- Rule scope
    entity_type TEXT CHECK (entity_type IN ('vendor', 'department', 'gl_code', 'tenant', 'approver')),
    entity_id TEXT, -- NULL = applies to all entities of this type

    -- Anomaly type
    anomaly_type TEXT NOT NULL CHECK (anomaly_type IN (
        'invoice_amount_outlier',
        'duplicate_invoice',
        'vendor_volume_spike',
        'approval_time_anomaly',
        'budget_threshold',
        'vendor_concentration'
    )),

    -- Thresholds
    zscore_threshold DECIMAL(4, 2) DEFAULT 3.0,
    iqr_multiplier DECIMAL(4, 2) DEFAULT 1.5,
    volume_spike_threshold DECIMAL(4, 2) DEFAULT 2.0,

    -- Notification preferences
    notification_channels TEXT[] DEFAULT ARRAY['email', 'in_app'],
    notify_on_severity TEXT[] DEFAULT ARRAY['medium', 'high', 'critical'],

    -- Rule status
    enabled BOOLEAN DEFAULT TRUE,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by UUID NOT NULL REFERENCES users(id),

    UNIQUE(tenant_id, anomaly_type, entity_type, entity_id)
);

CREATE INDEX idx_anomaly_rules_tenant ON anomaly_rules(tenant_id);
CREATE INDEX idx_anomaly_rules_type ON anomaly_rules(anomaly_type);
CREATE INDEX idx_anomaly_rules_enabled ON anomaly_rules(tenant_id, enabled) WHERE enabled = TRUE;

-- Comments for documentation
COMMENT ON TABLE spend_forecasts IS 'Time-series forecasts for spend, invoice volume, and approval times';
COMMENT ON TABLE invoice_anomalies IS 'Detected anomalies in invoice amounts, duplicates, vendor behavior, and approvals';
COMMENT ON TABLE forecast_accuracy_log IS 'Tracks forecast accuracy for model evaluation and improvement';
COMMENT ON TABLE budget_alerts IS 'Proactive alerts for budget thresholds and vendor concentration warnings';
COMMENT ON TABLE anomaly_rules IS 'Configurable thresholds for anomaly detection per tenant';
