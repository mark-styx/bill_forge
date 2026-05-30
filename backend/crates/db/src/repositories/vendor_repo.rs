//! Vendor repository implementation

use async_trait::async_trait;
use billforge_core::{
    domain::{
        AccountType, BankingVerification, BankingVerificationStatus, BankAccount,
        CreateVendorInput, UpdateVendorInput, Vendor, VendorAddress, VendorContact, VendorFilters,
        VendorId, VendorStatus, VendorType,
    },
    traits::VendorRepository,
    types::{PaginatedResponse, Pagination, PaginationMeta, TenantId},
    Error, Result,
};
use chrono::Utc;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

pub struct VendorRepositoryImpl {
    pool: Arc<PgPool>,
}

impl VendorRepositoryImpl {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl VendorRepository for VendorRepositoryImpl {
    async fn create(&self, tenant_id: &TenantId, input: CreateVendorInput) -> Result<Vendor> {
        let id = VendorId::new();
        let now = Utc::now();

        sqlx::query(
            r#"INSERT INTO vendors (
                id, tenant_id, name, vendor_type, tax_id, address, contact_email, contact_phone,
                payment_terms, is_active, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"#,
        )
        .bind(id.0)
        .bind(*tenant_id.as_uuid())
        .bind(&input.name)
        .bind(
            serde_json::to_value(input.vendor_type)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| "business".to_string()),
        )
        .bind(&input.tax_id)
        .bind(input.address.as_ref().map(sqlx::types::Json))
        .bind(&input.email)
        .bind(&input.phone)
        .bind(&input.payment_terms)
        .bind(true)
        .bind(now)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to create vendor: {}", e)))?;

        Ok(Vendor {
            id,
            tenant_id: tenant_id.clone(),
            name: input.name,
            legal_name: input.legal_name,
            vendor_type: input.vendor_type,
            status: VendorStatus::Active,
            email: input.email,
            phone: input.phone,
            website: input.website,
            address: input.address,
            tax_id: input.tax_id,
            tax_id_type: input.tax_id_type,
            w9_on_file: false,
            w9_received_date: None,
            payment_terms: input.payment_terms,
            default_payment_method: input.default_payment_method,
            bank_account: None,
            payment_hold: false,
            payment_hold_reason: None,
            vendor_code: input.vendor_code,
            default_gl_code: input.default_gl_code,
            default_department: input.default_department,
            primary_contact: None,
            contacts: Vec::new(),
            notes: input.notes,
            tags: input.tags,
            custom_fields: serde_json::Value::Object(serde_json::Map::new()),
            created_at: now,
            updated_at: now,
        })
    }

    async fn get_by_id(&self, tenant_id: &TenantId, id: &VendorId) -> Result<Option<Vendor>> {
        let result = sqlx::query_as::<_, VendorRow>(
            "SELECT * FROM vendors WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id.0)
        .bind(*tenant_id.as_uuid())
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get vendor: {}", e)))?;

        Ok(result.map(|row| row.into_vendor(tenant_id)))
    }

    async fn list(
        &self,
        tenant_id: &TenantId,
        _filters: &VendorFilters,
        pagination: &Pagination,
    ) -> Result<PaginatedResponse<Vendor>> {
        let offset = ((pagination.page - 1) * pagination.per_page) as i32;

        let rows = sqlx::query_as::<_, VendorRow>(
            "SELECT * FROM vendors WHERE tenant_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        )
        .bind(*tenant_id.as_uuid())
        .bind(pagination.per_page as i32)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to list vendors: {}", e)))?;

        let vendors: Vec<Vendor> = rows
            .into_iter()
            .map(|row| row.into_vendor(tenant_id))
            .collect();

        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vendors WHERE tenant_id = $1")
            .bind(*tenant_id.as_uuid())
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to count vendors: {}", e)))?;

        Ok(PaginatedResponse {
            data: vendors,
            pagination: PaginationMeta {
                page: pagination.page,
                per_page: pagination.per_page,
                total_items: total as u64,
                total_pages: ((total as f64) / (pagination.per_page as f64)).ceil() as u32,
            },
        })
    }

    async fn update(
        &self,
        tenant_id: &TenantId,
        id: &VendorId,
        input: UpdateVendorInput,
    ) -> Result<Vendor> {
        let now = Utc::now();

        // Simple implementation - update specific fields
        if let Some(name) = input.name {
            sqlx::query(
                "UPDATE vendors SET name = $1, updated_at = $2 WHERE id = $3 AND tenant_id = $4",
            )
            .bind(name)
            .bind(now)
            .bind(id.0)
            .bind(*tenant_id.as_uuid())
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to update vendor: {}", e)))?;
        }

        self.get_by_id(tenant_id, id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource_type: "Vendor".to_string(),
                id: id.0.to_string(),
            })
    }

    async fn delete(&self, tenant_id: &TenantId, id: &VendorId) -> Result<()> {
        sqlx::query("DELETE FROM vendors WHERE id = $1 AND tenant_id = $2")
            .bind(id.0)
            .bind(*tenant_id.as_uuid())
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to delete vendor: {}", e)))?;

        Ok(())
    }

    async fn find_by_name(&self, tenant_id: &TenantId, name: &str) -> Result<Option<Vendor>> {
        let result = sqlx::query_as::<_, VendorRow>(
            "SELECT * FROM vendors WHERE tenant_id = $1 AND name = $2",
        )
        .bind(*tenant_id.as_uuid())
        .bind(name)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to find vendor: {}", e)))?;

        Ok(result.map(|row| row.into_vendor(tenant_id)))
    }

    async fn add_contact(
        &self,
        tenant_id: &TenantId,
        vendor_id: &VendorId,
        contact: VendorContact,
    ) -> Result<()> {
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO vendor_contacts (
                id, tenant_id, vendor_id, name, title, email, phone, is_primary, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#
        )
        .bind(contact.id)
        .bind(*tenant_id.as_uuid())
        .bind(vendor_id.0)
        .bind(&contact.name)
        .bind(&contact.title)
        .bind(&contact.email)
        .bind(&contact.phone)
        .bind(contact.is_primary)
        .bind(now)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to add vendor contact: {}", e)))?;

        Ok(())
    }

    async fn remove_contact(
        &self,
        tenant_id: &TenantId,
        vendor_id: &VendorId,
        contact_id: Uuid,
    ) -> Result<()> {
        sqlx::query(
            "DELETE FROM vendor_contacts WHERE id = $1 AND tenant_id = $2 AND vendor_id = $3",
        )
        .bind(contact_id)
        .bind(*tenant_id.as_uuid())
        .bind(vendor_id.0)
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to remove vendor contact: {}", e)))?;

        Ok(())
    }

    async fn list_messages(
        &self,
        tenant_id: &TenantId,
        vendor_id: &VendorId,
        limit: u32,
    ) -> Result<Vec<billforge_core::domain::VendorMessage>> {
        let rows = sqlx::query_as::<_, MessageRow>(
            r#"
            SELECT id, vendor_id, tenant_id, subject, body, sent_by, sent_at, status
            FROM vendor_messages
            WHERE tenant_id = $1 AND vendor_id = $2
            ORDER BY sent_at DESC
            LIMIT $3
            "#,
        )
        .bind(*tenant_id.as_uuid())
        .bind(vendor_id.0)
        .bind(limit as i32)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to list messages: {}", e)))?;

        Ok(rows
            .into_iter()
            .map(|row| row.into_message(tenant_id, vendor_id))
            .collect())
    }

    async fn send_message(
        &self,
        tenant_id: &TenantId,
        vendor_id: &VendorId,
        subject: String,
        body: String,
        sent_by: Option<Uuid>,
    ) -> Result<billforge_core::domain::VendorMessage> {
        let id = Uuid::new_v4();
        let now = chrono::Utc::now();

        sqlx::query(
            r#"
            INSERT INTO vendor_messages (id, tenant_id, vendor_id, subject, body, sent_by, sent_at, status)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#
        )
        .bind(id)
        .bind(*tenant_id.as_uuid())
        .bind(vendor_id.0)
        .bind(&subject)
        .bind(&body)
        .bind(sent_by)
        .bind(now)
        .bind("sent")
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to send message: {}", e)))?;

        Ok(billforge_core::domain::VendorMessage {
            id,
            vendor_id: vendor_id.clone(),
            tenant_id: tenant_id.clone(),
            subject,
            body,
            sender_type: billforge_core::domain::MessageSender::Internal,
            sender_id: sent_by,
            sender_name: "System".to_string(),
            attachments: Vec::new(),
            read_at: None,
            created_at: now,
        })
    }
}

/// Helper struct for mapping database rows
#[derive(sqlx::FromRow)]
#[allow(dead_code)]
struct VendorRow {
    id: Uuid,
    tenant_id: Uuid,
    name: String,
    vendor_type: Option<String>,
    tax_id: Option<String>,
    address: Option<sqlx::types::Json<VendorAddress>>,
    contact_email: Option<String>,
    contact_phone: Option<String>,
    payment_terms: Option<String>,
    is_active: bool,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
    // Banking columns (migration 097)
    bank_name: Option<String>,
    bank_account_last_four: Option<String>,
    bank_account_encrypted: Option<String>,
    bank_routing_encrypted: Option<String>,
    bank_account_type: Option<String>,
    payment_hold: Option<bool>,
    payment_hold_reason: Option<String>,
}

impl VendorRow {
    fn into_vendor(self, tenant_id: &TenantId) -> Vendor {
        Vendor {
            id: VendorId(self.id),
            tenant_id: tenant_id.clone(),
            name: self.name,
            legal_name: None,
            vendor_type: self
                .vendor_type
                .as_deref()
                .and_then(|vt| {
                    serde_json::from_value(serde_json::Value::String(vt.to_string())).ok()
                })
                .unwrap_or(VendorType::Business),
            status: if self.is_active {
                VendorStatus::Active
            } else {
                VendorStatus::Inactive
            },
            email: self.contact_email,
            phone: self.contact_phone,
            website: None,
            address: self.address.map(|a| a.0),
            tax_id: self.tax_id,
            tax_id_type: None,
            w9_on_file: false,
            w9_received_date: None,
            payment_terms: self.payment_terms,
            default_payment_method: None,
            bank_account: match (
                &self.bank_name,
                &self.bank_account_last_four,
                &self.bank_account_encrypted,
                &self.bank_routing_encrypted,
                &self.bank_account_type,
            ) {
                (Some(bn), Some(lf), Some(ae), Some(re), Some(at)) => Some(BankAccount {
                    bank_name: bn.clone(),
                    account_type: match at.as_str() {
                        "savings" => AccountType::Savings,
                        _ => AccountType::Checking,
                    },
                    account_last_four: lf.clone(),
                    account_number_encrypted: ae.clone(),
                    routing_number_encrypted: re.clone(),
                }),
                _ => None,
            },
            payment_hold: self.payment_hold.unwrap_or(false),
            payment_hold_reason: self.payment_hold_reason,
            vendor_code: None,
            default_gl_code: None,
            default_department: None,
            primary_contact: None,
            contacts: Vec::new(),
            notes: None,
            tags: Vec::new(),
            custom_fields: serde_json::Value::Object(serde_json::Map::new()),
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Banking verification methods (outside the trait, called directly)
// ---------------------------------------------------------------------------

impl VendorRepositoryImpl {
    /// Record a banking detail change: updates the vendor's encrypted banking columns,
    /// sets payment_hold = true, and creates a pending verification row.
    pub async fn record_banking_change(
        &self,
        tenant_id: &TenantId,
        vendor_id: &VendorId,
        prev_last_four: Option<&str>,
        new_last_four: &str,
        bank_name: &str,
        account_type: &str,
        account_encrypted: &str,
        routing_encrypted: &str,
        requested_by: Uuid,
    ) -> Result<BankingVerification> {
        let verification_id = Uuid::new_v4();
        let now = chrono::Utc::now();

        // Update vendor banking columns + set payment_hold
        sqlx::query(
            r#"UPDATE vendors SET
                bank_name = $3,
                bank_account_last_four = $4,
                bank_account_encrypted = $5,
                bank_routing_encrypted = $6,
                bank_account_type = $7,
                bank_account_updated_at = $8,
                payment_hold = true,
                payment_hold_reason = 'Banking details changed - pending verification',
                updated_at = $8
            WHERE id = $1 AND tenant_id = $2"#,
        )
        .bind(vendor_id.0)
        .bind(*tenant_id.as_uuid())
        .bind(bank_name)
        .bind(new_last_four)
        .bind(account_encrypted)
        .bind(routing_encrypted)
        .bind(account_type)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to update vendor banking: {}", e)))?;

        // Create pending verification row
        sqlx::query(
            r#"INSERT INTO vendor_banking_verifications
                (id, tenant_id, vendor_id, previous_account_last_four, new_account_last_four,
                 status, requested_by, requested_at)
            VALUES ($1, $2, $3, $4, $5, 'pending', $6, $7)"#,
        )
        .bind(verification_id)
        .bind(*tenant_id.as_uuid())
        .bind(vendor_id.0)
        .bind(prev_last_four)
        .bind(new_last_four)
        .bind(requested_by)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to create banking verification: {}", e)))?;

        Ok(BankingVerification {
            id: verification_id,
            tenant_id: tenant_id.clone(),
            vendor_id: vendor_id.clone(),
            previous_account_last_four: prev_last_four.map(String::from),
            new_account_last_four: new_last_four.to_string(),
            status: BankingVerificationStatus::Pending,
            callback_method: None,
            callback_contact: None,
            verifier_notes: None,
            requested_by,
            requested_at: now,
            verified_by: None,
            verified_at: None,
        })
    }

    /// Verify a pending banking change: sets status = verified, clears payment_hold.
    pub async fn verify_banking_change(
        &self,
        tenant_id: &TenantId,
        verification_id: Uuid,
        verifier_user_id: Uuid,
        callback_method: &str,
        callback_contact: &str,
        notes: Option<&str>,
    ) -> Result<BankingVerification> {
        let now = chrono::Utc::now();

        // Update verification row
        let result = sqlx::query_as::<_, VerificationRow>(
            r#"UPDATE vendor_banking_verifications SET
                status = 'verified',
                verified_by = $3,
                verified_at = $4,
                callback_method = $5,
                callback_contact = $6,
                verifier_notes = $7
            WHERE id = $1 AND tenant_id = $2 AND status = 'pending'
            RETURNING id, tenant_id, vendor_id, previous_account_last_four, new_account_last_four,
                      status, requested_by, requested_at, verified_by, verified_at,
                      callback_method, callback_contact, verifier_notes"#,
        )
        .bind(verification_id)
        .bind(*tenant_id.as_uuid())
        .bind(verifier_user_id)
        .bind(now)
        .bind(callback_method)
        .bind(callback_contact)
        .bind(notes)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to verify banking change: {}", e)))?;

        let row = result.ok_or_else(|| Error::NotFound {
            resource_type: "BankingVerification".to_string(),
            id: verification_id.to_string(),
        })?;

        // Clear payment hold on the vendor
        sqlx::query(
            "UPDATE vendors SET payment_hold = false, payment_hold_reason = NULL, updated_at = $3 \
             WHERE id = $1 AND tenant_id = $2",
        )
        .bind(row.vendor_id)
        .bind(*tenant_id.as_uuid())
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to clear payment hold: {}", e)))?;

        Ok(BankingVerification {
            id: row.id,
            tenant_id: tenant_id.clone(),
            vendor_id: VendorId(row.vendor_id),
            previous_account_last_four: row.previous_account_last_four,
            new_account_last_four: row.new_account_last_four,
            status: BankingVerificationStatus::Verified,
            callback_method: match row.callback_method.as_deref() {
                Some("phone") => Some(billforge_core::domain::CallbackMethod::Phone),
                Some("in_person") => Some(billforge_core::domain::CallbackMethod::InPerson),
                Some("known_email") => Some(billforge_core::domain::CallbackMethod::KnownEmail),
                _ => None,
            },
            callback_contact: row.callback_contact,
            verifier_notes: row.verifier_notes,
            requested_by: row.requested_by,
            requested_at: row.requested_at,
            verified_by: row.verified_by,
            verified_at: row.verified_at,
        })
    }

    /// Check if a vendor has a pending banking verification (used by ERP sync guard).
    pub async fn has_pending_banking_verification(
        &self,
        tenant_id: &TenantId,
        vendor_id: &VendorId,
    ) -> Result<bool> {
        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT COUNT(*) FROM vendor_banking_verifications \
             WHERE tenant_id = $1 AND vendor_id = $2 AND status = 'pending'",
        )
        .bind(*tenant_id.as_uuid())
        .bind(vendor_id.0)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to check pending verification: {}", e)))?;

        Ok(row.map(|(c,)| c > 0).unwrap_or(false))
    }

    /// List banking verifications for a vendor.
    pub async fn list_banking_verifications(
        &self,
        tenant_id: &TenantId,
        vendor_id: &VendorId,
    ) -> Result<Vec<BankingVerification>> {
        let rows = sqlx::query_as::<_, VerificationRow>(
            "SELECT id, tenant_id, vendor_id, previous_account_last_four, new_account_last_four, \
                    status, requested_by, requested_at, verified_by, verified_at, \
                    callback_method, callback_contact, verifier_notes \
             FROM vendor_banking_verifications \
             WHERE tenant_id = $1 AND vendor_id = $2 \
             ORDER BY requested_at DESC",
        )
        .bind(*tenant_id.as_uuid())
        .bind(vendor_id.0)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to list banking verifications: {}", e)))?;

        Ok(rows
            .into_iter()
            .map(|r| BankingVerification {
                id: r.id,
                tenant_id: tenant_id.clone(),
                vendor_id: VendorId(r.vendor_id),
                previous_account_last_four: r.previous_account_last_four,
                new_account_last_four: r.new_account_last_four,
                status: match r.status.as_str() {
                    "verified" => BankingVerificationStatus::Verified,
                    "rejected" => BankingVerificationStatus::Rejected,
                    _ => BankingVerificationStatus::Pending,
                },
                callback_method: match r.callback_method.as_deref() {
                    Some("phone") => Some(billforge_core::domain::CallbackMethod::Phone),
                    Some("in_person") => Some(billforge_core::domain::CallbackMethod::InPerson),
                    Some("known_email") => Some(billforge_core::domain::CallbackMethod::KnownEmail),
                    _ => None,
                },
                callback_contact: r.callback_contact,
                verifier_notes: r.verifier_notes,
                requested_by: r.requested_by,
                requested_at: r.requested_at,
                verified_by: r.verified_by,
                verified_at: r.verified_at,
            })
            .collect())
    }
}

/// Helper struct for mapping verification rows
#[derive(sqlx::FromRow)]
struct VerificationRow {
    id: Uuid,
    tenant_id: Uuid,
    vendor_id: Uuid,
    previous_account_last_four: Option<String>,
    new_account_last_four: String,
    status: String,
    requested_by: Uuid,
    requested_at: chrono::DateTime<Utc>,
    verified_by: Option<Uuid>,
    verified_at: Option<chrono::DateTime<Utc>>,
    callback_method: Option<String>,
    callback_contact: Option<String>,
    verifier_notes: Option<String>,
}

/// Helper struct for mapping message rows
#[derive(sqlx::FromRow)]
#[allow(dead_code)]
struct MessageRow {
    id: Uuid,
    vendor_id: Uuid,
    tenant_id: String,
    subject: String,
    body: String,
    sent_by: Option<Uuid>,
    sent_at: chrono::DateTime<Utc>,
    status: String,
}

impl MessageRow {
    fn into_message(
        self,
        tenant_id: &TenantId,
        vendor_id: &VendorId,
    ) -> billforge_core::domain::VendorMessage {
        billforge_core::domain::VendorMessage {
            id: self.id,
            vendor_id: vendor_id.clone(),
            tenant_id: tenant_id.clone(),
            subject: self.subject,
            body: self.body,
            sender_type: billforge_core::domain::MessageSender::Internal,
            sender_id: self.sent_by,
            sender_name: "System".to_string(),
            attachments: Vec::new(),
            read_at: None,
            created_at: self.sent_at,
        }
    }
}
