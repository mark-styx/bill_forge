//! Contract CRUD and matching endpoints.
//!
//! POST /api/v1/contracts          - create a contract
//! GET  /api/v1/contracts          - list contracts (filter by vendor_id, status)
//! GET  /api/v1/contracts/:id      - fetch single contract
//! PATCH /api/v1/contracts/:id     - update contract fields
//! POST /api/v1/contracts/match    - match an invoice against a contract

use crate::error::ApiResult;
use crate::extractors::{AuthUser, TenantCtx};
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    routing::{get, patch, post},
    Json, Router,
};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct ContractResponse {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub vendor_id: Uuid,
    pub contract_number: Option<String>,
    pub description: Option<String>,
    pub monthly_amount: f64,
    pub currency: String,
    pub escalator_pct: f64,
    pub escalator_anniversary_month: Option<i16>,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub tolerance_pct: f64,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateContractRequest {
    pub vendor_id: Uuid,
    pub contract_number: Option<String>,
    pub description: Option<String>,
    pub monthly_amount: f64,
    #[serde(default = "default_currency")]
    pub currency: String,
    #[serde(default)]
    pub escalator_pct: f64,
    pub escalator_anniversary_month: Option<i16>,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    #[serde(default = "default_tolerance")]
    pub tolerance_pct: f64,
}

fn default_currency() -> String {
    "USD".to_string()
}

fn default_tolerance() -> f64 {
    2.0 // 2%
}

#[derive(Debug, Deserialize)]
pub struct UpdateContractRequest {
    pub status: Option<String>,
    pub end_date: Option<NaiveDate>,
    pub monthly_amount: Option<f64>,
    pub tolerance_pct: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct ListContractsQuery {
    pub vendor_id: Option<Uuid>,
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MatchRequest {
    pub invoice_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct MatchResponse {
    pub outcome: ContractMatchOutcomeDto,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "outcome", content = "detail", rename_all = "snake_case")]
pub enum ContractMatchOutcomeDto {
    InBand {
        contract_id: Uuid,
        expected: f64,
        variance_pct: f64,
    },
    OutOfBand {
        contract_id: Uuid,
        expected: f64,
        variance_pct: f64,
    },
    Expired {
        contract_id: Uuid,
    },
    NoActiveContract,
}

// ---------------------------------------------------------------------------
// Row types for SQL queries
// ---------------------------------------------------------------------------

#[derive(Debug, sqlx::FromRow)]
struct ContractRow {
    id: Uuid,
    tenant_id: Uuid,
    vendor_id: Uuid,
    contract_number: Option<String>,
    description: Option<String>,
    monthly_amount: f64,
    currency: String,
    escalator_pct: f64,
    escalator_anniversary_month: Option<i16>,
    start_date: NaiveDate,
    end_date: NaiveDate,
    tolerance_pct: f64,
    status: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct InvoiceRow {
    id: Uuid,
    vendor_id: Option<Uuid>,
    invoice_date: Option<NaiveDate>,
    total_amount_cents: i64,
    currency: String,
    po_number: Option<String>,
}

impl From<ContractRow> for ContractResponse {
    fn from(r: ContractRow) -> Self {
        Self {
            id: r.id,
            tenant_id: r.tenant_id,
            vendor_id: r.vendor_id,
            contract_number: r.contract_number,
            description: r.description,
            monthly_amount: r.monthly_amount,
            currency: r.currency,
            escalator_pct: r.escalator_pct,
            escalator_anniversary_month: r.escalator_anniversary_month,
            start_date: r.start_date,
            end_date: r.end_date,
            tolerance_pct: r.tolerance_pct,
            status: r.status,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", post(create_contract))
        .route("/", get(list_contracts))
        .route("/:id", get(get_contract))
        .route("/:id", patch(update_contract))
        .route("/match", post(match_invoice))
}

/// POST /api/v1/contracts
async fn create_contract(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    _user: AuthUser,
    Json(body): Json<CreateContractRequest>,
) -> ApiResult<Json<ContractResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    if body.monthly_amount <= 0.0 {
        return Err(billforge_core::Error::Validation(
            "monthly_amount must be positive".to_string(),
        )
        .into());
    }
    if body.end_date <= body.start_date {
        return Err(billforge_core::Error::Validation(
            "end_date must be after start_date".to_string(),
        )
        .into());
    }

    let row = sqlx::query_as::<_, ContractRow>(
        r#"INSERT INTO contracts
               (tenant_id, vendor_id, contract_number, description,
                monthly_amount, currency, escalator_pct, escalator_anniversary_month,
                start_date, end_date, tolerance_pct)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
           RETURNING id, tenant_id, vendor_id, contract_number, description,
                     CAST(monthly_amount AS DOUBLE PRECISION) AS monthly_amount,
                     currency,
                     CAST(escalator_pct AS DOUBLE PRECISION) AS escalator_pct,
                     escalator_anniversary_month,
                     start_date, end_date,
                     CAST(tolerance_pct AS DOUBLE PRECISION) AS tolerance_pct,
                     status, created_at, updated_at"#,
    )
    .bind(*tenant.tenant_id.as_uuid())
    .bind(body.vendor_id)
    .bind(&body.contract_number)
    .bind(&body.description)
    .bind(body.monthly_amount)
    .bind(&body.currency)
    .bind(body.escalator_pct)
    .bind(body.escalator_anniversary_month)
    .bind(body.start_date)
    .bind(body.end_date)
    .bind(body.tolerance_pct)
    .fetch_one(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to create contract: {}", e)))?;

    Ok(Json(ContractResponse::from(row)))
}

/// GET /api/v1/contracts?vendor_id=&status=
async fn list_contracts(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    _user: AuthUser,
    Query(query): Query<ListContractsQuery>,
) -> ApiResult<Json<Vec<ContractResponse>>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let rows = sqlx::query_as::<_, ContractRow>(
        r#"SELECT id, tenant_id, vendor_id, contract_number, description,
                  CAST(monthly_amount AS DOUBLE PRECISION) AS monthly_amount,
                  currency,
                  CAST(escalator_pct AS DOUBLE PRECISION) AS escalator_pct,
                  escalator_anniversary_month,
                  start_date, end_date,
                  CAST(tolerance_pct AS DOUBLE PRECISION) AS tolerance_pct,
                  status, created_at, updated_at
           FROM contracts
           WHERE tenant_id = $1
             AND ($2::UUID IS NULL OR vendor_id = $2)
             AND ($3::TEXT IS NULL OR status = $3)
           ORDER BY created_at DESC"#,
    )
    .bind(*tenant.tenant_id.as_uuid())
    .bind(query.vendor_id)
    .bind(query.status.as_deref())
    .fetch_all(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to list contracts: {}", e)))?;

    Ok(Json(rows.into_iter().map(ContractResponse::from).collect()))
}

/// GET /api/v1/contracts/:id
async fn get_contract(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    _user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ContractResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let row = sqlx::query_as::<_, ContractRow>(
        r#"SELECT id, tenant_id, vendor_id, contract_number, description,
                  CAST(monthly_amount AS DOUBLE PRECISION) AS monthly_amount,
                  currency,
                  CAST(escalator_pct AS DOUBLE PRECISION) AS escalator_pct,
                  escalator_anniversary_month,
                  start_date, end_date,
                  CAST(tolerance_pct AS DOUBLE PRECISION) AS tolerance_pct,
                  status, created_at, updated_at
           FROM contracts
           WHERE id = $1 AND tenant_id = $2"#,
    )
    .bind(id)
    .bind(*tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to fetch contract: {}", e)))?
    .ok_or_else(|| billforge_core::Error::NotFound {
        resource_type: "Contract".to_string(),
        id: id.to_string(),
    })?;

    Ok(Json(ContractResponse::from(row)))
}

/// PATCH /api/v1/contracts/:id
async fn update_contract(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    _user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateContractRequest>,
) -> ApiResult<Json<ContractResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Verify ownership.
    let _existing =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM contracts WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(*tenant.tenant_id.as_uuid())
            .fetch_optional(&*pool)
            .await
            .map_err(|e| {
                billforge_core::Error::Database(format!("Failed to find contract: {}", e))
            })?
            .ok_or_else(|| billforge_core::Error::NotFound {
                resource_type: "Contract".to_string(),
                id: id.to_string(),
            })?;

    // Update each provided field individually.
    // We already verified existence; update each provided field.
    if let Some(ref s) = body.status {
        sqlx::query("UPDATE contracts SET status = $1, updated_at = NOW() WHERE id = $2")
            .bind(s)
            .bind(id)
            .execute(&*pool)
            .await
            .map_err(|e| {
                billforge_core::Error::Database(format!("Failed to update contract: {}", e))
            })?;
    }
    if let Some(d) = body.end_date {
        sqlx::query("UPDATE contracts SET end_date = $1, updated_at = NOW() WHERE id = $2")
            .bind(d)
            .bind(id)
            .execute(&*pool)
            .await
            .map_err(|e| {
                billforge_core::Error::Database(format!("Failed to update contract: {}", e))
            })?;
    }
    if let Some(a) = body.monthly_amount {
        if a <= 0.0 {
            return Err(billforge_core::Error::Validation(
                "monthly_amount must be positive".to_string(),
            )
            .into());
        }
        sqlx::query("UPDATE contracts SET monthly_amount = $1, updated_at = NOW() WHERE id = $2")
            .bind(a)
            .bind(id)
            .execute(&*pool)
            .await
            .map_err(|e| {
                billforge_core::Error::Database(format!("Failed to update contract: {}", e))
            })?;
    }
    if let Some(t) = body.tolerance_pct {
        sqlx::query("UPDATE contracts SET tolerance_pct = $1, updated_at = NOW() WHERE id = $2")
            .bind(t)
            .bind(id)
            .execute(&*pool)
            .await
            .map_err(|e| {
                billforge_core::Error::Database(format!("Failed to update contract: {}", e))
            })?;
    }

    // Fetch updated row.
    let row = sqlx::query_as::<_, ContractRow>(
        r#"SELECT id, tenant_id, vendor_id, contract_number, description,
                  CAST(monthly_amount AS DOUBLE PRECISION) AS monthly_amount,
                  currency,
                  CAST(escalator_pct AS DOUBLE PRECISION) AS escalator_pct,
                  escalator_anniversary_month,
                  start_date, end_date,
                  CAST(tolerance_pct AS DOUBLE PRECISION) AS tolerance_pct,
                  status, created_at, updated_at
           FROM contracts
           WHERE id = $1"#,
    )
    .bind(id)
    .fetch_one(&*pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to fetch updated contract: {}", e))
    })?;

    Ok(Json(ContractResponse::from(row)))
}

/// POST /api/v1/contracts/match
///
/// Loads an invoice, calls `match_invoice_to_contract`, and returns the
/// outcome. If `InBand`, advances the invoice to touchless-approved;
/// if `OutOfBand` or `Expired`, raises a contract-mismatch exception.
async fn match_invoice(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    _user: AuthUser,
    Json(body): Json<MatchRequest>,
) -> ApiResult<Json<MatchResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Load invoice.
    let inv = sqlx::query_as::<_, InvoiceRow>(
        r#"SELECT id, vendor_id, invoice_date, total_amount_cents, currency, po_number
           FROM invoices
           WHERE id = $1 AND tenant_id = $2"#,
    )
    .bind(body.invoice_id)
    .bind(*tenant.tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to fetch invoice: {}", e)))?
    .ok_or_else(|| billforge_core::Error::NotFound {
        resource_type: "Invoice".to_string(),
        id: body.invoice_id.to_string(),
    })?;

    // Only match non-PO invoices.
    if inv.po_number.is_some() {
        return Err(billforge_core::Error::Validation(
            "Invoice already has a PO; contract matching is for non-PO invoices".to_string(),
        )
        .into());
    }

    let vendor_id = inv
        .vendor_id
        .ok_or_else(|| billforge_core::Error::Validation("Invoice has no vendor_id".to_string()))?;

    let invoice_date = inv
        .invoice_date
        .unwrap_or_else(|| chrono::Utc::now().date_naive());

    // Convert cents to dollars.
    let amount_dollars = inv.total_amount_cents as f64 / 100.0;

    let input = billforge_invoice_processing::ContractMatchInput {
        tenant_id: *tenant.tenant_id.as_uuid(),
        vendor_id,
        invoice_date,
        amount: amount_dollars,
        currency: inv.currency,
    };

    let outcome =
        billforge_invoice_processing::match_invoice_to_contract(&pool, &input, body.invoice_id)
            .await
            .map_err(|e| {
                billforge_core::Error::Database(format!("Contract matching failed: {}", e))
            })?;

    // Route based on outcome.
    match &outcome {
        billforge_invoice_processing::ContractMatchOutcome::InBand { .. } => {
            // Touchless-approve: update processing status.
            sqlx::query(
                r#"UPDATE invoices
                   SET processing_status = 'approved', updated_at = NOW()
                   WHERE id = $1 AND tenant_id = $2"#,
            )
            .bind(body.invoice_id)
            .bind(*tenant.tenant_id.as_uuid())
            .execute(&*pool)
            .await
            .map_err(|e| {
                billforge_core::Error::Database(format!(
                    "Failed to auto-approve contract-matched invoice: {}",
                    e
                ))
            })?;
        }
        billforge_invoice_processing::ContractMatchOutcome::OutOfBand {
            contract_id,
            expected,
            variance_pct,
            ..
        } => {
            // Flag as on-hold with a mismatch note.
            let msg = format!(
                "Contract mismatch: expected {:.2}, variance {:.2}% (contract {})",
                expected, variance_pct, contract_id
            );
            sqlx::query(
                r#"UPDATE invoices
                   SET processing_status = 'on_hold',
                       notes = CONCAT(COALESCE(notes, ''), $3, E'\n'),
                       updated_at = NOW()
                   WHERE id = $1 AND tenant_id = $2"#,
            )
            .bind(body.invoice_id)
            .bind(*tenant.tenant_id.as_uuid())
            .bind(&msg)
            .execute(&*pool)
            .await
            .map_err(|e| {
                billforge_core::Error::Database(format!(
                    "Failed to flag contract-mismatched invoice: {}",
                    e
                ))
            })?;
        }
        billforge_invoice_processing::ContractMatchOutcome::Expired { contract_id } => {
            let msg = format!(
                "Contract expired: invoice date is past contract {} end date\n",
                contract_id
            );
            sqlx::query(
                r#"UPDATE invoices
                   SET processing_status = 'on_hold',
                       notes = CONCAT(COALESCE(notes, ''), $3, E'\n'),
                       updated_at = NOW()
                   WHERE id = $1 AND tenant_id = $2"#,
            )
            .bind(body.invoice_id)
            .bind(*tenant.tenant_id.as_uuid())
            .bind(&msg)
            .execute(&*pool)
            .await
            .map_err(|e| {
                billforge_core::Error::Database(format!(
                    "Failed to flag contract-expired invoice: {}",
                    e
                ))
            })?;
        }
        billforge_invoice_processing::ContractMatchOutcome::NoActiveContract => {
            // No-op: caller falls back to PO/manual flow.
        }
    }

    let dto = match outcome {
        billforge_invoice_processing::ContractMatchOutcome::InBand {
            contract_id,
            expected,
            variance_pct,
        } => ContractMatchOutcomeDto::InBand {
            contract_id,
            expected,
            variance_pct,
        },
        billforge_invoice_processing::ContractMatchOutcome::OutOfBand {
            contract_id,
            expected,
            variance_pct,
        } => ContractMatchOutcomeDto::OutOfBand {
            contract_id,
            expected,
            variance_pct,
        },
        billforge_invoice_processing::ContractMatchOutcome::Expired { contract_id } => {
            ContractMatchOutcomeDto::Expired { contract_id }
        }
        billforge_invoice_processing::ContractMatchOutcome::NoActiveContract => {
            ContractMatchOutcomeDto::NoActiveContract
        }
    };

    Ok(Json(MatchResponse { outcome: dto }))
}
