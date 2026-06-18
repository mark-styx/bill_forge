//! Analytics Module
//!
//! Provides user behavior tracking, feature usage metrics, performance analytics,
//! predictive forecasting, and anomaly detection.

#![allow(warnings)]

pub mod anomaly_detection;
pub mod benchmark;
pub mod forecasting;
pub mod handlers;
pub mod jobs;
pub mod models;
pub mod predictive_models;
pub mod predictive_repository;
pub mod predictive_service;
pub mod repository;
pub mod service;

pub use anomaly_detection::*;
pub use benchmark::{
    compute_tenant_kpis, fetch_cohort_percentiles, publish_tenant_kpis, BenchmarkKpis,
    BenchmarkOptInRequest, BenchmarkResponse, CohortDescriptor, CohortPercentiles,
};
pub use forecasting::*;
pub use handlers::*;
pub use models::*;
pub use predictive_models::*;
pub use predictive_repository::{ForecastAccuracySummary, PredictiveRepository};
pub use predictive_service::PredictiveService;
pub use repository::AnalyticsRepository;
pub use service::AnalyticsService;
