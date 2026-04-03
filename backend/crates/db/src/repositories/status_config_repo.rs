//! Invoice status configuration repository implementation

use async_trait::async_trait;
use billforge_core::{domain::*, traits::InvoiceStatusConfigRepository, types::*, Error, Result};
use chrono::Utc;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

pub struct InvoiceStatusConfigRepositoryImpl {
    pool: Arc<PgPool>,
}

impl InvoiceStatusConfigRepositoryImpl {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct StatusConfigRow {
    id: Uuid,
    tenant_id: Uuid,
    status_key: String,
    display_label: String,
    color: String,
    bg_color: String,
    text_color: String,
    sort_order: i32,
    is_terminal: bool,
    is_active: bool,
    category: String,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
}

impl StatusConfigRow {
    fn into_domain(self, tenant_id: &TenantId) -> InvoiceStatusConfig {
        InvoiceStatusConfig {
            id: self.id,
            tenant_id: tenant_id.clone(),
            status_key: self.status_key,
            display_label: self.display_label,
            color: self.color,
            bg_color: self.bg_color,
            text_color: self.text_color,
            sort_order: self.sort_order,
            is_terminal: self.is_terminal,
            is_active: self.is_active,
            category: self.category,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[async_trait]
impl InvoiceStatusConfigRepository for InvoiceStatusConfigRepositoryImpl {
    async fn list(
        &self,
        tenant_id: &TenantId,
        category: Option<&str>,
    ) -> Result<Vec<InvoiceStatusConfig>> {
        let rows = if let Some(cat) = category {
            sqlx::query_as::<_, StatusConfigRow>(
                "SELECT id, tenant_id, status_key, display_label, color, bg_color, text_color,
                        sort_order, is_terminal, is_active, category, created_at, updated_at
                 FROM invoice_status_config
                 WHERE tenant_id = $1 AND category = $2
                 ORDER BY sort_order ASC",
            )
            .bind(*tenant_id.as_uuid())
            .bind(cat)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| Error::Database(e.to_string()))?
        } else {
            sqlx::query_as::<_, StatusConfigRow>(
                "SELECT id, tenant_id, status_key, display_label, color, bg_color, text_color,
                        sort_order, is_terminal, is_active, category, created_at, updated_at
                 FROM invoice_status_config
                 WHERE tenant_id = $1
                 ORDER BY category ASC, sort_order ASC",
            )
            .bind(*tenant_id.as_uuid())
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| Error::Database(e.to_string()))?
        };

        Ok(rows.into_iter().map(|r| r.into_domain(tenant_id)).collect())
    }

    async fn get_by_key(
        &self,
        tenant_id: &TenantId,
        status_key: &str,
    ) -> Result<Option<InvoiceStatusConfig>> {
        let row = sqlx::query_as::<_, StatusConfigRow>(
            "SELECT id, tenant_id, status_key, display_label, color, bg_color, text_color,
                    sort_order, is_terminal, is_active, category, created_at, updated_at
             FROM invoice_status_config
             WHERE tenant_id = $1 AND status_key = $2",
        )
        .bind(*tenant_id.as_uuid())
        .bind(status_key)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

        Ok(row.map(|r| r.into_domain(tenant_id)))
    }

    async fn upsert(
        &self,
        tenant_id: &TenantId,
        input: InvoiceStatusConfigInput,
    ) -> Result<InvoiceStatusConfig> {
        let now = Utc::now();
        let row = sqlx::query_as::<_, StatusConfigRow>(
            "INSERT INTO invoice_status_config
                (id, tenant_id, status_key, display_label, color, bg_color, text_color,
                 sort_order, is_terminal, is_active, category, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
             ON CONFLICT (tenant_id, status_key) DO UPDATE SET
                display_label = EXCLUDED.display_label,
                color = EXCLUDED.color,
                bg_color = EXCLUDED.bg_color,
                text_color = EXCLUDED.text_color,
                sort_order = EXCLUDED.sort_order,
                is_terminal = EXCLUDED.is_terminal,
                is_active = EXCLUDED.is_active,
                category = EXCLUDED.category,
                updated_at = EXCLUDED.updated_at
             RETURNING id, tenant_id, status_key, display_label, color, bg_color, text_color,
                       sort_order, is_terminal, is_active, category, created_at, updated_at",
        )
        .bind(Uuid::new_v4())
        .bind(*tenant_id.as_uuid())
        .bind(&input.status_key)
        .bind(&input.display_label)
        .bind(&input.color)
        .bind(&input.bg_color)
        .bind(&input.text_color)
        .bind(input.sort_order)
        .bind(input.is_terminal)
        .bind(input.is_active)
        .bind(&input.category)
        .bind(now)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

        Ok(row.into_domain(tenant_id))
    }

    async fn upsert_batch(
        &self,
        tenant_id: &TenantId,
        inputs: Vec<InvoiceStatusConfigInput>,
    ) -> Result<Vec<InvoiceStatusConfig>> {
        let mut results = Vec::with_capacity(inputs.len());
        for input in inputs {
            let config = self.upsert(tenant_id, input).await?;
            results.push(config);
        }
        Ok(results)
    }

    async fn delete(&self, tenant_id: &TenantId, status_key: &str) -> Result<()> {
        sqlx::query("DELETE FROM invoice_status_config WHERE tenant_id = $1 AND status_key = $2")
            .bind(*tenant_id.as_uuid())
            .bind(status_key)
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;

        Ok(())
    }

    async fn seed_defaults(&self, tenant_id: &TenantId) -> Result<()> {
        let mut all_defaults = default_processing_statuses();
        all_defaults.extend(default_capture_statuses());
        self.upsert_batch(tenant_id, all_defaults).await?;
        Ok(())
    }
}
