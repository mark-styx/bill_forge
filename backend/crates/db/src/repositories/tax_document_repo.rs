//! Tax document repository implementation

use async_trait::async_trait;
use billforge_core::{
    domain::{TaxDocument, VendorId},
    traits::TaxDocumentRepository,
    types::TenantId,
    Error, Result,
};
use chrono::Datelike;
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

pub struct TaxDocumentRepositoryImpl {
    pool: Arc<PgPool>,
}

impl TaxDocumentRepositoryImpl {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TaxDocumentRepository for TaxDocumentRepositoryImpl {
    async fn create(
        &self,
        tenant_id: &TenantId,
        vendor_id: &VendorId,
        document_type: String,
        file_name: String,
        file_path: String,
        file_size: i64,
        mime_type: String,
        uploaded_by: Option<Uuid>,
    ) -> Result<Uuid> {
        let id = Uuid::new_v4();
        let now = chrono::Utc::now();

        sqlx::query(
            r#"
            INSERT INTO vendor_documents (
                id, tenant_id, vendor_id, document_type, file_name, file_path,
                file_size, mime_type, uploaded_by, uploaded_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(id)
        .bind(*tenant_id.as_uuid())
        .bind(vendor_id.0)
        .bind(&document_type)
        .bind(&file_name)
        .bind(&file_path)
        .bind(file_size)
        .bind(&mime_type)
        .bind(uploaded_by)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to create tax document: {}", e)))?;

        Ok(id)
    }

    async fn get_by_id(&self, tenant_id: &TenantId, id: Uuid) -> Result<Option<TaxDocument>> {
        let result = sqlx::query_as::<_, TaxDocumentRow>(
            "SELECT * FROM vendor_documents WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(*tenant_id.as_uuid())
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get tax document: {}", e)))?;

        Ok(result.map(|row| row.into_tax_document()))
    }

    async fn list_for_vendor(
        &self,
        tenant_id: &TenantId,
        vendor_id: &VendorId,
    ) -> Result<Vec<TaxDocument>> {
        let rows = sqlx::query_as::<_, TaxDocumentRow>(
            "SELECT * FROM vendor_documents WHERE tenant_id = $1 AND vendor_id = $2 ORDER BY uploaded_at DESC"
        )
        .bind(*tenant_id.as_uuid())
        .bind(vendor_id.0)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to list tax documents: {}", e)))?;

        Ok(rows
            .into_iter()
            .map(|row| row.into_tax_document())
            .collect())
    }

    async fn delete(&self, tenant_id: &TenantId, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM vendor_documents WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(*tenant_id.as_uuid())
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to delete tax document: {}", e)))?;

        Ok(())
    }
}

/// Helper struct for mapping database rows
#[derive(sqlx::FromRow)]
struct TaxDocumentRow {
    id: Uuid,
    tenant_id: Uuid,
    vendor_id: Uuid,
    document_type: String,
    file_name: String,
    file_path: String,
    file_size: i64,
    mime_type: String,
    uploaded_by: Option<Uuid>,
    uploaded_at: chrono::DateTime<chrono::Utc>,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    metadata: Option<serde_json::Value>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl TaxDocumentRow {
    fn into_tax_document(self) -> TaxDocument {
        use billforge_core::domain::{TaxDocumentType, VendorId};

        let tax_year = self.uploaded_at.year();

        TaxDocument {
            id: self.id,
            vendor_id: VendorId(self.vendor_id),
            tenant_id: TenantId(self.tenant_id),
            document_type: match self.document_type.as_str() {
                "w9" => TaxDocumentType::W9,
                "w8_ben" => TaxDocumentType::W8Ben,
                "w8_ben_e" => TaxDocumentType::W8BenE,
                "form_1099" => TaxDocumentType::Form1099,
                _ => TaxDocumentType::Other,
            },
            tax_year,
            file_id: self.id,
            file_name: self.file_name,
            received_date: self.uploaded_at.naive_utc().date(),
            expires_date: self.expires_at.map(|dt| dt.naive_utc().date()),
            notes: None,
            created_at: self.created_at,
        }
    }
}
