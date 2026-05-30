//! Early-payment discount optimizer
//!
//! Parses vendor payment terms (e.g., "2/10 net 30"), scores each invoice's
//! discount opportunity against a tenant-configured cost-of-capital, surfaces
//! a daily capture worklist with deadline and net savings, and allows one-click
//! capture that schedules an early payment via the existing payment_requests
//! batching. Tracks captured vs. missed discounts as a KPI.

use crate::error::ApiResult;
use crate::extractors::{AuthUser, TenantCtx};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Payment-terms parser
// ---------------------------------------------------------------------------

/// Parse a payment-terms string like "2/10 net 30", "1.5/15 Net 45",
/// "2/10 N30", or "Net 30".
///
/// Returns `(discount_percent, discount_days, net_days)` when a discount
/// term is present, or `None` when no discount is offered.
pub fn parse_payment_terms(s: &str) -> Option<(f64, i32, i32)> {
    let normalized = s.to_lowercase().replace(',', " ");
    let re = regex::Regex::new(r"(\d+(?:\.\d+)?)\s*/\s*(\d+)\s+(?:net|n)\s*(\d+)").ok()?;
    let caps = re.captures(&normalized)?;
    let discount_pct: f64 = caps.get(1)?.as_str().parse().ok()?;
    let discount_days: i32 = caps.get(2)?.as_str().parse().ok()?;
    let net_days: i32 = caps.get(3)?.as_str().parse().ok()?;
    if discount_pct <= 0.0 || discount_days <= 0 || net_days <= 0 {
        return None;
    }
    Some((discount_pct, discount_days, net_days))
}

// ---------------------------------------------------------------------------
// Discount scorer
// ---------------------------------------------------------------------------

/// Scored discount opportunity for a single invoice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscountScore {
    pub net_savings_cents: i64,
    pub effective_apr_bps: i64,
    pub deadline: NaiveDate,
    pub recommended: bool,
}

/// Compute the discount opportunity for an invoice.
///
/// * `invoice_amount_cents` - total invoice amount
/// * `discount_pct` - early-pay discount percentage (e.g. 2.0 for 2%)
/// * `discount_days` - days within which to pay for the discount
/// * `net_days` - full payment term (e.g. 30)
/// * `invoice_date` - date on the invoice
/// * `cost_of_capital_bps` - tenant's cost of capital in basis points
pub fn score_discount(
    invoice_amount_cents: i64,
    discount_pct: f64,
    discount_days: i32,
    net_days: i32,
    invoice_date: NaiveDate,
    cost_of_capital_bps: i64,
) -> Option<DiscountScore> {
    if discount_pct <= 0.0 || discount_days <= 0 || net_days <= discount_days {
        return None;
    }
    let discount_amount_cents =
        ((invoice_amount_cents as f64) * discount_pct / 100.0).round() as i64;
    if discount_amount_cents <= 0 {
        return None;
    }

    let days_early = net_days - discount_days;
    if days_early <= 0 {
        return None;
    }

    // Effective annualised APR of taking the discount (in bps):
    //   APR = (discount_pct / (100 - discount_pct)) * (365 / days_early) * 10000
    let effective_apr_bps =
        ((discount_pct / (100.0 - discount_pct)) * (365.0 / days_early as f64) * 10000.0).round()
            as i64;

    // Net savings = discount minus cost of using cash `days_early` sooner
    let cost_of_early_payment_cents =
        ((invoice_amount_cents as f64) * (cost_of_capital_bps as f64) / 10000.0 / 365.0
            * (days_early as f64))
            .round() as i64;
    let net_savings_cents = discount_amount_cents - cost_of_early_payment_cents;

    let deadline = invoice_date + chrono::Duration::days(discount_days as i64);
    let recommended = effective_apr_bps > cost_of_capital_bps;

    Some(DiscountScore {
        net_savings_cents,
        effective_apr_bps,
        deadline,
        recommended,
    })
}

// ---------------------------------------------------------------------------
// Route types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
struct WorklistRow {
    invoice_id: Uuid,
    vendor_name: String,
    invoice_number: String,
    amount_cents: i64,
    currency: String,
    discount_percent: f64,
    discount_days: i32,
    net_days: i32,
    discount_deadline: NaiveDate,
    days_remaining: i64,
    net_savings_cents: i64,
    effective_apr_bps: i64,
    recommended: bool,
}

#[derive(Debug, Serialize)]
struct WorklistResponse {
    total_potential_savings_cents: i64,
    count_recommended: i64,
    items: Vec<WorklistRow>,
}

#[derive(Debug, Serialize)]
struct KpiResponse {
    captured_count_30d: i64,
    captured_savings_cents_30d: i64,
    missed_count_30d: i64,
    missed_savings_cents_30d: i64,
    capture_rate_pct: f64,
    captured_count_90d: i64,
    captured_savings_cents_90d: i64,
    missed_count_90d: i64,
    missed_savings_cents_90d: i64,
}

#[derive(Debug, Serialize)]
struct CaptureResponse {
    payment_request_id: Uuid,
    invoice_id: Uuid,
    discounted_amount_cents: i64,
}

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/worklist", get(get_worklist))
        .route("/:invoice_id/capture", post(capture_discount))
        .route("/kpi", get(get_kpi))
}

/// GET /api/v1/discounts/worklist
async fn get_worklist(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    _user: AuthUser,
) -> ApiResult<Json<WorklistResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Lazy sweep: mark expired discounts as missed
    sqlx::query(
        r#"UPDATE invoices
           SET discount_missed_at = NOW(), updated_at = NOW()
           WHERE tenant_id = $1
             AND discount_percent IS NOT NULL
             AND discount_captured_at IS NULL
             AND discount_missed_at IS NULL
             AND discount_deadline < CURRENT_DATE"#,
    )
    .bind(*tenant.tenant_id.as_uuid())
    .execute(&*pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to mark missed discounts: {}", e))
    })?;

    // Get tenant cost-of-capital (default 800 bps = 8%)
    let cost_of_capital_bps: i64 = sqlx::query_scalar::<_, i32>(
        r#"SELECT cost_of_capital_bps FROM tenant_discount_settings WHERE tenant_id = $1"#,
    )
    .bind(*tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to get cost of capital: {}", e)))?
    .unwrap_or(800) as i64;

    // Fetch discount-eligible invoices
    let rows = sqlx::query_as::<_, WorklistQueryRow>(
        r#"SELECT i.id, i.vendor_name, i.invoice_number, i.total_amount_cents,
                  i.currency,
                  CAST(i.discount_percent AS DOUBLE PRECISION) AS discount_pct,
                  i.discount_days,
                  i.discount_deadline, i.invoice_date,
                  v.payment_terms,
                  COALESCE(v.payment_terms_days, 30) AS net_days
           FROM invoices i
           LEFT JOIN vendors v ON v.id = i.vendor_id
           WHERE i.tenant_id = $1
             AND i.discount_percent IS NOT NULL
             AND i.discount_captured_at IS NULL
             AND i.discount_missed_at IS NULL
             AND i.discount_deadline >= CURRENT_DATE
           ORDER BY i.discount_deadline ASC"#,
    )
    .bind(*tenant.tenant_id.as_uuid())
    .fetch_all(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to fetch worklist: {}", e)))?;

    let today = chrono::Local::now().date_naive();
    let mut items = Vec::new();
    let mut total_savings: i64 = 0;
    let mut count_recommended: i64 = 0;

    for row in rows {
        let days_remaining = (row.discount_deadline - today).num_days();
        let discount_pct = row.discount_pct.unwrap_or(0.0);
        let discount_days = row.discount_days.unwrap_or(0);
        let net_days = row.net_days.unwrap_or(30);
        let invoice_date = row.invoice_date.unwrap_or(today);

        let score = score_discount(
            row.total_amount_cents,
            discount_pct,
            discount_days,
            net_days,
            invoice_date,
            cost_of_capital_bps,
        );

        if let Some(s) = score {
            total_savings += s.net_savings_cents;
            if s.recommended {
                count_recommended += 1;
            }
            items.push(WorklistRow {
                invoice_id: row.id,
                vendor_name: row.vendor_name,
                invoice_number: row.invoice_number,
                amount_cents: row.total_amount_cents,
                currency: row.currency,
                discount_percent: discount_pct,
                discount_days,
                net_days,
                discount_deadline: row.discount_deadline,
                days_remaining,
                net_savings_cents: s.net_savings_cents,
                effective_apr_bps: s.effective_apr_bps,
                recommended: s.recommended,
            });
        }
    }

    Ok(Json(WorklistResponse {
        total_potential_savings_cents: total_savings,
        count_recommended,
        items,
    }))
}

/// POST /api/v1/discounts/{invoice_id}/capture
async fn capture_discount(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    AuthUser(user): AuthUser,
    Path(invoice_id): Path<Uuid>,
) -> ApiResult<Json<CaptureResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let invoice = sqlx::query_as::<_, CaptureInvoiceRow>(
        r#"SELECT id, vendor_id, vendor_name, invoice_number, total_amount_cents,
                  currency,
                  CAST(discount_percent AS DOUBLE PRECISION) AS discount_pct,
                  discount_deadline, discount_captured_at, discount_missed_at
           FROM invoices
           WHERE id = $1 AND tenant_id = $2"#,
    )
    .bind(invoice_id)
    .bind(*tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to fetch invoice: {}", e)))?
    .ok_or_else(|| billforge_core::Error::NotFound {
        resource_type: "Invoice".to_string(),
        id: invoice_id.to_string(),
    })?;

    if invoice.discount_captured_at.is_some() {
        return Err(
            billforge_core::Error::Validation("Discount already captured".to_string()).into(),
        );
    }
    if invoice.discount_missed_at.is_some() {
        return Err(
            billforge_core::Error::Validation("Discount deadline has passed".to_string()).into(),
        );
    }
    if invoice.discount_pct.is_none() {
        return Err(billforge_core::Error::Validation(
            "Invoice has no early-payment discount".to_string(),
        )
        .into());
    }

    let discount_pct = invoice.discount_pct.unwrap();
    let discounted_amount_cents =
        ((invoice.total_amount_cents as f64) * (100.0 - discount_pct) / 100.0).round() as i64;

    let now = chrono::Utc::now();

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| billforge_core::Error::Database(format!("Failed to begin tx: {}", e)))?;

    // Mark invoice as captured
    sqlx::query(
        r#"UPDATE invoices
           SET discount_captured_at = $1, updated_at = $1
           WHERE id = $2 AND tenant_id = $3"#,
    )
    .bind(now)
    .bind(invoice_id)
    .bind(*tenant.tenant_id.as_uuid())
    .execute(&mut *tx)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to mark captured: {}", e)))?;

    // Create payment request
    let pr_id = Uuid::new_v4();
    let request_number = generate_pr_number(&now);

    sqlx::query(
        r#"INSERT INTO payment_requests
            (id, tenant_id, request_number, status, vendor_id,
             total_amount_cents, currency, invoice_count,
             earliest_due_date, latest_due_date, notes,
             created_by, created_at, updated_at)
           VALUES ($1, $2, $3, 'draft', $4, $5, $6, 1, CURRENT_DATE, CURRENT_DATE,
                   $7, $8, $9, $9)"#,
    )
    .bind(pr_id)
    .bind(*tenant.tenant_id.as_uuid())
    .bind(&request_number)
    .bind(invoice.vendor_id)
    .bind(discounted_amount_cents)
    .bind(&invoice.currency)
    .bind(format!(
        "Early-payment discount capture for invoice {} ({:.1}% discount)",
        invoice.invoice_number, discount_pct
    ))
    .bind(*user.user_id.as_uuid())
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to create payment request: {}", e))
    })?;

    // Create payment request item
    let item_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO payment_request_items
            (id, payment_request_id, invoice_id, amount_cents, currency, created_at)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(item_id)
    .bind(pr_id)
    .bind(invoice_id)
    .bind(discounted_amount_cents)
    .bind(&invoice.currency)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to create payment request item: {}", e))
    })?;

    tx.commit()
        .await
        .map_err(|e| billforge_core::Error::Database(format!("Failed to commit capture: {}", e)))?;

    Ok(Json(CaptureResponse {
        payment_request_id: pr_id,
        invoice_id,
        discounted_amount_cents,
    }))
}

/// GET /api/v1/discounts/kpi
async fn get_kpi(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    _user: AuthUser,
) -> ApiResult<Json<KpiResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let stats_30d = fetch_kpi_stats(&pool, *tenant.tenant_id.as_uuid(), "30 days").await?;
    let stats_90d = fetch_kpi_stats(&pool, *tenant.tenant_id.as_uuid(), "90 days").await?;

    let total_30d = stats_30d.captured_count + stats_30d.missed_count;
    let capture_rate = if total_30d > 0 {
        (stats_30d.captured_count as f64 / total_30d as f64) * 100.0
    } else {
        0.0
    };

    Ok(Json(KpiResponse {
        captured_count_30d: stats_30d.captured_count,
        captured_savings_cents_30d: stats_30d.captured_savings_cents,
        missed_count_30d: stats_30d.missed_count,
        missed_savings_cents_30d: stats_30d.missed_savings_cents,
        capture_rate_pct: (capture_rate * 10.0).round() / 10.0,
        captured_count_90d: stats_90d.captured_count,
        captured_savings_cents_90d: stats_90d.captured_savings_cents,
        missed_count_90d: stats_90d.missed_count,
        missed_savings_cents_90d: stats_90d.missed_savings_cents,
    }))
}

async fn fetch_kpi_stats(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    interval: &str,
) -> Result<KpiStatRow, billforge_core::Error> {
    let sql = format!(
        r#"SELECT
             COUNT(*) FILTER (WHERE discount_captured_at >= NOW() - INTERVAL '{interval}') AS captured_count,
             COALESCE(SUM(
               CASE WHEN discount_captured_at >= NOW() - INTERVAL '{interval}'
                    THEN ROUND(total_amount_cents * discount_percent / 100.0)
                    ELSE 0 END
             ), 0) AS captured_savings_cents,
             COUNT(*) FILTER (WHERE discount_missed_at >= NOW() - INTERVAL '{interval}') AS missed_count,
             COALESCE(SUM(
               CASE WHEN discount_missed_at >= NOW() - INTERVAL '{interval}'
                    THEN ROUND(total_amount_cents * discount_percent / 100.0)
                    ELSE 0 END
             ), 0) AS missed_savings_cents
           FROM invoices
           WHERE tenant_id = $1
             AND (discount_captured_at IS NOT NULL OR discount_missed_at IS NOT NULL)"#,
        interval = interval
    );

    sqlx::query_as::<_, KpiStatRow>(&sql)
        .bind(tenant_id)
        .fetch_one(pool)
        .await
        .map_err(|e| billforge_core::Error::Database(format!("Failed to fetch KPI stats: {}", e)))
}

// ---------------------------------------------------------------------------
// Populate discount columns on invoice insert
// ---------------------------------------------------------------------------

/// After inserting an invoice, look up the vendor's payment_terms, parse them,
/// and populate the discount columns on the invoice.
pub async fn populate_discount_columns(
    pool: &sqlx::PgPool,
    tenant_id: &Uuid,
    invoice_id: &Uuid,
    vendor_id: Option<&Uuid>,
    invoice_date: Option<&NaiveDate>,
) -> Result<(), billforge_core::Error> {
    let vendor_id = match vendor_id {
        Some(id) => *id,
        None => return Ok(()),
    };
    let invoice_date = match invoice_date {
        Some(d) => *d,
        None => return Ok(()),
    };

    let payment_terms: Option<String> =
        sqlx::query_scalar("SELECT payment_terms FROM vendors WHERE id = $1 AND tenant_id = $2")
            .bind(vendor_id)
            .bind(tenant_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| {
                billforge_core::Error::Database(format!("Failed to fetch vendor terms: {}", e))
            })?
            .flatten();

    let terms_str = match payment_terms {
        Some(t) => t,
        None => return Ok(()),
    };

    let parsed = match parse_payment_terms(&terms_str) {
        Some(p) => p,
        None => return Ok(()),
    };

    let (discount_pct, discount_days, _net_days) = parsed;
    let deadline = invoice_date + chrono::Duration::days(discount_days as i64);

    sqlx::query(
        r#"UPDATE invoices
           SET discount_percent = $1, discount_days = $2, discount_deadline = $3, updated_at = NOW()
           WHERE id = $4 AND tenant_id = $5"#,
    )
    .bind(discount_pct)
    .bind(discount_days)
    .bind(deadline)
    .bind(invoice_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to populate discount cols: {}", e))
    })?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn generate_pr_number(now: &chrono::DateTime<chrono::Utc>) -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(1);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("DSC-{}-{:04}", now.format("%Y%m%d"), seq)
}

// ---------------------------------------------------------------------------
// Row types
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct WorklistQueryRow {
    id: Uuid,
    vendor_name: String,
    invoice_number: String,
    total_amount_cents: i64,
    currency: String,
    discount_pct: Option<f64>,
    discount_days: Option<i32>,
    discount_deadline: NaiveDate,
    invoice_date: Option<NaiveDate>,
    payment_terms: Option<String>,
    net_days: Option<i32>,
}

#[derive(sqlx::FromRow)]
struct CaptureInvoiceRow {
    id: Uuid,
    vendor_id: Option<Uuid>,
    vendor_name: String,
    invoice_number: String,
    total_amount_cents: i64,
    currency: String,
    discount_pct: Option<f64>,
    discount_deadline: Option<NaiveDate>,
    discount_captured_at: Option<chrono::DateTime<chrono::Utc>>,
    discount_missed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(sqlx::FromRow)]
struct KpiStatRow {
    captured_count: i64,
    captured_savings_cents: i64,
    missed_count: i64,
    missed_savings_cents: i64,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    // --- parse_payment_terms ---

    #[test]
    fn parse_standard_terms() {
        let result = parse_payment_terms("2/10 net 30");
        assert!(result.is_some());
        let (pct, days, net) = result.unwrap();
        assert!((pct - 2.0).abs() < 0.01);
        assert_eq!(days, 10);
        assert_eq!(net, 30);
    }

    #[test]
    fn parse_decimal_discount() {
        let result = parse_payment_terms("1.5/15 Net 45");
        assert!(result.is_some());
        let (pct, days, net) = result.unwrap();
        assert!((pct - 1.5).abs() < 0.01);
        assert_eq!(days, 15);
        assert_eq!(net, 45);
    }

    #[test]
    fn parse_shorthand_n() {
        let result = parse_payment_terms("2/10 N30");
        assert!(result.is_some());
        let (pct, days, net) = result.unwrap();
        assert!((pct - 2.0).abs() < 0.01);
        assert_eq!(days, 10);
        assert_eq!(net, 30);
    }

    #[test]
    fn parse_case_insensitive() {
        assert!(parse_payment_terms("2/10 NET 30").is_some());
    }

    #[test]
    fn parse_no_discount_returns_none() {
        assert!(parse_payment_terms("Net 30").is_none());
        assert!(parse_payment_terms("Due on receipt").is_none());
        assert!(parse_payment_terms("").is_none());
    }

    #[test]
    fn parse_garbage_returns_none() {
        assert!(parse_payment_terms("some random text 123").is_none());
        assert!(parse_payment_terms("COD").is_none());
    }

    #[test]
    fn parse_with_extra_whitespace() {
        let result = parse_payment_terms("2 / 10  net  30");
        assert!(result.is_some());
        let (pct, _, net) = result.unwrap();
        assert!((pct - 2.0).abs() < 0.01);
        assert_eq!(net, 30);
    }

    // --- score_discount ---

    #[test]
    fn score_basic_discount() {
        // $10,000 invoice, 2/10 net 30, cost of capital 800 bps (8%)
        let score = score_discount(
            1_000_000, // $10,000 in cents
            2.0,
            10,
            30,
            date(2026, 1, 1),
            800,
        )
        .unwrap();

        // Discount amount = $200 (20000 cents)
        // Days early = 20
        // Cost of early payment = 1000000 * 800/10000 / 365 * 20 = ~4383 cents
        // Net savings ≈ 20000 - 4383 = ~15617
        assert!(score.net_savings_cents > 0);
        assert!(score.effective_apr_bps > 800); // should be recommended
        assert!(score.recommended);
        assert_eq!(score.deadline, date(2026, 1, 11)); // Jan 1 + 10 days
    }

    #[test]
    fn score_low_discount_not_recommended() {
        // $100 invoice, 0.1/5 net 60, cost of capital 800 bps
        // Very low discount relative to cost of capital
        let score = score_discount(
            10_000, // $100
            0.1,
            5,
            60,
            date(2026, 1, 1),
            800,
        );

        // With such a tiny discount (0.1%) the effective APR is still very high
        // because the annualization formula amplifies it.
        // Actually let's check: (0.1/99.9)*(365/55)*10000 = ~66 bps
        // That's below 800 bps cost of capital, so not recommended
        if let Some(s) = score {
            assert!(!s.recommended);
        }
        // If score is None that's also acceptable (net_savings <= 0)
    }

    #[test]
    fn score_zero_discount_returns_none() {
        let result = score_discount(100_000, 0.0, 10, 30, date(2026, 1, 1), 800);
        assert!(result.is_none());
    }

    #[test]
    fn score_discount_days_equal_net_returns_none() {
        // If discount_days == net_days, there's no "early" payment
        let result = score_discount(100_000, 2.0, 30, 30, date(2026, 1, 1), 800);
        assert!(result.is_none());
    }

    #[test]
    fn score_negative_discount_returns_none() {
        let result = score_discount(100_000, -1.0, 10, 30, date(2026, 1, 1), 800);
        assert!(result.is_none());
    }

    #[test]
    fn score_deadline_computed_from_invoice_date() {
        let score = score_discount(100_000, 2.0, 10, 30, date(2026, 6, 15), 800).unwrap();
        assert_eq!(score.deadline, date(2026, 6, 25)); // June 15 + 10 days
    }

    #[test]
    fn score_high_cost_of_capital_reduces_savings() {
        let score_low_coc = score_discount(1_000_000, 2.0, 10, 30, date(2026, 1, 1), 200).unwrap();
        let score_high_coc =
            score_discount(1_000_000, 2.0, 10, 30, date(2026, 1, 1), 2000).unwrap();
        // Higher cost of capital means lower net savings
        assert!(score_low_coc.net_savings_cents > score_high_coc.net_savings_cents);
    }

    #[test]
    fn score_effective_apr_2_10_net_30() {
        // 2/10 net 30: (2/98) * (365/20) * 10000 = ~3724 bps = ~37.2%
        let score = score_discount(100_000, 2.0, 10, 30, date(2026, 1, 1), 800).unwrap();
        assert!(score.effective_apr_bps > 3700);
        assert!(score.effective_apr_bps < 3800);
    }
}
