//! Vendor self-service portal routes.
//!
//! Public endpoints (bypass the global tenant-JWT middleware via PUBLIC_PATHS)
//! that validate their own vendor-portal JWT. The token carries tenant_id +
//! vendor_id so all queries stay properly scoped.

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::{
    extract::State,
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use billforge_core::{
    domain::{
        AuditAction, AuditEntry, CreateInvoiceInput, InvoiceId, ResourceType, VendorId,
    },
    traits::{AuditService, InvoiceRepository, VendorRepository},
    types::{Money, TenantId},
    Error,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Route registration
// ---------------------------------------------------------------------------

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/invoices", get(list_invoices))
        .route("/invoices", post(submit_invoice))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract tenant_id and vendor_id from a VendorPortal JWT in the
/// Authorization header. Returns 401 on any failure.
fn vendor_ctx(
    headers: &HeaderMap,
    auth: &billforge_auth::AuthService,
) -> Result<(TenantId, VendorId), ApiError> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or_else(|| ApiError(Error::Unauthenticated))?;

    let claims = auth.jwt_service().validate_vendor_portal_token(token)?;

    let tenant_id = claims
        .tenant_id()
        .map_err(|e| ApiError(Error::InvalidToken(e.to_string())))?;

    let vid_str = claims
        .vendor_id
        .ok_or_else(|| ApiError(Error::InvalidToken("Missing vendor_id claim".to_string())))?;
    let vendor_uuid =
        Uuid::parse_str(&vid_str).map_err(|_| ApiError(Error::InvalidToken("Invalid vendor_id".to_string())))?;
    let vendor_id = VendorId(vendor_uuid);

    Ok((tenant_id, vendor_id))
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct VendorInvoiceRow {
    pub id: String,
    pub invoice_number: String,
    pub invoice_date: Option<String>,
    pub due_date: Option<String>,
    pub total_amount: i64,
    pub currency: String,
    pub processing_status: String,
}

#[derive(Debug, Serialize)]
pub struct SubmitInvoiceResponse {
    pub id: String,
    pub invoice_number: String,
}

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SubmitInvoiceBody {
    pub invoice_number: String,
    pub invoice_date: Option<String>,
    pub due_date: Option<String>,
    pub amount: i64,
    pub currency: Option<String>,
    pub notes: Option<String>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn list_invoices(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<Vec<VendorInvoiceRow>>> {
    let (tenant_id, vendor_id) = vendor_ctx(&headers, &state.auth)?;

    let pool = state.db.tenant(&tenant_id).await?;
    let repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());

    let filters = billforge_core::domain::InvoiceFilters {
        vendor_id: Some(vendor_id.0),
        ..Default::default()
    };
    let pagination = billforge_core::types::Pagination {
        page: 1,
        per_page: 100,
    };

    let invoices = repo.list(&tenant_id, &filters, &pagination).await?;

    let rows: Vec<VendorInvoiceRow> = invoices
        .data
        .into_iter()
        .map(|inv| VendorInvoiceRow {
            id: inv.id.to_string(),
            invoice_number: inv.invoice_number,
            invoice_date: inv.invoice_date.map(|d| d.to_string()),
            due_date: inv.due_date.map(|d| d.to_string()),
            total_amount: inv.total_amount.amount,
            currency: inv.currency,
            processing_status: inv.processing_status.as_str().to_string(),
        })
        .collect();

    Ok(Json(rows))
}

async fn submit_invoice(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SubmitInvoiceBody>,
) -> ApiResult<Json<SubmitInvoiceResponse>> {
    let (tenant_id, vendor_id) = vendor_ctx(&headers, &state.auth)?;

    // Look up vendor name for the audit-friendly display
    let pool = state.db.tenant(&tenant_id).await?;
    let vendor_repo = billforge_db::repositories::VendorRepositoryImpl::new(pool.clone());
    let vendor = vendor_repo
        .get_by_id(&tenant_id, &vendor_id)
        .await?
        .ok_or_else(|| Error::NotFound {
            resource_type: "Vendor".to_string(),
            id: vendor_id.to_string(),
        })?;

    let invoice_date = body
        .invoice_date
        .and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok());
    let due_date = body
        .due_date
        .and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok());
    let currency = body.currency.unwrap_or_else(|| "USD".to_string());

    let input = CreateInvoiceInput {
        document_id: Uuid::new_v4(),
        vendor_id: Some(vendor_id.0),
        vendor_name: vendor.name.clone(),
        invoice_number: body.invoice_number.clone(),
        invoice_date,
        due_date,
        po_number: None,
        subtotal: None,
        tax_amount: None,
        total_amount: Money {
            amount: body.amount,
            currency: currency.clone(),
        },
        currency,
        line_items: vec![],
        ocr_confidence: None,
        department: None,
        gl_code: None,
        cost_center: None,
        notes: body.notes,
        tags: vec![],
    };

    let invoice_repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());

    // Vendor portal submissions have no internal user
    let invoice = invoice_repo.create(&tenant_id, input, None).await?;

    // Set processing_status to 'submitted' for vendor-submitted invoices
    sqlx::query(
        "UPDATE invoices SET processing_status = 'submitted', updated_at = NOW() WHERE id = $1 AND tenant_id = $2",
    )
    .bind(invoice.id.0)
    .bind(*tenant_id.as_uuid())
    .execute(&*pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to set submitted status: {}", e)))?;

    // Audit entry
    let audit_entry = AuditEntry::new(
        tenant_id.clone(),
        None,
        AuditAction::Create,
        ResourceType::Invoice,
        invoice.id.to_string(),
        format!(
            "Vendor portal invoice submission by vendor {} ({})",
            vendor.name, vendor_id
        ),
    )
    .with_metadata(serde_json::json!({
        "source": "vendor_portal",
        "vendor_id": vendor_id.to_string(),
    }));
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log vendor portal audit entry");
    }

    Ok(Json(SubmitInvoiceResponse {
        id: invoice.id.to_string(),
        invoice_number: body.invoice_number,
    }))
}
