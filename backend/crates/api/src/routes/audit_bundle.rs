//! Audit evidence bundle export - signed, time-stamped ZIP for SOC/SOX compliance
//!
//! Generates a single ZIP containing: invoice PDFs, OCR diffs, approval chain with IPs,
//! GL coding history, policy version, and a tamper-evident hash-chain manifest signed
//! with ed25519.

use crate::error::ApiError;
use crate::extractors::AuthUser;
use crate::state::AppState;
use axum::{
    extract::{Query, State},
    http::header,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use base64::Engine as _;
use billforge_core::{traits::InvoiceRepository, types::Role, Error};
use ed25519_dalek::{SigningKey, Signer};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Write;

#[derive(Debug, Deserialize)]
pub struct BundleQuery {
    from: String,
    to: String,
    invoice_ids: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ManifestEntry {
    path: String,
    sha256: String,
    size: u64,
    prev_hash: String,
    entry_hash: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    tenant_id: String,
    from: String,
    to: String,
    generated_at: String,
    generator_version: String,
    entry_count: usize,
    entries: Vec<ManifestEntry>,
    root_hash: String,
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/evidence_bundle", get(export))
}

#[utoipa::path(
    get,
    path = "/api/v1/audit/evidence_bundle",
    tag = "Audit",
    params(
        ("from" = String, Query, description = "Start date (ISO 8601)"),
        ("to" = String, Query, description = "End date (ISO 8601)"),
        ("invoice_ids" = Option<String>, Query, description = "Comma-separated invoice IDs to include"),
    ),
    responses(
        (status = 200, description = "Signed audit evidence bundle ZIP", content_type = "application/zip"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Signing key not configured or internal error"),
    )
)]
async fn export(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Query(params): Query<BundleQuery>,
) -> Result<Response, ApiError> {
    // Only admins can generate evidence bundles
    if !user.has_role(Role::TenantAdmin) {
        return Err(ApiError(Error::Forbidden(
            "Only administrators can generate audit evidence bundles".to_string(),
        )));
    }

    // Validate signing key is present
    let signing_key_b64 = std::env::var("BILLFORGE_EVIDENCE_SIGNING_KEY").map_err(|_| {
        ApiError(Error::Configuration(
            "BILLFORGE_EVIDENCE_SIGNING_KEY environment variable not set".to_string(),
        ))
    })?;

    let signing_key_bytes = base64::engine::general_purpose::STANDARD
        .decode(&signing_key_b64)
        .map_err(|e| {
            ApiError(Error::Configuration(format!(
                "Invalid BILLFORGE_EVIDENCE_SIGNING_KEY (base64 decode failed): {}",
                e
            )))
        })?;

    if signing_key_bytes.len() != 32 {
        return Err(ApiError(Error::Configuration(
            "BILLFORGE_EVIDENCE_SIGNING_KEY must be 32 bytes (ed25519 seed)".to_string(),
        )));
    }

    let mut seed = [0u8; 32];
    seed.copy_from_slice(&signing_key_bytes);
    let signing_key = SigningKey::from_bytes(&seed);
    let verifying_key = signing_key.verifying_key();

    let tenant_id = &user.tenant_id;
    let pool = state.db.tenant(tenant_id).await?;

    // Resolve date range
    let from_dt = chrono::DateTime::parse_from_rfc3339(&params.from)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .map_err(|e| ApiError(Error::Validation(format!("Invalid 'from' date: {}", e))))?;
    let _to_dt = chrono::DateTime::parse_from_rfc3339(&params.to)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .map_err(|e| ApiError(Error::Validation(format!("Invalid 'to' date: {}", e))))?;

    // Fetch invoices for the date range, tenant-scoped
    let invoice_filters = billforge_core::domain::InvoiceFilters {
        date_from: Some(from_dt.date_naive()),
        date_to: Some(_to_dt.date_naive()),
        ..Default::default()
    };

    // If specific invoice_ids provided, we'll filter after fetch
    let requested_ids: Option<Vec<uuid::Uuid>> = params.invoice_ids.as_ref().and_then(|csv| {
        csv.split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(uuid::Uuid::parse_str)
            .collect::<Result<Vec<_>, _>>()
            .ok()
    });

    let pagination = billforge_core::types::Pagination {
        page: 1,
        per_page: 10000,
    };

    let invoice_repo = billforge_db::repositories::InvoiceRepositoryImpl::new(pool.clone());
    let result = invoice_repo
        .list(tenant_id, &invoice_filters, &pagination)
        .await?;

    // Filter to requested IDs if specified, always tenant-scoped
    let invoices: Vec<_> = result
        .data
        .into_iter()
        .filter(|inv| {
            requested_ids
                .as_ref()
                .map_or(true, |ids| ids.contains(&inv.id.0))
        })
        .collect();

    // Build ZIP in memory
    let mut zip_buf = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_buf));
        let zip_options =
            zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        let mut manifest_entries: Vec<ManifestEntry> = Vec::new();
        let mut prev_hash =
            "0000000000000000000000000000000000000000000000000000000000000000".to_string();

        for invoice in &invoices {
            let inv_id = invoice.id.0.to_string();

            // 1. Invoice PDF
            let pdf_path = format!("invoices/{}.pdf", inv_id);
            match state.storage.download(tenant_id, invoice.document_id).await {
                Ok(pdf_bytes) => {
                    let (sha, size) = hash_and_size(&pdf_bytes);
                    zip.start_file(&pdf_path, zip_options).map_err(zip_err)?;
                    zip.write_all(&pdf_bytes).map_err(io_err)?;
                    append_entry(&mut manifest_entries, &mut prev_hash, &pdf_path, &sha, size);
                }
                Err(_) => {
                    let missing = format!("invoices/{}.missing", inv_id);
                    let content = format!(
                        "PDF not available for invoice {}. Storage lookup failed or document_id missing.",
                        inv_id
                    );
                    let (sha, size) = hash_and_size(content.as_bytes());
                    zip.start_file(&missing, zip_options).map_err(zip_err)?;
                    zip.write_all(content.as_bytes()).map_err(io_err)?;
                    append_entry(&mut manifest_entries, &mut prev_hash, &missing, &sha, size);
                }
            }

            // 2. OCR diff
            let ocr_path = format!("ocr/{}.diff.json", inv_id);
            let ocr_content = fetch_ocr_record(&pool, &inv_id).await;
            let (sha, size) = hash_and_size(ocr_content.as_bytes());
            zip.start_file(&ocr_path, zip_options).map_err(zip_err)?;
            zip.write_all(ocr_content.as_bytes()).map_err(io_err)?;
            append_entry(&mut manifest_entries, &mut prev_hash, &ocr_path, &sha, size);

            // 3. Approval chain
            let approvals_path = format!("approvals/{}.json", inv_id);
            let approvals = fetch_approvals(&pool, tenant_id.as_uuid(), &inv_id).await;
            let approvals_json =
                serde_json::to_string_pretty(&approvals).unwrap_or_else(|_| "[]".to_string());
            let (sha, size) = hash_and_size(approvals_json.as_bytes());
            zip.start_file(&approvals_path, zip_options).map_err(zip_err)?;
            zip.write_all(approvals_json.as_bytes()).map_err(io_err)?;
            append_entry(&mut manifest_entries, &mut prev_hash, &approvals_path, &sha, size);

            // 4. GL coding history
            let gl_path = format!("gl_coding/{}.json", inv_id);
            let gl_coding = fetch_gl_coding(&pool, tenant_id.as_uuid(), &inv_id).await;
            let gl_json =
                serde_json::to_string_pretty(&gl_coding).unwrap_or_else(|_| "[]".to_string());
            let (sha, size) = hash_and_size(gl_json.as_bytes());
            zip.start_file(&gl_path, zip_options).map_err(zip_err)?;
            zip.write_all(gl_json.as_bytes()).map_err(io_err)?;
            append_entry(&mut manifest_entries, &mut prev_hash, &gl_path, &sha, size);
        }

        // 5. Policy version
        let policy_path = "policy/policy_versions.json".to_string();
        let policy_json = serde_json::to_string_pretty(&serde_json::json!({
            "policy_version_source": "current_at_export",
            "note": "Historical policy snapshots begin from export date. See deferred scope."
        }))
        .unwrap_or_else(|_| "{}".to_string());
        let (sha, size) = hash_and_size(policy_json.as_bytes());
        zip.start_file(&policy_path, zip_options).map_err(zip_err)?;
        zip.write_all(policy_json.as_bytes()).map_err(io_err)?;
        append_entry(&mut manifest_entries, &mut prev_hash, &policy_path, &sha, size);

        // 6. WARNING.txt for policy snapshot caveat
        let warning_path = "WARNING.txt".to_string();
        let warning = "NOTE: Policy version embedded in this bundle reflects the current policy at export time, \
            not the policy in effect when each invoice was originally approved. \
            Historical policy snapshot tracking is planned for a future release.";
        let (sha, size) = hash_and_size(warning.as_bytes());
        zip.start_file(&warning_path, zip_options).map_err(zip_err)?;
        zip.write_all(warning.as_bytes()).map_err(io_err)?;
        append_entry(&mut manifest_entries, &mut prev_hash, &warning_path, &sha, size);

        // 7. Manifest
        let root_hash = manifest_entries
            .last()
            .map(|e| e.entry_hash.clone())
            .unwrap_or_default();

        let manifest = Manifest {
            tenant_id: tenant_id.as_str().to_string(),
            from: params.from.clone(),
            to: params.to.clone(),
            generated_at: chrono::Utc::now().to_rfc3339(),
            generator_version: env!("CARGO_PKG_VERSION").to_string(),
            entry_count: manifest_entries.len(),
            entries: manifest_entries,
            root_hash: root_hash.clone(),
        };

        let manifest_json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| ApiError(Error::Internal(format!("Failed to serialize manifest: {}", e))))?;

        // manifest.json
        zip.start_file("manifest.json", zip_options).map_err(zip_err)?;
        zip.write_all(manifest_json.as_bytes()).map_err(io_err)?;

        // manifest.sig - ed25519 signature of manifest.json bytes
        let signature = signing_key.sign(manifest_json.as_bytes());
        zip.start_file("manifest.sig", zip_options).map_err(zip_err)?;
        zip.write_all(signature.to_bytes().as_slice()).map_err(io_err)?;

        // manifest.pubkey
        let pubkey_bytes = verifying_key.to_bytes();
        zip.start_file("manifest.pubkey", zip_options).map_err(zip_err)?;
        zip.write_all(pubkey_bytes.as_slice()).map_err(io_err)?;

        zip.finish().map_err(zip_err)?;
    }

    let filename = format!(
        "evidence_{}_{}_{}.zip",
        tenant_id.as_str(),
        params.from.replace(':', "-"),
        params.to.replace(':', "-")
    );

    let response = Response::builder()
        .header(header::CONTENT_TYPE, "application/zip")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename),
        )
        .header(header::CONTENT_LENGTH, zip_buf.len())
        .body(zip_buf.into())
        .map_err(|e| ApiError(Error::Internal(format!("Failed to build response: {}", e))))?;

    Ok(response)
}

/// Compute SHA-256 and size of bytes
fn hash_and_size(data: &[u8]) -> (String, u64) {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let hash = hasher.finalize();
    (hex::encode(hash), data.len() as u64)
}

/// Compute entry_hash = SHA256(prev_hash || path || file_sha256)
fn compute_entry_hash(prev_hash: &str, path: &str, file_sha256: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(prev_hash.as_bytes());
    hasher.update(path.as_bytes());
    hasher.update(file_sha256.as_bytes());
    hex::encode(hasher.finalize())
}

/// Append an entry to the manifest entries list, advancing the hash chain
fn append_entry(
    entries: &mut Vec<ManifestEntry>,
    prev_hash: &mut String,
    path: &str,
    sha256: &str,
    size: u64,
) {
    let entry_hash = compute_entry_hash(prev_hash, path, sha256);
    let entry = ManifestEntry {
        path: path.to_string(),
        sha256: sha256.to_string(),
        size,
        prev_hash: prev_hash.clone(),
        entry_hash: entry_hash.clone(),
    };
    *prev_hash = entry_hash.clone();
    entries.push(entry);
}

/// Fetch OCR record for an invoice. Returns a JSON string.
async fn fetch_ocr_record(pool: &sqlx::PgPool, invoice_id: &str) -> String {
    let row: Option<(Option<String>, Option<serde_json::Value>)> = sqlx::query_as(
        "SELECT raw_text, extracted_fields FROM capture_jobs WHERE invoice_id = $1 LIMIT 1",
    )
    .bind(uuid::Uuid::parse_str(invoice_id).unwrap_or_default())
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    match row {
        Some((raw_text, extracted_fields)) => {
            serde_json::to_string_pretty(&serde_json::json!({
                "raw_text": raw_text.unwrap_or_default(),
                "extracted_fields": extracted_fields.unwrap_or(serde_json::json!({})),
            }))
            .unwrap_or_else(|_| r#"{"status": "serialization_error"}"#.to_string())
        }
        None => r#"{"status": "no_ocr_record"}"#.to_string(),
    }
}

/// Fetch approval audit rows for an invoice
async fn fetch_approvals(
    pool: &sqlx::PgPool,
    tenant_uuid: &uuid::Uuid,
    invoice_id: &str,
) -> Vec<serde_json::Value> {
    let rows: Vec<(
        String,
        Option<uuid::Uuid>,
        Option<String>,
        Option<String>,
        chrono::DateTime<chrono::Utc>,
    )> = sqlx::query_as(
        r#"SELECT action, user_id, ip_address, user_agent, created_at
           FROM audit_log
           WHERE tenant_id = $1
             AND resource_id = $2
             AND resource_type = 'Invoice'
             AND action IN ('approve', 'reject', 'submit', 'reassign', 'InvoiceApproved', 'InvoiceSubmitted', 'InvoiceRejected')
           ORDER BY created_at ASC"#,
    )
    .bind(tenant_uuid)
    .bind(invoice_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    rows.into_iter()
        .map(|(action, user_id, ip_address, user_agent, created_at)| {
            serde_json::json!({
                "action": action,
                "actor_user_id": user_id.map(|u| u.to_string()),
                "ip_address": ip_address,
                "user_agent": user_agent,
                "created_at": created_at.to_rfc3339(),
            })
        })
        .collect()
}

/// Fetch GL coding audit rows for an invoice
async fn fetch_gl_coding(
    pool: &sqlx::PgPool,
    tenant_uuid: &uuid::Uuid,
    invoice_id: &str,
) -> Vec<serde_json::Value> {
    let rows: Vec<(String, Option<serde_json::Value>, chrono::DateTime<chrono::Utc>)> =
        sqlx::query_as(
            r#"SELECT action, changes, created_at
               FROM audit_log
               WHERE tenant_id = $1
                 AND resource_id = $2
                 AND (action LIKE 'gl_%' OR resource_type = 'invoice_line_item')
               ORDER BY created_at ASC"#,
        )
        .bind(tenant_uuid)
        .bind(invoice_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

    rows.into_iter()
        .map(|(action, changes, created_at)| {
            serde_json::json!({
                "action": action,
                "changes": changes,
                "created_at": created_at.to_rfc3339(),
            })
        })
        .collect()
}

fn io_err(e: std::io::Error) -> ApiError {
    ApiError(Error::Internal(format!("IO error: {}", e)))
}

fn zip_err(e: zip::result::ZipError) -> ApiError {
    ApiError(Error::Internal(format!("ZIP error: {}", e)))
}
