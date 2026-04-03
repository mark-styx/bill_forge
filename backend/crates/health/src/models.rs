use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Health score for a tenant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthScore {
    pub tenant_id: String,
    pub score: i32,
    pub classification: HealthClassification,
    pub usage_score: f64,
    pub feature_adoption_score: f64,
    pub error_rate_score: f64,
    pub sentiment_score: f64,
    pub payment_score: f64,
    pub calculated_at: DateTime<Utc>,
}

/// Risk classification based on health score
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum HealthClassification {
    AtRisk,         // score < 50
    NeedsAttention, // 50-70
    Healthy,        // 70+
}

impl HealthClassification {
    pub fn from_score(score: i32) -> Self {
        if score < 50 {
            Self::AtRisk
        } else if score < 70 {
            Self::NeedsAttention
        } else {
            Self::Healthy
        }
    }

    pub fn emoji(&self) -> &'static str {
        match self {
            Self::AtRisk => "🔴",
            Self::NeedsAttention => "🟡",
            Self::Healthy => "🟢",
        }
    }
}

/// Request to refresh health score
#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub tenant_id: String,
}

/// Response with health score details
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub tenant_id: String,
    pub score: i32,
    pub classification: HealthClassification,
    pub breakdown: HealthBreakdown,
    pub calculated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct HealthBreakdown {
    pub usage_score: f64,
    pub feature_adoption_score: f64,
    pub error_rate_score: f64,
    pub sentiment_score: f64,
    pub payment_score: f64,
}
