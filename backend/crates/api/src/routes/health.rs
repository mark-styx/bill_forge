//! Health check endpoints for monitoring and observability

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;
use std::time::Instant;

use crate::state::AppState;
use billforge_invoice_capture::ocr;

/// Basic health check response
#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
}

/// Detailed health check response for monitoring
#[derive(Serialize)]
pub struct DetailedHealthResponse {
    pub status: HealthStatus,
    pub version: &'static str,
    pub uptime_seconds: u64,
    pub checks: HealthChecks,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Serialize)]
pub struct HealthChecks {
    pub database: ComponentHealth,
    pub storage: ComponentHealth,
    pub ocr: ComponentHealth,
}

/// OCR health check response
#[derive(Serialize)]
pub struct OcrHealthResponse {
    pub configured_provider: String,
    pub providers: Vec<OcrProviderStatus>,
}

#[derive(Serialize)]
pub struct OcrProviderStatus {
    pub name: String,
    pub available: bool,
}

#[derive(Serialize)]
pub struct ComponentHealth {
    pub status: ComponentStatus,
    pub latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ComponentStatus {
    Up,
    Down,
    Degraded,
}

// Track server start time
static START_TIME: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

pub fn init_start_time() {
    START_TIME.get_or_init(Instant::now);
}

/// Simple health check for load balancers (returns 200 OK if server is running)
pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy",
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// Liveness probe - indicates if the application is running
/// Used by Kubernetes to determine if a pod should be restarted
pub async fn liveness() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

/// Readiness probe - indicates if the application is ready to accept traffic
/// Checks database connectivity before declaring ready
pub async fn readiness(State(state): State<AppState>) -> impl IntoResponse {
    // Check if database is accessible
    let db_ok = sqlx::query("SELECT 1")
        .fetch_one(&*state.db.metadata())
        .await
        .is_ok();

    if db_ok {
        (StatusCode::OK, "READY")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "NOT READY")
    }
}

/// Detailed health check with component status
/// Used for monitoring dashboards and detailed diagnostics
pub async fn detailed_health(State(state): State<AppState>) -> Json<DetailedHealthResponse> {
    let start_time = START_TIME.get().map(|t| t.elapsed().as_secs()).unwrap_or(0);

    // Check database health
    let db_start = Instant::now();
    let db_check = sqlx::query("SELECT 1")
        .fetch_one(&*state.db.metadata())
        .await;
    let db_latency = db_start.elapsed().as_millis() as u64;

    let database = match db_check {
        Ok(_) => ComponentHealth {
            status: if db_latency < 100 {
                ComponentStatus::Up
            } else {
                ComponentStatus::Degraded
            },
            latency_ms: Some(db_latency),
            message: None,
        },
        Err(e) => ComponentHealth {
            status: ComponentStatus::Down,
            latency_ms: Some(db_latency),
            message: Some(e.to_string()),
        },
    };

    // Check storage health
    let storage_start = Instant::now();
    let storage_check = state.storage.health_check().await;
    let storage_latency = storage_start.elapsed().as_millis() as u64;

    let storage = match storage_check {
        Ok(_) => ComponentHealth {
            status: if storage_latency < 200 {
                ComponentStatus::Up
            } else {
                ComponentStatus::Degraded
            },
            latency_ms: Some(storage_latency),
            message: None,
        },
        Err(e) => ComponentHealth {
            status: ComponentStatus::Down,
            latency_ms: Some(storage_latency),
            message: Some(e.to_string()),
        },
    };

    // Check OCR health
    let providers = ocr::available_providers();
    let configured_provider = state.config.ocr_provider.clone();
    let ocr_available = providers
        .iter()
        .any(|(name, available)| name == &configured_provider && *available);
    let ocr = ComponentHealth {
        status: if ocr_available {
            ComponentStatus::Up
        } else {
            ComponentStatus::Degraded
        },
        latency_ms: None,
        message: if ocr_available {
            None
        } else {
            Some(format!(
                "Configured provider '{}' not available, falling back to mock OCR",
                configured_provider
            ))
        },
    };

    // Determine overall status
    let overall_status = match (&database.status, &storage.status) {
        (ComponentStatus::Up, ComponentStatus::Up) => HealthStatus::Healthy,
        (ComponentStatus::Down, _) | (_, ComponentStatus::Down) => HealthStatus::Unhealthy,
        _ => HealthStatus::Degraded,
    };

    let environment = std::env::var("ENVIRONMENT").ok();

    Json(DetailedHealthResponse {
        status: overall_status,
        version: env!("CARGO_PKG_VERSION"),
        uptime_seconds: start_time,
        checks: HealthChecks {
            database,
            storage,
            ocr,
        },
        environment,
    })
}

/// OCR-specific health check endpoint
pub async fn ocr_health(State(state): State<AppState>) -> Json<OcrHealthResponse> {
    let providers = ocr::available_providers();
    let configured_provider = state.config.ocr_provider.clone();

    Json(OcrHealthResponse {
        configured_provider,
        providers: providers
            .into_iter()
            .map(|(name, available)| OcrProviderStatus {
                name: name.to_string(),
                available,
            })
            .collect(),
    })
}
