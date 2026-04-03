//! Data export routes

use crate::error::ApiResult;
use crate::extractors::InvoiceCaptureAccess;
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    http::header,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use billforge_core::traits::{InvoiceRepository, VendorRepository};
use serde::Deserialize;

/// Helper function to escape CSV fields (quotes and commas)
fn csv_escape(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/invoices/csv", get(export_invoices_csv))
        .route("/invoices/json", get(export_invoices_json))
        .route("/vendors/csv", get(export_vendors_csv))
}

#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub status: Option<String>,
    pub vendor_id: Option<String>,
}

async fn export_invoices_csv(
    State(state): State<AppState>,
    InvoiceCaptureAccess(user, tenant): InvoiceCaptureAccess,
    Query(query): Query<ExportQuery>,
) -> ApiResult<Response> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Build filters from query parameters
    let filters = billforge_core::domain::InvoiceFilters {
        vendor_id: query.vendor_id.and_then(|v| uuid::Uuid::parse_str(&v).ok()),
        processing_status: query
            .status
            .and_then(|s| billforge_core::domain::ProcessingStatus::from_str(&s)),
        date_from: query
            .start_date
            .and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok()),
        date_to: query
            .end_date
            .and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok()),
        ..Default::default()
    };

    let pagination = billforge_core::types::Pagination {
        page: 1,
        per_page: 10000, // Large limit for export
    };

    let invoice_repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool);
    let result = invoice_repo
        .list(&tenant.tenant_id, &filters, &pagination)
        .await?;

    // Build CSV content
    let mut csv_rows = vec![
        "invoice_id,vendor_name,invoice_number,invoice_date,due_date,total_amount,currency,status,po_number,department"
            .to_string(),
    ];

    for invoice in result.data {
        let row = format!(
            "{},{},{},{},{},{},{},{},{},{}",
            invoice.id.0,
            csv_escape(&invoice.vendor_name),
            csv_escape(&invoice.invoice_number),
            invoice
                .invoice_date
                .map(|d| d.to_string())
                .unwrap_or_default(),
            invoice.due_date.map(|d| d.to_string()).unwrap_or_default(),
            invoice.total_amount.amount as f64 / 100.0,
            invoice.currency,
            invoice.processing_status.as_str(),
            csv_escape(invoice.po_number.as_deref().unwrap_or("")),
            csv_escape(invoice.department.as_deref().unwrap_or("")),
        );
        csv_rows.push(row);
    }

    let csv_content = csv_rows.join("\n");

    let response = Response::builder()
        .header(header::CONTENT_TYPE, "text/csv")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"invoices.csv\"",
        )
        .body(csv_content)
        .unwrap();

    Ok(response.into_response())
}

async fn export_invoices_json(
    State(state): State<AppState>,
    InvoiceCaptureAccess(user, tenant): InvoiceCaptureAccess,
    Query(query): Query<ExportQuery>,
) -> ApiResult<Response> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    // Build filters from query parameters
    let filters = billforge_core::domain::InvoiceFilters {
        vendor_id: query.vendor_id.and_then(|v| uuid::Uuid::parse_str(&v).ok()),
        processing_status: query
            .status
            .and_then(|s| billforge_core::domain::ProcessingStatus::from_str(&s)),
        date_from: query
            .start_date
            .and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok()),
        date_to: query
            .end_date
            .and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok()),
        ..Default::default()
    };

    let pagination = billforge_core::types::Pagination {
        page: 1,
        per_page: 10000, // Large limit for export
    };

    let invoice_repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool);
    let result = invoice_repo
        .list(&tenant.tenant_id, &filters, &pagination)
        .await?;

    // Build JSON structure
    let json_content = serde_json::json!({
        "invoices": result.data.into_iter().map(|inv| serde_json::json!({
            "invoice_id": inv.id.0.to_string(),
            "vendor_id": inv.vendor_id.map(|v| v.to_string()),
            "vendor_name": inv.vendor_name,
            "invoice_number": inv.invoice_number,
            "invoice_date": inv.invoice_date.map(|d| d.to_string()),
            "due_date": inv.due_date.map(|d| d.to_string()),
            "po_number": inv.po_number,
            "subtotal": inv.subtotal.map(|m| m.amount as f64 / 100.0),
            "tax_amount": inv.tax_amount.map(|m| m.amount as f64 / 100.0),
            "total_amount": inv.total_amount.amount as f64 / 100.0,
            "currency": inv.currency,
            "status": inv.processing_status.as_str(),
            "capture_status": inv.capture_status.as_str(),
            "department": inv.department,
            "gl_code": inv.gl_code,
            "line_items": inv.line_items.into_iter().map(|item| serde_json::json!({
                "line_number": item.line_number,
                "description": item.description,
                "quantity": item.quantity,
                "unit_price": item.unit_price.map(|p| p.amount as f64 / 100.0),
                "amount": item.amount.amount as f64 / 100.0,
                "gl_code": item.gl_code,
                "department": item.department,
            })).collect::<Vec<_>>(),
            "created_at": inv.created_at.to_rfc3339(),
        })).collect::<Vec<_>>(),
        "exported_at": chrono::Utc::now().to_rfc3339(),
        "tenant_id": tenant.tenant_id.as_str(),
        "total_count": result.pagination.total_items,
    });

    let response = Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"invoices.json\"",
        )
        .body(serde_json::to_string_pretty(&json_content).unwrap())
        .unwrap();

    Ok(response.into_response())
}

async fn export_vendors_csv(
    State(state): State<AppState>,
    InvoiceCaptureAccess(user, tenant): InvoiceCaptureAccess,
    Query(_query): Query<ExportQuery>,
) -> ApiResult<Response> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let filters = billforge_core::domain::VendorFilters::default();
    let pagination = billforge_core::types::Pagination {
        page: 1,
        per_page: 10000, // Large limit for export
    };

    let vendor_repo = billforge_db::repositories::VendorRepositoryImpl::new(pool);
    let result = vendor_repo
        .list(&tenant.tenant_id, &filters, &pagination)
        .await?;

    // Build CSV content
    let mut csv_rows = vec![
        "vendor_id,name,legal_name,email,phone,status,vendor_type,tax_id,payment_terms".to_string(),
    ];

    for vendor in result.data {
        let status_str = serde_json::to_string(&vendor.status)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        let type_str = serde_json::to_string(&vendor.vendor_type)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();

        let row = format!(
            "{},{},{},{},{},{},{},{},{}",
            vendor.id.0,
            csv_escape(&vendor.name),
            csv_escape(&vendor.legal_name.unwrap_or_default()),
            csv_escape(vendor.email.as_deref().unwrap_or("")),
            csv_escape(vendor.phone.as_deref().unwrap_or("")),
            status_str,
            type_str,
            csv_escape(&vendor.tax_id.unwrap_or_default()),
            csv_escape(&vendor.payment_terms.unwrap_or_default()),
        );
        csv_rows.push(row);
    }

    let csv_content = csv_rows.join("\n");

    let response = Response::builder()
        .header(header::CONTENT_TYPE, "text/csv")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"vendors.csv\"",
        )
        .body(csv_content)
        .unwrap();

    Ok(response.into_response())
}
