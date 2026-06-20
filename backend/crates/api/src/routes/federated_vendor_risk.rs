//! API routes for the Federated Vendor Risk Network (#408).
//!
//! Endpoints (mounted under `/api/v1`):
//!   - GET  /vendors/:id/federated_risk
//!         Returns aggregated network-wide signals for the local vendor,
//!         with a k-anonymized 'why flagged' explanation grounded solely in
//!         network signals.
//!   - POST /tenants/risk_network/opt_in
//!   - POST /tenants/risk_network/opt_out
//!         Toggle the tenant's participation in the network.
//!
//! All endpoints sit behind the existing VendorMgmtAccess extractor so they
//! inherit the JWT + tenant + module-entitlement gating used by the rest of
//! the vendor surface.

use crate::error::{ApiError, ApiResult};
use crate::extractors::VendorMgmtAccess;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use billforge_core::{
    domain::{AuditAction, AuditEntry, ResourceType},
    traits::AuditService,
    Error,
};
use billforge_vendor_mgmt::federated_risk::{
    self, FederatedSignalType, NetworkSignalAggregate, DEFAULT_K_ANONYMITY_FLOOR,
};
use serde::Serialize;
use std::sync::Arc;
use uuid::Uuid;

/// Sub-router merged into `/api/v1/vendors` (the `:id/federated_risk` route)
/// alongside a second sub-router merged at `/api/v1` for the consent toggle
/// endpoints. See `routes/mod.rs::api_routes`.
pub fn vendor_routes() -> Router<AppState> {
    Router::new().route("/:id/federated_risk", get(get_federated_risk))
}

pub fn tenant_routes() -> Router<AppState> {
    Router::new()
        .route("/tenants/risk_network/opt_in", post(opt_in))
        .route("/tenants/risk_network/opt_out", post(opt_out))
}

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct NetworkSignalAggregateDto {
    pub signal_type: String,
    pub contributor_count: i64,
    pub weighted_score: f64,
    pub explanation: String,
}

impl From<NetworkSignalAggregate> for NetworkSignalAggregateDto {
    fn from(a: NetworkSignalAggregate) -> Self {
        Self {
            signal_type: a.signal_type.as_db_str().to_string(),
            contributor_count: a.contributor_count,
            weighted_score: a.weighted_score,
            explanation: a.explanation,
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct FederatedVendorRiskResponse {
    pub opted_in: bool,
    pub contributor_floor: i64,
    pub signals: Vec<NetworkSignalAggregateDto>,
    /// Concatenated network-only explanation paragraphs. Empty when there
    /// are no signals meeting the k-anonymity floor.
    pub why_flagged: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ConsentResponse {
    pub opted_in: bool,
}

// ---------------------------------------------------------------------------
// GET /api/v1/vendors/{id}/federated_risk
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/v1/vendors/{id}/federated_risk",
    tag = "Vendors",
    params(("id" = String, Path, description = "Local vendor id")),
    responses(
        (status = 200, description = "Federated risk aggregates", body = FederatedVendorRiskResponse),
        (status = 404, description = "Vendor not found"),
    )
)]
async fn get_federated_risk(
    VendorMgmtAccess(_user, tenant): VendorMgmtAccess,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<FederatedVendorRiskResponse>> {
    let vendor_id: Uuid = id
        .parse()
        .map_err(|_| ApiError(Error::Validation("Invalid vendor ID".to_string())))?;

    let metadata_pool = state.db.metadata();
    let tenant_uuid = *tenant.tenant_id.as_uuid();

    // Opt-in gate: do not surface any aggregates for tenants not in the network.
    let opted_in = federated_risk::is_tenant_opted_in(&metadata_pool, tenant_uuid)
        .await
        .map_err(|e| {
            ApiError(Error::Internal(format!(
                "Failed to read risk-network consent: {e}"
            )))
        })?;
    if !opted_in {
        return Ok(Json(FederatedVendorRiskResponse {
            opted_in: false,
            contributor_floor: DEFAULT_K_ANONYMITY_FLOOR,
            signals: Vec::new(),
            why_flagged: String::new(),
        }));
    }

    // Pull the vendor identity components from the tenant pool to build the hash.
    let tenant_pool = state.db.tenant(&tenant.tenant_id).await?;
    let row: Option<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT name, tax_id, bank_account_last_four FROM vendors WHERE id = $1 AND tenant_id = $2",
    )
    .bind(vendor_id)
    .bind(tenant_uuid)
    .fetch_optional(&*tenant_pool)
    .await
    .map_err(|e| ApiError(Error::Database(format!("Failed to read vendor: {e}"))))?;

    let (name, tax_id, bank_last_four) = row.ok_or_else(|| {
        ApiError(Error::NotFound {
            resource_type: "Vendor".to_string(),
            id: id.clone(),
        })
    })?;

    let salt = network_hash_salt()?;
    let normalized = federated_risk::normalize_vendor_name(&name);
    let bank_fp = bank_last_four.as_deref();
    let vendor_hash =
        federated_risk::vendor_hash(&normalized, tax_id.as_deref(), bank_fp, &salt);

    let aggregates = federated_risk::aggregate_for_vendor(
        &metadata_pool,
        &vendor_hash,
        DEFAULT_K_ANONYMITY_FLOOR,
    )
    .await
    .map_err(|e| {
        ApiError(Error::Internal(format!(
            "Failed to aggregate federated vendor risk: {e}"
        )))
    })?;

    let why_flagged = aggregates
        .iter()
        .map(|a| a.explanation.clone())
        .collect::<Vec<_>>()
        .join(" ");

    let signals: Vec<NetworkSignalAggregateDto> = aggregates.into_iter().map(Into::into).collect();

    Ok(Json(FederatedVendorRiskResponse {
        opted_in: true,
        contributor_floor: DEFAULT_K_ANONYMITY_FLOOR,
        signals,
        why_flagged,
    }))
}

// ---------------------------------------------------------------------------
// POST /api/v1/tenants/risk_network/opt_in | opt_out
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/v1/tenants/risk_network/opt_in",
    tag = "Vendors",
    responses((status = 200, description = "Tenant opted into risk network", body = ConsentResponse))
)]
async fn opt_in(
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    State(state): State<AppState>,
) -> ApiResult<Json<ConsentResponse>> {
    let metadata_pool = state.db.metadata();
    let tenant_uuid = *tenant.tenant_id.as_uuid();

    federated_risk::opt_in_tenant(&metadata_pool, tenant_uuid)
        .await
        .map_err(|e| ApiError(Error::Internal(format!("Failed to opt in: {e}"))))?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(),
        Some(user.user_id.clone()),
        AuditAction::Update,
        ResourceType::Tenant,
        tenant_uuid.to_string(),
        "Opted tenant in to the Federated Vendor Risk Network".to_string(),
    )
    .with_user_email(&user.email);
    let audit_repo =
        billforge_db::repositories::AuditRepositoryImpl::new(Arc::new((*metadata_pool).clone()));
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log risk-network opt-in audit entry");
    }

    Ok(Json(ConsentResponse { opted_in: true }))
}

#[utoipa::path(
    post,
    path = "/api/v1/tenants/risk_network/opt_out",
    tag = "Vendors",
    responses((status = 200, description = "Tenant opted out of risk network", body = ConsentResponse))
)]
async fn opt_out(
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    State(state): State<AppState>,
) -> ApiResult<Json<ConsentResponse>> {
    let metadata_pool = state.db.metadata();
    let tenant_uuid = *tenant.tenant_id.as_uuid();

    federated_risk::opt_out_tenant(&metadata_pool, tenant_uuid)
        .await
        .map_err(|e| ApiError(Error::Internal(format!("Failed to opt out: {e}"))))?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(),
        Some(user.user_id.clone()),
        AuditAction::Update,
        ResourceType::Tenant,
        tenant_uuid.to_string(),
        "Opted tenant out of the Federated Vendor Risk Network".to_string(),
    )
    .with_user_email(&user.email);
    let audit_repo =
        billforge_db::repositories::AuditRepositoryImpl::new(Arc::new((*metadata_pool).clone()));
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log risk-network opt-out audit entry");
    }

    Ok(Json(ConsentResponse { opted_in: false }))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Load the network hash salt from the environment. Fail-fast: returning an
/// error here surfaces in the API response, which is preferable to silently
/// computing the wrong hash bucket.
fn network_hash_salt() -> Result<String, ApiError> {
    std::env::var("NETWORK_HASH_SALT")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| {
            ApiError(Error::Internal(
                "NETWORK_HASH_SALT is not configured".to_string(),
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_dto_preserves_signal_type_token() {
        let agg = NetworkSignalAggregate {
            signal_type: FederatedSignalType::BankAccountChange,
            contributor_count: 7,
            weighted_score: 12.5,
            explanation: "7 other tenants ...".to_string(),
        };
        let dto: NetworkSignalAggregateDto = agg.into();
        assert_eq!(dto.signal_type, "bank_account_change");
        assert_eq!(dto.contributor_count, 7);
    }
}
