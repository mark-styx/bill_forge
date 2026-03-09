//! Feedback Data Models

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Customer feedback entry
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Feedback {
    pub id: Uuid,
    pub tenant_id: String,
    pub user_id: Uuid,
    pub category: String,
    pub rating: i32,
    pub comment: Option<String>,
    pub sentiment: Option<String>,
    pub sentiment_score: Option<f64>,
    pub created_at: DateTime<Utc>,
}

/// Feedback category enum
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackCategory {
    General,
    InvoiceProcessing,
    WorkflowApproval,
    VendorManagement,
    Reporting,
    Performance,
    UiUx,
    Bug,
    FeatureRequest,
    Other,
}

impl FeedbackCategory {
    pub fn as_str(&self) -> &str {
        match self {
            Self::General => "general",
            Self::InvoiceProcessing => "invoice_processing",
            Self::WorkflowApproval => "workflow_approval",
            Self::VendorManagement => "vendor_management",
            Self::Reporting => "reporting",
            Self::Performance => "performance",
            Self::UiUx => "ui_ux",
            Self::Bug => "bug",
            Self::FeatureRequest => "feature_request",
            Self::Other => "other",
        }
    }
}

/// Feedback aggregation by category
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FeedbackAggregation {
    pub category: String,
    pub total_feedback: i64,
    pub average_rating: f64,
    pub positive_count: i64,
    pub neutral_count: i64,
    pub negative_count: i64,
    pub last_feedback_at: DateTime<Utc>,
}

/// Feedback trend over time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackTrend {
    pub period: String,
    pub start_date: DateTime<Utc>,
    pub end_date: DateTime<Utc>,
    pub average_rating: f64,
    pub total_feedback: i64,
    pub sentiment_breakdown: SentimentBreakdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentimentBreakdown {
    pub positive: i64,
    pub neutral: i64,
    pub negative: i64,
}

/// Submit feedback request
#[derive(Debug, Deserialize)]
pub struct SubmitFeedbackRequest {
    pub category: String,
    pub rating: i32,
    pub comment: Option<String>,
}

/// Feedback query parameters
#[derive(Debug, Deserialize)]
pub struct FeedbackQuery {
    pub category: Option<String>,
    pub min_rating: Option<i32>,
    pub max_rating: Option<i32>,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub sentiment: Option<String>,
}
