//! Predictive Model Abstractions
//!
//! Core types and traits for predictive analytics models.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Result type for predictive analytics operations
pub type PredictiveResult<T> = Result<T, PredictiveError>;

/// Errors from predictive analytics operations
#[derive(Debug, thiserror::Error)]
pub enum PredictiveError {
    #[error(
        "Insufficient data for prediction: need at least {required} data points, got {actual}"
    )]
    InsufficientData { required: usize, actual: usize },

    #[error("Model training failed: {0}")]
    TrainingFailed(String),

    #[error("Prediction failed: {0}")]
    PredictionFailed(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Time series data point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSeriesPoint {
    pub timestamp: DateTime<Utc>,
    pub value: f64,
}

/// Time series with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSeries {
    pub entity_id: String,
    pub entity_type: EntityType,
    pub metric_name: String,
    pub points: Vec<TimeSeriesPoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Vendor,
    Department,
    GlCode,
    Tenant,
    Approver,
}

/// Forecast horizon (days in the future)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForecastHorizon {
    Days30,
    Days60,
    Days90,
}

impl ForecastHorizon {
    pub fn days(&self) -> u32 {
        match self {
            ForecastHorizon::Days30 => 30,
            ForecastHorizon::Days60 => 60,
            ForecastHorizon::Days90 => 90,
        }
    }
}

/// Forecast result with confidence interval
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Anomaly detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub acknowledged_by: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnomalyType {
    InvoiceAmountOutlier,
    DuplicateInvoice,
    VendorVolumeSpike,
    ApprovalTimeAnomaly,
    BudgetThreshold,
    VendorConcentration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnomalySeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Trait for time-series forecasting models
#[async_trait::async_trait]
pub trait ForecastingModel: Send + Sync {
    /// Fit the model to historical time series data
    async fn fit(&mut self, data: &TimeSeries) -> PredictiveResult<()>;

    /// Generate forecast for the specified horizon
    async fn forecast(&self, horizon: ForecastHorizon) -> PredictiveResult<Forecast>;

    /// Model name/version
    fn model_name(&self) -> &str;
}

/// Trait for anomaly detection models
#[async_trait::async_trait]
pub trait AnomalyDetector: Send + Sync {
    /// Detect anomalies in time series data
    async fn detect(&self, data: &TimeSeries) -> PredictiveResult<Vec<Anomaly>>;

    /// Model name/version
    fn model_name(&self) -> &str;
}

/// Forecast accuracy metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForecastAccuracy {
    pub entity_id: String,
    pub horizon: ForecastHorizon,
    pub mape: f64, // Mean Absolute Percentage Error
    pub mae: f64,  // Mean Absolute Error
    pub rmse: f64, // Root Mean Squared Error
    pub calculated_at: DateTime<Utc>,
}
