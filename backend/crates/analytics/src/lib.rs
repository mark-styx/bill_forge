//! Analytics Module
//!
//! Provides user behavior tracking, feature usage metrics, performance analytics,
//! predictive forecasting, and anomaly detection.

pub mod anomaly_detection;
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
pub use forecasting::*;
pub use handlers::create_router;
pub use models::*;
pub use predictive_models::*;
pub use predictive_repository::{ForecastAccuracySummary, PredictiveRepository};
pub use predictive_service::PredictiveService;
pub use repository::AnalyticsRepository;
pub use service::AnalyticsService;
