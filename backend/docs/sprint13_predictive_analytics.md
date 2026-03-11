# Sprint 14 Feature #2: Predictive Analytics & Anomaly Detection

## Overview

The Predictive Analytics & Anomaly Detection feature transforms BillForge from a reactive analytics platform to a proactive intelligence system that forecasts spend, detects anomalies, and alerts teams to potential issues before they become problems.

## Problem Statement

BillForge currently provides historical analytics only, requiring users to:
- Manually identify spending trends and anomalies
- React to budget overruns after they happen
- Miss opportunities for early payment discounts due to poor cash flow forecasting
- Spend time hunting for duplicate invoices and suspicious charges

This results in:
- Budget overruns caught too late
- Cash flow unpredictability
- Manual effort to identify problematic invoices
- Delayed approvals due to lack of proactive insights

## Solution

Comprehensive predictive analytics system with:
1. **Spend Forecasting** (Day 1)
   - 30/60/90 day forecasts per vendor, department, GL code
   - Seasonality detection (weekly, monthly, quarterly patterns)
   - Confidence intervals for budget planning
   - ARIMA-inspired time-series models

2. **Anomaly Detection** (Day 2)
   - Invoice amount outliers (z-score + IQR methods)
   - Duplicate invoice detection (fuzzy matching)
   - Vendor behavior changes (volume spikes)
   - Approval time anomalies
   - Configurable thresholds per tenant

3. **Proactive Alerts** (Day 3)
   - Budget threshold alerts (approaching quarterly limits)
   - Vendor concentration warnings (>40% spend with single vendor)
   - Approval bottleneck predictions
   - Integration with notification system

## Architecture

### Core Components

#### 1. Predictive Models (`crates/analytics/src/predictive_models.rs`)

**Main Types:**
```rust
pub struct TimeSeries {
    pub entity_id: String,
    pub entity_type: EntityType,
    pub metric_name: String,
    pub points: Vec<TimeSeriesPoint>,
}

pub struct Forecast {
    pub entity_id: String,
    pub entity_type: EntityType,
    pub metric_name: String,
    pub horizon: ForecastHorizon,
    pub predicted_value: f64,
    pub confidence_lower: f64,
    pub confidence_upper: f64,
    pub confidence_level: f64,
    pub generated_at: DateTime<Utc>,
    pub model_version: String,
    pub seasonality_detected: bool,
}

pub struct Anomaly {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub anomaly_type: AnomalyType,
    pub entity_id: String,
    pub entity_type: EntityType,
    pub severity: AnomalySeverity,
    pub detected_value: f64,
    pub expected_range: (f64, f64),
    pub deviation_score: f64,
    pub detected_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
    pub acknowledged: bool,
}
```

**Key Features:**
- Builder pattern for flexible construction
- Entity-specific forecasting (vendor, department, GL code)
- Multiple forecast horizons (30/60/90 days)
- Severity classification for anomalies

#### 2. Forecasting (`crates/analytics/src/forecasting.rs`)

```rust
pub struct ArimaForecaster {
    model_version: String,
    data: Option<TimeSeries>,
    statistics: Option<ArimaStatistics>,
}

impl ArimaForecaster {
    pub fn new() -> Self;
    fn decompose_time_series(&self, data: &TimeSeries) -> PredictiveResult<ArimaStatistics>;
    fn detect_seasonality_autocorr(&self, data: &[f64]) -> (bool, Option<u32>);
}

#[async_trait::async_trait]
impl ForecastingModel for ArimaForecaster {
    async fn fit(&mut self, data: &TimeSeries) -> PredictiveResult<()>;
    async fn forecast(&self, horizon: ForecastHorizon) -> PredictiveResult<Forecast>;
}
```

**Key Features:**
- ARIMA-inspired time-series decomposition
- Trend component extraction (linear regression)
- Seasonality detection via autocorrelation
- Confidence interval calculation
- Seasonal period detection (7, 14, 30 days)

#### 3. Anomaly Detection (`crates/analytics/src/anomaly_detection.rs`)

```rust
pub struct StatisticalAnomalyDetector {
    tenant_id: Uuid,
    zscore_threshold: f64,
    iqr_multiplier: f64,
}

impl StatisticalAnomalyDetector {
    pub fn new(tenant_id: Uuid) -> Self;
    pub fn detect_amount_outliers(&self, data: &TimeSeries) -> PredictiveResult<Vec<Anomaly>>;
    pub fn detect_vendor_volume_spikes(&self, data: &TimeSeries) -> PredictiveResult<Vec<Anomaly>>;
    pub fn detect_approval_time_anomalies(&self, data: &TimeSeries) -> PredictiveResult<Vec<Anomaly>>;
}

pub struct DuplicateDetector {
    tenant_id: Uuid,
    amount_tolerance: f64,
    date_tolerance_days: i64,
}

impl DuplicateDetector {
    pub fn new(tenant_id: Uuid) -> Self;
    pub fn detect_duplicates(&self, invoices: &[InvoiceRecord]) -> PredictiveResult<Vec<Anomaly>>;
}
```

**Key Features:**
- Dual-method outlier detection (z-score + IQR)
- Duplicate invoice detection with fuzzy matching
- Vendor volume spike detection
- Approval time anomaly identification
- Configurable thresholds per tenant

### Database Schema

#### `spend_forecasts`
Time-series forecasts for budget planning.

**Key Columns:**
- `entity_id`: Vendor ID, department, GL code, etc.
- `entity_type`: Type of entity being forecasted
- `horizon`: Forecast horizon (30/60/90 days)
- `predicted_value`: Predicted spend amount
- `confidence_lower/upper`: 95% confidence interval
- `seasonality_detected`: Whether seasonal patterns were found
- `valid_until`: Forecast expiration date

#### `invoice_anomalies`
Detected anomalies requiring attention.

**Key Columns:**
- `anomaly_type`: Type of anomaly (amount outlier, duplicate, volume spike, etc.)
- `severity`: Low/Medium/High/Critical
- `detected_value`: The anomalous value
- `expected_range_min/max`: Expected normal range
- `deviation_score`: How far from normal (z-score or multiplier)
- `acknowledged`: Whether someone acknowledged the anomaly
- `resolved`: Whether the issue was addressed

#### `forecast_accuracy_log`
Track forecast accuracy for model improvement.

**Key Columns:**
- `predicted_value`: Original forecast
- `actual_value`: What actually happened
- `absolute_error`: |predicted - actual|
- `percentage_error`: Error as % of actual
- `squared_error`: For RMSE calculation

#### `budget_alerts`
Proactive alerts for budget thresholds.

**Key Columns:**
- `alert_type`: Type of alert (budget threshold, vendor concentration, etc.)
- `severity`: Alert severity level
- `threshold_value`: Configured threshold
- `current_value`: Current metric value
- `threshold_percentage`: How close to threshold
- `recommended_action`: Suggested next step

#### `anomaly_rules`
Configurable thresholds per tenant.

**Key Columns:**
- `anomaly_type`: Which anomaly type to configure
- `zscore_threshold`: Z-score cutoff (default: 3.0)
- `iqr_multiplier`: IQR cutoff (default: 1.5)
- `volume_spike_threshold`: Volume increase ratio (default: 2.0)
- `notification_channels`: Where to send alerts
- `notify_on_severity`: Which severity levels trigger notifications

## API Endpoints

### 1. Get Forecasts
```http
GET /api/v1/analytics/predictive/forecasts?entity_type=vendor&entity_id=v123&horizon=30
```

Retrieve forecasts for a specific entity and horizon.

**Response:**
```json
{
  "success": true,
  "data": [
    {
      "entity_id": "v123",
      "entity_type": "vendor",
      "metric_name": "spend",
      "horizon": "days_30",
      "predicted_value": 12500.00,
      "confidence_lower": 10200.00,
      "confidence_upper": 14800.00,
      "confidence_level": 0.95,
      "generated_at": "2026-03-10T12:00:00Z",
      "model_version": "arima_v1",
      "seasonality_detected": true
    }
  ]
}
```

### 2. Generate Forecast
```http
POST /api/v1/analytics/predictive/forecasts/generate
```

Generate a new forecast for an entity.

**Request:**
```json
{
  "entity_type": "vendor",
  "entity_id": "v123",
  "horizon": "30"
}
```

**Response:** Same as GET forecasts

### 3. Get Anomalies
```http
GET /api/v1/analytics/predictive/anomalies
```

Retrieve all anomalies for tenant.

**Response:**
```json
{
  "success": true,
  "data": [
    {
      "id": "uuid",
      "tenant_id": "uuid",
      "anomaly_type": "invoice_amount_outlier",
      "entity_id": "inv-123",
      "entity_type": "vendor",
      "severity": "high",
      "detected_value": 5000.00,
      "expected_range": [800.00, 1500.00],
      "deviation_score": 4.2,
      "detected_at": "2026-03-10T12:00:00Z",
      "acknowledged": false
    }
  ]
}
```

### 4. Acknowledge Anomaly
```http
POST /api/v1/analytics/predictive/anomalies/:anomaly_id/acknowledge
```

Mark an anomaly as reviewed.

**Request:**
```json
{
  "notes": "False positive - approved by manager"
}
```

### 5. Detect Anomalies (Manual Trigger)
```http
POST /api/v1/analytics/predictive/anomalies/detect
```

Run anomaly detection on recent invoices.

### 6. Get Budget Alerts
```http
GET /api/v1/analytics/predictive/alerts
```

Retrieve active budget alerts.

**Response:**
```json
{
  "success": true,
  "data": [
    {
      "id": "uuid",
      "alert_type": "budget_threshold_approaching",
      "severity": "medium",
      "title": "IT Department approaching Q1 budget limit",
      "message": "IT department has spent 85% of Q1 budget ($127,500 of $150,000)",
      "threshold_value": 150000,
      "current_value": 127500,
      "threshold_percentage": 85.0,
      "recommended_action": "Review pending IT invoices and defer non-essential spending",
      "triggered_at": "2026-03-10T12:00:00Z"
    }
  ]
}
```

### 7. Configure Anomaly Rule
```http
POST /api/v1/analytics/predictive/rules
```

Set custom thresholds for anomaly detection.

**Request:**
```json
{
  "entity_type": "vendor",
  "anomaly_type": "invoice_amount_outlier",
  "zscore_threshold": 2.5,
  "iqr_multiplier": 1.5,
  "notification_channels": ["email", "slack"],
  "notify_on_severity": ["high", "critical"],
  "enabled": true
}
```

## Forecasting Methodology

### ARIMA-Inspired Model

The forecasting model uses a simplified ARIMA (AutoRegressive Integrated Moving Average) approach:

1. **Trend Component**
   - Linear regression on historical data
   - Slope and intercept extraction
   - Projects trend forward

2. **Seasonal Component**
   - Autocorrelation analysis at lags 7, 14, 30 days
   - Detects weekly/bi-weekly/monthly patterns
   - Adjusts forecast for seasonal position

3. **Residual Analysis**
   - Removes trend and seasonality
   - Calculates residual standard deviation
   - Uses for confidence interval width

4. **Confidence Intervals**
   - 95% confidence level by default
   - Widen based on forecast horizon (1.5x, 2.0x, 2.5x multiplier)
   - Ensures lower bound >= 0 for spend forecasts

### Accuracy Tracking

All forecasts are logged for accuracy measurement:
- Mean Absolute Percentage Error (MAPE)
- Mean Absolute Error (MAE)
- Root Mean Squared Error (RMSE)

This enables continuous model improvement.

## Anomaly Detection Methods

### 1. Invoice Amount Outliers

**Dual-Method Approach:**

- **Z-Score Method**: Flags values > 3 standard deviations from mean
- **IQR Method**: Flags values outside [Q1 - 1.5×IQR, Q3 + 1.5×IQR]

An anomaly is flagged if either method triggers.

### 2. Duplicate Invoice Detection

**Multi-Factor Similarity:**

- **Vendor Name** (30% weight): Jaccard similarity on words
- **Amount** (40% weight): Within 1% tolerance
- **Date** (30% weight): Within 30 days

Threshold: Overall similarity > 0.8 triggers alert

### 3. Vendor Volume Spikes

**Comparison Logic:**

- Compare recent 7 days to previous 7+ days
- Calculate increase ratio: recent_avg / historical_avg
- Threshold: > 2x increase triggers alert

### 4. Approval Time Anomalies

**Statistical Analysis:**

- Calculate mean and std dev of approval times
- Flag approvals > mean + 2×std_dev
- Severity increases with deviation (2×, 3×, 4× std_dev)

## Success Metrics

### Target Goals (Sprint 14)
- **95% duplicate invoice detection rate** before payment
- **Forecast accuracy within 15%** of actual spend
- **50% reduction in budget overruns**
- **30% of users configure custom anomaly thresholds**

### Measurement
- Track forecast accuracy via `forecast_accuracy_log` table
- Monitor anomaly acknowledgment rate
- Measure time-to-detection for duplicate invoices
- Survey users on forecast usefulness

## Configuration

### Default Thresholds

```rust
// Statistical anomaly detector defaults
zscore_threshold: 3.0,  // 3 standard deviations
iqr_multiplier: 1.5,    // Standard IQR outlier threshold

// Duplicate detector defaults
amount_tolerance: 0.01,  // 1% tolerance
date_tolerance_days: 30, // Within 30 days

// Volume spike threshold
volume_spike_threshold: 2.0,  // 2x increase
```

### Custom Thresholds

Tenants can configure custom thresholds via the `/rules` API:

```json
{
  "anomaly_type": "invoice_amount_outlier",
  "zscore_threshold": 2.5,  // More sensitive
  "notification_channels": ["email", "slack", "in_app"],
  "notify_on_severity": ["high", "critical"]
}
```

## Future Enhancements

### Sprint 15+
1. **Machine Learning Models**
   - Replace linear regression with LSTM neural networks
   - Add Prophet-style forecasting
   - Improve seasonal pattern detection

2. **Predictive Approval Routing**
   - Predict which approvers will be fastest
   - Suggest optimal routing before submission
   - Learn from historical approval patterns

3. **Cash Flow Forecasting**
   - Combine vendor forecasts into cash flow projections
   - Predict payment timing based on approval history
   - Recommend early payment opportunities

4. **Budget Optimization**
   - Suggest budget reallocations based on forecasts
   - Identify cost-saving opportunities
   - Vendor consolidation recommendations

## Testing

### Unit Tests
All core modules include comprehensive unit tests:
- `predictive_models.rs`: 0 tests (type definitions only)
- `forecasting.rs`: 4 tests (seasonality, fitting, forecasting)
- `anomaly_detection.rs`: 5 tests (outliers, spikes, duplicates, similarity)

### Integration Tests
Run with: `cargo test --lib -p billforge-analytics`

```bash
running 11 tests
test forecasting::tests::test_detect_weekly_seasonality ... ok
test anomaly_detection::tests::test_detect_vendor_volume_spike ... ok
test anomaly_detection::tests::test_detect_amount_outliers ... ok
test anomaly_detection::tests::test_detect_approval_time_anomalies ... ok
test anomaly_detection::tests::test_detect_duplicate_invoices ... ok
test anomaly_detection::tests::test_string_similarity ... ok
test forecasting::tests::test_naive_forecaster_insufficient_data ... ok
test forecasting::tests::test_arima_forecaster_forecast ... ok
test forecasting::tests::test_naive_forecaster_fit ... ok
test forecasting::tests::test_arima_forecaster_fit ... ok
test jobs::daily_aggregation::tests::test_daily_aggregation_job_creation ... ok

test result: ok. 11 passed; 0 failed
```

### Manual Testing

```bash
# Generate forecast
curl -X POST http://localhost:8080/api/v1/analytics/predictive/forecasts/generate \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"entity_type":"vendor","entity_id":"v123","horizon":"30"}'

# Get anomalies
curl -X GET http://localhost:8080/api/v1/analytics/predictive/anomalies \
  -H "Authorization: Bearer $TOKEN"

# Acknowledge anomaly
curl -X POST http://localhost:8080/api/v1/analytics/predictive/anomalies/$ANOMALY_ID/acknowledge \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"notes":"False positive"}'

# Configure custom rule
curl -X POST http://localhost:8080/api/v1/analytics/predictive/rules \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"anomaly_type":"invoice_amount_outlier","zscore_threshold":2.5}'
```

## Migration

### Database Migration
```bash
# Run migration
sqlx migrate run

# Verify tables created
psql -d billforge -c "\dt spend_*"
psql -d billforge -c "\dt invoice_anomalies"
psql -d billforge -c "\dt budget_alerts"
psql -d billforge -c "\dt anomaly_rules"
```

### Backfill Data
No backfill required - forecasts are generated on-demand and anomalies are detected from existing invoice data.

## Monitoring

### Key Metrics
- `forecast_generation_count{horizon}`: Number of forecasts generated by horizon
- `forecast_accuracy_mape{horizon}`: Mean Absolute Percentage Error
- `anomaly_detection_count{type, severity}`: Anomalies detected by type/severity
- `anomaly_acknowledgment_rate`: % of anomalies acknowledged
- `duplicate_detection_recall`: % of actual duplicates caught

### Alerts
- Forecast accuracy MAPE > 20%
- Anomaly detection latency > 5 minutes
- Duplicate detection recall < 90%
- Budget threshold exceeded without alert

## Dependencies

### New Dependencies
All dependencies already exist in the workspace - no new crates added.

### Existing Dependencies
- `chrono`: Date/time handling
- `serde`, `serde_json`: Serialization
- `uuid`: ID generation
- `sqlx`: Database operations
- `tracing`: Logging
- `async-trait`: Async trait support

## Files Changed

### New Files
- `crates/analytics/src/predictive_models.rs` (220 lines)
- `crates/analytics/src/forecasting.rs` (400 lines)
- `crates/analytics/src/anomaly_detection.rs` (550 lines)
- `crates/api/src/routes/predictive.rs` (550 lines)
- `migrations/053_add_predictive_analytics.sql` (250 lines)

### Modified Files
- `crates/analytics/Cargo.toml`: Added `async-trait` dependency
- `crates/analytics/src/lib.rs`: Exported new modules
- `crates/api/src/routes/mod.rs`: Added predictive routes

## References

- [Sprint 13 Feature Plan](../sprint_13_feature_plan.md#p0-2-predictive-analytics--anomaly-detection-3-days)
- [Time Series Forecasting](https://otexts.com/fpp3/)
- [Anomaly Detection Methods](https://scikit-learn.org/stable/modules/outlier_detection.html)
- [ARIMA Models](https://www.statsmodels.org/stable/generated/statsmodels.tsa.arima.model.ARIMA.html)
