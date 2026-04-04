//! Purchase order repository implementation

use async_trait::async_trait;
use billforge_core::{
    domain::*,
    traits::{POFilters, PurchaseOrderRepository},
    types::*,
    Error, Result,
};
use chrono::Utc;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

pub struct PurchaseOrderRepositoryImpl {
    pool: Arc<PgPool>,
}

impl PurchaseOrderRepositoryImpl {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PurchaseOrderRepository for PurchaseOrderRepositoryImpl {
    async fn create(
        &self,
        tenant_id: &TenantId,
        input: CreatePurchaseOrderInput,
        created_by: &UserId,
    ) -> Result<PurchaseOrder> {
        let id = PurchaseOrderId::new();
        let now = Utc::now();

        sqlx::query(
            r#"INSERT INTO purchase_orders (
                id, tenant_id, po_number, vendor_id, vendor_name, order_date,
                expected_delivery, status, total_amount_cents, total_currency,
                ship_to_address, notes, created_by, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $14)"#,
        )
        .bind(id.0)
        .bind(*tenant_id.as_uuid())
        .bind(&input.po_number)
        .bind(input.vendor_id)
        .bind(&input.vendor_name)
        .bind(input.order_date)
        .bind(input.expected_delivery)
        .bind("open")
        .bind(input.total_amount.amount)
        .bind(&input.total_amount.currency)
        .bind(&input.ship_to_address)
        .bind(&input.notes)
        .bind(created_by.0)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            if e.to_string().contains("duplicate key") {
                Error::AlreadyExists {
                    resource_type: "PurchaseOrder".to_string(),
                }
            } else {
                Error::Database(format!("Failed to create purchase order: {}", e))
            }
        })?;

        // Insert line items
        let mut line_items = Vec::new();
        for (i, li) in input.line_items.iter().enumerate() {
            let li_id = Uuid::new_v4();
            // Use the external line number if provided (from EDI PO1-01),
            // otherwise default to sequential numbering.
            let line_number = li.line_number.unwrap_or((i + 1) as u32);

            sqlx::query(
                r#"INSERT INTO po_line_items (
                    id, po_id, line_number, description, quantity, unit_of_measure,
                    unit_price_cents, unit_price_currency, total_cents, total_currency,
                    product_id, received_quantity, invoiced_quantity
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 0, 0)"#,
            )
            .bind(li_id)
            .bind(id.0)
            .bind(line_number as i32)
            .bind(&li.description)
            .bind(li.quantity)
            .bind(&li.unit_of_measure)
            .bind(li.unit_price.amount)
            .bind(&li.unit_price.currency)
            .bind(li.total.amount)
            .bind(&li.total.currency)
            .bind(&li.product_id)
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to create PO line item: {}", e)))?;

            line_items.push(POLineItem {
                id: li_id,
                line_number,
                description: li.description.clone(),
                quantity: li.quantity,
                unit_of_measure: li.unit_of_measure.clone(),
                unit_price: li.unit_price.clone(),
                total: li.total.clone(),
                product_id: li.product_id.clone(),
                received_quantity: 0.0,
                invoiced_quantity: 0.0,
            });
        }

        Ok(PurchaseOrder {
            id,
            tenant_id: tenant_id.clone(),
            po_number: input.po_number,
            vendor_id: input.vendor_id,
            vendor_name: input.vendor_name,
            order_date: input.order_date,
            expected_delivery: input.expected_delivery,
            status: POStatus::Open,
            line_items,
            total_amount: input.total_amount,
            ship_to_address: input.ship_to_address,
            notes: input.notes,
            created_by: created_by.clone(),
            created_at: now,
            updated_at: now,
        })
    }

    async fn get_by_id(
        &self,
        tenant_id: &TenantId,
        id: &PurchaseOrderId,
    ) -> Result<Option<PurchaseOrder>> {
        let row = sqlx::query_as::<_, PurchaseOrderRow>(
            r#"SELECT id, tenant_id, po_number, vendor_id, vendor_name, order_date,
                      expected_delivery, status, total_amount_cents, total_currency,
                      ship_to_address, notes, created_by, created_at, updated_at
               FROM purchase_orders WHERE id = $1 AND tenant_id = $2"#,
        )
        .bind(id.0)
        .bind(*tenant_id.as_uuid())
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get purchase order: {}", e)))?;

        match row {
            Some(r) => {
                let line_items = self.fetch_line_items(id.0).await?;
                Ok(Some(r.into_purchase_order(line_items)))
            }
            None => Ok(None),
        }
    }

    async fn find_by_po_number(
        &self,
        tenant_id: &TenantId,
        po_number: &str,
    ) -> Result<Option<PurchaseOrder>> {
        let row = sqlx::query_as::<_, PurchaseOrderRow>(
            r#"SELECT id, tenant_id, po_number, vendor_id, vendor_name, order_date,
                      expected_delivery, status, total_amount_cents, total_currency,
                      ship_to_address, notes, created_by, created_at, updated_at
               FROM purchase_orders WHERE tenant_id = $1 AND po_number = $2"#,
        )
        .bind(*tenant_id.as_uuid())
        .bind(po_number)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to find purchase order: {}", e)))?;

        match row {
            Some(r) => {
                let line_items = self.fetch_line_items(r.id).await?;
                Ok(Some(r.into_purchase_order(line_items)))
            }
            None => Ok(None),
        }
    }

    async fn list(
        &self,
        tenant_id: &TenantId,
        filters: &POFilters,
        pagination: &Pagination,
    ) -> Result<PaginatedResponse<PurchaseOrder>> {
        let mut where_clauses = vec!["tenant_id = $1".to_string()];
        let mut bind_idx = 2u32;

        if filters.vendor_id.is_some() {
            where_clauses.push(format!("vendor_id = ${}", bind_idx));
            bind_idx += 1;
        }
        if filters.status.is_some() {
            where_clauses.push(format!("status = ${}", bind_idx));
            bind_idx += 1;
        }
        if filters.search.is_some() {
            where_clauses.push(format!(
                "(po_number ILIKE ${0} OR vendor_name ILIKE ${0})",
                bind_idx
            ));
            bind_idx += 1;
        }

        let where_clause = where_clauses.join(" AND ");
        let count_sql = format!("SELECT COUNT(*) FROM purchase_orders WHERE {}", where_clause);
        let list_sql = format!(
            r#"SELECT id, tenant_id, po_number, vendor_id, vendor_name, order_date,
                      expected_delivery, status, total_amount_cents, total_currency,
                      ship_to_address, notes, created_by, created_at, updated_at
               FROM purchase_orders WHERE {}
               ORDER BY created_at DESC LIMIT ${} OFFSET ${}"#,
            where_clause, bind_idx, bind_idx + 1
        );

        // Build count query
        let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql)
            .bind(*tenant_id.as_uuid());
        if let Some(vendor_id) = &filters.vendor_id {
            count_query = count_query.bind(vendor_id);
        }
        if let Some(status) = &filters.status {
            count_query = count_query.bind(status.as_str());
        }
        if let Some(search) = &filters.search {
            count_query = count_query.bind(format!("%{}%", search));
        }

        let total_items = count_query
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to count purchase orders: {}", e)))? as u64;

        // Build list query
        let mut list_query = sqlx::query_as::<_, PurchaseOrderRow>(&list_sql)
            .bind(*tenant_id.as_uuid());
        if let Some(vendor_id) = &filters.vendor_id {
            list_query = list_query.bind(vendor_id);
        }
        if let Some(status) = &filters.status {
            list_query = list_query.bind(status.as_str());
        }
        if let Some(search) = &filters.search {
            list_query = list_query.bind(format!("%{}%", search));
        }
        list_query = list_query
            .bind(pagination.per_page as i32)
            .bind(pagination.offset() as i32);

        let rows = list_query
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to list purchase orders: {}", e)))?;

        let mut data = Vec::new();
        for r in rows {
            let line_items = self.fetch_line_items(r.id).await?;
            data.push(r.into_purchase_order(line_items));
        }

        let total_pages = ((total_items as f64) / (pagination.per_page as f64)).ceil() as u32;

        Ok(PaginatedResponse {
            data,
            pagination: PaginationMeta {
                page: pagination.page,
                per_page: pagination.per_page,
                total_items,
                total_pages,
            },
        })
    }

    async fn update_status(
        &self,
        tenant_id: &TenantId,
        id: &PurchaseOrderId,
        status: POStatus,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE purchase_orders SET status = $1, updated_at = NOW() WHERE id = $2 AND tenant_id = $3",
        )
        .bind(status.as_str())
        .bind(id.0)
        .bind(*tenant_id.as_uuid())
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to update PO status: {}", e)))?;

        Ok(())
    }

    async fn update_received_quantities(
        &self,
        _tenant_id: &TenantId,
        po_id: &PurchaseOrderId,
        line_number: u32,
        received_qty: f64,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE po_line_items SET received_quantity = received_quantity + $1 WHERE po_id = $2 AND line_number = $3",
        )
        .bind(received_qty)
        .bind(po_id.0)
        .bind(line_number as i32)
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to update received quantities: {}", e)))?;

        Ok(())
    }

    async fn update_invoiced_quantities(
        &self,
        _tenant_id: &TenantId,
        po_id: &PurchaseOrderId,
        line_number: u32,
        invoiced_qty: f64,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE po_line_items SET invoiced_quantity = invoiced_quantity + $1 WHERE po_id = $2 AND line_number = $3",
        )
        .bind(invoiced_qty)
        .bind(po_id.0)
        .bind(line_number as i32)
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to update invoiced quantities: {}", e)))?;

        Ok(())
    }

    async fn delete(
        &self,
        tenant_id: &TenantId,
        id: &PurchaseOrderId,
    ) -> Result<()> {
        sqlx::query("DELETE FROM purchase_orders WHERE id = $1 AND tenant_id = $2")
            .bind(id.0)
            .bind(*tenant_id.as_uuid())
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to delete purchase order: {}", e)))?;

        Ok(())
    }
}

impl PurchaseOrderRepositoryImpl {
    async fn fetch_line_items(&self, po_id: Uuid) -> Result<Vec<POLineItem>> {
        let rows = sqlx::query_as::<_, POLineItemRow>(
            r#"SELECT id, line_number, description, quantity, unit_of_measure,
                      unit_price_cents, unit_price_currency, total_cents, total_currency,
                      product_id, received_quantity, invoiced_quantity
               FROM po_line_items WHERE po_id = $1 ORDER BY line_number"#,
        )
        .bind(po_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to fetch PO line items: {}", e)))?;

        Ok(rows.into_iter().map(|r| r.into_line_item()).collect())
    }
}

// ──────────────────────────── Row types ────────────────────────────

#[derive(Debug, sqlx::FromRow)]
struct PurchaseOrderRow {
    id: Uuid,
    tenant_id: Uuid,
    po_number: String,
    vendor_id: Uuid,
    vendor_name: String,
    order_date: chrono::NaiveDate,
    expected_delivery: Option<chrono::NaiveDate>,
    status: String,
    total_amount_cents: i64,
    total_currency: String,
    ship_to_address: Option<String>,
    notes: Option<String>,
    created_by: Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl PurchaseOrderRow {
    fn into_purchase_order(self, line_items: Vec<POLineItem>) -> PurchaseOrder {
        PurchaseOrder {
            id: PurchaseOrderId(self.id),
            tenant_id: TenantId::from_uuid(self.tenant_id),
            po_number: self.po_number,
            vendor_id: self.vendor_id,
            vendor_name: self.vendor_name,
            order_date: self.order_date,
            expected_delivery: self.expected_delivery,
            status: POStatus::from_str(&self.status).unwrap_or_default(),
            line_items,
            total_amount: Money::new(self.total_amount_cents, self.total_currency),
            ship_to_address: self.ship_to_address,
            notes: self.notes,
            created_by: UserId::from_uuid(self.created_by),
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct POLineItemRow {
    id: Uuid,
    line_number: i32,
    description: String,
    quantity: f32,
    unit_of_measure: String,
    unit_price_cents: i64,
    unit_price_currency: String,
    total_cents: i64,
    total_currency: String,
    product_id: Option<String>,
    received_quantity: f32,
    invoiced_quantity: f32,
}

impl POLineItemRow {
    fn into_line_item(self) -> POLineItem {
        POLineItem {
            id: self.id,
            line_number: self.line_number as u32,
            description: self.description,
            quantity: self.quantity as f64,
            unit_of_measure: self.unit_of_measure,
            unit_price: Money::new(self.unit_price_cents, self.unit_price_currency),
            total: Money::new(self.total_cents, self.total_currency),
            product_id: self.product_id,
            received_quantity: self.received_quantity as f64,
            invoiced_quantity: self.invoiced_quantity as f64,
        }
    }
}
