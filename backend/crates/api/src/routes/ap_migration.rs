//! AP-to-AP Migration Importer (refs #405).
//!
//! Accepts a BILL.com or Coupa export bundle (ZIP) containing vendors, open
//! invoices, approval workflows, GL mappings, approver hierarchy, and
//! documents. Parses into a staged preview model and renders side-by-side
//! preview before any tenant write. Commit is transactional and tenant-scoped.
//!
//! Endpoints:
//! - POST   /migrate/ap/bundle              - upload + parse
//! - GET    /migrate/ap/bundle/:id/preview  - staged side-by-side preview
//! - POST   /migrate/ap/bundle/:id/commit   - apply non-skip rows
//! - POST   /migrate/ap/bundle/:id/cancel   - drop preview rows

use crate::error::ApiResult;
use crate::extractors::{AuthUser, TenantCtx};
use crate::state::AppState;
use axum::{
    extract::{Multipart, Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/bundle", post(upload_bundle))
        .route("/bundle/:id/preview", get(get_preview))
        .route("/bundle/:id/commit", post(commit_bundle))
        .route("/bundle/:id/cancel", post(cancel_bundle))
}

// ===========================================================================
// DTOs
// ===========================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct UploadBundleResponse {
    pub bundle_id: Uuid,
    pub source: String,
    pub status: String,
    pub parse_errors: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BundleSummary {
    pub id: Uuid,
    pub source: String,
    pub status: String,
    pub original_filename: String,
    pub uploaded_at: String,
    pub error_text: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PreviewRow {
    pub id: Uuid,
    pub entity_type: String,
    pub source_payload: serde_json::Value,
    pub target_action: String,
    pub target_match_id: Option<Uuid>,
    pub conflict_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct PreviewEntities {
    pub vendors: Vec<PreviewRow>,
    pub invoices: Vec<PreviewRow>,
    pub approval_workflows: Vec<PreviewRow>,
    pub gl_mappings: Vec<PreviewRow>,
    pub approvers: Vec<PreviewRow>,
    pub documents: Vec<PreviewRow>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PreviewResponse {
    pub bundle: BundleSummary,
    pub entities: PreviewEntities,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct CommitResponse {
    pub bundle_id: Uuid,
    pub status: String,
    pub vendors_created: u64,
    pub vendors_updated: u64,
    pub invoices_created: u64,
    pub invoices_updated: u64,
    pub approval_workflows_created: u64,
    pub gl_mappings_created: u64,
    pub gl_mappings_updated: u64,
    pub approvers_created: u64,
    pub approvers_updated: u64,
    pub documents_created: u64,
    pub skipped: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CancelResponse {
    pub bundle_id: Uuid,
    pub status: String,
}

// ===========================================================================
// Handlers
// ===========================================================================

/// POST /migrate/ap/bundle - upload a BILL/Coupa ZIP and stage a preview.
async fn upload_bundle(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    AuthUser(user): AuthUser,
    mut multipart: Multipart,
) -> ApiResult<Json<UploadBundleResponse>> {
    let mut bytes: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        billforge_core::Error::Validation(format!("Failed to read multipart data: {}", e))
    })? {
        if field.name().unwrap_or("") == "file" {
            filename = field.file_name().map(|s| s.to_string());
            let data = field.bytes().await.map_err(|e| {
                billforge_core::Error::Validation(format!("Failed to read file bytes: {}", e))
            })?;
            bytes = Some(data.to_vec());
            break;
        }
    }

    let bytes = bytes
        .ok_or_else(|| billforge_core::Error::Validation("No file uploaded".to_string()))?;
    let filename = filename.unwrap_or_else(|| "bundle.zip".to_string());

    let parsed = parse_bundle(&bytes)
        .map_err(|e| billforge_core::Error::Validation(format!("Bundle parse failed: {}", e)))?;

    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let bundle_id = Uuid::new_v4();
    let storage_path = persist_bundle_bytes(tenant.tenant_id.as_uuid(), &bundle_id, &bytes);

    sqlx::query(
        r#"INSERT INTO ap_migration_bundle
            (id, tenant_id, source, status, uploaded_by, original_filename, storage_path)
           VALUES ($1, $2, $3, 'parsed', $4, $5, $6)"#,
    )
    .bind(bundle_id)
    .bind(tenant.tenant_id.as_uuid())
    .bind(parsed.source.as_str())
    .bind(user.user_id.as_uuid())
    .bind(&filename)
    .bind(&storage_path)
    .execute(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to insert bundle: {}", e)))?;

    // Match staged entities against the tenant's current state and persist preview rows.
    let preview_rows = match_and_stage(&pool, tenant.tenant_id.as_uuid(), &parsed).await?;

    for row in &preview_rows {
        sqlx::query(
            r#"INSERT INTO ap_migration_preview
                (bundle_id, tenant_id, entity_type, source_payload, target_action,
                 target_match_id, conflict_reason)
               VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
        )
        .bind(bundle_id)
        .bind(tenant.tenant_id.as_uuid())
        .bind(&row.entity_type)
        .bind(&row.source_payload)
        .bind(&row.target_action)
        .bind(row.target_match_id)
        .bind(&row.conflict_reason)
        .execute(&*pool)
        .await
        .map_err(|e| billforge_core::Error::Database(format!("Failed to stage preview row: {}", e)))?;
    }

    record_audit(
        &pool,
        tenant.tenant_id.as_uuid(),
        &bundle_id,
        user.user_id.as_uuid(),
        "bundle_uploaded",
        serde_json::json!({
            "source": parsed.source.as_str(),
            "filename": filename,
            "row_count": preview_rows.len(),
        }),
    )
    .await;

    Ok(Json(UploadBundleResponse {
        bundle_id,
        source: parsed.source.to_string(),
        status: "parsed".to_string(),
        parse_errors: parsed.errors,
    }))
}

/// GET /migrate/ap/bundle/:id/preview
async fn get_preview(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Path(bundle_id): Path<Uuid>,
) -> ApiResult<Json<PreviewResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let bundle = fetch_bundle(&pool, tenant.tenant_id.as_uuid(), &bundle_id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "AP migration bundle".to_string(),
            id: bundle_id.to_string(),
        })?;

    let rows: Vec<(
        Uuid,
        String,
        serde_json::Value,
        String,
        Option<Uuid>,
        Option<String>,
    )> = sqlx::query_as(
        r#"SELECT id, entity_type, source_payload, target_action, target_match_id, conflict_reason
           FROM ap_migration_preview
           WHERE bundle_id = $1 AND tenant_id = $2
           ORDER BY entity_type, created_at"#,
    )
    .bind(bundle_id)
    .bind(tenant.tenant_id.as_uuid())
    .fetch_all(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to load preview rows: {}", e)))?;

    let mut entities = PreviewEntities::default();
    for (id, entity_type, source_payload, target_action, target_match_id, conflict_reason) in rows {
        let row = PreviewRow {
            id,
            entity_type: entity_type.clone(),
            source_payload,
            target_action,
            target_match_id,
            conflict_reason,
        };
        match entity_type.as_str() {
            "vendor" => entities.vendors.push(row),
            "invoice" => entities.invoices.push(row),
            "approval_workflow" => entities.approval_workflows.push(row),
            "gl_mapping" => entities.gl_mappings.push(row),
            "approver" => entities.approvers.push(row),
            "document" => entities.documents.push(row),
            _ => {}
        }
    }

    Ok(Json(PreviewResponse { bundle, entities }))
}

/// POST /migrate/ap/bundle/:id/commit - apply all non-skip preview rows.
async fn commit_bundle(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    AuthUser(user): AuthUser,
    Path(bundle_id): Path<Uuid>,
) -> ApiResult<Json<CommitResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    let bundle = fetch_bundle(&pool, tenant.tenant_id.as_uuid(), &bundle_id)
        .await?
        .ok_or_else(|| billforge_core::Error::NotFound {
            resource_type: "AP migration bundle".to_string(),
            id: bundle_id.to_string(),
        })?;

    if bundle.status == "committed" {
        return Err(billforge_core::Error::Validation(
            "Bundle already committed".to_string(),
        )
        .into());
    }

    let rows: Vec<(
        Uuid,
        String,
        serde_json::Value,
        String,
        Option<Uuid>,
    )> = sqlx::query_as(
        r#"SELECT id, entity_type, source_payload, target_action, target_match_id
           FROM ap_migration_preview
           WHERE bundle_id = $1 AND tenant_id = $2
           ORDER BY
             CASE entity_type
                WHEN 'vendor' THEN 1
                WHEN 'gl_mapping' THEN 2
                WHEN 'approver' THEN 3
                WHEN 'approval_workflow' THEN 4
                WHEN 'invoice' THEN 5
                WHEN 'document' THEN 6
                ELSE 99
             END,
             created_at"#,
    )
    .bind(bundle_id)
    .bind(tenant.tenant_id.as_uuid())
    .fetch_all(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to load preview rows: {}", e)))?;

    let mut tx = pool.begin().await.map_err(|e| {
        billforge_core::Error::Database(format!("Failed to start transaction: {}", e))
    })?;

    let tenant_uuid = *tenant.tenant_id.as_uuid();
    let user_uuid = *user.user_id.as_uuid();
    let mut result = CommitResponse {
        bundle_id,
        status: "committed".to_string(),
        ..Default::default()
    };

    for (_row_id, entity_type, source_payload, target_action, target_match_id) in rows {
        if target_action == "skip" {
            result.skipped += 1;
            continue;
        }
        match entity_type.as_str() {
            "vendor" => {
                let created = apply_vendor(
                    &mut tx,
                    tenant_uuid,
                    &source_payload,
                    target_match_id,
                    &target_action,
                )
                .await?;
                if created {
                    result.vendors_created += 1;
                } else {
                    result.vendors_updated += 1;
                }
            }
            "invoice" => {
                let created = apply_invoice(
                    &mut tx,
                    tenant_uuid,
                    user_uuid,
                    &source_payload,
                    target_match_id,
                    &target_action,
                )
                .await?;
                if created {
                    result.invoices_created += 1;
                } else {
                    result.invoices_updated += 1;
                }
            }
            "approval_workflow" => {
                apply_approval_workflow(&mut tx, tenant_uuid, &bundle_id, &source_payload).await?;
                result.approval_workflows_created += 1;
            }
            "gl_mapping" => {
                let created = apply_gl_mapping(
                    &mut tx,
                    tenant_uuid,
                    &bundle_id,
                    &source_payload,
                    target_match_id,
                    &target_action,
                )
                .await?;
                if created {
                    result.gl_mappings_created += 1;
                } else {
                    result.gl_mappings_updated += 1;
                }
            }
            "approver" => {
                let created = apply_approver(
                    &mut tx,
                    tenant_uuid,
                    &bundle_id,
                    &source_payload,
                    target_match_id,
                    &target_action,
                )
                .await?;
                if created {
                    result.approvers_created += 1;
                } else {
                    result.approvers_updated += 1;
                }
            }
            "document" => {
                apply_document(&mut tx, tenant_uuid, &bundle_id, &source_payload).await?;
                result.documents_created += 1;
            }
            _ => {}
        }
    }

    sqlx::query(
        r#"UPDATE ap_migration_bundle
           SET status = 'committed', updated_at = NOW()
           WHERE id = $1 AND tenant_id = $2"#,
    )
    .bind(bundle_id)
    .bind(tenant_uuid)
    .execute(&mut *tx)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to mark committed: {}", e)))?;

    sqlx::query(
        r#"INSERT INTO ap_migration_audit
            (bundle_id, tenant_id, actor_id, action, detail)
           VALUES ($1, $2, $3, 'bundle_committed', $4)"#,
    )
    .bind(bundle_id)
    .bind(tenant_uuid)
    .bind(user_uuid)
    .bind(serde_json::to_value(&result).unwrap_or(serde_json::Value::Null))
    .execute(&mut *tx)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to write audit row: {}", e)))?;

    tx.commit().await.map_err(|e| {
        billforge_core::Error::Database(format!("Failed to commit transaction: {}", e))
    })?;

    Ok(Json(result))
}

/// POST /migrate/ap/bundle/:id/cancel - mark failed and purge preview rows.
async fn cancel_bundle(
    State(state): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    AuthUser(user): AuthUser,
    Path(bundle_id): Path<Uuid>,
) -> ApiResult<Json<CancelResponse>> {
    let pool = state.db.tenant(&tenant.tenant_id).await?;

    sqlx::query(
        r#"DELETE FROM ap_migration_preview
           WHERE bundle_id = $1 AND tenant_id = $2"#,
    )
    .bind(bundle_id)
    .bind(tenant.tenant_id.as_uuid())
    .execute(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to purge preview: {}", e)))?;

    let affected = sqlx::query(
        r#"UPDATE ap_migration_bundle
           SET status = 'failed', error_text = 'Cancelled by user', updated_at = NOW()
           WHERE id = $1 AND tenant_id = $2"#,
    )
    .bind(bundle_id)
    .bind(tenant.tenant_id.as_uuid())
    .execute(&*pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to update bundle: {}", e)))?
    .rows_affected();

    if affected == 0 {
        return Err(billforge_core::Error::NotFound {
            resource_type: "AP migration bundle".to_string(),
            id: bundle_id.to_string(),
        }
        .into());
    }

    record_audit(
        &pool,
        tenant.tenant_id.as_uuid(),
        &bundle_id,
        user.user_id.as_uuid(),
        "bundle_cancelled",
        serde_json::json!({}),
    )
    .await;

    Ok(Json(CancelResponse {
        bundle_id,
        status: "failed".to_string(),
    }))
}

// ===========================================================================
// Helpers
// ===========================================================================

async fn fetch_bundle(
    pool: &sqlx::PgPool,
    tenant_uuid: &Uuid,
    bundle_id: &Uuid,
) -> ApiResult<Option<BundleSummary>> {
    let row: Option<(Uuid, String, String, String, chrono::DateTime<chrono::Utc>, Option<String>)> =
        sqlx::query_as(
            r#"SELECT id, source, status, original_filename, uploaded_at, error_text
               FROM ap_migration_bundle
               WHERE id = $1 AND tenant_id = $2"#,
        )
        .bind(bundle_id)
        .bind(tenant_uuid)
        .fetch_optional(pool)
        .await
        .map_err(|e| billforge_core::Error::Database(format!("Failed to fetch bundle: {}", e)))?;

    Ok(row.map(
        |(id, source, status, original_filename, uploaded_at, error_text)| BundleSummary {
            id,
            source,
            status,
            original_filename,
            uploaded_at: uploaded_at.to_rfc3339(),
            error_text,
        },
    ))
}

async fn record_audit(
    pool: &sqlx::PgPool,
    tenant_uuid: &Uuid,
    bundle_id: &Uuid,
    actor_uuid: &Uuid,
    action: &str,
    detail: serde_json::Value,
) {
    if let Err(e) = sqlx::query(
        r#"INSERT INTO ap_migration_audit
            (bundle_id, tenant_id, actor_id, action, detail)
           VALUES ($1, $2, $3, $4, $5)"#,
    )
    .bind(bundle_id)
    .bind(tenant_uuid)
    .bind(actor_uuid)
    .bind(action)
    .bind(detail)
    .execute(pool)
    .await
    {
        tracing::warn!(error = %e, "Failed to write ap_migration_audit row");
    }
}

fn persist_bundle_bytes(tenant_uuid: &Uuid, bundle_id: &Uuid, bytes: &[u8]) -> String {
    // Mirror the existing local-storage pattern (inbox_addin.rs, inbound_email.rs).
    let storage_root =
        std::env::var("LOCAL_STORAGE_PATH").unwrap_or_else(|_| "./data/files".to_string());
    let dir = std::path::Path::new(&storage_root)
        .join(tenant_uuid.to_string())
        .join("ap_migration");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!(error = %e, "Failed to create ap_migration storage dir");
    }
    let file_path = dir.join(format!("{}.zip", bundle_id));
    if let Err(e) = std::fs::write(&file_path, bytes) {
        tracing::warn!(error = %e, "Failed to persist ap_migration bundle bytes");
    }
    file_path.to_string_lossy().to_string()
}

// ---------------------------------------------------------------------------
// Bundle parsing
// ---------------------------------------------------------------------------

/// Detected source for the bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleSource {
    Bill,
    Coupa,
}

impl BundleSource {
    pub fn as_str(self) -> &'static str {
        match self {
            BundleSource::Bill => "bill",
            BundleSource::Coupa => "coupa",
        }
    }
}

impl std::fmt::Display for BundleSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Staged entities parsed from a bundle, before tenant matching.
#[derive(Debug)]
pub struct StagedEntities {
    pub source: BundleSource,
    pub vendors: Vec<HashMap<String, String>>,
    pub invoices: Vec<HashMap<String, String>>,
    pub approval_workflows: Vec<HashMap<String, String>>,
    pub gl_mappings: Vec<HashMap<String, String>>,
    pub approvers: Vec<HashMap<String, String>>,
    pub documents: Vec<HashMap<String, String>>,
    pub errors: Vec<String>,
}

/// Public entry-point that selects per-source parser based on manifest.json.
pub fn parse_bundle(bytes: &[u8]) -> Result<StagedEntities, String> {
    let reader = std::io::Cursor::new(bytes);
    let mut zip = zip::ZipArchive::new(reader).map_err(|e| format!("invalid ZIP: {}", e))?;

    let mut files: HashMap<String, Vec<u8>> = HashMap::new();
    for i in 0..zip.len() {
        let mut f = zip
            .by_index(i)
            .map_err(|e| format!("zip entry read failed: {}", e))?;
        if f.is_file() {
            let name = f.name().to_string();
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)
                .map_err(|e| format!("zip entry decode failed: {}", e))?;
            files.insert(name, buf);
        }
    }

    let manifest_raw = files
        .get("manifest.json")
        .ok_or_else(|| "manifest.json missing from bundle".to_string())?;
    let manifest: serde_json::Value = serde_json::from_slice(manifest_raw)
        .map_err(|e| format!("manifest.json invalid: {}", e))?;
    let source_str = manifest
        .get("source")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "manifest.json: missing 'source' field".to_string())?;
    let source = match source_str.to_lowercase().as_str() {
        "bill" => BundleSource::Bill,
        "coupa" => BundleSource::Coupa,
        other => return Err(format!("unknown source '{}', expected 'bill' or 'coupa'", other)),
    };

    match source {
        BundleSource::Bill => parse_with_column_map(&files, source, &bill_column_map()),
        BundleSource::Coupa => parse_with_column_map(&files, source, &coupa_column_map()),
    }
}

/// Maps from canonical staged field-name → source-specific CSV column name.
struct ColumnMap {
    vendors: &'static [(&'static str, &'static str)],
    invoices: &'static [(&'static str, &'static str)],
    approval_workflows: &'static [(&'static str, &'static str)],
    gl_mappings: &'static [(&'static str, &'static str)],
    approvers: &'static [(&'static str, &'static str)],
}

fn bill_column_map() -> ColumnMap {
    ColumnMap {
        vendors: &[
            ("name", "Name"),
            ("legal_name", "LegalName"),
            ("tax_id", "TaxId"),
            ("email", "Email"),
            ("phone", "Phone"),
            ("payment_terms", "PaymentTerms"),
        ],
        invoices: &[
            ("invoice_number", "InvoiceNumber"),
            ("vendor_name", "VendorName"),
            ("vendor_tax_id", "VendorTaxId"),
            ("amount", "Amount"),
            ("invoice_date", "InvoiceDate"),
            ("due_date", "DueDate"),
            ("currency", "Currency"),
            ("po_number", "PoNumber"),
        ],
        approval_workflows: &[
            ("name", "WorkflowName"),
            ("rule_json", "Rule"),
        ],
        gl_mappings: &[
            ("source_gl_code", "GLCode"),
            ("source_gl_name", "GLName"),
            ("department", "Department"),
        ],
        approvers: &[
            ("email", "Email"),
            ("name", "Name"),
            ("role", "Role"),
            ("manager_email", "ManagerEmail"),
        ],
    }
}

fn coupa_column_map() -> ColumnMap {
    // Coupa export CSVs use snake_case column headers.
    ColumnMap {
        vendors: &[
            ("name", "supplier_name"),
            ("legal_name", "legal_name"),
            ("tax_id", "tax_id_number"),
            ("email", "primary_contact_email"),
            ("phone", "primary_contact_phone"),
            ("payment_terms", "payment_term_code"),
        ],
        invoices: &[
            ("invoice_number", "invoice_number"),
            ("vendor_name", "supplier_name"),
            ("vendor_tax_id", "supplier_tax_id"),
            ("amount", "gross_total"),
            ("invoice_date", "invoice_date"),
            ("due_date", "payment_due_date"),
            ("currency", "currency_code"),
            ("po_number", "po_number"),
        ],
        approval_workflows: &[
            ("name", "approval_chain_name"),
            ("rule_json", "rule_definition"),
        ],
        gl_mappings: &[
            ("source_gl_code", "account_code"),
            ("source_gl_name", "account_name"),
            ("department", "department_code"),
        ],
        approvers: &[
            ("email", "user_email"),
            ("name", "user_full_name"),
            ("role", "approval_role"),
            ("manager_email", "manager_user_email"),
        ],
    }
}

fn parse_with_column_map(
    files: &HashMap<String, Vec<u8>>,
    source: BundleSource,
    map: &ColumnMap,
) -> Result<StagedEntities, String> {
    let mut errors: Vec<String> = Vec::new();

    let vendors = parse_csv_section(files, "vendors.csv", map.vendors, &mut errors);
    let invoices = parse_csv_section(files, "invoices.csv", map.invoices, &mut errors);
    let approval_workflows =
        parse_csv_section(files, "approval_workflows.csv", map.approval_workflows, &mut errors);
    let gl_mappings = parse_csv_section(files, "gl_mappings.csv", map.gl_mappings, &mut errors);
    let approvers = parse_csv_section(files, "approvers.csv", map.approvers, &mut errors);

    // Documents: file entries under documents/. The "row" is a metadata record.
    let mut documents: Vec<HashMap<String, String>> = Vec::new();
    for (name, bytes) in files.iter() {
        if let Some(rest) = name.strip_prefix("documents/") {
            if rest.is_empty() {
                continue;
            }
            let mut row: HashMap<String, String> = HashMap::new();
            row.insert("filename".to_string(), rest.to_string());
            row.insert("byte_count".to_string(), bytes.len().to_string());
            documents.push(row);
        }
    }

    Ok(StagedEntities {
        source,
        vendors,
        invoices,
        approval_workflows,
        gl_mappings,
        approvers,
        documents,
        errors,
    })
}

fn parse_csv_section(
    files: &HashMap<String, Vec<u8>>,
    name: &str,
    columns: &[(&'static str, &'static str)],
    errors: &mut Vec<String>,
) -> Vec<HashMap<String, String>> {
    let Some(bytes) = files.get(name) else {
        // Missing section is non-fatal — the bundle just doesn't carry that entity type.
        return Vec::new();
    };
    let text = match std::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => {
            errors.push(format!("{}: not UTF-8 ({})", name, e));
            return Vec::new();
        }
    };
    parse_csv(text, columns)
}

/// Minimal RFC 4180-ish CSV parser: supports double-quoted fields and embedded commas.
/// Avoids pulling a new dependency for the smallest viable slice; the same convention
/// is used in vendors.rs::parse_vendor_csv.
fn parse_csv(text: &str, columns: &[(&'static str, &'static str)]) -> Vec<HashMap<String, String>> {
    let mut lines = text.lines();
    let Some(header_line) = lines.next() else {
        return Vec::new();
    };
    let headers: Vec<String> = split_csv_line(header_line);

    let mut rows: Vec<HashMap<String, String>> = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let fields = split_csv_line(line);
        let mut row: HashMap<String, String> = HashMap::new();
        for (canonical, source_col) in columns.iter() {
            if let Some(idx) = headers.iter().position(|h| h.eq_ignore_ascii_case(source_col)) {
                if let Some(value) = fields.get(idx) {
                    row.insert((*canonical).to_string(), value.trim().to_string());
                }
            }
        }
        if !row.is_empty() {
            rows.push(row);
        }
    }
    rows
}

fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                if in_quotes && chars.peek() == Some(&'"') {
                    current.push('"');
                    chars.next();
                } else {
                    in_quotes = !in_quotes;
                }
            }
            ',' if !in_quotes => {
                fields.push(std::mem::take(&mut current));
            }
            _ => current.push(ch),
        }
    }
    fields.push(current);
    fields
}

// ---------------------------------------------------------------------------
// Staging + matching
// ---------------------------------------------------------------------------

struct StagedPreviewRow {
    entity_type: String,
    source_payload: serde_json::Value,
    target_action: String,
    target_match_id: Option<Uuid>,
    conflict_reason: Option<String>,
}

async fn match_and_stage(
    pool: &sqlx::PgPool,
    tenant_uuid: &Uuid,
    parsed: &StagedEntities,
) -> ApiResult<Vec<StagedPreviewRow>> {
    let mut rows: Vec<StagedPreviewRow> = Vec::new();

    // Cache existing tenant vendors keyed by normalized tax_id and lowercased name.
    let existing_vendors: Vec<(Uuid, String, Option<String>)> = sqlx::query_as(
        r#"SELECT id, name, tax_id FROM vendors WHERE tenant_id = $1"#,
    )
    .bind(tenant_uuid)
    .fetch_all(pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to load vendors: {}", e)))?;

    let mut vendor_by_tax: HashMap<String, Uuid> = HashMap::new();
    let mut vendor_by_name: HashMap<String, Uuid> = HashMap::new();
    for (id, name, tax_id) in &existing_vendors {
        if let Some(t) = tax_id.as_ref().map(|s| normalize_tax_id(s)).filter(|s| !s.is_empty()) {
            vendor_by_tax.insert(t, *id);
        }
        vendor_by_name.insert(name.to_lowercase(), *id);
    }

    // Vendors
    let mut staged_vendor_for_tax: HashMap<String, Uuid> = HashMap::new();
    let mut staged_vendor_for_name: HashMap<String, Uuid> = HashMap::new();
    for raw in &parsed.vendors {
        let payload = serde_json::to_value(raw).unwrap_or(serde_json::Value::Null);
        let tax_id = raw.get("tax_id").map(|s| normalize_tax_id(s));
        let name_lower = raw.get("name").map(|s| s.to_lowercase()).unwrap_or_default();

        let (action, match_id, conflict) = if let Some(tax) = tax_id.as_ref().filter(|s| !s.is_empty()) {
            if let Some(id) = vendor_by_tax.get(tax) {
                ("update", Some(*id), None)
            } else {
                ("create", None, None)
            }
        } else if let Some(id) = vendor_by_name.get(&name_lower) {
            (
                "update",
                Some(*id),
                Some("Matched by name; tax_id missing".to_string()),
            )
        } else {
            (
                "create",
                None,
                if name_lower.is_empty() {
                    Some("Missing vendor name".to_string())
                } else if tax_id.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
                    Some("Missing tax_id; cannot dedupe with confidence".to_string())
                } else {
                    None
                },
            )
        };

        let staged_id = Uuid::new_v4();
        if let Some(tax) = tax_id.as_ref().filter(|s| !s.is_empty()) {
            staged_vendor_for_tax.insert(tax.clone(), match_id.unwrap_or(staged_id));
        }
        if !name_lower.is_empty() {
            staged_vendor_for_name.insert(name_lower, match_id.unwrap_or(staged_id));
        }

        rows.push(StagedPreviewRow {
            entity_type: "vendor".to_string(),
            source_payload: payload,
            target_action: action.to_string(),
            target_match_id: match_id,
            conflict_reason: conflict,
        });
    }

    // Invoices — match on (vendor, invoice_number). Resolves vendor via tax_id then name.
    let existing_invoices: Vec<(Uuid, String, Option<Uuid>)> = sqlx::query_as(
        r#"SELECT id, invoice_number, vendor_id FROM invoices WHERE tenant_id = $1"#,
    )
    .bind(tenant_uuid)
    .fetch_all(pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to load invoices: {}", e)))?;
    let mut invoice_lookup: HashMap<(Uuid, String), Uuid> = HashMap::new();
    for (id, number, vendor_id) in &existing_invoices {
        if let Some(vid) = vendor_id {
            invoice_lookup.insert((*vid, number.to_lowercase()), *id);
        }
    }

    for raw in &parsed.invoices {
        let payload = serde_json::to_value(raw).unwrap_or(serde_json::Value::Null);
        let invoice_number = raw.get("invoice_number").cloned().unwrap_or_default();
        let vendor_match = raw
            .get("vendor_tax_id")
            .map(|s| normalize_tax_id(s))
            .filter(|s| !s.is_empty())
            .and_then(|t| vendor_by_tax.get(&t).copied().or_else(|| staged_vendor_for_tax.get(&t).copied()))
            .or_else(|| {
                raw.get("vendor_name")
                    .map(|s| s.to_lowercase())
                    .and_then(|n| vendor_by_name.get(&n).copied().or_else(|| staged_vendor_for_name.get(&n).copied()))
            });

        let (action, match_id, conflict) = if invoice_number.trim().is_empty() {
            (
                "skip",
                None,
                Some("Invoice number missing".to_string()),
            )
        } else if let Some(vid) = vendor_match {
            if let Some(existing) = invoice_lookup.get(&(vid, invoice_number.to_lowercase())) {
                (
                    "update",
                    Some(*existing),
                    Some("Matched existing invoice number for vendor".to_string()),
                )
            } else {
                ("create", None, None)
            }
        } else {
            (
                "create",
                None,
                Some("Vendor could not be matched; will be linked on commit if vendor staged".to_string()),
            )
        };

        rows.push(StagedPreviewRow {
            entity_type: "invoice".to_string(),
            source_payload: payload,
            target_action: action.to_string(),
            target_match_id: match_id,
            conflict_reason: conflict,
        });
    }

    // Approval workflows — always create
    for raw in &parsed.approval_workflows {
        rows.push(StagedPreviewRow {
            entity_type: "approval_workflow".to_string(),
            source_payload: serde_json::to_value(raw).unwrap_or(serde_json::Value::Null),
            target_action: "create".to_string(),
            target_match_id: None,
            conflict_reason: None,
        });
    }

    // GL mappings — slice has no canonical GL mapping table yet, so every
    // row is staged as 'create' and recorded in the audit trail on commit.
    for raw in &parsed.gl_mappings {
        rows.push(StagedPreviewRow {
            entity_type: "gl_mapping".to_string(),
            source_payload: serde_json::to_value(raw).unwrap_or(serde_json::Value::Null),
            target_action: "create".to_string(),
            target_match_id: None,
            conflict_reason: None,
        });
    }

    // Approvers — match on email against users table
    let user_emails: Vec<(Uuid, String)> = sqlx::query_as(
        r#"SELECT id, email FROM users WHERE tenant_id = $1"#,
    )
    .bind(tenant_uuid)
    .fetch_all(pool)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to load users: {}", e)))?;
    let mut user_by_email: HashMap<String, Uuid> = HashMap::new();
    for (id, email) in user_emails {
        user_by_email.insert(email.to_lowercase(), id);
    }
    for raw in &parsed.approvers {
        let email = raw.get("email").cloned().unwrap_or_default().to_lowercase();
        let (action, match_id, conflict) = if email.is_empty() {
            (
                "skip",
                None,
                Some("Approver email missing".to_string()),
            )
        } else if let Some(id) = user_by_email.get(&email) {
            ("update", Some(*id), None)
        } else {
            ("create", None, None)
        };
        rows.push(StagedPreviewRow {
            entity_type: "approver".to_string(),
            source_payload: serde_json::to_value(raw).unwrap_or(serde_json::Value::Null),
            target_action: action.to_string(),
            target_match_id: match_id,
            conflict_reason: conflict,
        });
    }

    // Documents — always create
    for raw in &parsed.documents {
        rows.push(StagedPreviewRow {
            entity_type: "document".to_string(),
            source_payload: serde_json::to_value(raw).unwrap_or(serde_json::Value::Null),
            target_action: "create".to_string(),
            target_match_id: None,
            conflict_reason: None,
        });
    }

    Ok(rows)
}

fn normalize_tax_id(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_uppercase()
}

// ---------------------------------------------------------------------------
// Commit appliers
// ---------------------------------------------------------------------------

async fn apply_vendor(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_uuid: Uuid,
    payload: &serde_json::Value,
    match_id: Option<Uuid>,
    target_action: &str,
) -> ApiResult<bool> {
    let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if name.is_empty() {
        return Ok(false);
    }
    let tax_id = payload.get("tax_id").and_then(|v| v.as_str()).map(|s| s.to_string());
    let email = payload.get("email").and_then(|v| v.as_str()).map(|s| s.to_string());
    let phone = payload.get("phone").and_then(|v| v.as_str()).map(|s| s.to_string());
    let payment_terms = payload
        .get("payment_terms")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    if target_action == "update" {
        if let Some(id) = match_id {
            sqlx::query(
                r#"UPDATE vendors
                   SET name = $3,
                       tax_id = COALESCE($4, tax_id),
                       contact_email = COALESCE($5, contact_email),
                       contact_phone = COALESCE($6, contact_phone),
                       payment_terms = COALESCE($7, payment_terms),
                       updated_at = NOW()
                   WHERE id = $1 AND tenant_id = $2"#,
            )
            .bind(id)
            .bind(tenant_uuid)
            .bind(&name)
            .bind(&tax_id)
            .bind(&email)
            .bind(&phone)
            .bind(&payment_terms)
            .execute(&mut **tx)
            .await
            .map_err(|e| billforge_core::Error::Database(format!("update vendor failed: {}", e)))?;
            return Ok(false);
        }
    }

    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO vendors (id, tenant_id, name, tax_id, contact_email, contact_phone, payment_terms, is_active)
           VALUES ($1, $2, $3, $4, $5, $6, $7, true)
           ON CONFLICT (tenant_id, name) DO UPDATE SET
               tax_id = COALESCE(EXCLUDED.tax_id, vendors.tax_id),
               contact_email = COALESCE(EXCLUDED.contact_email, vendors.contact_email),
               contact_phone = COALESCE(EXCLUDED.contact_phone, vendors.contact_phone),
               payment_terms = COALESCE(EXCLUDED.payment_terms, vendors.payment_terms),
               updated_at = NOW()"#,
    )
    .bind(id)
    .bind(tenant_uuid)
    .bind(&name)
    .bind(&tax_id)
    .bind(&email)
    .bind(&phone)
    .bind(&payment_terms)
    .execute(&mut **tx)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("insert vendor failed: {}", e)))?;
    Ok(true)
}

async fn apply_invoice(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_uuid: Uuid,
    actor_uuid: Uuid,
    payload: &serde_json::Value,
    match_id: Option<Uuid>,
    target_action: &str,
) -> ApiResult<bool> {
    let invoice_number = payload
        .get("invoice_number")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if invoice_number.is_empty() {
        return Ok(false);
    }

    let vendor_name = payload
        .get("vendor_name")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown")
        .to_string();
    let vendor_tax_id = payload.get("vendor_tax_id").and_then(|v| v.as_str()).map(normalize_tax_id);
    let amount: f64 = payload
        .get("amount")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);
    let total_cents = (amount * 100.0) as i64;
    let currency = payload
        .get("currency")
        .and_then(|v| v.as_str())
        .unwrap_or("USD")
        .to_string();
    let invoice_date = payload
        .get("invoice_date")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
    let due_date = payload
        .get("due_date")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
    let po_number = payload.get("po_number").and_then(|v| v.as_str()).map(|s| s.to_string());

    // Resolve vendor_id: prefer match, else attempt lookup in current tx state.
    let vendor_id: Option<Uuid> = if let Some(tax) = vendor_tax_id.as_ref().filter(|s| !s.is_empty()) {
        sqlx::query_scalar::<_, Uuid>(
            r#"SELECT id FROM vendors WHERE tenant_id = $1 AND tax_id IS NOT NULL
               AND regexp_replace(upper(tax_id), '[^A-Z0-9]', '', 'g') = $2 LIMIT 1"#,
        )
        .bind(tenant_uuid)
        .bind(tax)
        .fetch_optional(&mut **tx)
        .await
        .ok()
        .flatten()
    } else {
        sqlx::query_scalar::<_, Uuid>(
            r#"SELECT id FROM vendors WHERE tenant_id = $1 AND lower(name) = lower($2) LIMIT 1"#,
        )
        .bind(tenant_uuid)
        .bind(&vendor_name)
        .fetch_optional(&mut **tx)
        .await
        .ok()
        .flatten()
    };

    if target_action == "update" {
        if let Some(id) = match_id {
            sqlx::query(
                r#"UPDATE invoices
                   SET vendor_id = COALESCE($3, vendor_id),
                       vendor_name = $4,
                       total_amount_cents = $5,
                       currency = $6,
                       invoice_date = COALESCE($7, invoice_date),
                       due_date = COALESCE($8, due_date),
                       po_number = COALESCE($9, po_number),
                       updated_at = NOW()
                   WHERE id = $1 AND tenant_id = $2"#,
            )
            .bind(id)
            .bind(tenant_uuid)
            .bind(vendor_id)
            .bind(&vendor_name)
            .bind(total_cents)
            .bind(&currency)
            .bind(invoice_date)
            .bind(due_date)
            .bind(&po_number)
            .execute(&mut **tx)
            .await
            .map_err(|e| billforge_core::Error::Database(format!("update invoice failed: {}", e)))?;
            return Ok(false);
        }
    }

    let id = Uuid::new_v4();
    let document_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO invoices
            (id, tenant_id, vendor_id, vendor_name, invoice_number, invoice_date, due_date,
             po_number, total_amount_cents, currency, capture_status, processing_status,
             document_id, created_by)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'complete', 'submitted', $11, $12)
           ON CONFLICT (tenant_id, invoice_number) DO NOTHING"#,
    )
    .bind(id)
    .bind(tenant_uuid)
    .bind(vendor_id)
    .bind(&vendor_name)
    .bind(&invoice_number)
    .bind(invoice_date)
    .bind(due_date)
    .bind(&po_number)
    .bind(total_cents)
    .bind(&currency)
    .bind(document_id)
    .bind(actor_uuid)
    .execute(&mut **tx)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("insert invoice failed: {}", e)))?;
    Ok(true)
}

async fn apply_approval_workflow(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_uuid: Uuid,
    bundle_id: &Uuid,
    payload: &serde_json::Value,
) -> ApiResult<()> {
    // The slice persists the workflow definition into the audit log so it is
    // captured + linkable, leaving translation into the workflow_rules table
    // to a follow-up (deferred per plan: 'Out of Scope: Mapping editor UI').
    sqlx::query(
        r#"INSERT INTO ap_migration_audit
            (bundle_id, tenant_id, action, detail)
           VALUES ($1, $2, 'approval_workflow_imported', $3)"#,
    )
    .bind(bundle_id)
    .bind(tenant_uuid)
    .bind(payload)
    .execute(&mut **tx)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("workflow audit failed: {}", e)))?;
    Ok(())
}

async fn apply_gl_mapping(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_uuid: Uuid,
    bundle_id: &Uuid,
    payload: &serde_json::Value,
    _match_id: Option<Uuid>,
    _target_action: &str,
) -> ApiResult<bool> {
    // Persist GL mapping into the migration audit trail; the canonical GL
    // mapping table is owned by the Mapping editor UI (out of slice scope).
    sqlx::query(
        r#"INSERT INTO ap_migration_audit
            (bundle_id, tenant_id, action, detail)
           VALUES ($1, $2, 'gl_mapping_imported', $3)"#,
    )
    .bind(bundle_id)
    .bind(tenant_uuid)
    .bind(payload)
    .execute(&mut **tx)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("gl mapping audit failed: {}", e)))?;
    Ok(true)
}

async fn apply_approver(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_uuid: Uuid,
    bundle_id: &Uuid,
    payload: &serde_json::Value,
    _match_id: Option<Uuid>,
    _target_action: &str,
) -> ApiResult<bool> {
    // Approver hierarchy is recorded in the migration audit; user provisioning
    // and role assignment is handled by the regular admin surface.
    sqlx::query(
        r#"INSERT INTO ap_migration_audit
            (bundle_id, tenant_id, action, detail)
           VALUES ($1, $2, 'approver_imported', $3)"#,
    )
    .bind(bundle_id)
    .bind(tenant_uuid)
    .bind(payload)
    .execute(&mut **tx)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("approver audit failed: {}", e)))?;
    Ok(true)
}

async fn apply_document(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_uuid: Uuid,
    bundle_id: &Uuid,
    payload: &serde_json::Value,
) -> ApiResult<()> {
    sqlx::query(
        r#"INSERT INTO ap_migration_audit
            (bundle_id, tenant_id, action, detail)
           VALUES ($1, $2, 'document_imported', $3)"#,
    )
    .bind(bundle_id)
    .bind(tenant_uuid)
    .bind(payload)
    .execute(&mut **tx)
    .await
    .map_err(|e| billforge_core::Error::Database(format!("document audit failed: {}", e)))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_csv_line_handles_quotes_and_embedded_commas() {
        let line = r#""Acme, LLC","11-2222222","ap@acme.com""#;
        let fields = split_csv_line(line);
        assert_eq!(fields, vec!["Acme, LLC", "11-2222222", "ap@acme.com"]);
    }

    #[test]
    fn parse_csv_maps_to_canonical_keys() {
        let csv = "Name,TaxId,Email\nAcme,11-2222222,ap@acme.com\n";
        let columns: &[(&'static str, &'static str)] = &[
            ("name", "Name"),
            ("tax_id", "TaxId"),
            ("email", "Email"),
        ];
        let rows = parse_csv(csv, columns);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("name").map(String::as_str), Some("Acme"));
        assert_eq!(rows[0].get("tax_id").map(String::as_str), Some("11-2222222"));
    }

    #[test]
    fn normalize_tax_id_strips_punctuation() {
        assert_eq!(normalize_tax_id("11-2222222"), "112222222");
        assert_eq!(normalize_tax_id("  AB-12.34 "), "AB1234");
    }
}
