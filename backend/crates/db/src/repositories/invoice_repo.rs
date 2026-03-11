//! Invoice repository implementation

use async_trait::async_trait;
use billforge_core::{
    domain::*,
    traits::InvoiceRepository,
    types::*,
    Error, Result,
};
use chrono::{NaiveDate, Utc};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

pub struct InvoiceRepositoryImpl {
    pool: Arc<PgPool>,
}

impl InvoiceRepositoryImpl {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl InvoiceRepository for InvoiceRepositoryImpl {
    async fn create(
        &self,
        tenant_id: &TenantId,
        input: CreateInvoiceInput,
        created_by: &UserId,
    ) -> Result<Invoice> {
        let id = InvoiceId::new();
        let now = Utc::now();

        // Insert invoice
        sqlx::query(
            r#"INSERT INTO invoices (
                id, tenant_id, vendor_id, vendor_name, invoice_number, invoice_date, due_date,
                po_number, subtotal_cents, tax_amount_cents, total_amount_cents, currency,
                line_items, document_id, ocr_confidence, notes, tags, custom_fields,
                capture_status, processing_status, department, gl_code, cost_center, created_by
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24)"#
        )
        .bind(id.0)
        .bind(tenant_id.as_str())
        .bind(input.vendor_id.map(|v| v.to_string()).unwrap_or_default())
        .bind(&input.vendor_name)
        .bind(&input.invoice_number)
        .bind(input.invoice_date)
        .bind(input.due_date)
        .bind(&input.po_number)
        .bind(input.subtotal.as_ref().map(|m| m.amount))
        .bind(input.tax_amount.as_ref().map(|m| m.amount))
        .bind(input.total_amount.amount)
        .bind(&input.currency)
        .bind(sqlx::types::Json(&input.line_items))
        .bind(input.document_id)
        .bind(input.ocr_confidence)
        .bind(&input.notes)
        .bind(sqlx::types::Json(&input.tags))
        .bind(sqlx::types::Json(&serde_json::Value::Object(serde_json::Map::new())))
        .bind(CaptureStatus::Pending.as_str())
        .bind(ProcessingStatus::Draft.as_str())
        .bind(&input.department)
        .bind(&input.gl_code)
        .bind(&input.cost_center)
        .bind(created_by.0)
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to create invoice: {}", e)))?;

        // Build and return the invoice
        let invoice = Invoice {
            id,
            tenant_id: tenant_id.clone(),
            vendor_id: input.vendor_id,
            vendor_name: input.vendor_name,
            invoice_number: input.invoice_number,
            invoice_date: input.invoice_date,
            due_date: input.due_date,
            po_number: input.po_number,
            subtotal: input.subtotal,
            tax_amount: input.tax_amount,
            total_amount: input.total_amount,
            currency: input.currency,
            line_items: input.line_items.into_iter().enumerate().map(|(idx, item)| {
                billforge_core::domain::InvoiceLineItem {
                    id: Uuid::new_v4(),
                    line_number: (idx + 1) as u32,
                    description: item.description,
                    quantity: item.quantity,
                    unit_price: item.unit_price,
                    amount: item.amount,
                    gl_code: item.gl_code,
                    department: item.department,
                    project: item.project,
                }
            }).collect(),
            capture_status: CaptureStatus::Pending,
            processing_status: ProcessingStatus::Draft,
            current_queue_id: None,
            assigned_to: None,
            document_id: input.document_id,
            supporting_documents: vec![],
            ocr_confidence: input.ocr_confidence,
            categorization_confidence: None, // Will be set when ML categorization runs
            department: input.department,
            gl_code: input.gl_code,
            cost_center: input.cost_center,
            notes: input.notes,
            tags: input.tags,
            custom_fields: serde_json::Value::Object(serde_json::Map::new()),
            created_by: created_by.clone(),
            created_at: now,
            updated_at: now,
        };

        Ok(invoice)
    }

    async fn get_by_id(&self, tenant_id: &TenantId, id: &InvoiceId) -> Result<Option<Invoice>> {
        let result = sqlx::query_as::<_, InvoiceRow>(
            "SELECT * FROM invoices WHERE id = $1 AND tenant_id = $2"
        )
        .bind(id.0)
        .bind(tenant_id.as_str())
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get invoice: {}", e)))?;

        Ok(result.map(|row| row.into_invoice(tenant_id)))
    }

    async fn list(
        &self,
        tenant_id: &TenantId,
        filters: &InvoiceFilters,
        pagination: &Pagination,
    ) -> Result<PaginatedResponse<Invoice>> {
        let offset = ((pagination.page - 1) * pagination.per_page) as i32;

        // Build dynamic query with filters
        let mut query_str = String::from("SELECT * FROM invoices WHERE tenant_id = $1");
        let mut param_count = 2;

        if filters.vendor_id.is_some() {
            query_str.push_str(&format!(" AND vendor_id = ${}", param_count));
            param_count += 1;
        }
        if filters.capture_status.is_some() {
            query_str.push_str(&format!(" AND capture_status = ${}", param_count));
            param_count += 1;
        }
        if filters.processing_status.is_some() {
            query_str.push_str(&format!(" AND processing_status = ${}", param_count));
            param_count += 1;
        }

        query_str.push_str(&format!(
            " ORDER BY created_at DESC LIMIT ${} OFFSET ${}",
            param_count,
            param_count + 1
        ));

        // Build query
        let mut query = sqlx::query_as::<_, InvoiceRow>(&query_str)
            .bind(tenant_id.as_str());

        if let Some(ref vendor_id) = filters.vendor_id {
            query = query.bind(vendor_id.to_string());
        }
        if let Some(ref capture_status) = filters.capture_status {
            query = query.bind(capture_status.as_str());
        }
        if let Some(ref processing_status) = filters.processing_status {
            query = query.bind(processing_status.as_str());
        }

        query = query.bind(pagination.per_page as i32).bind(offset);

        let rows = query
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to list invoices: {}", e)))?;

        let invoices: Vec<Invoice> = rows
            .into_iter()
            .map(|row| row.into_invoice(tenant_id))
            .collect();

        // Get total count
        let count_query_str = query_str.replace("SELECT *", "SELECT COUNT(*)");
        let count_str = count_query_str.split(" ORDER BY").next().unwrap();

        let mut count_query = sqlx::query_scalar::<_, i64>(count_str)
            .bind(tenant_id.as_str());

        if let Some(ref vendor_id) = filters.vendor_id {
            count_query = count_query.bind(vendor_id.to_string());
        }
        if let Some(ref capture_status) = filters.capture_status {
            count_query = count_query.bind(capture_status.as_str());
        }
        if let Some(ref processing_status) = filters.processing_status {
            count_query = count_query.bind(processing_status.as_str());
        }

        let total = count_query
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to count invoices: {}", e)))?;

        Ok(PaginatedResponse {
            data: invoices,
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
        id: &InvoiceId,
        updates: serde_json::Value,
    ) -> Result<Invoice> {
        let now = Utc::now();

        // Simple update implementation - update common fields
        if let Some(obj) = updates.as_object() {
            if let Some(vendor_name) = obj.get("vendor_name").and_then(|v| v.as_str()) {
                sqlx::query("UPDATE invoices SET vendor_name = $1, updated_at = $2 WHERE id = $3 AND tenant_id = $4")
                    .bind(vendor_name)
                    .bind(now)
                    .bind(id.0)
                    .bind(tenant_id.as_str())
                    .execute(&*self.pool)
                    .await
                    .map_err(|e| Error::Database(format!("Failed to update invoice: {}", e)))?;
            }
        }

        // Fetch and return updated invoice
        self.get_by_id(tenant_id, id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource_type: "Invoice".to_string(),
                id: id.to_string(),
            })
    }

    async fn delete(&self, tenant_id: &TenantId, id: &InvoiceId) -> Result<()> {
        sqlx::query("DELETE FROM invoices WHERE id = $1 AND tenant_id = $2")
            .bind(id.0)
            .bind(tenant_id.as_str())
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to delete invoice: {}", e)))?;

        Ok(())
    }

    async fn update_capture_status(
        &self,
        tenant_id: &TenantId,
        id: &InvoiceId,
        status: CaptureStatus,
    ) -> Result<()> {
        sqlx::query("UPDATE invoices SET capture_status = $1, updated_at = $2 WHERE id = $3 AND tenant_id = $4")
            .bind(status.as_str())
            .bind(Utc::now())
            .bind(id.0)
            .bind(tenant_id.as_str())
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to update capture status: {}", e)))?;

        Ok(())
    }

    async fn update_processing_status(
        &self,
        tenant_id: &TenantId,
        id: &InvoiceId,
        status: ProcessingStatus,
    ) -> Result<()> {
        sqlx::query("UPDATE invoices SET processing_status = $1, updated_at = $2 WHERE id = $3 AND tenant_id = $4")
            .bind(status.as_str())
            .bind(Utc::now())
            .bind(id.0)
            .bind(tenant_id.as_str())
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to update processing status: {}", e)))?;

        Ok(())
    }
}

/// Helper struct for mapping database rows
#[derive(sqlx::FromRow)]
struct InvoiceRow {
    id: Uuid,
    tenant_id: String,
    vendor_id: Option<Uuid>,
    vendor_name: String,
    invoice_number: String,
    invoice_date: Option<NaiveDate>,
    due_date: Option<NaiveDate>,
    po_number: Option<String>,
    subtotal_cents: Option<i64>,
    tax_amount_cents: Option<i64>,
    total_amount_cents: i64,
    currency: String,
    line_items: sqlx::types::Json<Vec<InvoiceLineItem>>,
    capture_status: String,
    processing_status: String,
    current_queue_id: Option<Uuid>,
    assigned_to: Option<Uuid>,
    document_id: Uuid,
    supporting_documents: sqlx::types::Json<Vec<Uuid>>,
    ocr_confidence: Option<f32>,
    categorization_confidence: Option<f32>,
    department: Option<String>,
    gl_code: Option<String>,
    cost_center: Option<String>,
    notes: Option<String>,
    tags: sqlx::types::Json<Vec<String>>,
    custom_fields: sqlx::types::Json<serde_json::Value>,
    created_by: Uuid,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
}

impl InvoiceRow {
    fn into_invoice(self, tenant_id: &TenantId) -> Invoice {
        Invoice {
            id: InvoiceId(self.id),
            tenant_id: tenant_id.clone(),
            vendor_id: self.vendor_id,
            vendor_name: self.vendor_name,
            invoice_number: self.invoice_number,
            invoice_date: self.invoice_date,
            due_date: self.due_date,
            po_number: self.po_number,
            subtotal: self.subtotal_cents.map(|cents| Money::new(cents, self.currency.clone())),
            tax_amount: self.tax_amount_cents.map(|cents| Money::new(cents, self.currency.clone())),
            total_amount: Money::new(self.total_amount_cents, self.currency.clone()),
            currency: self.currency,
            line_items: self.line_items.0,
            capture_status: CaptureStatus::from_str(&self.capture_status).unwrap_or(CaptureStatus::Pending),
            processing_status: ProcessingStatus::from_str(&self.processing_status).unwrap_or(ProcessingStatus::Draft),
            current_queue_id: self.current_queue_id,
            assigned_to: self.assigned_to.map(UserId),
            document_id: self.document_id,
            supporting_documents: self.supporting_documents.0,
            ocr_confidence: self.ocr_confidence,
            categorization_confidence: self.categorization_confidence,
            department: self.department,
            gl_code: self.gl_code,
            cost_center: self.cost_center,
            notes: self.notes,
            tags: self.tags.0,
            custom_fields: self.custom_fields.0,
            created_by: UserId(self.created_by),
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}
