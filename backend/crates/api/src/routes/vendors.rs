//! Vendor routes (Vendor Management module)

use crate::error::ApiResult;
use crate::extractors::VendorMgmtAccess;
use crate::fraud_guard;
use crate::state::AppState;
use axum::{
    extract::{Multipart, Path, Query, State},
    routing::{delete, get, patch, post, put},
    Json, Router,
};
use billforge_core::{
    domain::{
        AuditAction, AuditEntry, BankingVerification, BankingVerificationStatus, CreateVendorInput,
        ResourceType, UpdateVendorInput, Vendor, VendorContact, VendorFilters, VendorId,
        VendorType,
    },
    traits::{AuditService, TaxDocumentRepository, VendorRepository},
    types::{PaginatedResponse, Pagination, TenantId},
    Error, Result as CoreResult,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

/// Vendor-specific routing rules consumed by the approval engine.
///
/// Stored as JSONB in the `vendors.routing_rules` column.
/// The approval engine calls [`get_routing_rules`] to read these.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RoutingRules {
    /// Email address of the designated approver for this vendor's invoices
    pub approver_email: Option<String>,
    /// Invoices at or below this amount (in cents) are auto-approved
    pub auto_approve_threshold_cents: Option<i64>,
    /// Whether this vendor requires dual (two-person) approval
    pub requires_dual_approval: Option<bool>,
}

impl Default for RoutingRules {
    fn default() -> Self {
        Self {
            approver_email: None,
            auto_approve_threshold_cents: None,
            requires_dual_approval: None,
        }
    }
}

/// Request body for PATCH /api/v1/vendors/{id}
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateVendorRequest {
    pub name: Option<String>,
    pub status: Option<String>,
    pub email: Option<String>,
    pub tax_id: Option<String>,
    pub payment_terms_days: Option<i32>,
    pub routing_rules: Option<RoutingRules>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_vendors))
        .route("/", post(create_vendor))
        .route("/:id", get(get_vendor))
        .route("/:id", put(update_vendor))
        .route("/:id", patch(patch_vendor))
        .route("/:id", delete(delete_vendor))
        .route("/:id/contacts", post(add_contact))
        .route("/:id/contacts/:contact_id", delete(remove_contact))
        .route("/:id/documents", get(list_tax_documents))
        .route("/:id/documents", post(upload_tax_document))
        .route("/:id/messages", get(list_messages))
        .route("/:id/messages", post(send_message))
        .route("/:id/portal-link", post(create_portal_link))
        .route("/:id/banking", put(update_banking))
        .route(
            "/:id/banking-verifications",
            get(list_banking_verifications),
        )
        .route(
            "/:id/banking-verifications/:vid/verify",
            post(verify_banking),
        )
        .route("/import", post(import_vendors_csv))
}

#[derive(Debug, Deserialize)]
pub struct ListVendorsQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub status: Option<String>,
    pub vendor_type: Option<String>,
    pub search: Option<String>,
}

#[utoipa::path(get, path = "/api/v1/vendors", tag = "Vendors",
    params(("page" = Option<u32>, Query,), ("per_page" = Option<u32>, Query,), ("status" = Option<String>, Query,), ("search" = Option<String>, Query,)),
    responses((status = 200, description = "Paginated vendor list", body = crate::openapi::VendorList), (status = 401, description = "Unauthorized")))]
async fn list_vendors(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Query(query): Query<ListVendorsQuery>,
) -> ApiResult<Json<PaginatedResponse<Vendor>>> {
    let pagination = Pagination {
        page: query.page.unwrap_or(1),
        per_page: query.per_page.unwrap_or(25).min(100),
    };

    let filters = VendorFilters {
        search: query.search,
        ..Default::default()
    };

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool.clone());
    let vendors = repo.list(&tenant.tenant_id, &filters, &pagination).await?;

    Ok(Json(vendors))
}

#[utoipa::path(get, path = "/api/v1/vendors/{id}", tag = "Vendors",
    params(("id" = String, Path, description = "Vendor ID")),
    responses((status = 200, description = "Vendor details", body = crate::openapi::Vendor), (status = 404, description = "Vendor not found")))]
async fn get_vendor(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<Vendor>> {
    let vendor_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool.clone());
    let vendor = repo
        .get_by_id(&tenant.tenant_id, &vendor_id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "Vendor".to_string(),
            id: id.clone(),
        })?;

    Ok(Json(vendor))
}

#[utoipa::path(post, path = "/api/v1/vendors", tag = "Vendors", request_body = serde_json::Value,
    responses((status = 200, description = "Vendor created", body = crate::openapi::Vendor), (status = 401, description = "Unauthorized")))]
async fn create_vendor(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Json(input): Json<CreateVendorInput>,
) -> ApiResult<Json<Vendor>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool.clone());

    // Run fraud-guard checks before creating the vendor
    let domain = fraud_guard::extract_domain(input.email.as_deref(), input.website.as_deref());
    let vendor_country = input.address.as_ref().map(|a| a.country.as_str());
    let signals = fraud_guard::run_fraud_guard(
        &tenant.tenant_id,
        None, // no vendor_id yet
        &input.name,
        &domain,
        vendor_country,
        None, // no bank country at creation time
        &pool,
    )
    .await;

    // Upsert domain so future checks can compare age
    fraud_guard::upsert_domain_first_seen(&tenant.tenant_id, &domain, &pool).await;

    // Run OFAC/sanctions screening against the latest persisted SDN list
    // (falls back to embedded seed when the refresh table is empty).
    let screener = crate::ofac_screening::OfacScreener::load_latest(&pool)
        .await
        .map_err(|e| billforge_core::Error::Database(format!("Failed to load OFAC list: {}", e)))?;
    let ofac_outcome = screener.screen(&input.name, None);

    let needs_hold =
        signals.overall_risk == fraud_guard::RiskLevel::High || ofac_outcome.status != "pass";

    // If overall risk is high or OFAC flagged, the vendor should start on payment_hold
    let vendor = if needs_hold {
        // Create vendor, then set payment_hold via direct query
        let v = repo.create(&tenant.tenant_id, input).await?;
        let reason = if ofac_outcome.status != "pass" {
            format!(
                "OFAC screening: {} ({} match{})",
                ofac_outcome.status,
                ofac_outcome.matches.len(),
                if ofac_outcome.matches.len() != 1 {
                    "es"
                } else {
                    ""
                }
            )
        } else {
            "Fraud guard: high-risk signals detected".to_string()
        };
        sqlx::query(
            "UPDATE vendors SET payment_hold = true, payment_hold_reason = $3, updated_at = NOW() WHERE id = $1 AND tenant_id = $2",
        )
        .bind(v.id.0)
        .bind(*tenant.tenant_id.as_uuid())
        .bind(&reason)
        .execute(&*pool)
        .await
        .map_err(|e| billforge_core::Error::Database(format!("Failed to set payment_hold: {}", e)))?;
        // Re-fetch to get updated payment_hold
        repo.get_by_id(&tenant.tenant_id, &v.id).await?.unwrap_or(v)
    } else {
        repo.create(&tenant.tenant_id, input).await?
    };

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(),
        Some(user.user_id.clone()),
        AuditAction::Create,
        ResourceType::Vendor,
        vendor.id.to_string(),
        format!(
            "Created vendor {} (fraud guard: {}, OFAC: {})",
            vendor.name,
            serde_json::to_string(&signals.overall_risk).unwrap_or_default(),
            ofac_outcome.status,
        ),
    )
    .with_user_email(&user.email)
    .with_new_value(serde_json::to_value(&vendor).unwrap_or_default())
    .with_metadata(serde_json::json!({
        "fraud_signals": fraud_guard::fraud_signals_to_json(&signals),
        "ofac_screening": {
            "status": ofac_outcome.status,
            "matches": ofac_outcome.matches,
        },
    }));
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(vendor))
}

#[utoipa::path(put, path = "/api/v1/vendors/{id}", tag = "Vendors", request_body = serde_json::Value,
    params(("id" = String, Path, description = "Vendor ID")),
    responses((status = 200, description = "Vendor updated", body = crate::openapi::Vendor), (status = 404, description = "Vendor not found")))]
async fn update_vendor(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
    Json(input): Json<UpdateVendorInput>,
) -> ApiResult<Json<Vendor>> {
    let vendor_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool.clone());

    let old_vendor = repo.get_by_id(&tenant.tenant_id, &vendor_id).await?;
    let vendor = repo.update(&tenant.tenant_id, &vendor_id, input).await?;

    // Run OFAC/sanctions screening on the updated vendor name against the
    // latest persisted SDN list.
    let screener = crate::ofac_screening::OfacScreener::load_latest(&pool)
        .await
        .map_err(|e| billforge_core::Error::Database(format!("Failed to load OFAC list: {}", e)))?;
    let ofac_outcome = screener.screen(&vendor.name, None);
    let mut vendor = vendor;

    if ofac_outcome.status != "pass" {
        let reason = format!(
            "OFAC screening: {} ({} match{})",
            ofac_outcome.status,
            ofac_outcome.matches.len(),
            if ofac_outcome.matches.len() != 1 {
                "es"
            } else {
                ""
            }
        );
        sqlx::query(
            "UPDATE vendors SET payment_hold = true, payment_hold_reason = $3, updated_at = NOW() WHERE id = $1 AND tenant_id = $2",
        )
        .bind(vendor.id.0)
        .bind(*tenant.tenant_id.as_uuid())
        .bind(&reason)
        .execute(&*pool)
        .await
        .map_err(|e| billforge_core::Error::Database(format!("Failed to set payment_hold: {}", e)))?;
        vendor = repo
            .get_by_id(&tenant.tenant_id, &vendor_id)
            .await?
            .unwrap_or(vendor);
    }

    let mut audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(),
        Some(user.user_id.clone()),
        AuditAction::Update,
        ResourceType::Vendor,
        vendor.id.to_string(),
        format!(
            "Updated vendor {} (OFAC: {})",
            vendor.name, ofac_outcome.status,
        ),
    )
    .with_user_email(&user.email)
    .with_new_value(serde_json::to_value(&vendor).unwrap_or_default())
    .with_metadata(serde_json::json!({
        "ofac_screening": {
            "status": ofac_outcome.status,
            "matches": ofac_outcome.matches,
        },
    }));
    if let Some(old) = old_vendor {
        audit_entry = audit_entry.with_old_value(serde_json::to_value(&old).unwrap_or_default());
    }
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(vendor))
}

#[utoipa::path(delete, path = "/api/v1/vendors/{id}", tag = "Vendors",
    params(("id" = String, Path, description = "Vendor ID")),
    responses((status = 200, description = "Vendor deleted"), (status = 404, description = "Vendor not found")))]
async fn delete_vendor(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let vendor_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool.clone());

    let old_vendor = repo.get_by_id(&tenant.tenant_id, &vendor_id).await?;

    // Soft delete: set status='inactive' instead of hard-deleting the row
    sqlx::query("UPDATE vendors SET status = 'inactive', updated_at = NOW() WHERE id = $1 AND tenant_id = $2")
        .bind(vendor_id.0)
        .bind(*tenant.tenant_id.as_uuid())
        .execute(&*pool)
        .await
        .map_err(|e| billforge_core::Error::Database(format!("Failed to soft-delete vendor: {}", e)))?;

    let mut audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(),
        Some(user.user_id.clone()),
        AuditAction::Delete,
        ResourceType::Vendor,
        id.clone(),
        "Soft-deleted vendor (set inactive)",
    )
    .with_user_email(&user.email);
    if let Some(old) = old_vendor {
        audit_entry = audit_entry.with_old_value(serde_json::to_value(&old).unwrap_or_default());
    }
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(serde_json::json!({ "success": true })))
}

/// PATCH /api/v1/vendors/{id} - partial update of vendor fields including routing_rules.
///
/// Unlike the PUT endpoint (which takes a full `UpdateVendorInput`), this handler
/// directly patches individual columns: name, status, email, tax_id,
/// payment_terms_days, and the JSONB routing_rules blob.
#[utoipa::path(patch, path = "/api/v1/vendors/{id}", tag = "Vendors",
    request_body = UpdateVendorRequest,
    params(("id" = String, Path, description = "Vendor ID")),
    responses((status = 200, description = "Vendor patched"), (status = 404, description = "Vendor not found")))]
async fn patch_vendor(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
    Json(req): Json<UpdateVendorRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let vendor_id: VendorId = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Build a dynamic SET clause for the supplied fields
    let mut set_clauses: Vec<String> = Vec::new();
    let mut param_idx = 1u32; // $1 is always vendor_id, $2 is always tenant_id
    let mut bind_name: Option<String> = None;
    let mut bind_status: Option<String> = None;
    let mut bind_email: Option<String> = None;
    let mut bind_tax_id: Option<String> = None;
    let mut bind_payment_days: Option<i32> = None;
    let mut bind_routing_rules: Option<serde_json::Value> = None;

    // $1 = vendor_id, $2 = tenant_id (reserved)
    param_idx = 3;

    if let Some(ref name) = req.name {
        if name.trim().is_empty() {
            return Err(
                billforge_core::Error::Validation("name must not be empty".to_string()).into(),
            );
        }
        set_clauses.push(format!("name = ${}", param_idx));
        bind_name = Some(name.clone());
        param_idx += 1;
    }
    if let Some(ref status) = req.status {
        match status.as_str() {
            "active" | "inactive" | "on_hold" => {}
            _ => {
                return Err(billforge_core::Error::Validation(
                    "status must be one of: active, inactive, on_hold".to_string(),
                )
                .into())
            }
        }
        set_clauses.push(format!("status = ${}", param_idx));
        bind_status = Some(status.clone());
        param_idx += 1;
    }
    if let Some(ref email) = req.email {
        set_clauses.push(format!("email = ${}", param_idx));
        bind_email = Some(email.clone());
        param_idx += 1;
    }
    if let Some(ref tax_id) = req.tax_id {
        set_clauses.push(format!("tax_id = ${}", param_idx));
        bind_tax_id = Some(tax_id.clone());
        param_idx += 1;
    }
    if let Some(days) = req.payment_terms_days {
        set_clauses.push(format!("payment_terms_days = ${}", param_idx));
        bind_payment_days = Some(days);
        param_idx += 1;
    }
    if let Some(ref rules) = req.routing_rules {
        set_clauses.push(format!("routing_rules = ${}::jsonb", param_idx));
        bind_routing_rules = Some(serde_json::to_value(rules).map_err(|e| {
            billforge_core::Error::Validation(format!("Invalid routing_rules JSON: {}", e))
        })?);
        param_idx += 1;
    }

    if set_clauses.is_empty() {
        return Err(billforge_core::Error::Validation("No fields to update".to_string()).into());
    }

    // Always touch updated_at
    set_clauses.push("updated_at = NOW()".to_string());

    let sql = format!(
        "UPDATE vendors SET {} WHERE id = $1 AND tenant_id = $2",
        set_clauses.join(", ")
    );

    let mut query = sqlx::query(&sql)
        .bind(vendor_id.0)
        .bind(*tenant.tenant_id.as_uuid());

    if let Some(v) = bind_name {
        query = query.bind(v);
    }
    if let Some(v) = bind_status {
        query = query.bind(v);
    }
    if let Some(v) = bind_email {
        query = query.bind(v);
    }
    if let Some(v) = bind_tax_id {
        query = query.bind(v);
    }
    if let Some(v) = bind_payment_days {
        query = query.bind(v);
    }
    if let Some(v) = bind_routing_rules {
        query = query.bind(v);
    }

    let result = query
        .execute(&*pool)
        .await
        .map_err(|e| billforge_core::Error::Database(format!("Failed to patch vendor: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(billforge_core::Error::NotFound {
            resource_type: "Vendor".to_string(),
            id: id.clone(),
        }
        .into());
    }

    // Audit
    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(),
        Some(user.user_id.clone()),
        AuditAction::Update,
        ResourceType::Vendor,
        id.clone(),
        format!("Patched vendor {}", id),
    )
    .with_user_email(&user.email);
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(serde_json::json!({ "success": true })))
}

/// Look up the routing rules for a specific vendor, scoped to a tenant.
///
/// This is the public helper that the approval engine calls to determine
/// vendor-specific approval routing (approver email, auto-approve threshold,
/// dual-approval requirement).
///
/// Returns a default (empty) [`RoutingRules`] when the vendor has no rules set.
pub async fn get_routing_rules(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    vendor_id: &VendorId,
) -> CoreResult<RoutingRules> {
    let row: Option<(Option<serde_json::Value>,)> =
        sqlx::query_as("SELECT routing_rules FROM vendors WHERE id = $1 AND tenant_id = $2")
            .bind(vendor_id.0)
            .bind(*tenant_id.as_uuid())
            .fetch_optional(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to fetch routing rules: {}", e)))?;

    let rules_json = match row {
        Some((Some(v),)) => v,
        Some((None,)) => serde_json::Value::Object(serde_json::Map::new()),
        None => {
            return Err(Error::NotFound {
                resource_type: "Vendor".to_string(),
                id: vendor_id.to_string(),
            })
        }
    };

    serde_json::from_value(rules_json)
        .map_err(|e| Error::Database(format!("Failed to parse routing_rules JSON: {}", e)))
}

#[utoipa::path(post, path = "/api/v1/vendors/{id}/contacts", tag = "Vendors", request_body = serde_json::Value,
    params(("id" = String, Path, description = "Vendor ID")),
    responses((status = 200, description = "Contact added"), (status = 404, description = "Vendor not found")))]
async fn add_contact(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
    Json(contact): Json<VendorContact>,
) -> ApiResult<Json<serde_json::Value>> {
    let vendor_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool.clone());
    repo.add_contact(&tenant.tenant_id, &vendor_id, contact)
        .await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(),
        Some(user.user_id.clone()),
        AuditAction::Create,
        ResourceType::Vendor,
        id.clone(),
        "Added vendor contact",
    )
    .with_user_email(&user.email)
    .with_metadata(serde_json::json!({ "sub_resource": "contact" }));
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(serde_json::json!({ "success": true })))
}

#[utoipa::path(delete, path = "/api/v1/vendors/{id}/contacts/{contact_id}", tag = "Vendors",
    params(("id" = String, Path, description = "Vendor ID"), ("contact_id" = String, Path, description = "Contact ID")),
    responses((status = 200, description = "Contact removed")))]
async fn remove_contact(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path((id, contact_id)): Path<(String, String)>,
) -> ApiResult<Json<serde_json::Value>> {
    let vendor_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;
    let contact_uuid = Uuid::parse_str(&contact_id)
        .map_err(|_| billforge_core::Error::Validation("Invalid contact ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool.clone());
    repo.remove_contact(&tenant.tenant_id, &vendor_id, contact_uuid)
        .await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(),
        Some(user.user_id.clone()),
        AuditAction::Delete,
        ResourceType::Vendor,
        id.clone(),
        "Removed vendor contact",
    )
    .with_user_email(&user.email)
    .with_metadata(serde_json::json!({ "sub_resource": "contact", "contact_id": contact_id }));
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(serde_json::json!({ "success": true })))
}

#[utoipa::path(get, path = "/api/v1/vendors/{id}/documents", tag = "Vendors",
    params(("id" = String, Path, description = "Vendor ID")),
    responses((status = 200, description = "Tax documents list")))]
async fn list_tax_documents(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    let vendor_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::TaxDocumentRepositoryImpl::new(pool);
    let documents = repo.list_for_vendor(&tenant.tenant_id, &vendor_id).await?;

    let result: Vec<serde_json::Value> = documents
        .into_iter()
        .map(|doc| {
            serde_json::json!({
                "id": doc.id,
                "document_type": doc.document_type,
                "tax_year": doc.tax_year,
                "file_name": doc.file_name,
                "received_date": doc.received_date,
                "expires_date": doc.expires_date,
                "created_at": doc.created_at,
            })
        })
        .collect();

    Ok(Json(result))
}

#[utoipa::path(post, path = "/api/v1/vendors/{id}/documents", tag = "Vendors",
    request_body(content = inline(String), content_type = "multipart/form-data"),
    params(("id" = String, Path, description = "Vendor ID")),
    responses((status = 200, description = "Document uploaded")))]
async fn upload_tax_document(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
    mut multipart: Multipart,
) -> ApiResult<Json<serde_json::Value>> {
    let vendor_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::TaxDocumentRepositoryImpl::new(pool.clone());

    // Extract file from multipart form
    while let Some(field) = multipart.next_field().await.map_err(|e| {
        billforge_core::Error::Validation(format!("Failed to read multipart data: {}", e))
    })? {
        let file_name = field.file_name().unwrap_or("document.pdf").to_string();
        let content_type = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();

        if let Ok(data) = field.bytes().await {
            let file_size = data.len() as i64;

            // Generate file path (in production, this would upload to S3 or similar)
            let file_path = format!("vendor_documents/{}/{}", vendor_id, uuid::Uuid::new_v4());

            // Store metadata in database
            let doc_id = repo
                .create(
                    &tenant.tenant_id,
                    &vendor_id,
                    "w9".to_string(), // Default to W9, could be configurable
                    file_name.clone(),
                    file_path,
                    file_size,
                    content_type,
                    Some(user.user_id.0),
                )
                .await?;

            let audit_entry = AuditEntry::new(
                tenant.tenant_id.clone(),
                Some(user.user_id.clone()),
                AuditAction::Create,
                ResourceType::Document,
                doc_id.to_string(),
                format!("Uploaded tax document '{}'", file_name),
            )
            .with_user_email(&user.email)
            .with_metadata(serde_json::json!({ "vendor_id": id, "file_name": file_name }));
            let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
            if let Err(e) = audit_repo.log(audit_entry).await {
                tracing::warn!(error = %e, "Failed to log audit entry");
            }

            return Ok(Json(serde_json::json!({
                "id": doc_id,
                "message": "Tax document uploaded successfully",
                "file_name": file_name
            })));
        }
    }

    Err(billforge_core::Error::Validation("No file uploaded".to_string()).into())
}

#[utoipa::path(get, path = "/api/v1/vendors/{id}/messages", tag = "Vendors",
    params(("id" = String, Path, description = "Vendor ID")),
    responses((status = 200, description = "Vendor messages")))]
async fn list_messages(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    let vendor_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool.clone());
    let messages = repo
        .list_messages(&tenant.tenant_id, &vendor_id, 50)
        .await?;

    let result: Vec<serde_json::Value> = messages
        .into_iter()
        .map(|msg| {
            serde_json::json!({
                "id": msg.id,
                "subject": msg.subject,
                "body": msg.body,
                "sender_type": msg.sender_type,
                "sender_name": msg.sender_name,
                "created_at": msg.created_at,
            })
        })
        .collect();

    Ok(Json(result))
}

#[derive(Deserialize)]
pub struct SendMessageInput {
    pub subject: String,
    pub body: String,
}

#[utoipa::path(post, path = "/api/v1/vendors/{id}/messages", tag = "Vendors", request_body = serde_json::Value,
    params(("id" = String, Path, description = "Vendor ID")),
    responses((status = 200, description = "Message sent")))]
async fn send_message(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
    Json(input): Json<SendMessageInput>,
) -> ApiResult<Json<serde_json::Value>> {
    let vendor_id = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool.clone());

    let message = repo
        .send_message(
            &tenant.tenant_id,
            &vendor_id,
            input.subject,
            input.body,
            Some(user.user_id.0),
        )
        .await?;

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(),
        Some(user.user_id.clone()),
        AuditAction::Create,
        ResourceType::Vendor,
        id.clone(),
        "Sent vendor message",
    )
    .with_user_email(&user.email)
    .with_metadata(
        serde_json::json!({ "sub_resource": "message", "message_id": message.id.to_string() }),
    );
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(serde_json::json!({
        "id": message.id,
        "message": "Message sent successfully",
        "created_at": message.created_at
    })))
}

/// Generate a vendor-portal access token and URL for a specific vendor.
/// Internal endpoint - requires VendorManager or TenantAdmin role.
#[utoipa::path(post, path = "/api/v1/vendors/{id}/portal-link", tag = "Vendors",
    params(("id" = String, Path, description = "Vendor ID")),
    responses((status = 200, description = "Portal link generated"), (status = 404, description = "Vendor not found")))]
async fn create_portal_link(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let vendor_id: VendorId = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool.clone());

    // Verify vendor exists
    let vendor = repo
        .get_by_id(&tenant.tenant_id, &vendor_id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "Vendor".to_string(),
            id: id.clone(),
        })?;

    // Create vendor-portal token
    let token = state
        .auth
        .jwt_service()
        .create_vendor_portal_token(&tenant.tenant_id, &vendor_id)
        .map_err(|e| {
            billforge_core::Error::Internal(format!("Failed to create portal token: {}", e))
        })?;

    let app_url = std::env::var("APP_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
    let portal_url = format!("{}/vendor-portal?token={}", app_url, token);

    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(),
        Some(user.user_id.clone()),
        AuditAction::Create,
        ResourceType::Vendor,
        id.clone(),
        format!("Generated portal link for vendor {}", vendor.name),
    )
    .with_user_email(&user.email)
    .with_metadata(serde_json::json!({ "vendor_id": id }));
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(serde_json::json!({
        "token": token,
        "url": portal_url,
    })))
}

/// Response for CSV vendor import
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ImportVendorsResponse {
    /// Number of vendors imported
    pub imported: u64,
    /// Number of vendors skipped (duplicate name)
    pub skipped: u64,
    /// Number of rows that failed to parse or create
    pub errors: u64,
    /// Up to 20 error detail strings
    #[serde(default)]
    pub error_details: Vec<String>,
}

/// Parse a CSV byte slice into a list of CreateVendorInput values.
///
/// Returns a tuple of (parsed inputs, row-level error strings).
/// The first non-empty line is treated as the header.
fn parse_vendor_csv(bytes: &[u8]) -> Result<(Vec<CreateVendorInput>, Vec<String>), String> {
    let text = std::str::from_utf8(bytes).map_err(|e| format!("Invalid UTF-8: {}", e))?;

    let mut lines = text.lines().enumerate().peekable();
    let mut header_cols: Vec<String> = Vec::new();

    // Find first non-empty line as header
    while let Some((_, line)) = lines.peek() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            header_cols = parse_csv_line(trimmed)
                .into_iter()
                .map(|c| c.to_lowercase())
                .collect();
            lines.next();
            break;
        }
        lines.next();
    }

    let header_cols = header_cols;
    let name_idx = header_cols
        .iter()
        .position(|c| c == "name")
        .ok_or_else(|| "Missing required column header: name".to_string())?;
    let email_idx = header_cols.iter().position(|c| c == "email");
    let vendor_type_idx = header_cols.iter().position(|c| c == "vendor_type");
    let phone_idx = header_cols.iter().position(|c| c == "phone");
    let tax_id_idx = header_cols.iter().position(|c| c == "tax_id");
    let payment_terms_idx = header_cols.iter().position(|c| c == "payment_terms");
    let vendor_code_idx = header_cols.iter().position(|c| c == "vendor_code");

    let mut inputs: Vec<CreateVendorInput> = Vec::new();
    let mut parse_errors: Vec<String> = Vec::new();

    for (line_num, line) in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let fields = parse_csv_line(trimmed);

        let get_field = |idx: Option<usize>| -> Option<String> {
            idx.and_then(|i| fields.get(i))
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };

        let name = match get_field(Some(name_idx)) {
            Some(n) => n,
            None => {
                parse_errors.push(format!("Line {}: missing vendor name", line_num + 1));
                continue;
            }
        };

        let vendor_type_str = get_field(vendor_type_idx).unwrap_or_default();
        let vendor_type = match vendor_type_str.to_lowercase().as_str() {
            "business" | "" => VendorType::Business,
            "contractor" => VendorType::Contractor,
            "employee" => VendorType::Employee,
            "government" => VendorType::Government,
            "nonprofit" | "non_profit" | "non-profit" => VendorType::NonProfit,
            _ => VendorType::Business,
        };

        inputs.push(CreateVendorInput {
            name,
            legal_name: None,
            vendor_type,
            email: get_field(email_idx),
            phone: get_field(phone_idx),
            website: None,
            address: None,
            tax_id: get_field(tax_id_idx),
            tax_id_type: None,
            payment_terms: get_field(payment_terms_idx),
            default_payment_method: None,
            vendor_code: get_field(vendor_code_idx),
            default_gl_code: None,
            default_department: None,
            notes: None,
            tags: Vec::new(),
        });
    }

    Ok((inputs, parse_errors))
}

/// Parse a single CSV line respecting double-quoted fields containing commas.
fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                // Peek ahead: doubled quote means literal quote
                if chars.peek() == Some(&'"') {
                    chars.next();
                    current.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                current.push(ch);
            }
        } else if ch == '"' {
            in_quotes = true;
        } else if ch == ',' {
            fields.push(current.clone());
            current.clear();
        } else {
            current.push(ch);
        }
    }
    fields.push(current);
    fields
}

#[utoipa::path(post, path = "/api/v1/vendors/import", tag = "Vendors",
    request_body(content = inline(()), content_type = "multipart/form-data"),
    responses((status = 200, description = "Vendors imported", body = ImportVendorsResponse),
              (status = 401, description = "Unauthorized")))]
async fn import_vendors_csv(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    mut multipart: Multipart,
) -> ApiResult<Json<ImportVendorsResponse>> {
    let mut file_bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        billforge_core::Error::Validation(format!("Failed to read multipart data: {}", e))
    })? {
        let field_name = field.name().unwrap_or("").to_string();
        if field_name == "file" {
            file_bytes = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| {
                        billforge_core::Error::Validation(format!(
                            "Failed to read file bytes: {}",
                            e
                        ))
                    })?
                    .to_vec(),
            );
            break;
        }
    }

    let file_bytes = file_bytes
        .ok_or_else(|| billforge_core::Error::Validation("No file uploaded".to_string()))?;

    let (inputs, parse_errors) =
        parse_vendor_csv(&file_bytes).map_err(|e| billforge_core::Error::Validation(e))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool.clone());

    let mut imported: u64 = 0;
    let mut skipped: u64 = 0;
    let mut error_count: u64 = 0;
    let mut error_details: Vec<String> = parse_errors.clone();
    let pagination = Pagination {
        page: 1,
        per_page: 10000,
    };

    for input in &inputs {
        // Idempotency: skip if vendor with same name already exists
        let filters = VendorFilters {
            search: Some(input.name.clone()),
            ..Default::default()
        };
        match repo.list(&tenant.tenant_id, &filters, &pagination).await {
            Ok(existing) => {
                if existing
                    .data
                    .iter()
                    .any(|v| v.name.eq_ignore_ascii_case(&input.name))
                {
                    skipped += 1;
                    continue;
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to check existing vendors");
            }
        }

        match repo.create(&tenant.tenant_id, input.clone()).await {
            Ok(_) => imported += 1,
            Err(e) => {
                error_count += 1;
                if error_details.len() < 20 {
                    error_details.push(format!("Failed to create vendor '{}': {}", input.name, e));
                }
            }
        }
    }

    // Add parse errors to error count
    error_count += parse_errors.len() as u64;

    // Audit
    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(),
        Some(user.user_id.clone()),
        AuditAction::Create,
        ResourceType::Vendor,
        "bulk-import".to_string(),
        format!(
            "Imported {} vendors via spreadsheet ({} skipped, {} errors)",
            imported, skipped, error_count
        ),
    )
    .with_user_email(&user.email);
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    Ok(Json(ImportVendorsResponse {
        imported,
        skipped,
        errors: error_count,
        error_details,
    }))
}

// ---------------------------------------------------------------------------
// Banking verification endpoints (refs #243)
// ---------------------------------------------------------------------------

/// Request body for PUT /:id/banking
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateBankingRequest {
    pub bank_name: String,
    pub account_type: String,
    pub account_number: String,
    pub routing_number: String,
    /// ISO country code for the bank account (e.g. "US", "NG").
    /// Used by fraud guard for country-mismatch detection.
    #[serde(default)]
    pub bank_country: Option<String>,
}

/// Request body for POST /:id/banking-verifications/:vid/verify
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct VerifyBankingRequest {
    pub callback_method: String,
    pub callback_contact: String,
    pub verifier_notes: Option<String>,
}

/// PUT /api/v1/vendors/:id/banking - Update vendor banking details.
///
/// Encrypts account/routing numbers, creates a pending verification row,
/// sets payment_hold=true, and emits an audit entry. Returns 202 Accepted
/// indicating payments are frozen until verified.
async fn update_banking(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
    Json(req): Json<UpdateBankingRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let vendor_id: VendorId = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool.clone());

    // Fetch existing vendor to get previous banking info
    let vendor = repo
        .get_by_id(&tenant.tenant_id, &vendor_id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "Vendor".to_string(),
            id: id.clone(),
        })?;

    let prev_last_four = vendor
        .bank_account
        .as_ref()
        .map(|ba| ba.account_last_four.as_str());

    let new_last_four = req
        .account_number
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();

    // "Encrypt" account and routing numbers.
    // In production this uses envelope encryption; here we use a placeholder
    // that matches the existing BankAccount struct shape.
    // TODO: replace with actual encryption util when available.
    let account_encrypted = format!("enc:{}", req.account_number);
    let routing_encrypted = format!("enc:{}", req.routing_number);

    let verification = repo
        .record_banking_change(
            &tenant.tenant_id,
            &vendor_id,
            prev_last_four,
            &new_last_four,
            &req.bank_name,
            &req.account_type,
            &account_encrypted,
            &routing_encrypted,
            user.user_id.0,
        )
        .await?;

    // Persist bank_country alongside the encrypted banking columns so that
    // verify_banking can read it back and the country_mismatch fraud signal
    // can actually elevate risk on the dual-approval flow (refs #394).
    if let Some(bc) = req.bank_country.as_deref() {
        sqlx::query(
            "UPDATE vendors SET bank_country = $3, updated_at = NOW() WHERE id = $1 AND tenant_id = $2",
        )
        .bind(vendor_id.0)
        .bind(*tenant.tenant_id.as_uuid())
        .bind(bc)
        .execute(&*pool)
        .await
        .map_err(|e| billforge_core::Error::Database(format!("Failed to persist bank country: {}", e)))?;
    }

    // Run fraud-guard checks for this banking change
    let domain = fraud_guard::extract_domain(vendor.email.as_deref(), vendor.website.as_deref());
    let vendor_country = vendor.address.as_ref().map(|a| a.country.as_str());
    let signals = fraud_guard::run_fraud_guard(
        &tenant.tenant_id,
        Some(&vendor_id),
        &vendor.name,
        &domain,
        vendor_country,
        req.bank_country.as_deref(),
        &pool,
    )
    .await;

    // If country mismatch or lookalike is high, force the dual-approver path
    // by adding a note to the hold reason
    let hold_reason = if signals.country_mismatch.risk == fraud_guard::RiskLevel::High
        || signals.lookalike.risk == fraud_guard::RiskLevel::High
    {
        "Banking details changed - fraud guard flagged high risk - dual approval required"
    } else {
        "Banking details changed - pending verification"
    };

    // Update hold reason if fraud guard escalated
    if hold_reason != "Banking details changed - pending verification" {
        sqlx::query(
            "UPDATE vendors SET payment_hold_reason = $3, updated_at = NOW() WHERE id = $1 AND tenant_id = $2",
        )
        .bind(vendor_id.0)
        .bind(*tenant.tenant_id.as_uuid())
        .bind(hold_reason)
        .execute(&*pool)
        .await
        .map_err(|e| billforge_core::Error::Database(format!("Failed to update hold reason: {}", e)))?;
    }

    // Audit entry
    let audit_entry = AuditEntry::new(
        tenant.tenant_id.clone(),
        Some(user.user_id.clone()),
        AuditAction::VendorBankingChanged,
        ResourceType::Vendor,
        id.clone(),
        format!(
            "Banking details changed for vendor {} (pending verification {})",
            vendor.name, verification.id
        ),
    )
    .with_user_email(&user.email)
    .with_metadata(serde_json::json!({
        "verification_id": verification.id.to_string(),
        "prev_last_four": prev_last_four,
        "new_last_four": new_last_four,
        "fraud_signals": fraud_guard::fraud_signals_to_json(&signals),
    }));
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log audit entry");
    }

    // Continuous vendor-risk monitor (#381): a banking-detail change is a
    // leading BEC-fraud signal. Write a critical vendor_risk_alert now so the
    // per-tenant risk dashboard surfaces it and payment release stays blocked
    // until acknowledged. Idempotent on (vendor_id, alert_type, open + same
    // payload hash) so a duplicate PUT does not create duplicate alerts.
    if let Err(e) = crate::routes::vendor_risk_alerts::insert_banking_change_alert(
        &pool,
        &tenant.tenant_id,
        vendor_id.0,
        verification.id,
        prev_last_four,
        &new_last_four,
    )
    .await
    {
        tracing::warn!(error = %e, "Failed to insert banking-change risk alert");
    }

    Ok(Json(serde_json::json!({
        "status": "accepted",
        "verification_id": verification.id,
        "message": "Banking details updated. Payments are frozen until verified.",
        "payment_hold": true,
        "fraud_signals": fraud_guard::fraud_signals_to_json(&signals),
    })))
}

/// POST /api/v1/vendors/:id/banking-verifications/:vid/verify
///
/// Dual-approval state machine for banking verification:
///   - First call (no first_approver): runs screening stubs, records first approval,
///     sets status = pending_second_approval, does NOT clear payment_hold. Returns 202.
///   - Second call by a different user: records second approval, sets status = verified,
///     clears payment_hold. Returns 200.
///   - Same user attempting second approval: returns 403.
///   - Already verified: returns 409.
async fn verify_banking(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path((id, vid)): Path<(String, String)>,
    Json(req): Json<VerifyBankingRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let vendor_id: VendorId = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;
    let verification_id: Uuid = vid
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid verification ID".to_string()))?;

    // Validate callback_method
    match req.callback_method.as_str() {
        "phone" | "in_person" | "known_email" => {}
        _ => {
            return Err(billforge_core::Error::Validation(
                "callback_method must be one of: phone, in_person, known_email".to_string(),
            )
            .into())
        }
    }

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool.clone());

    // Read existing verification row
    let verification = repo
        .get_banking_verification(&tenant.tenant_id, verification_id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "BankingVerification".to_string(),
            id: vid.clone(),
        })?;

    match verification.status {
        BankingVerificationStatus::Verified => {
            return Err(billforge_core::Error::Conflict(
                "Banking verification already completed".to_string(),
            )
            .into());
        }
        BankingVerificationStatus::Rejected => {
            return Err(billforge_core::Error::Conflict(
                "Banking verification was rejected".to_string(),
            )
            .into());
        }
        _ => {}
    }

    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());

    // Fetch vendor for fraud-guard context
    let vendor = repo
        .get_by_id(&tenant.tenant_id, &vendor_id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "Vendor".to_string(),
            id: id.clone(),
        })?;

    if verification.first_approver_id.is_none() {
        // ---- FIRST APPROVAL ----
        // Read the bank_country that update_banking persisted so the
        // country_mismatch fraud signal can actually evaluate on this
        // dual-approval flow (refs #394).
        let bank_country = vendor
            .bank_account
            .as_ref()
            .and_then(|b| b.country.clone());

        // Run screening checks (fraud guard + legacy stubs)
        let screening = run_screening_with_fraud_guard(
            &tenant.tenant_id,
            &vendor_id,
            &vendor.name,
            vendor.email.as_deref(),
            vendor.website.as_deref(),
            vendor.address.as_ref().map(|a| a.country.as_str()),
            bank_country.as_deref(),
            &pool,
        )
        .await;

        let updated = repo
            .record_first_approval(
                &tenant.tenant_id,
                verification_id,
                user.user_id.0,
                &screening,
                &req.callback_method,
                &req.callback_contact,
                req.verifier_notes.as_deref(),
            )
            .await?;

        // Audit: first approval
        let audit_entry = AuditEntry::new(
            tenant.tenant_id.clone(),
            Some(user.user_id.clone()),
            AuditAction::VendorBankingFirstApproved,
            ResourceType::VendorBankingVerification,
            verification_id.to_string(),
            format!(
                "First approval recorded for vendor {} banking change via {}",
                vendor_id, req.callback_method
            ),
        )
        .with_user_email(&user.email)
        .with_metadata(serde_json::json!({
            "verification_id": verification_id.to_string(),
            "vendor_id": id,
            "callback_method": req.callback_method,
            "callback_contact": req.callback_contact,
            "screening_results": screening,
        }));
        if let Err(e) = audit_repo.log(audit_entry).await {
            tracing::warn!(error = %e, "Failed to log audit entry");
        }

        Ok(Json(serde_json::json!({
            "status": "pending_second_approval",
            "verification_id": verification_id,
            "payment_hold": true,
            "screening_results": screening,
        })))
    } else if verification.second_approver_id.is_none() {
        // ---- SECOND APPROVAL ----
        // Separation of duties: caller must not be the same as first approver
        if verification.first_approver_id == Some(user.user_id.0) {
            return Err(billforge_core::Error::Forbidden(
                "Same user cannot provide both approvals (separation of duties)".to_string(),
            )
            .into());
        }

        let updated = repo
            .record_second_approval(&tenant.tenant_id, verification_id, user.user_id.0)
            .await?;

        // Audit: second approval (full verification)
        let audit_entry = AuditEntry::new(
            tenant.tenant_id.clone(),
            Some(user.user_id.clone()),
            AuditAction::VendorBankingVerified,
            ResourceType::VendorBankingVerification,
            verification_id.to_string(),
            format!(
                "Banking change fully verified for vendor {} (dual approval complete)",
                vendor_id
            ),
        )
        .with_user_email(&user.email)
        .with_metadata(serde_json::json!({
            "verification_id": verification_id.to_string(),
            "vendor_id": id,
            "first_approver_id": verification.first_approver_id.map(|u| u.to_string()),
            "second_approver_id": user.user_id.0.to_string(),
        }));
        if let Err(e) = audit_repo.log(audit_entry).await {
            tracing::warn!(error = %e, "Failed to log audit entry");
        }

        Ok(Json(serde_json::json!({
            "status": "verified",
            "verification_id": verification_id,
            "payment_hold": false,
        })))
    } else {
        // Already fully approved (both approvers set)
        Err(
            billforge_core::Error::Conflict("Banking verification already completed".to_string())
                .into(),
        )
    }
}

/// Run screening checks: OFAC screening against bundled SDN list, honest AVS/Plaid
/// stubs, plus fraud-guard signals. Returns a merged JSON value with all keys.
/// The `pool` argument is used for the async fraud-guard checks.
async fn run_screening_with_fraud_guard(
    tenant_id: &TenantId,
    vendor_id: &VendorId,
    vendor_name: &str,
    email: Option<&str>,
    website: Option<&str>,
    vendor_country: Option<&str>,
    bank_country: Option<&str>,
    pool: &sqlx::PgPool,
) -> serde_json::Value {
    let domain = fraud_guard::extract_domain(email, website);
    let signals = fraud_guard::run_fraud_guard(
        tenant_id,
        Some(vendor_id),
        vendor_name,
        &domain,
        vendor_country,
        bank_country,
        pool,
    )
    .await;
    let screener = crate::ofac_screening::OfacScreener::load_latest(pool)
        .await
        .unwrap_or_else(|_| crate::ofac_screening::OfacScreener::load_from_embedded());
    fraud_guard::build_screening_results(&signals, &screener, vendor_name, None)
}

/// GET /api/v1/vendors/:id/banking-verifications
///
/// Lists all banking verifications for a vendor.
async fn list_banking_verifications(
    State(state): State<AppState>,
    VendorMgmtAccess(user, tenant): VendorMgmtAccess,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<BankingVerification>>> {
    let vendor_id: VendorId = id
        .parse()
        .map_err(|_| billforge_core::Error::Validation("Invalid vendor ID".to_string()))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(pool);

    let verifications = repo
        .list_banking_verifications(&tenant.tenant_id, &vendor_id)
        .await?;

    Ok(Json(verifications))
}

/// Check if payments are blocked for a vendor due to pending banking verification.
/// Used by ERP sync and payment code to guard against BEC fraud.
///
/// Also blocks when an open critical vendor_risk_alert exists for the vendor
/// (continuous risk monitor #381), preserving the OFAC + banking-verification
/// holds already in place.
pub async fn is_payment_blocked(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    vendor_id: &VendorId,
) -> bool {
    let repo = billforge_db::repositories::VendorRepositoryImpl::new(Arc::new(pool.clone()));
    if repo
        .has_pending_banking_verification(tenant_id, vendor_id)
        .await
        .unwrap_or(false)
    {
        return true;
    }
    crate::routes::vendor_risk_alerts::vendor_has_open_critical_alert(pool, tenant_id, vendor_id.0)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_vendor_csv_happy_path() {
        let csv = "name,email,vendor_type\nAcme Corp,acme@test.com,Business\nBeta LLC,beta@test.com,Contractor\n";
        let (inputs, errors) = parse_vendor_csv(csv.as_bytes()).unwrap();
        assert_eq!(inputs.len(), 2);
        assert!(errors.is_empty());

        assert_eq!(inputs[0].name, "Acme Corp");
        assert_eq!(inputs[0].email.as_deref(), Some("acme@test.com"));
        assert_eq!(inputs[0].vendor_type, VendorType::Business);

        assert_eq!(inputs[1].name, "Beta LLC");
        assert_eq!(inputs[1].email.as_deref(), Some("beta@test.com"));
        assert_eq!(inputs[1].vendor_type, VendorType::Contractor);
    }

    #[test]
    fn test_parse_vendor_csv_missing_name_column() {
        let csv = "email,vendor_type\ntest@test.com,Business\n";
        let result = parse_vendor_csv(csv.as_bytes());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Missing required column header: name"));
    }

    #[test]
    fn test_parse_vendor_csv_blank_vendor_type_defaults_to_business() {
        let csv = "name,email,vendor_type\nAcme Corp,acme@test.com,\n";
        let (inputs, _) = parse_vendor_csv(csv.as_bytes()).unwrap();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].vendor_type, VendorType::Business);
    }

    #[test]
    fn test_parse_vendor_csv_unknown_vendor_type_defaults_to_business() {
        let csv = "name,vendor_type\nAcme Corp,something_unknown\n";
        let (inputs, _) = parse_vendor_csv(csv.as_bytes()).unwrap();
        assert_eq!(inputs[0].vendor_type, VendorType::Business);
    }

    #[test]
    fn test_parse_vendor_csv_quoted_field_with_comma() {
        let csv = "name,email\n\"Acme, Inc.\",acme@test.com\n";
        let (inputs, _) = parse_vendor_csv(csv.as_bytes()).unwrap();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].name, "Acme, Inc.");
        assert_eq!(inputs[0].email.as_deref(), Some("acme@test.com"));
    }

    #[test]
    fn test_parse_vendor_csv_missing_name_in_row() {
        let csv = "name,email\n,beta@test.com\n";
        let (inputs, errors) = parse_vendor_csv(csv.as_bytes()).unwrap();
        assert!(inputs.is_empty());
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("missing vendor name"));
    }

    #[test]
    fn test_parse_vendor_csv_rejects_invalid_utf8() {
        let invalid = [0xff, 0xfe, b'n', b'a', b'm', b'e'];
        let result = parse_vendor_csv(&invalid);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid UTF-8"));
    }

    #[test]
    fn test_parse_vendor_csv_skips_blank_lines_before_header() {
        let csv = "\n\n name,email\n Acme Corp,ap@acme.test\n";
        let (inputs, errors) = parse_vendor_csv(csv.as_bytes()).unwrap();

        assert!(errors.is_empty());
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].name, "Acme Corp");
        assert_eq!(inputs[0].email.as_deref(), Some("ap@acme.test"));
    }

    #[test]
    fn test_parse_vendor_csv_optional_fields() {
        let csv = "name,tax_id,payment_terms,vendor_code\nAcme Corp,12-3456789,Net 30,V-001\n";
        let (inputs, _) = parse_vendor_csv(csv.as_bytes()).unwrap();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].tax_id.as_deref(), Some("12-3456789"));
        assert_eq!(inputs[0].payment_terms.as_deref(), Some("Net 30"));
        assert_eq!(inputs[0].vendor_code.as_deref(), Some("V-001"));
        assert!(inputs[0].email.is_none());
    }

    #[test]
    fn test_parse_csv_line_simple() {
        let result = parse_csv_line("a,b,c");
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_parse_csv_line_quoted_comma() {
        let result = parse_csv_line("\"hello, world\",b,c");
        assert_eq!(result, vec!["hello, world", "b", "c"]);
    }

    #[test]
    fn test_parse_csv_line_doubled_quote() {
        let result = parse_csv_line("\"he said \"\"hi\"\"\",b");
        assert_eq!(result, vec!["he said \"hi\"", "b"]);
    }
}
