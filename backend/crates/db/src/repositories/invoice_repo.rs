//! Invoice repository implementation

use crate::manager::DatabaseManager;
use async_trait::async_trait;
use billforge_core::{
    domain::*,
    traits::InvoiceRepository,
    types::*,
    Error, Result,
};
use chrono::{NaiveDate, Utc};
use rusqlite::params;
use std::sync::Arc;
use uuid::Uuid;

pub struct InvoiceRepositoryImpl {
    db_manager: Arc<DatabaseManager>,
}

impl InvoiceRepositoryImpl {
    pub fn new(db_manager: Arc<DatabaseManager>) -> Self {
        Self { db_manager }
    }
    
    /// The full list of columns for SELECT queries
    const SELECT_COLUMNS: &'static str = r#"
        id, vendor_id, vendor_name, invoice_number, invoice_date, due_date,
        po_number, subtotal_amount, subtotal_currency, tax_amount, tax_currency,
        total_amount, total_currency, capture_status, processing_status,
        document_id, ocr_confidence, notes, tags, custom_fields,
        created_by, created_at, updated_at, current_queue_id, assigned_to,
        department, gl_code, cost_center
    "#;
}

#[async_trait]
impl InvoiceRepository for InvoiceRepositoryImpl {
    async fn create(
        &self,
        tenant_id: &TenantId,
        input: CreateInvoiceInput,
        created_by: &UserId,
    ) -> Result<Invoice> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let id = InvoiceId::new();
        let now = Utc::now();

        // Insert invoice
        conn.execute(
            r#"INSERT INTO invoices (
                id, vendor_id, vendor_name, invoice_number, invoice_date, due_date,
                po_number, subtotal_amount, subtotal_currency, tax_amount, tax_currency,
                total_amount, total_currency, document_id, ocr_confidence, notes, tags,
                custom_fields, created_by, created_at, updated_at, capture_status, processing_status,
                department, gl_code, cost_center
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', 'draft', ?, ?, ?)"#,
            params![
                id.0.to_string(),
                input.vendor_id.map(|v| v.to_string()),
                input.vendor_name,
                input.invoice_number,
                input.invoice_date.map(|d| d.to_string()),
                input.due_date.map(|d| d.to_string()),
                input.po_number,
                input.subtotal.as_ref().map(|m| m.amount),
                input.subtotal.as_ref().map(|m| m.currency.clone()),
                input.tax_amount.as_ref().map(|m| m.amount),
                input.tax_amount.as_ref().map(|m| m.currency.clone()),
                input.total_amount.amount,
                input.total_amount.currency,
                input.document_id.to_string(),
                input.ocr_confidence,
                input.notes,
                serde_json::to_string(&input.tags).unwrap(),
                "{}",
                created_by.0.to_string(),
                now.to_rfc3339(),
                now.to_rfc3339(),
                input.department,
                input.gl_code,
                input.cost_center,
            ],
        )
        .map_err(|e| Error::Database(format!("Failed to create invoice: {}", e)))?;

        // Insert line items
        for (idx, item) in input.line_items.iter().enumerate() {
            let item_id = Uuid::new_v4();
            conn.execute(
                r#"INSERT INTO invoice_line_items (
                    id, invoice_id, line_number, description, quantity,
                    unit_price_amount, unit_price_currency, amount, amount_currency,
                    gl_code, department, project
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
                params![
                    item_id.to_string(),
                    id.0.to_string(),
                    (idx + 1) as i32,
                    item.description,
                    item.quantity,
                    item.unit_price.as_ref().map(|m| m.amount),
                    item.unit_price.as_ref().map(|m| m.currency.clone()),
                    item.amount.amount,
                    item.amount.currency,
                    item.gl_code,
                    item.department,
                    item.project,
                ],
            )
            .map_err(|e| Error::Database(format!("Failed to create line item: {}", e)))?;
        }

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
            line_items: input
                .line_items
                .into_iter()
                .enumerate()
                .map(|(idx, item)| InvoiceLineItem {
                    id: Uuid::new_v4(),
                    line_number: (idx + 1) as u32,
                    description: item.description,
                    quantity: item.quantity,
                    unit_price: item.unit_price,
                    amount: item.amount,
                    gl_code: item.gl_code,
                    department: item.department,
                    project: item.project,
                })
                .collect(),
            capture_status: CaptureStatus::Pending,
            processing_status: ProcessingStatus::Draft,
            current_queue_id: None,
            assigned_to: None,
            document_id: input.document_id,
            supporting_documents: vec![],
            ocr_confidence: input.ocr_confidence,
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
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        let sql = format!("SELECT {} FROM invoices WHERE id = ?", Self::SELECT_COLUMNS);
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;

        let invoice = stmt
            .query_row(params![id.0.to_string()], |row| {
                Ok(self.map_invoice_row(row, tenant_id.clone()))
            })
            .ok();

        match invoice {
            Some(inv) => Ok(Some(inv?)),
            None => Ok(None),
        }
    }

    async fn list(
        &self,
        tenant_id: &TenantId,
        filters: &InvoiceFilters,
        pagination: &Pagination,
    ) -> Result<PaginatedResponse<Invoice>> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        // Build query with filters
        let mut conditions = Vec::new();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(vendor_id) = &filters.vendor_id {
            conditions.push("vendor_id = ?".to_string());
            params_vec.push(Box::new(vendor_id.to_string()));
        }

        if let Some(capture_status) = &filters.capture_status {
            conditions.push("capture_status = ?".to_string());
            params_vec.push(Box::new(capture_status.as_str().to_string()));
        }

        if let Some(processing_status) = &filters.processing_status {
            conditions.push("processing_status = ?".to_string());
            params_vec.push(Box::new(processing_status.as_str().to_string()));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        // Get total count
        let count_sql = format!("SELECT COUNT(*) FROM invoices {}", where_clause);
        let total_items: i64 = conn
            .query_row(&count_sql, [], |row| row.get(0))
            .map_err(|e| Error::Database(format!("Failed to count invoices: {}", e)))?;

        // Get paginated results
        let query_sql = format!(
            "SELECT {} FROM invoices {} ORDER BY created_at DESC LIMIT ? OFFSET ?",
            Self::SELECT_COLUMNS,
            where_clause
        );

        let mut stmt = conn
            .prepare(&query_sql)
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;

        let tenant_clone = tenant_id.clone();
        let invoices = stmt
            .query_map(
                params![pagination.per_page as i64, pagination.offset() as i64],
                |row| Ok(self.map_invoice_row(row, tenant_clone.clone())),
            )
            .map_err(|e| Error::Database(format!("Failed to list invoices: {}", e)))?;

        let mut results = Vec::new();
        for invoice in invoices {
            results.push(invoice.map_err(|e| Error::Database(e.to_string()))??);
        }

        Ok(PaginatedResponse {
            data: results,
            pagination: PaginationMeta {
                page: pagination.page,
                per_page: pagination.per_page,
                total_items: total_items as u64,
                total_pages: ((total_items as f64) / (pagination.per_page as f64)).ceil() as u32,
            },
        })
    }

    async fn update(
        &self,
        tenant_id: &TenantId,
        id: &InvoiceId,
        updates: serde_json::Value,
    ) -> Result<Invoice> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        // Build update query from JSON
        let mut set_clauses = Vec::new();
        let updates_obj = updates.as_object().ok_or_else(|| {
            Error::InvalidInput {
                field: "updates".to_string(),
                message: "Must be an object".to_string(),
            }
        })?;

        for (key, _value) in updates_obj {
            set_clauses.push(format!("{} = ?", key));
        }

        set_clauses.push("updated_at = ?".to_string());

        let sql = format!(
            "UPDATE invoices SET {} WHERE id = ?",
            set_clauses.join(", ")
        );

        conn.execute(&sql, params![Utc::now().to_rfc3339(), id.0.to_string()])
            .map_err(|e| Error::Database(format!("Failed to update invoice: {}", e)))?;

        // Fetch and return updated invoice
        self.get_by_id(tenant_id, id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource_type: "Invoice".to_string(),
                id: id.to_string(),
            })
    }

    async fn delete(&self, tenant_id: &TenantId, id: &InvoiceId) -> Result<()> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        conn.execute(
            "DELETE FROM invoices WHERE id = ?",
            params![id.0.to_string()],
        )
        .map_err(|e| Error::Database(format!("Failed to delete invoice: {}", e)))?;

        Ok(())
    }

    async fn update_capture_status(
        &self,
        tenant_id: &TenantId,
        id: &InvoiceId,
        status: CaptureStatus,
    ) -> Result<()> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        conn.execute(
            "UPDATE invoices SET capture_status = ?, updated_at = ? WHERE id = ?",
            params![status.as_str(), Utc::now().to_rfc3339(), id.0.to_string()],
        )
        .map_err(|e| Error::Database(format!("Failed to update capture status: {}", e)))?;

        Ok(())
    }

    async fn update_processing_status(
        &self,
        tenant_id: &TenantId,
        id: &InvoiceId,
        status: ProcessingStatus,
    ) -> Result<()> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        conn.execute(
            "UPDATE invoices SET processing_status = ?, updated_at = ? WHERE id = ?",
            params![status.as_str(), Utc::now().to_rfc3339(), id.0.to_string()],
        )
        .map_err(|e| Error::Database(format!("Failed to update processing status: {}", e)))?;

        Ok(())
    }
}

impl InvoiceRepositoryImpl {
    /// Maps a database row to an Invoice struct
    /// Column order must match SELECT_COLUMNS:
    /// 0: id, 1: vendor_id, 2: vendor_name, 3: invoice_number, 4: invoice_date, 5: due_date,
    /// 6: po_number, 7: subtotal_amount, 8: subtotal_currency, 9: tax_amount, 10: tax_currency,
    /// 11: total_amount, 12: total_currency, 13: capture_status, 14: processing_status,
    /// 15: document_id, 16: ocr_confidence, 17: notes, 18: tags, 19: custom_fields,
    /// 20: created_by, 21: created_at, 22: updated_at, 23: current_queue_id, 24: assigned_to,
    /// 25: department, 26: gl_code, 27: cost_center
    fn map_invoice_row(&self, row: &rusqlite::Row, tenant_id: TenantId) -> Result<Invoice> {
        let id_str: String = row.get(0).map_err(|e| Error::Database(e.to_string()))?;
        let vendor_id_str: Option<String> = row.get(1).map_err(|e| Error::Database(e.to_string()))?;
        
        // Parse amounts
        let total_amount: i64 = row.get(11).map_err(|e| Error::Database(e.to_string()))?;
        let total_currency: String = row.get(12).map_err(|e| Error::Database(e.to_string()))?;
        
        // Parse statuses
        let capture_status_str: String = row.get(13).unwrap_or_else(|_| "pending".to_string());
        let processing_status_str: String = row.get(14).unwrap_or_else(|_| "draft".to_string());
        
        // Parse dates
        let invoice_date_str: Option<String> = row.get(4).ok();
        let due_date_str: Option<String> = row.get(5).ok();
        
        // Parse document_id
        let document_id_str: String = row.get(15).unwrap_or_else(|_| Uuid::new_v4().to_string());
        
        // Parse queue and assignment
        let current_queue_id_str: Option<String> = row.get(23).ok();
        let assigned_to_str: Option<String> = row.get(24).ok();
        
        // Parse created_by
        let created_by_str: String = row.get(20).map_err(|e| Error::Database(e.to_string()))?;
        
        // Parse tags
        let tags_str: String = row.get(18).unwrap_or_else(|_| "[]".to_string());
        let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();

        Ok(Invoice {
            id: InvoiceId(Uuid::parse_str(&id_str).unwrap()),
            tenant_id,
            vendor_id: vendor_id_str.and_then(|s| Uuid::parse_str(&s).ok()),
            vendor_name: row.get(2).map_err(|e| Error::Database(e.to_string()))?,
            invoice_number: row.get(3).map_err(|e| Error::Database(e.to_string()))?,
            invoice_date: invoice_date_str.and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok()),
            due_date: due_date_str.and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok()),
            po_number: row.get(6).ok(),
            subtotal: None, // TODO: Parse from subtotal_amount/currency
            tax_amount: None, // TODO: Parse from tax_amount/currency  
            total_amount: Money::new(total_amount, total_currency.clone()),
            currency: total_currency,
            line_items: Vec::new(), // Load separately if needed
            capture_status: CaptureStatus::from_str(&capture_status_str).unwrap_or(CaptureStatus::Pending),
            processing_status: ProcessingStatus::from_str(&processing_status_str).unwrap_or(ProcessingStatus::Draft),
            current_queue_id: current_queue_id_str.and_then(|s| Uuid::parse_str(&s).ok()),
            assigned_to: assigned_to_str.and_then(|s| Uuid::parse_str(&s).ok()).map(UserId::from_uuid),
            document_id: Uuid::parse_str(&document_id_str).unwrap_or_else(|_| Uuid::new_v4()),
            supporting_documents: Vec::new(),
            ocr_confidence: row.get(16).ok(),
            department: row.get(25).ok(),
            gl_code: row.get(26).ok(),
            cost_center: row.get(27).ok(),
            notes: row.get(17).ok(),
            tags,
            custom_fields: serde_json::Value::Object(serde_json::Map::new()),
            created_by: UserId::from_uuid(Uuid::parse_str(&created_by_str).unwrap()),
            created_at: Utc::now(), // TODO: Parse from created_at
            updated_at: Utc::now(), // TODO: Parse from updated_at
        })
    }
}
