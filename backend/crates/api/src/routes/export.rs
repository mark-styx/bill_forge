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
use serde::Deserialize;

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
    // TODO: Implement actual CSV export using DuckDB's COPY TO
    let csv_content = r#"invoice_id,vendor_name,invoice_number,invoice_date,total_amount,status
inv-001,Acme Corp,INV-2024-001,2024-01-15,1500.00,approved
inv-002,TechSupplies Inc,TS-5678,2024-01-18,2300.00,pending_approval
"#;

    let response = Response::builder()
        .header(header::CONTENT_TYPE, "text/csv")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"invoices.csv\"",
        )
        .body(csv_content.to_string())
        .unwrap();

    Ok(response.into_response())
}

async fn export_invoices_json(
    State(state): State<AppState>,
    InvoiceCaptureAccess(user, tenant): InvoiceCaptureAccess,
    Query(query): Query<ExportQuery>,
) -> ApiResult<Response> {
    // TODO: Implement actual JSON export
    let json_content = serde_json::json!({
        "invoices": [
            {
                "invoice_id": "inv-001",
                "vendor_name": "Acme Corp",
                "invoice_number": "INV-2024-001",
                "invoice_date": "2024-01-15",
                "total_amount": 1500.00,
                "status": "approved"
            }
        ],
        "exported_at": chrono::Utc::now().to_rfc3339(),
        "tenant_id": tenant.tenant_id.as_str()
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
    Query(query): Query<ExportQuery>,
) -> ApiResult<Response> {
    // TODO: Implement actual vendor CSV export
    let csv_content = r#"vendor_id,name,email,status,vendor_type
v-001,Acme Corp,billing@acme.com,active,business
v-002,TechSupplies Inc,ap@techsupplies.com,active,business
"#;

    let response = Response::builder()
        .header(header::CONTENT_TYPE, "text/csv")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"vendors.csv\"",
        )
        .body(csv_content.to_string())
        .unwrap();

    Ok(response.into_response())
}
