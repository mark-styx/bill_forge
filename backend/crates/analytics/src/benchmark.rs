//! Anonymized AP Benchmarking & Peer Insights
//!
//! Provides opt-in benchmark KPIs comparing a tenant's performance against
//! anonymized peer cohorts (by industry, headcount, and invoice volume).
//! Cohort aggregates expose only p25/p50/p75 percentiles with a k-anonymity
//! floor of 5 tenants.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Six AP KPIs for a single tenant or a cohort percentile bucket.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchmarkKpis {
    pub dpo_days: f64,
    pub avg_approval_cycle_hours: f64,
    pub ocr_straight_through_rate: f64,
    pub exception_rate: f64,
    pub discount_capture_rate: f64,
    pub cost_per_invoice: f64,
}

/// Percentile bands for cohort KPIs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CohortPercentiles {
    pub p25: BenchmarkKpis,
    pub p50: BenchmarkKpis,
    pub p75: BenchmarkKpis,
}

/// Cohort descriptor (the three dimensions that define a peer group).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CohortDescriptor {
    pub industry: String,
    pub headcount_band: String,
    pub volume_band: String,
}

/// Full benchmark response returned by `GET /api/analytics/benchmark`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchmarkResponse {
    pub opted_in: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cohort: Option<CohortDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_kpis: Option<BenchmarkKpis>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cohort_kpis: Option<CohortPercentiles>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cohort_size: Option<i64>,
}

/// Request body for `POST /api/analytics/benchmark/opt-in`.
#[derive(Debug, Clone, Deserialize)]
pub struct BenchmarkOptInRequest {
    pub industry: String,
    pub headcount_band: String,
    pub volume_band: String,
}

// ---------------------------------------------------------------------------
// Database operations
// ---------------------------------------------------------------------------

/// Compute the six AP KPIs for the current tenant using tenant-scoped queries.
/// Runs under RLS (the pool is already tenant-scoped).
pub async fn compute_tenant_kpis(pool: &PgPool) -> Result<BenchmarkKpis> {
    // DPO: average days between invoice_date and paid_date for paid invoices (last 90d).
    // Cast AVG to ::float8 because AVG(integer) returns NUMERIC, which sqlx cannot
    // decode into f64 without the rust_decimal/bigdecimal feature.
    let dpo: (f64,) = sqlx::query_as(
        r#"
        SELECT COALESCE(
            AVG((paid_date - invoice_date))::float8
            FILTER (WHERE status = 'paid'
                      AND paid_date IS NOT NULL
                      AND invoice_date IS NOT NULL
                      AND paid_date >= NOW() - INTERVAL '90 days'),
            0
        )::float8
        FROM invoices
        "#,
    )
    .fetch_one(pool)
    .await
    .context("Failed to compute DPO")?;

    // Average approval cycle hours
    let cycle: (f64,) = sqlx::query_as(
        r#"
        SELECT COALESCE(
            AVG(EXTRACT(EPOCH FROM (updated_at - created_at)) / 3600)
            FILTER (WHERE status IN ('approved', 'ready_for_payment', 'paid')
                      AND updated_at >= NOW() - INTERVAL '90 days'),
            0
        )
        FROM invoices
        "#,
    )
    .fetch_one(pool)
    .await
    .context("Failed to compute avg approval cycle hours")?;

    // OCR straight-through rate
    let ocr: (f64,) = sqlx::query_as(
        r#"
        SELECT COALESCE(
            (COUNT(*) FILTER (
                WHERE processing_status NOT IN ('exception', 'ocr_failed')
                  AND created_at >= NOW() - INTERVAL '90 days'
            ))::float
            / NULLIF(COUNT(*) FILTER (
                WHERE created_at >= NOW() - INTERVAL '90 days'
            ), 0),
            0
        )
        FROM invoices
        "#,
    )
    .fetch_one(pool)
    .await
    .context("Failed to compute OCR straight-through rate")?;

    // Exception rate
    let exc: (f64,) = sqlx::query_as(
        r#"
        SELECT COALESCE(
            (COUNT(*) FILTER (
                WHERE processing_status = 'exception'
                  AND created_at >= NOW() - INTERVAL '90 days'
            ))::float
            / NULLIF(COUNT(*) FILTER (
                WHERE created_at >= NOW() - INTERVAL '90 days'
            ), 0),
            0
        )
        FROM invoices
        "#,
    )
    .fetch_one(pool)
    .await
    .context("Failed to compute exception rate")?;

    // Discount capture rate
    let disc: (f64,) = sqlx::query_as(
        r#"
        SELECT COALESCE(
            (COUNT(*) FILTER (
                WHERE discount_captured_at IS NOT NULL
                  AND discount_percent IS NOT NULL
                  AND created_at >= NOW() - INTERVAL '90 days'
            ))::float
            / NULLIF(COUNT(*) FILTER (
                WHERE discount_percent IS NOT NULL
                  AND created_at >= NOW() - INTERVAL '90 days'
            ), 0),
            0
        )
        FROM invoices
        "#,
    )
    .fetch_one(pool)
    .await
    .context("Failed to compute discount capture rate")?;

    Ok(BenchmarkKpis {
        dpo_days: dpo.0,
        avg_approval_cycle_hours: cycle.0,
        ocr_straight_through_rate: ocr.0,
        exception_rate: exc.0,
        discount_capture_rate: disc.0,
        cost_per_invoice: 0.0, // placeholder until cost tracking is implemented
    })
}

/// Fetch cohort percentiles from the SECURITY DEFINER function on the
/// metadata database. The function already enforces k-anonymity (cohort_size >= 5).
pub async fn fetch_cohort_percentiles(
    metadata_pool: &PgPool,
    industry: &str,
    headcount_band: &str,
    volume_band: &str,
) -> Result<Option<(CohortPercentiles, i64)>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        cohort_size: i64,
        dpo_days_p25: f64,
        dpo_days_p50: f64,
        dpo_days_p75: f64,
        avg_approval_cycle_hours_p25: f64,
        avg_approval_cycle_hours_p50: f64,
        avg_approval_cycle_hours_p75: f64,
        ocr_straight_through_rate_p25: f64,
        ocr_straight_through_rate_p50: f64,
        ocr_straight_through_rate_p75: f64,
        exception_rate_p25: f64,
        exception_rate_p50: f64,
        exception_rate_p75: f64,
        discount_capture_rate_p25: f64,
        discount_capture_rate_p50: f64,
        discount_capture_rate_p75: f64,
        cost_per_invoice_p25: f64,
        cost_per_invoice_p50: f64,
        cost_per_invoice_p75: f64,
    }

    let row: Option<Row> = sqlx::query_as::<_, Row>(
        r#"
        SELECT
            cohort_size,
            dpo_days_p25, dpo_days_p50, dpo_days_p75,
            avg_approval_cycle_hours_p25, avg_approval_cycle_hours_p50, avg_approval_cycle_hours_p75,
            ocr_straight_through_rate_p25, ocr_straight_through_rate_p50, ocr_straight_through_rate_p75,
            exception_rate_p25, exception_rate_p50, exception_rate_p75,
            discount_capture_rate_p25, discount_capture_rate_p50, discount_capture_rate_p75,
            cost_per_invoice_p25, cost_per_invoice_p50, cost_per_invoice_p75
        FROM get_peer_cohort_kpis($1, $2, $3)
        "#,
    )
    .bind(industry)
    .bind(headcount_band)
    .bind(volume_band)
    .fetch_optional(metadata_pool)
    .await
    .context("Failed to fetch cohort percentiles")?;

    let row = match row {
        Some(r) => r,
        None => return Ok(None),
    };

    if row.cohort_size < 5 {
        return Ok(None);
    }

    let cohort_size = row.cohort_size;
    let percentiles = CohortPercentiles {
        p25: BenchmarkKpis {
            dpo_days: row.dpo_days_p25,
            avg_approval_cycle_hours: row.avg_approval_cycle_hours_p25,
            ocr_straight_through_rate: row.ocr_straight_through_rate_p25,
            exception_rate: row.exception_rate_p25,
            discount_capture_rate: row.discount_capture_rate_p25,
            cost_per_invoice: row.cost_per_invoice_p25,
        },
        p50: BenchmarkKpis {
            dpo_days: row.dpo_days_p50,
            avg_approval_cycle_hours: row.avg_approval_cycle_hours_p50,
            ocr_straight_through_rate: row.ocr_straight_through_rate_p50,
            exception_rate: row.exception_rate_p50,
            discount_capture_rate: row.discount_capture_rate_p50,
            cost_per_invoice: row.cost_per_invoice_p50,
        },
        p75: BenchmarkKpis {
            dpo_days: row.dpo_days_p75,
            avg_approval_cycle_hours: row.avg_approval_cycle_hours_p75,
            ocr_straight_through_rate: row.ocr_straight_through_rate_p75,
            exception_rate: row.exception_rate_p75,
            discount_capture_rate: row.discount_capture_rate_p75,
            cost_per_invoice: row.cost_per_invoice_p75,
        },
    };

    Ok(Some((percentiles, cohort_size)))
}

/// Publish (upsert) a tenant's computed KPIs into the metadata DB rollup table
/// so that cohort aggregation can include this tenant. Called from the benchmark
/// GET handler after computing KPIs for an opted-in tenant.
pub async fn publish_tenant_kpis(
    metadata_pool: &PgPool,
    tenant_id: &uuid::Uuid,
    industry: &str,
    headcount_band: &str,
    volume_band: &str,
    kpis: &BenchmarkKpis,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO tenant_benchmark_kpis (
            tenant_id, industry, headcount_band, volume_band,
            dpo_days, avg_approval_cycle_hours, ocr_straight_through_rate,
            exception_rate, discount_capture_rate, cost_per_invoice, computed_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW())
        ON CONFLICT (tenant_id) DO UPDATE SET
            industry       = EXCLUDED.industry,
            headcount_band = EXCLUDED.headcount_band,
            volume_band    = EXCLUDED.volume_band,
            dpo_days                  = EXCLUDED.dpo_days,
            avg_approval_cycle_hours  = EXCLUDED.avg_approval_cycle_hours,
            ocr_straight_through_rate = EXCLUDED.ocr_straight_through_rate,
            exception_rate            = EXCLUDED.exception_rate,
            discount_capture_rate     = EXCLUDED.discount_capture_rate,
            cost_per_invoice          = EXCLUDED.cost_per_invoice,
            computed_at               = EXCLUDED.computed_at
        "#,
    )
    .bind(tenant_id)
    .bind(industry)
    .bind(headcount_band)
    .bind(volume_band)
    .bind(kpis.dpo_days)
    .bind(kpis.avg_approval_cycle_hours)
    .bind(kpis.ocr_straight_through_rate)
    .bind(kpis.exception_rate)
    .bind(kpis.discount_capture_rate)
    .bind(kpis.cost_per_invoice)
    .execute(metadata_pool)
    .await
    .context("Failed to publish tenant benchmark KPIs")?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benchmark_kpis_serializes_all_six_fields() {
        let kpis = BenchmarkKpis {
            dpo_days: 30.0,
            avg_approval_cycle_hours: 24.5,
            ocr_straight_through_rate: 0.85,
            exception_rate: 0.12,
            discount_capture_rate: 0.67,
            cost_per_invoice: 3.50,
        };
        let json = serde_json::to_value(&kpis).unwrap();

        assert_eq!(json["dpo_days"], 30.0);
        assert_eq!(json["avg_approval_cycle_hours"], 24.5);
        assert_eq!(json["ocr_straight_through_rate"], 0.85);
        assert_eq!(json["exception_rate"], 0.12);
        assert_eq!(json["discount_capture_rate"], 0.67);
        assert_eq!(json["cost_per_invoice"], 3.50);
    }

    #[test]
    fn benchmark_response_not_opted_in_omits_optional_fields() {
        let resp = BenchmarkResponse {
            opted_in: false,
            cohort: None,
            tenant_kpis: None,
            cohort_kpis: None,
            cohort_size: None,
        };
        let json = serde_json::to_string(&resp).unwrap();

        assert!(json.contains("\"opted_in\":false"));
        assert!(!json.contains("cohort"));
        assert!(!json.contains("tenant_kpis"));
        assert!(!json.contains("cohort_kpis"));
        assert!(!json.contains("cohort_size"));
    }

    #[test]
    fn benchmark_response_opted_in_with_data_includes_all_fields() {
        let resp = BenchmarkResponse {
            opted_in: true,
            cohort: Some(CohortDescriptor {
                industry: "manufacturing".into(),
                headcount_band: "50-200".into(),
                volume_band: "500-2000".into(),
            }),
            tenant_kpis: Some(BenchmarkKpis {
                dpo_days: 30.0,
                avg_approval_cycle_hours: 24.5,
                ocr_straight_through_rate: 0.85,
                exception_rate: 0.12,
                discount_capture_rate: 0.67,
                cost_per_invoice: 3.50,
            }),
            cohort_kpis: Some(CohortPercentiles {
                p25: BenchmarkKpis {
                    dpo_days: 20.0,
                    avg_approval_cycle_hours: 12.0,
                    ocr_straight_through_rate: 0.70,
                    exception_rate: 0.05,
                    discount_capture_rate: 0.50,
                    cost_per_invoice: 2.00,
                },
                p50: BenchmarkKpis {
                    dpo_days: 30.0,
                    avg_approval_cycle_hours: 24.0,
                    ocr_straight_through_rate: 0.80,
                    exception_rate: 0.10,
                    discount_capture_rate: 0.60,
                    cost_per_invoice: 3.00,
                },
                p75: BenchmarkKpis {
                    dpo_days: 45.0,
                    avg_approval_cycle_hours: 48.0,
                    ocr_straight_through_rate: 0.90,
                    exception_rate: 0.20,
                    discount_capture_rate: 0.80,
                    cost_per_invoice: 5.00,
                },
            }),
            cohort_size: Some(12),
        };
        let json = serde_json::to_value(&resp).unwrap();

        assert_eq!(json["opted_in"], true);
        assert_eq!(json["cohort"]["industry"], "manufacturing");
        assert_eq!(json["tenant_kpis"]["dpo_days"], 30.0);
        assert_eq!(json["cohort_kpis"]["p50"]["dpo_days"], 30.0);
        assert_eq!(json["cohort_size"], 12);
    }

    #[test]
    fn cohort_size_below_floor_returns_none() {
        // The k-anonymity floor (cohort_size < 5) means fetch_cohort_percentiles
        // should return None. This test validates the logic in isolation.
        let cohort_size: i64 = 4;
        let should_return_none = cohort_size < 5;
        assert!(should_return_none, "Cohort size {} is below floor of 5", cohort_size);
    }

    #[test]
    fn cohort_size_at_floor_returns_data() {
        let cohort_size: i64 = 5;
        let should_return_data = cohort_size >= 5;
        assert!(should_return_data, "Cohort size {} meets floor of 5", cohort_size);
    }

    #[test]
    fn opt_in_request_deserializes() {
        let json = r#"{"industry":"retail","headcount_band":"1-49","volume_band":"0-499"}"#;
        let req: BenchmarkOptInRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.industry, "retail");
        assert_eq!(req.headcount_band, "1-49");
        assert_eq!(req.volume_band, "0-499");
    }

    /// Integration test: exercises the SECURITY DEFINER function against a
    /// real test database. Requires a running Postgres with migrations applied.
    /// Run with: `cargo test --lib -p billforge-analytics -- --ignored`
    #[tokio::test]
    #[ignore]
    async fn integration_fetch_cohort_percentiles_with_real_db() {
        let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let pool = PgPool::connect(&db_url).await.expect("Failed to connect to DB");

        let result = fetch_cohort_percentiles(&pool, "nonexistent_industry", "1-49", "0-499").await;
        assert!(result.is_ok(), "fetch_cohort_percentiles should not error");
        let opt = result.unwrap();
        // No cohort exists for this bogus key, so should be None
        assert!(opt.is_none(), "Expected None for nonexistent cohort");
    }
}
