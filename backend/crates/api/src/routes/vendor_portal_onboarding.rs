//! Vendor self-service onboarding routes (tokenized portal flow).
//!
//! `POST /vendor-portal/onboarding` — authenticated by the existing vendor-portal
//! JWT. Accepts a multipart body with legal name, address, tax form type, optional
//! tax document upload, banking details, and remit-to contacts. Computes a diff
//! against the existing vendor record and stores field-level confidence scores.
//!
//! `GET /vendors/onboarding-submissions` and
//! `POST /vendors/onboarding-submissions/{id}/decision` — internal authenticated
//! routes for the AP review queue.

use crate::error::{ApiError, ApiResult};
use crate::extractors::VendorMgmtAccess;
use crate::state::AppState;
use axum::{
    extract::{Multipart, Path, Query, State},
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use billforge_core::{
    domain::{AuditAction, AuditEntry, ResourceType, VendorId},
    traits::AuditService,
    types::TenantId,
    Error,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Route registration
// ---------------------------------------------------------------------------

/// Vendor-portal onboarding route (mounted under /vendor-portal)
pub fn portal_routes() -> Router<AppState> {
    Router::new().route("/onboarding", post(submit_onboarding))
}

/// Internal review-queue routes (mounted under /vendors)
pub fn review_routes() -> Router<AppState> {
    Router::new()
        .route("/onboarding-submissions", get(list_submissions))
        .route("/onboarding-submissions/{id}/decision", post(decide_submission))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract tenant_id and optional vendor_id from a VendorPortal JWT in the
/// Authorization header.
fn vendor_onboarding_ctx(
    headers: &HeaderMap,
    auth: &billforge_auth::AuthService,
) -> Result<(TenantId, Option<VendorId>, String), ApiError> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or_else(|| ApiError(Error::Unauthenticated))?;

    let claims = auth.jwt_service().validate_vendor_portal_token(token)?;

    let tenant_id = claims
        .tenant_id()
        .map_err(|e| ApiError(Error::InvalidToken(e.to_string())))?;

    // vendor_id is optional: present for existing vendors, absent for net-new
    let vendor_id = claims
        .vendor_id
        .as_deref()
        .map(|vid_str| {
            Uuid::parse_str(vid_str)
                .map(|u| VendorId(u))
                .map_err(|_| Error::InvalidToken("Invalid vendor_id claim".to_string()))
        })
        .transpose()?;

    // Use JWT jti (or sub) as the portal token identifier
    let portal_token_jti = claims.sub.clone();

    Ok((tenant_id, vendor_id, portal_token_jti))
}

// ---------------------------------------------------------------------------
// Response / request types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct OnboardingSubmissionResponse {
    pub submission_id: Uuid,
    pub status: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct OnboardingSubmissionRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub vendor_id: Option<Uuid>,
    pub submitted_legal_name: String,
    pub submitted_dba: Option<String>,
    pub submitted_address: Option<serde_json::Value>,
    pub submitted_tax_form_type: String,
    pub submitted_tax_document_id: Option<Uuid>,
    pub submitted_banking: Option<serde_json::Value>,
    pub submitted_remit_contacts: Option<serde_json::Value>,
    pub field_confidence: serde_json::Value,
    pub diff: serde_json::Value,
    pub status: String,
    pub reviewed_by: Option<Uuid>,
    pub reviewed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub review_notes: Option<String>,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ListSubmissionsQuery {
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DecisionRequest {
    pub decision: String,
    pub notes: Option<String>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

const MAX_TAX_DOC_SIZE: usize = 15 * 1024 * 1024; // 15 MB

/// POST /vendor-portal/onboarding
///
/// Multipart body with JSON fields + optional tax_document file upload.
async fn submit_onboarding(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> ApiResult<Json<OnboardingSubmissionResponse>> {
    let (tenant_id, vendor_id, portal_token_jti) =
        vendor_onboarding_ctx(&headers, &state.auth)?;

    // Multipart fields
    let mut legal_name: Option<String> = None;
    let mut dba: Option<String> = None;
    let mut address_json: Option<String> = None;
    let mut tax_form_type: Option<String> = None;
    let mut banking_json: Option<String> = None;
    let mut remit_contacts_json: Option<String> = None;
    let mut tax_file_bytes: Option<Vec<u8>> = None;
    let mut tax_file_name: Option<String> = None;
    let mut tax_file_mime: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| Error::Validation(format!("Failed to read multipart data: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "legal_name" => {
                legal_name = Some(field.text().await.map_err(|e| {
                    Error::Validation(format!("Failed to read legal_name: {}", e))
                })?);
            }
            "dba" => {
                dba = Some(field.text().await.map_err(|e| {
                    Error::Validation(format!("Failed to read dba: {}", e))
                })?);
            }
            "address" => {
                address_json = Some(field.text().await.map_err(|e| {
                    Error::Validation(format!("Failed to read address: {}", e))
                })?);
            }
            "tax_form_type" => {
                tax_form_type = Some(field.text().await.map_err(|e| {
                    Error::Validation(format!("Failed to read tax_form_type: {}", e))
                })?);
            }
            "banking" => {
                banking_json = Some(field.text().await.map_err(|e| {
                    Error::Validation(format!("Failed to read banking: {}", e))
                })?);
            }
            "remit_contacts" => {
                remit_contacts_json = Some(field.text().await.map_err(|e| {
                    Error::Validation(format!("Failed to read remit_contacts: {}", e))
                })?);
            }
            "tax_document" => {
                tax_file_name = field.file_name().map(|s| s.to_string());
                tax_file_mime = field.content_type().map(|s| s.to_string());
                let data = field.bytes().await.map_err(|e| {
                    Error::Validation(format!("Failed to read tax_document: {}", e))
                })?;
                if data.len() > MAX_TAX_DOC_SIZE {
                    return Err(Error::Validation(
                        "Tax document exceeds maximum size (15 MB)".to_string(),
                    )
                    .into());
                }
                tax_file_bytes = Some(data.to_vec());
            }
            _ => {} // ignore unknown fields
        }
    }

    // Validate required fields
    let legal_name = legal_name
        .ok_or_else(|| Error::Validation("Missing required field: legal_name".to_string()))?;
    let tax_form_type = tax_form_type
        .ok_or_else(|| Error::Validation("Missing required field: tax_form_type".to_string()))?;

    if tax_form_type != "w9" && tax_form_type != "w8ben" {
        return Err(
            Error::Validation("tax_form_type must be 'w9' or 'w8ben'".to_string()).into(),
        );
    }

    // Parse JSON fields
    let address_value: Option<serde_json::Value> = address_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|e| Error::Validation(format!("Invalid address JSON: {}", e)))?;

    let banking_value: Option<serde_json::Value> = banking_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|e| Error::Validation(format!("Invalid banking JSON: {}", e)))?;

    let remit_contacts_value: Option<serde_json::Value> = remit_contacts_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|e| Error::Validation(format!("Invalid remit_contacts JSON: {}", e)))?;

    let pool = state.db.tenant(&tenant_id).await?;

    // Upload tax document if provided
    let mut tax_document_id: Option<Uuid> = None;
    if let (Some(file_data), Some(file_name), Some(mime)) =
        (tax_file_bytes, tax_file_name.as_deref(), tax_file_mime.as_deref())
    {
        let effective_vendor_id = vendor_id.as_ref()
            .ok_or_else(|| Error::Validation("Tax document upload requires an existing vendor".to_string()))?;

        let file_path = format!(
            "vendor_documents/{}/{}",
            effective_vendor_id.0,
            Uuid::new_v4()
        );
        let file_size = file_data.len() as i64;

        // Store via storage service
        let doc_id = state
            .storage
            .upload(&tenant_id, &file_name, &file_data, mime)
            .await
            .map_err(|e| Error::Database(format!("Failed to store tax document: {}", e)))?;

        // Insert vendor_documents row with uploaded_by_portal_token
        sqlx::query(
            r#"
            INSERT INTO vendor_documents (
                id, tenant_id, vendor_id, document_type, file_name, file_path,
                file_size, mime_type, uploaded_by, uploaded_by_portal_token, uploaded_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NULL, $9, NOW())
            "#,
        )
        .bind(doc_id)
        .bind(*tenant_id.as_uuid())
        .bind(effective_vendor_id.0)
        .bind(tax_form_type.as_str())
        .bind(file_name)
        .bind(&file_path)
        .bind(file_size)
        .bind(mime)
        .bind(&portal_token_jti)
        .execute(&*pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to insert tax document: {}", e)))?;

        tax_document_id = Some(doc_id);
    }

    // Compute diff and confidence against existing vendor record
    let (diff, confidence) = compute_diff_and_confidence(
        &pool,
        &tenant_id,
        vendor_id.as_ref(),
        &legal_name,
        dba.as_deref(),
        address_value.as_ref(),
        banking_value.as_ref(),
        tax_document_id.is_some(),
    )
    .await?;

    // Insert submission row
    let submission_id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO vendor_onboarding_submissions (
            id, tenant_id, vendor_id, portal_token_jti,
            submitted_legal_name, submitted_dba, submitted_address,
            submitted_tax_form_type, submitted_tax_document_id,
            submitted_banking, submitted_remit_contacts,
            field_confidence, diff,
            status, submitted_at, created_at, updated_at
        ) VALUES (
            $1, $2, $3, $4,
            $5, $6, $7,
            $8, $9,
            $10, $11,
            $12, $13,
            'pending', NOW(), NOW(), NOW()
        )
        "#,
    )
    .bind(submission_id)
    .bind(*tenant_id.as_uuid())
    .bind(vendor_id.as_ref().map(|v| v.0))
    .bind(&portal_token_jti)
    .bind(&legal_name)
    .bind(dba.as_deref())
    .bind(address_value)
    .bind(&tax_form_type)
    .bind(tax_document_id)
    .bind(banking_value)
    .bind(remit_contacts_value)
    .bind(&confidence)
    .bind(&diff)
    .execute(&*pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to insert onboarding submission: {}", e)))?;

    // Audit entry
    let audit_entry = AuditEntry::new(
        tenant_id.clone(),
        None,
        AuditAction::Create,
        ResourceType::Vendor,
        submission_id.to_string(),
        format!(
            "Vendor portal onboarding submitted by {}",
            vendor_id
                .as_ref()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "net-new vendor".to_string())
        ),
    )
    .with_metadata(serde_json::json!({
        "source": "vendor_portal_onboarding",
        "vendor_id": vendor_id.as_ref().map(|v| v.to_string()),
        "submission_id": submission_id.to_string(),
        "tax_form_type": tax_form_type,
    }));
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool.clone());
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log onboarding audit entry");
    }

    Ok(Json(OnboardingSubmissionResponse {
        submission_id,
        status: "pending".to_string(),
    }))
}

/// GET /vendors/onboarding-submissions?status=pending
async fn list_submissions(
    State(state): State<AppState>,
    claims: VendorMgmtAccess,
    Query(query): Query<ListSubmissionsQuery>,
) -> ApiResult<Json<Vec<OnboardingSubmissionRow>>> {
    let tenant_id = claims.1.tenant_id.clone();
    let pool = state.db.tenant(&tenant_id).await?;

    let sql = match query.status.as_deref() {
        Some(status) if status == "pending" || status == "approved" || status == "rejected" => {
            format!(
                "SELECT * FROM vendor_onboarding_submissions WHERE tenant_id = $1 AND status = '{}' ORDER BY submitted_at DESC",
                status
            )
        }
        _ => "SELECT * FROM vendor_onboarding_submissions WHERE tenant_id = $1 ORDER BY submitted_at DESC".to_string(),
    };

    let rows = sqlx::query_as::<_, OnboardingSubmissionRow>(&sql)
        .bind(*tenant_id.as_uuid())
        .fetch_all(&*pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to list onboarding submissions: {}", e)))?;

    Ok(Json(rows))
}

/// POST /vendors/onboarding-submissions/{id}/decision
async fn decide_submission(
    State(state): State<AppState>,
    claims: VendorMgmtAccess,
    Path(id): Path<Uuid>,
    Json(body): Json<DecisionRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let tenant_id = claims.1.tenant_id.clone();
    let user_id = claims.0.user_id.clone();
    let pool = state.db.tenant(&tenant_id).await?;

    // Load the submission
    let submission = sqlx::query_as::<_, OnboardingSubmissionRow>(
        "SELECT * FROM vendor_onboarding_submissions WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(*tenant_id.as_uuid())
    .fetch_optional(&*pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to load submission: {}", e)))?
    .ok_or_else(|| Error::NotFound {
        resource_type: "OnboardingSubmission".to_string(),
        id: id.to_string(),
    })?;

    if submission.status != "pending" {
        return Err(Error::Conflict(format!(
            "Submission is already {}",
            submission.status
        ))
        .into());
    }

    match body.decision.as_str() {
        "approve" => {
            // Write diffed fields back to vendor row if vendor_id is set
            if let Some(vendor_uuid) = submission.vendor_id {
                let vendor_id = VendorId(vendor_uuid);
                apply_diff_to_vendor(&pool, &tenant_id, &vendor_id, &submission.diff).await?;
            }

            // Stamp status
            sqlx::query(
                "UPDATE vendor_onboarding_submissions SET status = 'approved', reviewed_by = $3, reviewed_at = NOW(), review_notes = $4, updated_at = NOW() WHERE id = $1 AND tenant_id = $2",
            )
            .bind(id)
            .bind(*tenant_id.as_uuid())
            .bind(user_id.0)
            .bind(body.notes.as_deref())
            .execute(&*pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to approve submission: {}", e)))?;
        }
        "reject" => {
            sqlx::query(
                "UPDATE vendor_onboarding_submissions SET status = 'rejected', reviewed_by = $3, reviewed_at = NOW(), review_notes = $4, updated_at = NOW() WHERE id = $1 AND tenant_id = $2",
            )
            .bind(id)
            .bind(*tenant_id.as_uuid())
            .bind(user_id.0)
            .bind(body.notes.as_deref())
            .execute(&*pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to reject submission: {}", e)))?;
        }
        _ => {
            return Err(
                Error::Validation("decision must be 'approve' or 'reject'".to_string()).into(),
            );
        }
    }

    // Audit
    let audit_entry = AuditEntry::new(
        tenant_id.clone(),
        Some(user_id),
        AuditAction::Update,
        ResourceType::Vendor,
        id.to_string(),
        format!("Onboarding submission {} {}", id, body.decision),
    );
    let audit_repo = billforge_db::repositories::AuditRepositoryImpl::new(pool);
    if let Err(e) = audit_repo.log(audit_entry).await {
        tracing::warn!(error = %e, "Failed to log onboarding decision audit entry");
    }

    Ok(Json(serde_json::json!({
        "id": id,
        "status": body.decision,
    })))
}

// ---------------------------------------------------------------------------
// Diff & confidence computation
// ---------------------------------------------------------------------------

async fn compute_diff_and_confidence(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    vendor_id: Option<&VendorId>,
    legal_name: &str,
    _dba: Option<&str>,
    address: Option<&serde_json::Value>,
    banking: Option<&serde_json::Value>,
    has_tax_doc: bool,
) -> Result<(serde_json::Value, serde_json::Value), ApiError> {
    let mut diff = serde_json::Map::new();
    let mut confidence = serde_json::Map::new();

    if let Some(vid) = vendor_id {
        // Load existing vendor row for comparison
        // vendors.name is the primary name field (maps to legal_name in the diff)
        let row: Option<(Option<String>, Option<serde_json::Value>)> = sqlx::query_as(
            "SELECT name, address FROM vendors WHERE id = $1 AND tenant_id = $2",
        )
        .bind(vid.0)
        .bind(*tenant_id.as_uuid())
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to load vendor for diff: {}", e)))?;

        if let Some((existing_name, existing_address)) = row {
            // Diff legal_name (vendors.name vs submitted legal_name)
            let existing_name_str = existing_name.unwrap_or_default();
            let legal_changed = existing_name_str != legal_name;
            diff.insert(
                "legal_name".to_string(),
                serde_json::json!({
                    "existing": existing_name_str,
                    "submitted": legal_name,
                    "changed": legal_changed,
                }),
            );

            // Diff address
            let existing_addr_str = existing_address
                .as_ref()
                .map(|v| v.to_string())
                .unwrap_or_default();
            let submitted_addr_str = address
                .map(|v| v.to_string())
                .unwrap_or_default();
            let addr_changed = existing_addr_str != submitted_addr_str;
            diff.insert(
                "address".to_string(),
                serde_json::json!({
                    "existing": existing_address,
                    "submitted": address,
                    "changed": addr_changed,
                }),
            );

            // Diff banking
            diff.insert(
                "banking".to_string(),
                serde_json::json!({
                    "submitted": banking,
                    "changed": banking.is_some(),
                }),
            );
        }
    }

    // Confidence: v1 heuristic
    // 1.0 for explicitly typed fields, 0.5 for fields where a doc is attached
    let doc_bonus = if has_tax_doc { 0.0 } else { 0.0 };
    confidence.insert(
        "legal_name".to_string(),
        serde_json::json!(1.0),
    );
    confidence.insert(
        "address".to_string(),
        serde_json::json!(1.0),
    );
    confidence.insert(
        "tax_form_type".to_string(),
        serde_json::json!(if has_tax_doc { 1.0 } else { 0.7 }),
    );
    confidence.insert(
        "banking".to_string(),
        serde_json::json!(banking.map(|_| 1.0).unwrap_or(0.0)),
    );
    confidence.insert(
        "remit_contacts".to_string(),
        serde_json::json!(0.9),
    );

    Ok((
        serde_json::Value::Object(diff),
        serde_json::Value::Object(confidence),
    ))
}

/// Apply the diff fields back to the vendor row on approval.
async fn apply_diff_to_vendor(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    vendor_id: &VendorId,
    diff: &serde_json::Value,
) -> Result<(), ApiError> {
    let mut set_clauses: Vec<String> = Vec::new();
    let mut param_idx = 3u32;
    let mut bind_legal_name: Option<String> = None;
    let mut bind_address: Option<serde_json::Value> = None;

    if let Some(obj) = diff.as_object() {
        if let Some(ln) = obj.get("legal_name") {
            if ln.get("changed").and_then(|c| c.as_bool()).unwrap_or(false) {
                set_clauses.push(format!("name = ${}", param_idx));
                bind_legal_name = ln.get("submitted").and_then(|v| v.as_str()).map(|s| s.to_string());
                param_idx += 1;
            }
        }

        if let Some(addr) = obj.get("address") {
            if addr.get("changed").and_then(|c| c.as_bool()).unwrap_or(false) {
                set_clauses.push(format!("address = ${}::jsonb", param_idx));
                bind_address = addr.get("submitted").cloned();
                param_idx += 1;
            }
        }

        // Banking: store as encrypted columns (placeholder matching 097 pattern)
        if let Some(banking) = obj.get("banking") {
            if banking.get("changed").and_then(|c| c.as_bool()).unwrap_or(false) {
                if let Some(bank_obj) = banking.get("submitted") {
                    if let Some(account) = bank_obj.get("account_number").and_then(|v| v.as_str()) {
                        let last_four: String = account.chars().rev().take(4).collect::<String>().chars().rev().collect();
                        set_clauses.push(format!("bank_account_last_four = ${}", param_idx));
                        // We'll bind last_four below
                        set_clauses.push(format!("bank_account_encrypted = ${}", param_idx + 1));
                        set_clauses.push(format!("bank_routing_encrypted = ${}", param_idx + 2));
                        set_clauses.push(format!("bank_name = ${}", param_idx + 3));
                        set_clauses.push(format!("bank_account_type = ${}", param_idx + 4));
                        set_clauses.push(format!("bank_account_updated_at = NOW()"));
                        // We need to build the bind values
                        let enc_account = format!("enc:{}", account);
                        let routing = bank_obj.get("routing_number").and_then(|v| v.as_str()).unwrap_or("");
                        let enc_routing = format!("enc:{}", routing);
                        let bank_name = bank_obj.get("bank_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let account_type = bank_obj.get("account_type").and_then(|v| v.as_str()).unwrap_or("checking").to_string();

                        let sql = format!(
                            "UPDATE vendors SET {} WHERE id = $1 AND tenant_id = $2",
                            set_clauses.join(", ")
                        );
                        let mut query = sqlx::query(&sql)
                            .bind(vendor_id.0)
                            .bind(*tenant_id.as_uuid());
                        if let Some(ln) = bind_legal_name.take() {
                            query = query.bind(ln);
                        }
                        if let Some(addr) = bind_address.take() {
                            query = query.bind(addr);
                        }
                        query = query.bind(&last_four);
                        query = query.bind(&enc_account);
                        query = query.bind(&enc_routing);
                        query = query.bind(&bank_name);
                        query = query.bind(&account_type);
                        query
                            .execute(pool)
                            .await
                            .map_err(|e| Error::Database(format!("Failed to apply vendor diff: {}", e)))?;
                        return Ok(());
                    }
                }
            }
        }
    }

    if !set_clauses.is_empty() {
        set_clauses.push("updated_at = NOW()".to_string());
        let sql = format!(
            "UPDATE vendors SET {} WHERE id = $1 AND tenant_id = $2",
            set_clauses.join(", ")
        );
        let mut query = sqlx::query(&sql)
            .bind(vendor_id.0)
            .bind(*tenant_id.as_uuid());
        if let Some(ln) = bind_legal_name {
            query = query.bind(ln);
        }
        if let Some(addr) = bind_address {
            query = query.bind(addr);
        }
        query
            .execute(pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to apply vendor diff: {}", e)))?;
    }

    Ok(())
}
