//! Payment request repository implementation

use billforge_core::types::TenantId;
use billforge_core::{Error, Result};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

pub struct PaymentRequestRepositoryImpl {
    pool: Arc<PgPool>,
}

impl PaymentRequestRepositoryImpl {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    /// Create a new payment request with invoices in a single transaction.
    /// Validates that all invoices are in `ready_for_payment` processing status
    /// and belong to the same tenant.
    pub async fn create_payment_request(
        &self,
        tenant_id: &TenantId,
        created_by: Uuid,
        invoice_ids: &[Uuid],
        notes: Option<String>,
    ) -> Result<PaymentRequest> {
        if invoice_ids.is_empty() {
            return Err(Error::Validation(
                "At least one invoice ID is required".to_string(),
            ));
        }

        let id = Uuid::new_v4();
        let now = Utc::now();
        let request_number = self.generate_request_number(tenant_id).await?;

        let mut tx = self.pool.begin().await.map_err(|e| {
            Error::Database(format!("Failed to begin transaction: {}", e))
        })?;

        // Validate and fetch invoices
        let invoices: Vec<InvoiceRow> = sqlx::query_as::<_, InvoiceRow>(
            r#"SELECT id, vendor_id, vendor_name, invoice_number, total_amount_cents, currency, due_date
               FROM invoices
               WHERE id = ANY($1) AND tenant_id = $2 AND processing_status = 'ready_for_payment'"#,
        )
        .bind(invoice_ids)
        .bind(tenant_id.as_uuid())
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to fetch invoices: {}", e)))?;

        if invoices.len() != invoice_ids.len() {
            let found_ids: std::collections::HashSet<Uuid> =
                invoices.iter().map(|i| i.id).collect();
            let missing: Vec<Uuid> = invoice_ids
                .iter()
                .filter(|id| !found_ids.contains(id))
                .copied()
                .collect();
            return Err(Error::Validation(format!(
                "Some invoices are not in ready_for_payment status or don't belong to this tenant: {:?}",
                missing
            )));
        }

        // Check if any invoice is already in an active (draft/submitted) payment request
        let already_claimed: Vec<(Uuid,)> = sqlx::query_as(
            r#"SELECT pri.invoice_id
               FROM payment_request_items pri
               JOIN payment_requests pr ON pr.id = pri.payment_request_id
               WHERE pri.invoice_id = ANY($1)
                 AND pr.tenant_id = $2
                 AND pr.status IN ('draft', 'submitted')"#,
        )
        .bind(invoice_ids)
        .bind(tenant_id.as_uuid())
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to check for claimed invoices: {}", e)))?;

        if !already_claimed.is_empty() {
            let claimed_ids: Vec<Uuid> = already_claimed.into_iter().map(|(id,)| id).collect();
            return Err(Error::Validation(format!(
                "Some invoices are already in an active payment request: {:?}",
                claimed_ids
            )));
        }

        // Compute aggregates
        let total_amount_cents: i64 = invoices.iter().map(|i| i.total_amount_cents).sum();
        let invoice_count = invoices.len() as i32;
        let currency = invoices[0].currency.clone();

        let earliest_due_date = invoices
            .iter()
            .filter_map(|i| i.due_date)
            .min();
        let latest_due_date = invoices
            .iter()
            .filter_map(|i| i.due_date)
            .max();

        // Determine vendor_id: set if all invoices share the same vendor, otherwise NULL
        let vendor_ids: std::collections::HashSet<Option<Uuid>> =
            invoices.iter().map(|i| i.vendor_id).collect();
        let vendor_id = if vendor_ids.len() == 1 {
            vendor_ids.into_iter().next().flatten()
        } else {
            None
        };

        // Insert payment request
        sqlx::query(
            r#"INSERT INTO payment_requests
                (id, tenant_id, request_number, status, vendor_id,
                 total_amount_cents, currency, invoice_count,
                 earliest_due_date, latest_due_date, notes,
                 created_by, created_at, updated_at)
               VALUES ($1, $2, $3, 'draft', $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)"#,
        )
        .bind(id)
        .bind(tenant_id.as_uuid())
        .bind(&request_number)
        .bind(vendor_id)
        .bind(total_amount_cents)
        .bind(&currency)
        .bind(invoice_count)
        .bind(earliest_due_date)
        .bind(latest_due_date)
        .bind(&notes)
        .bind(created_by)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to create payment request: {}", e)))?;

        // Insert payment request items
        for invoice in &invoices {
            let item_id = Uuid::new_v4();
            sqlx::query(
                r#"INSERT INTO payment_request_items
                    (id, payment_request_id, invoice_id, amount_cents, currency, created_at)
                   VALUES ($1, $2, $3, $4, $5, $6)"#,
            )
            .bind(item_id)
            .bind(id)
            .bind(invoice.id)
            .bind(invoice.total_amount_cents)
            .bind(&invoice.currency)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| Error::Database(format!("Failed to create payment request item: {}", e)))?;
        }

        tx.commit().await.map_err(|e| {
            Error::Database(format!("Failed to commit payment request transaction: {}", e))
        })?;

        Ok(PaymentRequest {
            id,
            tenant_id: tenant_id.clone(),
            request_number,
            status: "draft".to_string(),
            vendor_id,
            total_amount_cents,
            currency,
            invoice_count,
            earliest_due_date,
            latest_due_date,
            notes,
            created_by,
            submitted_at: None,
            completed_at: None,
            created_at: now,
            updated_at: now,
        })
    }

    /// Add invoices to an existing draft payment request.
    pub async fn add_invoices_to_request(
        &self,
        tenant_id: &TenantId,
        request_id: Uuid,
        invoice_ids: &[Uuid],
    ) -> Result<Vec<PaymentRequestItem>> {
        if invoice_ids.is_empty() {
            return Err(Error::Validation(
                "At least one invoice ID is required".to_string(),
            ));
        }

        let mut tx = self.pool.begin().await.map_err(|e| {
            Error::Database(format!("Failed to begin transaction: {}", e))
        })?;

        // Verify request exists and is in draft status
        let request_row = sqlx::query_as::<_, RequestRow>(
            r#"SELECT id, tenant_id, request_number, status, vendor_id,
                      total_amount_cents, currency, invoice_count,
                      earliest_due_date, latest_due_date, notes,
                      created_by, submitted_at, completed_at,
                      created_at, updated_at
               FROM payment_requests
               WHERE id = $1 AND tenant_id = $2"#,
        )
        .bind(request_id)
        .bind(tenant_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to fetch payment request: {}", e)))?;

        let request_row = request_row.ok_or_else(|| Error::NotFound {
            resource_type: "PaymentRequest".to_string(),
            id: request_id.to_string(),
        })?;

        if request_row.status != "draft" {
            return Err(Error::Validation(
                "Can only add invoices to a draft payment request".to_string(),
            ));
        }

        // Validate and fetch invoices
        let invoices: Vec<InvoiceRow> = sqlx::query_as::<_, InvoiceRow>(
            r#"SELECT id, vendor_id, vendor_name, invoice_number, total_amount_cents, currency, due_date
               FROM invoices
               WHERE id = ANY($1) AND tenant_id = $2 AND processing_status = 'ready_for_payment'"#,
        )
        .bind(invoice_ids)
        .bind(tenant_id.as_uuid())
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to fetch invoices: {}", e)))?;

        if invoices.len() != invoice_ids.len() {
            let found_ids: std::collections::HashSet<Uuid> =
                invoices.iter().map(|i| i.id).collect();
            let missing: Vec<Uuid> = invoice_ids
                .iter()
                .filter(|id| !found_ids.contains(id))
                .copied()
                .collect();
            return Err(Error::Validation(format!(
                "Some invoices are not in ready_for_payment status or don't belong to this tenant: {:?}",
                missing
            )));
        }

        // Check for duplicates already in this payment request
        let already_added: std::collections::HashSet<Uuid> = sqlx::query_scalar(
            r#"SELECT invoice_id FROM payment_request_items
               WHERE payment_request_id = $1 AND invoice_id = ANY($2)"#,
        )
        .bind(request_id)
        .bind(invoice_ids)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to check for duplicate invoices: {}", e)))?
        .into_iter()
        .collect();

        if !already_added.is_empty() {
            return Err(Error::Validation(format!(
                "Some invoices are already in this payment request: {:?}",
                already_added.into_iter().collect::<Vec<_>>()
            )));
        }

        // Check if any invoice is already in another active (draft/submitted) payment request
        let already_claimed: Vec<(Uuid,)> = sqlx::query_as(
            r#"SELECT pri.invoice_id
               FROM payment_request_items pri
               JOIN payment_requests pr ON pr.id = pri.payment_request_id
               WHERE pri.invoice_id = ANY($1)
                 AND pr.tenant_id = $2
                 AND pr.id != $3
                 AND pr.status IN ('draft', 'submitted')"#,
        )
        .bind(invoice_ids)
        .bind(tenant_id.as_uuid())
        .bind(request_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to check for claimed invoices: {}", e)))?;

        if !already_claimed.is_empty() {
            let claimed_ids: Vec<Uuid> = already_claimed.into_iter().map(|(id,)| id).collect();
            return Err(Error::Validation(format!(
                "Some invoices are already in another active payment request: {:?}",
                claimed_ids
            )));
        }

        let now = Utc::now();
        let mut items = Vec::new();

        for invoice in &invoices {
            let item_id = Uuid::new_v4();
            sqlx::query(
                r#"INSERT INTO payment_request_items
                    (id, payment_request_id, invoice_id, amount_cents, currency, created_at)
                   VALUES ($1, $2, $3, $4, $5, $6)"#,
            )
            .bind(item_id)
            .bind(request_id)
            .bind(invoice.id)
            .bind(invoice.total_amount_cents)
            .bind(&invoice.currency)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| Error::Database(format!("Failed to create payment request item: {}", e)))?;

            items.push(PaymentRequestItem {
                id: item_id,
                payment_request_id: request_id,
                invoice_id: invoice.id,
                invoice_number: invoice.invoice_number.clone(),
                vendor_name: invoice.vendor_name.clone(),
                amount_cents: invoice.total_amount_cents,
                currency: invoice.currency.clone(),
                due_date: invoice.due_date,
                created_at: now,
            });
        }

        // Recompute aggregates from all items
        let all_items: Vec<(i64, Option<chrono::NaiveDate>, Option<Uuid>)> = sqlx::query_as(
            r#"SELECT i.total_amount_cents, i.due_date, i.vendor_id
               FROM payment_request_items pri
               JOIN invoices i ON i.id = pri.invoice_id AND i.tenant_id = $2
               WHERE pri.payment_request_id = $1"#,
        )
        .bind(request_id)
        .bind(tenant_id.as_uuid())
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to recompute aggregates: {}", e)))?;

        let total_amount_cents: i64 = all_items.iter().map(|(amt, _, _)| amt).sum();
        let invoice_count = all_items.len() as i32;
        let earliest_due_date = all_items.iter().filter_map(|(_, d, _)| *d).min();
        let latest_due_date = all_items.iter().filter_map(|(_, d, _)| *d).max();

        let vendor_ids: std::collections::HashSet<Option<Uuid>> =
            all_items.iter().map(|(_, _, v)| *v).collect();
        let vendor_id = if vendor_ids.len() == 1 {
            vendor_ids.into_iter().next().flatten()
        } else {
            None
        };

        sqlx::query(
            r#"UPDATE payment_requests
               SET total_amount_cents = $1, invoice_count = $2,
                   earliest_due_date = $3, latest_due_date = $4,
                   vendor_id = $5, updated_at = $6
               WHERE id = $7"#,
        )
        .bind(total_amount_cents)
        .bind(invoice_count)
        .bind(earliest_due_date)
        .bind(latest_due_date)
        .bind(vendor_id)
        .bind(now)
        .bind(request_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to update payment request aggregates: {}", e)))?;

        tx.commit().await.map_err(|e| {
            Error::Database(format!("Failed to commit add invoices transaction: {}", e))
        })?;

        Ok(items)
    }

    /// Get a payment request by ID with items, scoped to tenant.
    pub async fn get_payment_request(
        &self,
        tenant_id: &TenantId,
        id: Uuid,
    ) -> Result<Option<(PaymentRequest, Vec<PaymentRequestItem>)>> {
        let request_row = sqlx::query_as::<_, RequestRow>(
            r#"SELECT id, tenant_id, request_number, status, vendor_id,
                      total_amount_cents, currency, invoice_count,
                      earliest_due_date, latest_due_date, notes,
                      created_by, submitted_at, completed_at,
                      created_at, updated_at
               FROM payment_requests
               WHERE id = $1 AND tenant_id = $2"#,
        )
        .bind(id)
        .bind(tenant_id.as_uuid())
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get payment request: {}", e)))?;

        let request_row = match request_row {
            Some(r) => r,
            None => return Ok(None),
        };

        let item_rows = sqlx::query_as::<_, ItemRow>(
            r#"SELECT pri.id, pri.payment_request_id, pri.invoice_id,
                      i.invoice_number, i.vendor_name,
                      pri.amount_cents, pri.currency,
                      i.due_date, pri.created_at
               FROM payment_request_items pri
               JOIN invoices i ON i.id = pri.invoice_id AND i.tenant_id = $2
               WHERE pri.payment_request_id = $1"#,
        )
        .bind(id)
        .bind(tenant_id.as_uuid())
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get payment request items: {}", e)))?;

        let request = PaymentRequest::from_row(request_row);
        let items: Vec<PaymentRequestItem> = item_rows.into_iter().map(PaymentRequestItem::from_row).collect();

        Ok(Some((request, items)))
    }

    /// List payment requests with optional filters and pagination.
    pub async fn list_payment_requests(
        &self,
        tenant_id: &TenantId,
        status_filter: Option<&str>,
        vendor_id_filter: Option<Uuid>,
        page: u32,
        per_page: u32,
    ) -> Result<(Vec<PaymentRequest>, u64)> {
        let offset = (page.saturating_sub(1)) * per_page;
        let limit = per_page;

        let rows: Vec<RequestRow> = sqlx::query_as::<_, RequestRow>(
            r#"SELECT id, tenant_id, request_number, status, vendor_id,
                      total_amount_cents, currency, invoice_count,
                      earliest_due_date, latest_due_date, notes,
                      created_by, submitted_at, completed_at,
                      created_at, updated_at
               FROM payment_requests
               WHERE tenant_id = $1
                 AND ($2::text IS NULL OR status = $2)
                 AND ($3::uuid IS NULL OR vendor_id = $3)
               ORDER BY created_at DESC
               LIMIT $4 OFFSET $5"#,
        )
        .bind(tenant_id.as_uuid())
        .bind(status_filter)
        .bind(vendor_id_filter)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to list payment requests: {}", e)))?;

        let count: (i64,) = sqlx::query_as(
            r#"SELECT COUNT(*)
               FROM payment_requests
               WHERE tenant_id = $1
                 AND ($2::text IS NULL OR status = $2)
                 AND ($3::uuid IS NULL OR vendor_id = $3)"#,
        )
        .bind(tenant_id.as_uuid())
        .bind(status_filter)
        .bind(vendor_id_filter)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to count payment requests: {}", e)))?;

        Ok((
            rows.into_iter().map(PaymentRequest::from_row).collect(),
            count.0 as u64,
        ))
    }

    /// Submit a draft payment request, transitioning status and updating invoices.
    /// Both operations are wrapped in a single transaction to prevent inconsistent state
    /// where the request is marked submitted but invoices remain ready_for_payment.
    pub async fn submit_payment_request(
        &self,
        tenant_id: &TenantId,
        id: Uuid,
    ) -> Result<PaymentRequest> {
        let now = Utc::now();

        let mut tx = self.pool.begin().await.map_err(|e| {
            Error::Database(format!("Failed to begin transaction: {}", e))
        })?;

        let result = sqlx::query(
            r#"UPDATE payment_requests
               SET status = 'submitted', submitted_at = $1, updated_at = $2
               WHERE id = $3 AND tenant_id = $4 AND status = 'draft'"#,
        )
        .bind(now)
        .bind(now)
        .bind(id)
        .bind(tenant_id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to submit payment request: {}", e)))?;

        if result.rows_affected() == 0 {
            // Check if it exists at all
            let exists: Option<(String,)> = sqlx::query_as(
                "SELECT status FROM payment_requests WHERE id = $1 AND tenant_id = $2",
            )
            .bind(id)
            .bind(tenant_id.as_uuid())
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| Error::Database(format!("Failed to check payment request: {}", e)))?;

            match exists {
                None => {
                    return Err(Error::NotFound {
                        resource_type: "PaymentRequest".to_string(),
                        id: id.to_string(),
                    });
                }
                Some((status,)) => {
                    return Err(Error::Validation(format!(
                        "Payment request is in '{}' status, expected 'draft'",
                        status
                    )));
                }
            }
        }

        // Fetch the expected invoice count from the payment request
        let invoice_count: (i32,) = sqlx::query_as(
            "SELECT invoice_count FROM payment_requests WHERE id = $1",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to fetch invoice count: {}", e)))?;

        // Update invoice processing_status to 'payment_submitted' within same transaction.
        // Only update invoices that are still in 'ready_for_payment' status to prevent
        // double-counting when two payment requests include the same invoice.
        let invoice_result = sqlx::query(
            r#"UPDATE invoices
               SET processing_status = 'payment_submitted', updated_at = $1
               WHERE id IN (
                   SELECT invoice_id FROM payment_request_items WHERE payment_request_id = $2
               ) AND tenant_id = $3 AND processing_status = 'ready_for_payment'"#,
        )
        .bind(now)
        .bind(id)
        .bind(tenant_id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Database(format!("Failed to update invoice statuses: {}", e)))?;

        // Verify all invoices were still in ready_for_payment status.
        // If fewer were updated than expected, another request already claimed them.
        if invoice_result.rows_affected() != invoice_count.0 as u64 {
            return Err(Error::Validation(format!(
                "Could not submit: only {} of {} invoices are still in ready_for_payment status. \
                 Another payment request may have already claimed some invoices.",
                invoice_result.rows_affected(),
                invoice_count.0
            )));
        }

        tx.commit().await.map_err(|e| {
            Error::Database(format!("Failed to commit submit transaction: {}", e))
        })?;

        // Fetch and return the updated request
        let row = sqlx::query_as::<_, RequestRow>(
            r#"SELECT id, tenant_id, request_number, status, vendor_id,
                      total_amount_cents, currency, invoice_count,
                      earliest_due_date, latest_due_date, notes,
                      created_by, submitted_at, completed_at,
                      created_at, updated_at
               FROM payment_requests
               WHERE id = $1 AND tenant_id = $2"#,
        )
        .bind(id)
        .bind(tenant_id.as_uuid())
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to fetch submitted payment request: {}", e)))?;

        Ok(PaymentRequest::from_row(row))
    }

    /// Generate a unique request number: PR-YYYYMMDD-XXXX
    async fn generate_request_number(&self, tenant_id: &TenantId) -> Result<String> {
        let today = Utc::now().format("%Y%m%d").to_string();
        let prefix = format!("PR-{}-", today);

        let max_seq: Option<(String,)> = sqlx::query_as(
            r#"SELECT request_number FROM payment_requests
               WHERE tenant_id = $1 AND request_number LIKE $2
               ORDER BY request_number DESC LIMIT 1"#,
        )
        .bind(tenant_id.as_uuid())
        .bind(format!("{}%", prefix))
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to generate request number: {}", e)))?;

        let seq = match max_seq {
            Some((ref_num,)) => {
                let seq_str = ref_num.strip_prefix(&prefix).unwrap_or("0000");
                let seq: u32 = seq_str.parse().unwrap_or(0);
                seq + 1
            }
            None => 1,
        };

        Ok(format!("{}{:04}", prefix, seq))
    }
}

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentRequest {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub request_number: String,
    pub status: String,
    pub vendor_id: Option<Uuid>,
    pub total_amount_cents: i64,
    pub currency: String,
    pub invoice_count: i32,
    pub earliest_due_date: Option<chrono::NaiveDate>,
    pub latest_due_date: Option<chrono::NaiveDate>,
    pub notes: Option<String>,
    pub created_by: Uuid,
    pub submitted_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentRequestItem {
    pub id: Uuid,
    pub payment_request_id: Uuid,
    pub invoice_id: Uuid,
    pub invoice_number: String,
    pub vendor_name: String,
    pub amount_cents: i64,
    pub currency: String,
    pub due_date: Option<chrono::NaiveDate>,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Row types for sqlx mapping
// ---------------------------------------------------------------------------

use serde::{Deserialize, Serialize};

#[derive(sqlx::FromRow)]
struct RequestRow {
    id: Uuid,
    tenant_id: Uuid,
    request_number: String,
    status: String,
    vendor_id: Option<Uuid>,
    total_amount_cents: i64,
    currency: String,
    invoice_count: i32,
    earliest_due_date: Option<chrono::NaiveDate>,
    latest_due_date: Option<chrono::NaiveDate>,
    notes: Option<String>,
    created_by: Uuid,
    submitted_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl PaymentRequest {
    fn from_row(row: RequestRow) -> Self {
        Self {
            id: row.id,
            tenant_id: TenantId::from_uuid(row.tenant_id),
            request_number: row.request_number,
            status: row.status,
            vendor_id: row.vendor_id,
            total_amount_cents: row.total_amount_cents,
            currency: row.currency,
            invoice_count: row.invoice_count,
            earliest_due_date: row.earliest_due_date,
            latest_due_date: row.latest_due_date,
            notes: row.notes,
            created_by: row.created_by,
            submitted_at: row.submitted_at,
            completed_at: row.completed_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ItemRow {
    id: Uuid,
    payment_request_id: Uuid,
    invoice_id: Uuid,
    invoice_number: String,
    vendor_name: String,
    amount_cents: i64,
    currency: String,
    due_date: Option<chrono::NaiveDate>,
    created_at: DateTime<Utc>,
}

impl PaymentRequestItem {
    fn from_row(row: ItemRow) -> Self {
        Self {
            id: row.id,
            payment_request_id: row.payment_request_id,
            invoice_id: row.invoice_id,
            invoice_number: row.invoice_number,
            vendor_name: row.vendor_name,
            amount_cents: row.amount_cents,
            currency: row.currency,
            due_date: row.due_date,
            created_at: row.created_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct InvoiceRow {
    id: Uuid,
    vendor_id: Option<Uuid>,
    vendor_name: String,
    invoice_number: String,
    total_amount_cents: i64,
    currency: String,
    due_date: Option<chrono::NaiveDate>,
}
