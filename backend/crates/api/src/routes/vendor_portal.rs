//! Vendor self-service portal routes.
//!
//! Public endpoints (bypass the global tenant-JWT middleware via PUBLIC_PATHS)
//! that validate their own vendor-portal JWT. The token carries tenant_id +
//! vendor_id so all queries stay properly scoped.

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::{
    extract::{Multipart, State},
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use billforge_core::{
    domain::{AuditAction, AuditEntry, CreateInvoiceInput, ResourceType, VendorId},
    traits::{AuditService, InvoiceRepository, VendorRepository, StorageService},
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
        .route("/invoices/upload", post(upload_invoice_pdf))
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
    let vendor_uuid = Uuid::parse_str(&vid_str)
        .map_err(|_| ApiError(Error::InvalidToken("Invalid vendor_id".to_string())))?;
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

// ---------------------------------------------------------------------------
// PDF upload handler
// ---------------------------------------------------------------------------

const MAX_PDF_SIZE: usize = 15 * 1024 * 1024; // 15 MB

async fn upload_invoice_pdf(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> ApiResult<Json<SubmitInvoiceResponse>> {
    let (tenant_id, vendor_id) = vendor_ctx(&headers, &state.auth)?;

    let mut file_bytes: Option<Vec<u8>> = None;
    let mut original_filename: Option<String> = None;
    let mut invoice_number: Option<String> = None;
    let mut invoice_date: Option<String> = None;
    let mut due_date: Option<String> = None;
    let mut amount: Option<i64> = None;
    let mut currency: Option<String> = None;
    let mut notes: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| Error::Validation(format!("Failed to read multipart data: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "file" => {
                let ct = field
                    .content_type()
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                if ct != "application/pdf" {
                    return Err(Error::Validation("Only PDF files are accepted".to_string()).into());
                }
                original_filename = field.file_name().map(|s| s.to_string());
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| Error::Validation(format!("Failed to read file: {}", e)))?;
                if data.len() > MAX_PDF_SIZE {
                    return Err(
                        Error::Validation("PDF exceeds maximum size (15 MB)".to_string()).into(),
                    );
                }
                file_bytes = Some(data.to_vec());
            }
            "invoice_number" => {
                invoice_number = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| Error::Validation(format!("Failed to read field: {}", e)))?,
                );
            }
            "invoice_date" => {
                invoice_date = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| Error::Validation(format!("Failed to read field: {}", e)))?,
                );
            }
            "due_date" => {
                due_date = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| Error::Validation(format!("Failed to read field: {}", e)))?,
                );
            }
            "amount" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| Error::Validation(format!("Failed to read field: {}", e)))?;
                amount = Some(text.parse::<i64>().map_err(|_| {
                    Error::Validation("Invalid amount value".to_string())
                })?);
            }
            "currency" => {
                currency = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| Error::Validation(format!("Failed to read field: {}", e)))?,
                );
            }
            "notes" => {
                notes = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| Error::Validation(format!("Failed to read field: {}", e)))?,
                );
            }
            _ => {} // ignore unknown fields
        }
    }

    let file_data = file_bytes.ok_or_else(|| {
        Error::Validation("Missing required field: file".to_string())
    })?;
    let invoice_number_val = invoice_number.ok_or_else(|| {
        Error::Validation("Missing required field: invoice_number".to_string())
    })?;

    // Persist PDF via storage service (upload generates and returns the document_id)
    let document_id = state
        .storage
        .upload(
            &tenant_id,
            "invoice.pdf",
            &file_data,
            "application/pdf",
        )
        .await
        .map_err(|e| Error::Database(format!("Failed to store PDF: {}", e)))?;

    // Look up vendor name
    let pool = state.db.tenant(&tenant_id).await?;
    let vendor_repo = billforge_db::repositories::VendorRepositoryImpl::new(pool.clone());
    let vendor = vendor_repo
        .get_by_id(&tenant_id, &vendor_id)
        .await?
        .ok_or_else(|| Error::NotFound {
            resource_type: "Vendor".to_string(),
            id: vendor_id.to_string(),
        })?;

    let parsed_invoice_date = invoice_date
        .and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok());
    let parsed_due_date = due_date.and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok());
    let currency_val = currency.unwrap_or_else(|| "USD".to_string());
    let byte_size = file_data.len();

    let fname = original_filename.clone().unwrap_or_default();
    let combined_notes = match notes {
        Some(ref n) if !n.is_empty() => format!("Uploaded PDF: {} | {}", fname, n),
        _ => format!("Uploaded PDF: {}", fname),
    };

    let input = CreateInvoiceInput {
        document_id,
        vendor_id: Some(vendor_id.0),
        vendor_name: vendor.name.clone(),
        invoice_number: invoice_number_val.clone(),
        invoice_date: parsed_invoice_date,
        due_date: parsed_due_date,
        po_number: None,
        subtotal: None,
        tax_amount: None,
        total_amount: Money {
            amount: amount.unwrap_or(0),
            currency: currency_val.clone(),
        },
        currency: currency_val,
        line_items: vec![],
        ocr_confidence: None,
        department: None,
        gl_code: None,
        cost_center: None,
        notes: Some(combined_notes),
        tags: vec![],
    };

    let invoice_repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());
    let invoice = invoice_repo.create(&tenant_id, input, None).await?;

    // Set processing_status to 'submitted'
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
            "Vendor portal PDF upload by vendor {} ({})",
            vendor.name, vendor_id
        ),
    )
    .with_metadata(serde_json::json!({
        "source": "vendor_portal_pdf",
        "vendor_id": vendor_id.to_string(),
        "original_filename": original_filename,
        "byte_size": byte_size,
    }));
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log vendor portal PDF audit entry");
    }

    Ok(Json(SubmitInvoiceResponse {
        id: invoice.id.to_string(),
        invoice_number: invoice_number_val,
    }))
}
