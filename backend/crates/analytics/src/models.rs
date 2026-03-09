//! Analytics Data Models

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// User behavior tracking event
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AnalyticsEvent {
    pub id: Uuid,
    pub tenant_id: String,
    pub user_id: Uuid,
    pub event_type: String,
    pub event_category: String,
    pub event_data: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// Feature usage metrics
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FeatureUsage {
    pub feature_name: String,
    pub usage_count: i64,
    pub unique_users: i64,
    pub avg_duration_ms: Option<f64>,
    pub last_used: DateTime<Utc>,
}

/// Performance metrics
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PerformanceMetric {
    pub endpoint: String,
    pub avg_response_time_ms: f64,
    pub p95_response_time_ms: f64,
    pub p99_response_time_ms: f64,
    pub request_count: i64,
    pub error_count: i64,
    pub error_rate: f64,
}

/// Usage summary (daily/weekly/monthly)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSummary {
    pub period: String,
    pub start_date: DateTime<Utc>,
    pub end_date: DateTime<Utc>,
    pub total_events: i64,
    pub unique_users: i64,
    pub top_features: Vec<FeatureUsage>,
    pub performance_metrics: Vec<PerformanceMetric>,
}

/// Trend analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendData {
    pub metric_name: String,
    pub current_value: f64,
    pub previous_value: f64,
    pub change_percentage: f64,
    pub trend: Trend,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Trend {
    Increasing,
    Decreasing,
    Stable,
}

/// Analytics query parameters
#[derive(Debug, Deserialize)]
pub struct AnalyticsQuery {
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub event_type: Option<String>,
    pub user_id: Option<Uuid>,
}

/// Create analytics event request
#[derive(Debug, Deserialize)]
pub struct CreateEventRequest {
    pub event_type: String,
    pub event_category: String,
    pub event_data: serde_json::Value,
}
