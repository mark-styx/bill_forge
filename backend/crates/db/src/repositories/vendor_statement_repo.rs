//! Vendor statement repository implementation

use billforge_core::{
    domain::{self, vendor_statement::MatchResult, *},
    types::TenantId,
    Error, Result,
};
use chrono::{NaiveDate, Utc};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

pub struct VendorStatementRepositoryImpl {
    pool: Arc<PgPool>,
}

impl VendorStatementRepositoryImpl {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    /// Create a new vendor statement with line items, returning the statement.
    pub async fn create_statement(
        &self,
        tenant_id: &TenantId,
        input: CreateStatementInput,
        created_by: Uuid,
    ) -> Result<VendorStatement> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let currency = input.currency.unwrap_or_else(|| "USD".to_string());
        let status = StatementStatus::Pending.as_str();

        sqlx::query(
            r#"INSERT INTO vendor_statements
                (id, tenant_id, vendor_id, statement_number, statement_date,
                 statement_period_start, statement_period_end,
                 opening_balance_cents, closing_balance_cents, currency,
                 status, created_by, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)"#,
        )
        .bind(id)
        .bind(tenant_id.as_uuid())
        .bind(input.vendor_id)
        .bind(&input.statement_number)
        .bind(input.statement_date)
        .bind(input.period_start)
        .bind(input.period_end)
        .bind(input.opening_balance_cents)
        .bind(input.closing_balance_cents)
        .bind(&currency)
        .bind(status)
        .bind(created_by)
        .bind(now)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to create vendor statement: {}", e)))?;

        // Insert line items
        for line in &input.lines {
            let line_id = Uuid::new_v4();
            let line_type = line.line_type.unwrap_or(LineType::Invoice).as_str();
            sqlx::query(
                r#"INSERT INTO vendor_statement_lines
                    (id, statement_id, tenant_id, line_date, description,
                     reference_number, amount_cents, line_type, match_status,
                     matched_by, created_at, updated_at)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'unmatched', 'auto', $9, $10)"#,
            )
            .bind(line_id)
            .bind(id)
            .bind(tenant_id.as_uuid())
            .bind(line.line_date)
            .bind(&line.description)
            .bind(&line.reference_number)
            .bind(line.amount_cents)
            .bind(line_type)
            .bind(now)
            .bind(now)
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to create statement line: {}", e)))?;
        }

        Ok(VendorStatement {
            id: VendorStatementId(id),
            tenant_id: tenant_id.clone(),
            vendor_id: input.vendor_id,
            statement_number: input.statement_number,
            statement_date: input.statement_date,
            period_start: input.period_start,
            period_end: input.period_end,
            opening_balance_cents: input.opening_balance_cents,
            closing_balance_cents: input.closing_balance_cents,
            currency,
            status: StatementStatus::Pending,
            reconciled_by: None,
            reconciled_at: None,
            notes: input.notes,
            created_by,
            created_at: now,
            updated_at: now,
        })
    }

    /// Get a single statement by ID with tenant isolation.
    pub async fn get_statement(
        &self,
        tenant_id: &TenantId,
        id: Uuid,
    ) -> Result<Option<VendorStatement>> {
        let row = sqlx::query_as::<_, StatementRow>(
            r#"SELECT
                id, tenant_id, vendor_id, statement_number, statement_date,
                statement_period_start as period_start,
                statement_period_end as period_end,
                opening_balance_cents, closing_balance_cents, currency,
                status, reconciled_by, reconciled_at, notes, created_by,
                created_at, updated_at
               FROM vendor_statements
               WHERE id = $1 AND tenant_id = $2"#,
        )
        .bind(id)
        .bind(tenant_id.as_uuid())
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get vendor statement: {}", e)))?;

        Ok(row.map(|r| r.into_domain()))
    }

    /// List statements for a vendor with pagination.
    pub async fn list_statements(
        &self,
        tenant_id: &TenantId,
        vendor_id: Uuid,
        page: u32,
        per_page: u32,
        status_filter: Option<&str>,
    ) -> Result<(Vec<VendorStatement>, u64)> {
        let offset = (page.saturating_sub(1)) * per_page;
        let limit = per_page;

        let (rows, count): (Vec<StatementRow>, (i64,)) = if let Some(status) = status_filter {
            let rows = sqlx::query_as::<_, StatementRow>(
                r#"SELECT
                    id, tenant_id, vendor_id, statement_number, statement_date,
                    statement_period_start as period_start,
                    statement_period_end as period_end,
                    opening_balance_cents, closing_balance_cents, currency,
                    status, reconciled_by, reconciled_at, notes, created_by,
                    created_at, updated_at
                   FROM vendor_statements
                   WHERE tenant_id = $1 AND vendor_id = $2 AND status = $3
                   ORDER BY created_at DESC
                   LIMIT $4 OFFSET $5"#,
            )
            .bind(tenant_id.as_uuid())
            .bind(vendor_id)
            .bind(status)
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to list vendor statements: {}", e)))?;

            let count = sqlx::query_as::<_, (i64,)>(
                "SELECT COUNT(*) FROM vendor_statements WHERE tenant_id = $1 AND vendor_id = $2 AND status = $3",
            )
            .bind(tenant_id.as_uuid())
            .bind(vendor_id)
            .bind(status)
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to count vendor statements: {}", e)))?;

            (rows, count)
        } else {
            let rows = sqlx::query_as::<_, StatementRow>(
                r#"SELECT
                    id, tenant_id, vendor_id, statement_number, statement_date,
                    statement_period_start as period_start,
                    statement_period_end as period_end,
                    opening_balance_cents, closing_balance_cents, currency,
                    status, reconciled_by, reconciled_at, notes, created_by,
                    created_at, updated_at
                   FROM vendor_statements
                   WHERE tenant_id = $1 AND vendor_id = $2
                   ORDER BY created_at DESC
                   LIMIT $3 OFFSET $4"#,
            )
            .bind(tenant_id.as_uuid())
            .bind(vendor_id)
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to list vendor statements: {}", e)))?;

            let count = sqlx::query_as::<_, (i64,)>(
                "SELECT COUNT(*) FROM vendor_statements WHERE tenant_id = $1 AND vendor_id = $2",
            )
            .bind(tenant_id.as_uuid())
            .bind(vendor_id)
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to count vendor statements: {}", e)))?;

            (rows, count)
        };

        Ok((rows.into_iter().map(|r| r.into_domain()).collect(), count.0 as u64))
    }

    /// Get all line items for a statement.
    pub async fn get_lines(&self, statement_id: Uuid) -> Result<Vec<StatementLineItem>> {
        let rows = sqlx::query_as::<_, LineRow>(
            r#"SELECT
                id, statement_id, tenant_id, line_date, description,
                reference_number, amount_cents, line_type, match_status,
                matched_invoice_id, variance_cents, matched_at, matched_by,
                notes, created_at, updated_at
               FROM vendor_statement_lines
               WHERE statement_id = $1
               ORDER BY line_date, created_at"#,
        )
        .bind(statement_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get statement lines: {}", e)))?;

        Ok(rows.into_iter().map(|r| r.into_domain()).collect())
    }

    /// Update a line's match status and linked invoice.
    pub async fn update_line_match(
        &self,
        line_id: Uuid,
        matched_invoice_id: Option<Uuid>,
        variance_cents: i64,
        match_status: &LineMatchStatus,
        matched_by: &str,
    ) -> Result<()> {
        let now = Utc::now();
        sqlx::query(
            r#"UPDATE vendor_statement_lines
               SET matched_invoice_id = $1,
                   variance_cents = $2,
                   match_status = $3,
                   matched_by = $4,
                   matched_at = $5,
                   updated_at = $6
               WHERE id = $7"#,
        )
        .bind(matched_invoice_id)
        .bind(variance_cents)
        .bind(match_status.as_str())
        .bind(matched_by)
        .bind(now)
        .bind(now)
        .bind(line_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to update line match: {}", e)))?;

        Ok(())
    }

    /// Update statement status (e.g. to reconciled).
    pub async fn update_statement_status(
        &self,
        id: Uuid,
        status: &StatementStatus,
        reconciled_by: Option<Uuid>,
    ) -> Result<()> {
        let now = Utc::now();
        let reconciled_at = if *status == StatementStatus::Reconciled { Some(now) } else { None };

        sqlx::query(
            r#"UPDATE vendor_statements
               SET status = $1, reconciled_by = $2, reconciled_at = $3, updated_at = $4
               WHERE id = $5"#,
        )
        .bind(status.as_str())
        .bind(reconciled_by)
        .bind(reconciled_at)
        .bind(now)
        .bind(id)
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to update statement status: {}", e)))?;

        Ok(())
    }

    /// Get vendor invoices within a date range (for feeding the matcher).
    pub async fn get_vendor_invoices_in_range(
        &self,
        tenant_id: &TenantId,
        vendor_id: Uuid,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<InvoiceSummary>> {
        let rows = sqlx::query_as::<_, (Uuid, String, i64, Option<NaiveDate>, Option<Uuid>)>(
            r#"SELECT id, invoice_number, total_amount_cents, invoice_date, vendor_id
               FROM invoices
               WHERE tenant_id = $1
                 AND vendor_id = $2
                 AND invoice_date >= $3
                 AND invoice_date <= $4"#,
        )
        .bind(tenant_id.as_uuid())
        .bind(vendor_id)
        .bind(start)
        .bind(end)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get vendor invoices: {}", e)))?;

        Ok(rows
            .into_iter()
            .map(|(id, invoice_number, total_amount_cents, invoice_date, vendor_id)| {
                InvoiceSummary {
                    id,
                    invoice_number,
                    total_amount_cents,
                    invoice_date,
                    vendor_id,
                }
            })
            .collect())
    }

    /// Apply match results to the database.
    pub async fn apply_match_results(&self, results: &[MatchResult]) -> Result<()> {
        for result in results {
            if result.confidence != MatchConfidence::NoMatch {
                self.update_line_match(
                    result.line_id,
                    result.matched_invoice_id,
                    result.variance_cents,
                    &result.match_status,
                    "auto",
                )
                .await?;
            }
        }
        Ok(())
    }
}

// Helper row types for sqlx mapping

#[derive(sqlx::FromRow)]
struct StatementRow {
    id: Uuid,
    tenant_id: Uuid,
    vendor_id: Uuid,
    statement_number: Option<String>,
    statement_date: Option<NaiveDate>,
    period_start: NaiveDate,
    period_end: NaiveDate,
    opening_balance_cents: i64,
    closing_balance_cents: i64,
    currency: String,
    status: String,
    reconciled_by: Option<Uuid>,
    reconciled_at: Option<chrono::DateTime<Utc>>,
    notes: Option<String>,
    created_by: Uuid,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
}

impl StatementRow {
    fn into_domain(self) -> VendorStatement {
        VendorStatement {
            id: VendorStatementId(self.id),
            tenant_id: TenantId::from_uuid(self.tenant_id),
            vendor_id: self.vendor_id,
            statement_number: self.statement_number,
            statement_date: self.statement_date,
            period_start: self.period_start,
            period_end: self.period_end,
            opening_balance_cents: self.opening_balance_cents,
            closing_balance_cents: self.closing_balance_cents,
            currency: self.currency,
            status: StatementStatus::from_str(&self.status).unwrap_or_default(),
            reconciled_by: self.reconciled_by,
            reconciled_at: self.reconciled_at,
            notes: self.notes,
            created_by: self.created_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct LineRow {
    id: Uuid,
    statement_id: Uuid,
    tenant_id: Option<Uuid>,
    line_date: NaiveDate,
    description: String,
    reference_number: Option<String>,
    amount_cents: i64,
    line_type: String,
    match_status: String,
    matched_invoice_id: Option<Uuid>,
    variance_cents: Option<i64>,
    matched_at: Option<chrono::DateTime<Utc>>,
    matched_by: Option<String>,
    notes: Option<String>,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
}

impl LineRow {
    fn into_domain(self) -> StatementLineItem {
        StatementLineItem {
            id: self.id,
            statement_id: self.statement_id,
            tenant_id: self.tenant_id.map(TenantId::from_uuid),
            line_date: self.line_date,
            description: self.description,
            reference_number: self.reference_number,
            amount_cents: self.amount_cents,
            line_type: LineType::from_str(&self.line_type).unwrap_or_default(),
            match_status: LineMatchStatus::from_str(&self.match_status).unwrap_or_default(),
            matched_invoice_id: self.matched_invoice_id,
            variance_cents: self.variance_cents.unwrap_or(0),
            matched_at: self.matched_at,
            matched_by: self.matched_by,
            notes: self.notes,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}
