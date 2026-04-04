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
        .bind(*tenant_id.as_uuid())
        .bind(input.vendor_id)
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
        .bind(*tenant_id.as_uuid())
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
            .bind(*tenant_id.as_uuid());

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
            .bind(*tenant_id.as_uuid());

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
        if let Some(obj) = updates.as_object() {
            // Extract all possible field values upfront
            let vendor_name = obj.get("vendor_name").and_then(|v| v.as_str()).map(String::from);
            let invoice_number = obj.get("invoice_number").and_then(|v| v.as_str()).map(String::from);
            let invoice_date = obj.get("invoice_date").and_then(|v| v.as_str())
                .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
            let has_invoice_date = obj.contains_key("invoice_date");
            let due_date = obj.get("due_date").and_then(|v| v.as_str())
                .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
            let has_due_date = obj.contains_key("due_date");
            let po_number = obj.get("po_number").and_then(|v| v.as_str()).map(String::from);
            let has_po_number = obj.contains_key("po_number");
            let department = obj.get("department").and_then(|v| v.as_str()).map(String::from);
            let has_department = obj.contains_key("department");
            let gl_code = obj.get("gl_code").and_then(|v| v.as_str()).map(String::from);
            let has_gl_code = obj.contains_key("gl_code");
            let cost_center = obj.get("cost_center").and_then(|v| v.as_str()).map(String::from);
            let has_cost_center = obj.contains_key("cost_center");
            let notes = obj.get("notes").and_then(|v| v.as_str()).map(String::from);
            let has_notes = obj.contains_key("notes");
            let categorization_confidence = obj.get("categorization_confidence").and_then(|v| v.as_f64()).map(|f| f as f32);
            let has_categorization_confidence = obj.contains_key("categorization_confidence");
            let total_amount_cents = obj.get("total_amount")
                .and_then(|v| v.as_object())
                .and_then(|o| o.get("amount"))
                .and_then(|a| a.as_i64());
            let currency = obj.get("total_amount")
                .and_then(|v| v.as_object())
                .and_then(|o| o.get("currency"))
                .and_then(|c| c.as_str())
                .map(String::from);

            // Build SET clauses and track parameter positions
            let mut set_parts: Vec<String> = Vec::new();
            let mut param_idx = 1u32;

            // We use an enum to track bind order and types
            enum BindVal {
                Str(String),
                OptStr(Option<String>),
                OptDate(Option<NaiveDate>),
                I64(i64),
                OptF32(Option<f32>),
            }
            let mut bind_vals: Vec<BindVal> = Vec::new();

            if let Some(ref v) = vendor_name {
                set_parts.push(format!("vendor_name = ${}", param_idx));
                bind_vals.push(BindVal::Str(v.clone()));
                param_idx += 1;
            }
            if let Some(ref v) = invoice_number {
                set_parts.push(format!("invoice_number = ${}", param_idx));
                bind_vals.push(BindVal::Str(v.clone()));
                param_idx += 1;
            }
            if has_invoice_date {
                set_parts.push(format!("invoice_date = ${}", param_idx));
                bind_vals.push(BindVal::OptDate(invoice_date));
                param_idx += 1;
            }
            if has_due_date {
                set_parts.push(format!("due_date = ${}", param_idx));
                bind_vals.push(BindVal::OptDate(due_date));
                param_idx += 1;
            }
            if has_po_number {
                set_parts.push(format!("po_number = ${}", param_idx));
                bind_vals.push(BindVal::OptStr(po_number));
                param_idx += 1;
            }
            if has_department {
                set_parts.push(format!("department = ${}", param_idx));
                bind_vals.push(BindVal::OptStr(department));
                param_idx += 1;
            }
            if has_gl_code {
                set_parts.push(format!("gl_code = ${}", param_idx));
                bind_vals.push(BindVal::OptStr(gl_code));
                param_idx += 1;
            }
            if has_cost_center {
                set_parts.push(format!("cost_center = ${}", param_idx));
                bind_vals.push(BindVal::OptStr(cost_center));
                param_idx += 1;
            }
            if has_notes {
                set_parts.push(format!("notes = ${}", param_idx));
                bind_vals.push(BindVal::OptStr(notes));
                param_idx += 1;
            }
            if has_categorization_confidence {
                set_parts.push(format!("categorization_confidence = ${}", param_idx));
                bind_vals.push(BindVal::OptF32(categorization_confidence));
                param_idx += 1;
            }
            if let Some(amount) = total_amount_cents {
                set_parts.push(format!("total_amount_cents = ${}", param_idx));
                bind_vals.push(BindVal::I64(amount));
                param_idx += 1;
            }
            if let Some(ref cur) = currency {
                set_parts.push(format!("currency = ${}", param_idx));
                bind_vals.push(BindVal::Str(cur.clone()));
                param_idx += 1;
            }

            if !set_parts.is_empty() {
                let set_clause = set_parts.join(", ");
                let sql = format!(
                    "UPDATE invoices SET {}, updated_at = NOW() WHERE id = ${} AND tenant_id = ${}",
                    set_clause, param_idx, param_idx + 1
                );

                let mut query = sqlx::query(&sql);
                for val in bind_vals {
                    match val {
                        BindVal::Str(s) => query = query.bind(s),
                        BindVal::OptStr(s) => query = query.bind(s),
                        BindVal::OptDate(d) => query = query.bind(d),
                        BindVal::I64(n) => query = query.bind(n),
                        BindVal::OptF32(f) => query = query.bind(f),
                    }
                }
                query = query.bind(id.0).bind(*tenant_id.as_uuid());

                query.execute(&*self.pool)
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
            .bind(*tenant_id.as_uuid())
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
            .bind(*tenant_id.as_uuid())
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
            .bind(*tenant_id.as_uuid())
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
    tenant_id: Uuid,
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
