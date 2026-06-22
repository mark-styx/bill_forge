//! Natural-language AP query console routes.

use crate::error::{ApiError, ApiResult};
use crate::extractors::ReportingAccess;
use crate::state::AppState;
use axum::{extract::State, routing::post, Json, Router};
use billforge_core::Error;
use chrono::{Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use utoipa::ToSchema;
use uuid::Uuid;

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;

pub fn routes() -> Router<AppState> {
    Router::new().route("/nl-query", post(run_natural_language_query))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct NaturalLanguageQueryRequest {
    pub question: String,
    pub limit: Option<i64>,
    #[serde(default)]
    pub save_as_view: bool,
    pub view_name: Option<String>,
    pub schedule: Option<ScheduleRequest>,
    pub alert: Option<AlertRequest>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ScheduleRequest {
    pub schedule: String,
    #[serde(default)]
    pub recipients: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AlertRequest {
    pub condition: Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NaturalLanguageQueryResponse {
    pub query_kind: String,
    pub normalized_question: String,
    pub explanation: String,
    pub filters: Value,
    pub columns: Vec<String>,
    pub rows: Vec<Value>,
    pub saved_view_id: Option<Uuid>,
    pub schedule_id: Option<Uuid>,
    pub alert_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum QueryKind {
    Invoices,
    TopVendors,
}

impl QueryKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Invoices => "invoices",
            Self::TopVendors => "top_vendors",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedQuery {
    kind: QueryKind,
    normalized_question: String,
    explanation: String,
    limit: i64,
    min_amount_cents: Option<i64>,
    processing_status: Option<String>,
    pending_more_than_days: Option<i32>,
    start_date: Option<NaiveDate>,
    end_date: Option<NaiveDate>,
    exclude_utilities: bool,
    filters: Value,
    columns: Vec<String>,
}

async fn run_natural_language_query(
    State(state): State<AppState>,
    ReportingAccess(user, tenant): ReportingAccess,
    Json(request): Json<NaturalLanguageQueryRequest>,
) -> ApiResult<Json<NaturalLanguageQueryResponse>> {
    let parsed = parse_nl_query(&request.question, request.limit).map_err(ApiError::from)?;
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let pool_ref = pool.as_ref();
    let tenant_id = *tenant.tenant_id.as_uuid();

    let rows = match parsed.kind {
        QueryKind::Invoices => query_invoices(pool_ref, tenant_id, &parsed).await?,
        QueryKind::TopVendors => query_top_vendors(pool_ref, tenant_id, &parsed).await?,
    };

    let needs_view = request.save_as_view || request.schedule.is_some() || request.alert.is_some();
    let mut saved_view_id = None;
    let mut schedule_id = None;
    let mut alert_id = None;

    if needs_view {
        let name = request
            .view_name
            .clone()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| default_view_name(&request.question));
        let columns = json!(parsed.columns);
        let saved_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO nl_query_saved_views
                (tenant_id, user_id, name, question, query_kind, filters, columns)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id
            "#,
        )
        .bind(tenant_id)
        .bind(*user.user_id.as_uuid())
        .bind(name)
        .bind(&request.question)
        .bind(parsed.kind.as_str())
        .bind(&parsed.filters)
        .bind(&columns)
        .fetch_one(pool_ref)
        .await
        .map_err(|e| {
            Error::Database(format!("Failed to save natural-language query view: {}", e))
        })?;
        saved_view_id = Some(saved_id);

        if let Some(schedule) = &request.schedule {
            let id: Uuid = sqlx::query_scalar(
                r#"
                INSERT INTO nl_query_schedules
                    (tenant_id, saved_view_id, schedule, recipients)
                VALUES ($1, $2, $3, $4)
                RETURNING id
                "#,
            )
            .bind(tenant_id)
            .bind(saved_id)
            .bind(&schedule.schedule)
            .bind(json!(schedule.recipients))
            .fetch_one(pool_ref)
            .await
            .map_err(|e| {
                Error::Database(format!(
                    "Failed to schedule natural-language query view: {}",
                    e
                ))
            })?;
            schedule_id = Some(id);
        }

        if let Some(alert) = &request.alert {
            let id: Uuid = sqlx::query_scalar(
                r#"
                INSERT INTO nl_query_alerts
                    (tenant_id, saved_view_id, condition)
                VALUES ($1, $2, $3)
                RETURNING id
                "#,
            )
            .bind(tenant_id)
            .bind(saved_id)
            .bind(&alert.condition)
            .fetch_one(pool_ref)
            .await
            .map_err(|e| {
                Error::Database(format!(
                    "Failed to create natural-language query alert: {}",
                    e
                ))
            })?;
            alert_id = Some(id);
        }
    }

    Ok(Json(NaturalLanguageQueryResponse {
        query_kind: parsed.kind.as_str().to_string(),
        normalized_question: parsed.normalized_question,
        explanation: parsed.explanation,
        filters: parsed.filters,
        columns: parsed.columns,
        rows,
        saved_view_id,
        schedule_id,
        alert_id,
    }))
}

async fn query_invoices(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    parsed: &ParsedQuery,
) -> Result<Vec<Value>, Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            i.id,
            i.invoice_number,
            i.vendor_name,
            i.total_amount_cents,
            i.currency,
            i.processing_status,
            i.capture_status,
            i.invoice_date,
            i.due_date,
            i.created_at,
            ar.created_at AS approval_requested_at,
            EXTRACT(DAY FROM (NOW() - COALESCE(ar.created_at, i.created_at)))::int AS age_days
        FROM invoices i
        LEFT JOIN LATERAL (
            SELECT created_at
            FROM approval_requests ar
            WHERE ar.tenant_id = i.tenant_id
              AND ar.invoice_id = i.id
              AND ar.status = 'pending'
            ORDER BY ar.created_at ASC
            LIMIT 1
        ) ar ON TRUE
        WHERE i.tenant_id = $1
          AND ($2::bigint IS NULL OR i.total_amount_cents >= $2)
          AND ($3::text IS NULL OR i.processing_status = $3)
          AND ($4::int IS NULL OR ar.created_at <= NOW() - make_interval(days => $4))
        ORDER BY i.total_amount_cents DESC, i.created_at DESC
        LIMIT $5
        "#,
    )
    .bind(tenant_id)
    .bind(parsed.min_amount_cents)
    .bind(parsed.processing_status.as_deref())
    .bind(parsed.pending_more_than_days)
    .bind(parsed.limit)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        Error::Database(format!(
            "Failed to run natural-language invoice query: {}",
            e
        ))
    })?;

    Ok(rows
        .into_iter()
        .map(|row| {
            json!({
                "id": row.get::<Uuid, _>("id"),
                "invoice_number": row.get::<String, _>("invoice_number"),
                "vendor_name": row.get::<String, _>("vendor_name"),
                "total_amount_cents": row.get::<i64, _>("total_amount_cents"),
                "currency": row.get::<String, _>("currency"),
                "processing_status": row.get::<String, _>("processing_status"),
                "capture_status": row.get::<String, _>("capture_status"),
                "invoice_date": row.get::<Option<NaiveDate>, _>("invoice_date"),
                "due_date": row.get::<Option<NaiveDate>, _>("due_date"),
                "created_at": row.get::<chrono::DateTime<Utc>, _>("created_at"),
                "approval_requested_at": row.get::<Option<chrono::DateTime<Utc>>, _>("approval_requested_at"),
                "age_days": row.get::<Option<i32>, _>("age_days"),
            })
        })
        .collect())
}

async fn query_top_vendors(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    parsed: &ParsedQuery,
) -> Result<Vec<Value>, Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            i.vendor_id,
            COALESCE(i.vendor_name, 'Unknown') AS vendor_name,
            COUNT(*)::bigint AS invoice_count,
            COALESCE(SUM(i.total_amount_cents), 0)::bigint AS total_spend_cents
        FROM invoices i
        WHERE i.tenant_id = $1
          AND ($2::date IS NULL OR i.invoice_date >= $2)
          AND ($3::date IS NULL OR i.invoice_date <= $3)
          AND (
              $4::bool = false
              OR (
                  i.vendor_name NOT ILIKE '%utilit%'
                  AND COALESCE(i.gl_code, '') NOT ILIKE '%utilit%'
                  AND COALESCE(i.cost_center, '') NOT ILIKE '%utilit%'
              )
          )
        GROUP BY i.vendor_id, COALESCE(i.vendor_name, 'Unknown')
        ORDER BY total_spend_cents DESC
        LIMIT $5
        "#,
    )
    .bind(tenant_id)
    .bind(parsed.start_date)
    .bind(parsed.end_date)
    .bind(parsed.exclude_utilities)
    .bind(parsed.limit)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        Error::Database(format!(
            "Failed to run natural-language vendor spend query: {}",
            e
        ))
    })?;

    Ok(rows
        .into_iter()
        .map(|row| {
            json!({
                "vendor_id": row.get::<Option<Uuid>, _>("vendor_id"),
                "vendor_name": row.get::<String, _>("vendor_name"),
                "invoice_count": row.get::<i64, _>("invoice_count"),
                "total_spend_cents": row.get::<i64, _>("total_spend_cents"),
            })
        })
        .collect())
}

fn parse_nl_query(question: &str, requested_limit: Option<i64>) -> Result<ParsedQuery, Error> {
    let trimmed = question.trim();
    if trimmed.is_empty() {
        return Err(Error::Validation("Question must not be empty".to_string()));
    }

    let normalized = trimmed.to_lowercase();
    let limit = requested_limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);

    if normalized.contains("top") && normalized.contains("vendor") && normalized.contains("spend") {
        parse_top_vendors_query(trimmed, &normalized, requested_limit.unwrap_or(limit))
    } else if normalized.contains("invoice") || normalized.contains("approval") {
        parse_invoice_query(trimmed, &normalized, limit)
    } else {
        Err(Error::Validation(
            "Unsupported AP query. Try invoice approval or top vendor spend questions.".to_string(),
        ))
    }
}

fn parse_invoice_query(question: &str, normalized: &str, limit: i64) -> Result<ParsedQuery, Error> {
    let min_amount_cents = parse_amount_cents(normalized);
    let processing_status = if normalized.contains("pending approval") {
        Some("pending_approval".to_string())
    } else if normalized.contains("approved") {
        Some("approved".to_string())
    } else if normalized.contains("ready for payment") {
        Some("ready_for_payment".to_string())
    } else if normalized.contains("submitted") {
        Some("submitted".to_string())
    } else {
        None
    };
    let pending_more_than_days = parse_more_than_days(normalized);

    let filters = json!({
        "min_amount_cents": min_amount_cents,
        "processing_status": processing_status,
        "pending_more_than_days": pending_more_than_days,
        "limit": limit,
    });

    Ok(ParsedQuery {
        kind: QueryKind::Invoices,
        normalized_question: question.trim().to_string(),
        explanation:
            "Translated to tenant-scoped invoice filters with approval age and amount predicates."
                .to_string(),
        limit,
        min_amount_cents,
        processing_status,
        pending_more_than_days,
        start_date: None,
        end_date: None,
        exclude_utilities: false,
        filters,
        columns: vec![
            "id",
            "invoice_number",
            "vendor_name",
            "total_amount_cents",
            "currency",
            "processing_status",
            "capture_status",
            "invoice_date",
            "due_date",
            "approval_requested_at",
            "age_days",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
    })
}

fn parse_top_vendors_query(
    question: &str,
    normalized: &str,
    requested_limit: i64,
) -> Result<ParsedQuery, Error> {
    let limit = parse_top_limit(normalized)
        .or(Some(requested_limit))
        .unwrap_or(DEFAULT_LIMIT)
        .clamp(1, MAX_LIMIT);
    let (start_date, end_date) = if normalized.contains("last quarter") {
        previous_quarter(Utc::now().date_naive())
    } else {
        (None, None)
    };
    let exclude_utilities = normalized.contains("excluding utilities")
        || normalized.contains("exclude utilities")
        || normalized.contains("without utilities");

    let filters = json!({
        "start_date": start_date,
        "end_date": end_date,
        "exclude_utilities": exclude_utilities,
        "limit": limit,
    });

    Ok(ParsedQuery {
        kind: QueryKind::TopVendors,
        normalized_question: question.trim().to_string(),
        explanation: "Translated to tenant-scoped vendor spend aggregation over invoice data."
            .to_string(),
        limit,
        min_amount_cents: None,
        processing_status: None,
        pending_more_than_days: None,
        start_date,
        end_date,
        exclude_utilities,
        filters,
        columns: vec![
            "vendor_id",
            "vendor_name",
            "invoice_count",
            "total_spend_cents",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
    })
}

fn parse_amount_cents(text: &str) -> Option<i64> {
    let words: Vec<&str> = text.split_whitespace().collect();
    for idx in 0..words.len() {
        if matches!(words[idx], "over" | "above" | "greater" | "exceeding") {
            for candidate in words.iter().skip(idx + 1).take(4) {
                if let Some(amount) = parse_amount_token(candidate) {
                    return Some(amount);
                }
            }
        }
    }
    None
}

fn parse_amount_token(token: &str) -> Option<i64> {
    let trimmed = token
        .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != ',')
        .replace(['$', ','], "");
    if trimmed.is_empty() {
        return None;
    }
    let multiplier = if trimmed.ends_with('k') { 1_000.0 } else { 1.0 };
    let numeric = trimmed.trim_end_matches('k').parse::<f64>().ok()?;
    Some((numeric * multiplier * 100.0).round() as i64)
}

fn parse_more_than_days(text: &str) -> Option<i32> {
    let words: Vec<&str> = text.split_whitespace().collect();
    for idx in 0..words.len().saturating_sub(2) {
        if words[idx] == "more" && words.get(idx + 1) == Some(&"than") {
            if let Some(days) = words.get(idx + 2).and_then(|word| {
                word.trim_matches(|c: char| !c.is_ascii_digit())
                    .parse::<i32>()
                    .ok()
            }) {
                if words.iter().skip(idx + 3).take(3).any(|word| {
                    word.trim_matches(|c: char| !c.is_ascii_alphabetic())
                        .starts_with("day")
                }) {
                    return Some(days);
                }
            }
        }
    }
    None
}

fn parse_top_limit(text: &str) -> Option<i64> {
    let words: Vec<&str> = text.split_whitespace().collect();
    for idx in 0..words.len().saturating_sub(1) {
        if words[idx] == "top" {
            return words.get(idx + 1).and_then(|word| {
                word.trim_matches(|c: char| !c.is_ascii_digit())
                    .parse()
                    .ok()
            });
        }
    }
    None
}

fn previous_quarter(today: NaiveDate) -> (Option<NaiveDate>, Option<NaiveDate>) {
    let current_quarter = ((today.month() - 1) / 3) + 1;
    let (year, quarter) = if current_quarter == 1 {
        (today.year() - 1, 4)
    } else {
        (today.year(), current_quarter - 1)
    };
    let start_month = (quarter - 1) * 3 + 1;
    let end_month = start_month + 2;
    let start = NaiveDate::from_ymd_opt(year, start_month, 1);
    let end = if end_month == 12 {
        NaiveDate::from_ymd_opt(year, 12, 31)
    } else {
        NaiveDate::from_ymd_opt(year, end_month + 1, 1).and_then(|date| date.pred_opt())
    };
    (start, end)
}

fn default_view_name(question: &str) -> String {
    let trimmed = question.trim();
    let max_chars = 80;
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    trimmed.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pending_invoice_query_with_amount_and_age() {
        let parsed = parse_nl_query(
            "invoices over $10k pending approval more than 3 business days",
            None,
        )
        .unwrap();

        assert_eq!(parsed.kind, QueryKind::Invoices);
        assert_eq!(parsed.min_amount_cents, Some(1_000_000));
        assert_eq!(
            parsed.processing_status,
            Some("pending_approval".to_string())
        );
        assert_eq!(parsed.pending_more_than_days, Some(3));
        assert_eq!(parsed.limit, DEFAULT_LIMIT);
    }

    #[test]
    fn parses_top_vendor_spend_query_with_last_quarter_and_exclusion() {
        let parsed = parse_nl_query(
            "top 20 vendors by spend last quarter excluding utilities",
            None,
        )
        .unwrap();

        assert_eq!(parsed.kind, QueryKind::TopVendors);
        assert_eq!(parsed.limit, 20);
        assert!(parsed.start_date.is_some());
        assert!(parsed.end_date.is_some());
        assert!(parsed.exclude_utilities);
    }

    #[test]
    fn rejects_unsupported_questions() {
        let err = parse_nl_query("show me whatever", None).unwrap_err();
        assert!(matches!(err, Error::Validation(_)));
    }

    #[test]
    fn clamps_requested_limit() {
        let parsed = parse_nl_query("invoices pending approval", Some(1_000)).unwrap();
        assert_eq!(parsed.limit, MAX_LIMIT);
    }
}
