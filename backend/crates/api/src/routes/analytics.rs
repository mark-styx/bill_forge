//! Analytics API Routes
//!
//! Authenticated, tenant-scoped analytics endpoints. Tenant identity is derived
//! from the authenticated user context (AuthUser) - never from path parameters.
//! All queries run against the tenant-scoped pool obtained via `state.db.tenant(...)`.

use axum::{
    extract::{Query, State},
    response::Json,
    routing::{get, post},
    Router,
};

use crate::error::ApiResult;
use crate::extractors::AuthUser;
use crate::state::AppState;
use billforge_analytics::{
    models::{AnalyticsQuery, CreateEventRequest, PerformanceMetric, TrendData, UsageSummary},
    repository::AnalyticsRepository,
    service::AnalyticsService,
};

/// API routes for analytics (mounted under `/api/v1/analytics`)
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/events", post(track_event))
        .route("/usage/daily", get(get_daily_usage))
        .route("/usage/weekly", get(get_weekly_usage))
        .route("/usage/monthly", get(get_monthly_usage))
        .route("/usage", get(get_usage))
        .route("/performance", get(get_performance))
        .route("/trends", get(get_trends))
}

/// Track an analytics event. User identity comes from the auth context.
async fn track_event(
    State(state): State<AppState>,
    user: AuthUser,
    Json(request): Json<CreateEventRequest>,
) -> ApiResult<Json<billforge_analytics::models::AnalyticsEvent>> {
    let tenant_id = &user.0.tenant_id;
    let user_id = user.0.user_id;
    let pool = state.db.tenant(tenant_id).await?;

    let repo = AnalyticsRepository::new((*pool).clone());
    let service = AnalyticsService::with_repository(repo);
    let event = service
        .track_event(&tenant_id.to_string(), user_id.0, request)
        .await
        .map_err(|e| billforge_core::Error::Internal(e.to_string()))?;

    Ok(Json(event))
}

/// Get daily usage summary (last 24 hours)
async fn get_daily_usage(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<UsageSummary>> {
    let tenant_id = &user.0.tenant_id;
    let pool = state.db.tenant(tenant_id).await?;

    let repo = AnalyticsRepository::new((*pool).clone());
    let service = AnalyticsService::with_repository(repo);
    let summary = service
        .get_daily_summary(&tenant_id.to_string())
        .await
        .map_err(|e| billforge_core::Error::Internal(e.to_string()))?;

    Ok(Json(summary))
}

/// Get weekly usage summary (last 7 days)
async fn get_weekly_usage(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<UsageSummary>> {
    let tenant_id = &user.0.tenant_id;
    let pool = state.db.tenant(tenant_id).await?;

    let repo = AnalyticsRepository::new((*pool).clone());
    let service = AnalyticsService::with_repository(repo);
    let summary = service
        .get_weekly_summary(&tenant_id.to_string())
        .await
        .map_err(|e| billforge_core::Error::Internal(e.to_string()))?;

    Ok(Json(summary))
}

/// Get monthly usage summary (last 30 days)
async fn get_monthly_usage(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<UsageSummary>> {
    let tenant_id = &user.0.tenant_id;
    let pool = state.db.tenant(tenant_id).await?;

    let repo = AnalyticsRepository::new((*pool).clone());
    let service = AnalyticsService::with_repository(repo);
    let summary = service
        .get_monthly_summary(&tenant_id.to_string())
        .await
        .map_err(|e| billforge_core::Error::Internal(e.to_string()))?;

    Ok(Json(summary))
}

/// Get custom usage summary with optional date range
async fn get_usage(
    State(state): State<AppState>,
    user: AuthUser,
    Query(query): Query<AnalyticsQuery>,
) -> ApiResult<Json<UsageSummary>> {
    let tenant_id = &user.0.tenant_id;
    let pool = state.db.tenant(tenant_id).await?;

    let start_date = query
        .start_date
        .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::days(7));
    let end_date = query.end_date.unwrap_or_else(|| chrono::Utc::now());

    let repo = AnalyticsRepository::new((*pool).clone());
    let service = AnalyticsService::with_repository(repo);
    let summary = service
        .get_usage_summary(&tenant_id.to_string(), start_date, end_date)
        .await
        .map_err(|e| billforge_core::Error::Internal(e.to_string()))?;

    Ok(Json(summary))
}

/// Get performance metrics with optional date range
async fn get_performance(
    State(state): State<AppState>,
    user: AuthUser,
    Query(query): Query<AnalyticsQuery>,
) -> ApiResult<Json<Vec<PerformanceMetric>>> {
    let tenant_id = &user.0.tenant_id;
    let pool = state.db.tenant(tenant_id).await?;

    let start_date = query
        .start_date
        .unwrap_or_else(|| chrono::Utc::now() - chrono::Duration::days(7));
    let end_date = query.end_date.unwrap_or_else(|| chrono::Utc::now());

    let repo = AnalyticsRepository::new((*pool).clone());
    let service = AnalyticsService::with_repository(repo);
    let metrics = service
        .get_performance_metrics(&tenant_id.to_string(), start_date, end_date)
        .await
        .map_err(|e| billforge_core::Error::Internal(e.to_string()))?;

    Ok(Json(metrics))
}

/// Get trend analysis
async fn get_trends(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<TrendData>>> {
    let tenant_id = &user.0.tenant_id;
    let pool = state.db.tenant(tenant_id).await?;

    let repo = AnalyticsRepository::new((*pool).clone());
    let service = AnalyticsService::with_repository(repo);
    let trends = service
        .get_trends(&tenant_id.to_string())
        .await
        .map_err(|e| billforge_core::Error::Internal(e.to_string()))?;

    Ok(Json(trends))
}

// ---------------------------------------------------------------------------
// Benchmark routes (opt-in peer insights)
// ---------------------------------------------------------------------------

use billforge_analytics::benchmark::{
    BenchmarkOptInRequest, BenchmarkResponse, CohortDescriptor,
    compute_tenant_kpis, fetch_cohort_percentiles, publish_tenant_kpis,
};

/// Benchmark sub-router mounted at `/analytics/benchmark`
pub fn benchmark_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(get_benchmark))
        .route("/opt-in", post(benchmark_opt_in))
}

/// `GET /api/v1/analytics/benchmark`
///
/// Returns the tenant's six AP KPIs alongside anonymized cohort percentiles.
/// If `benchmark_opt_in = false`, returns `{ opted_in: false }` so the UI
/// can render the opt-in CTA.
async fn get_benchmark(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<BenchmarkResponse>> {
    let tenant_id = &user.0.tenant_id;
    let metadata_pool = state.db.metadata();

    let row: Option<(bool, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        r#"
        SELECT benchmark_opt_in, benchmark_industry, benchmark_headcount_band, benchmark_volume_band
        FROM tenants
        WHERE id = $1
        "#,
    )
    .bind(tenant_id.as_uuid())
    .fetch_optional(&*metadata_pool)
    .await
    .map_err(|e| billforge_core::Error::Internal(format!("Failed to read tenant benchmark settings: {}", e)))?;

    let (opted_in, industry, headcount_band, volume_band) = match row {
        Some(r) => r,
        None => {
            return Ok(Json(BenchmarkResponse {
                opted_in: false,
                cohort: None,
                tenant_kpis: None,
                cohort_kpis: None,
                cohort_size: None,
            }));
        }
    };

    if !opted_in {
        return Ok(Json(BenchmarkResponse {
            opted_in: false,
            cohort: None,
            tenant_kpis: None,
            cohort_kpis: None,
            cohort_size: None,
        }));
    }

    let ind = match industry {
        Some(v) => v,
        None => {
            return Ok(Json(BenchmarkResponse {
                opted_in: true,
                cohort: None,
                tenant_kpis: None,
                cohort_kpis: None,
                cohort_size: None,
            }));
        }
    };
    let hc = headcount_band.unwrap_or_default();
    let vb = volume_band.unwrap_or_default();

    let cohort = CohortDescriptor {
        industry: ind.clone(),
        headcount_band: hc.clone(),
        volume_band: vb.clone(),
    };

    // Compute tenant KPIs (tenant-scoped pool under RLS)
    let tenant_pool = state.db.tenant(tenant_id).await?;
    let tenant_kpis = match compute_tenant_kpis(&tenant_pool).await {
        Ok(kpis) => {
            // Publish into metadata DB so cohort aggregation includes this tenant
            if let Err(e) = publish_tenant_kpis(
                &metadata_pool,
                tenant_id.as_uuid(),
                &cohort.industry,
                &cohort.headcount_band,
                &cohort.volume_band,
                &kpis,
            )
            .await
            {
                tracing::warn!("Failed to publish tenant benchmark KPIs: {}", e);
            }
            Some(kpis)
        }
        Err(e) => {
            tracing::warn!("Failed to compute tenant benchmark KPIs: {}", e);
            None
        }
    };

    // Fetch cohort percentiles from the SECURITY DEFINER function
    let (cohort_kpis, cohort_size) =
        match fetch_cohort_percentiles(&metadata_pool, &cohort.industry, &cohort.headcount_band, &cohort.volume_band).await {
            Ok(Some((pct, sz))) => (Some(pct), Some(sz)),
            Ok(None) => (None, None),
            Err(e) => {
                tracing::warn!("Failed to fetch cohort percentiles: {}", e);
                (None, None)
            }
        };

    Ok(Json(BenchmarkResponse {
        opted_in: true,
        cohort: Some(cohort),
        tenant_kpis,
        cohort_kpis,
        cohort_size,
    }))
}

/// `POST /api/v1/analytics/benchmark/opt-in`
///
/// Toggles the benchmark_opt_in flag and stores the cohort descriptor.
async fn benchmark_opt_in(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<BenchmarkOptInRequest>,
) -> ApiResult<Json<BenchmarkResponse>> {
    let tenant_id = &user.0.tenant_id;
    let metadata_pool = state.db.metadata();

    sqlx::query(
        r#"
        UPDATE tenants
        SET benchmark_opt_in = TRUE,
            benchmark_industry = $2,
            benchmark_headcount_band = $3,
            benchmark_volume_band = $4,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(tenant_id.as_uuid())
    .bind(&body.industry)
    .bind(&body.headcount_band)
    .bind(&body.volume_band)
    .execute(&*metadata_pool)
    .await
    .map_err(|e| billforge_core::Error::Internal(format!("Failed to update benchmark opt-in: {}", e)))?;

    Ok(Json(BenchmarkResponse {
        opted_in: true,
        cohort: Some(CohortDescriptor {
            industry: body.industry,
            headcount_band: body.headcount_band,
            volume_band: body.volume_band,
        }),
        tenant_kpis: None,
        cohort_kpis: None,
        cohort_size: None,
    }))
}
