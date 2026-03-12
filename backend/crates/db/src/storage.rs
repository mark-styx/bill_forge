//! Document storage services with local and S3 support
//!
//! Provides a unified storage interface that supports:
//! - Local filesystem storage for development
//! - AWS S3 storage for production deployments
//!
//! Each tenant's files are isolated by storage key prefix: {tenant_id}/{document_id}

use billforge_core::{
    domain::{DocumentRef, DocumentType},
    traits::StorageService,
    types::TenantId,
    Error, Result,
};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;
use chrono::Utc;
use sqlx::PgPool;

// =============================================================================
// Storage Configuration
// =============================================================================

/// Configuration for storage backends
#[derive(Debug, Clone)]
pub enum StorageConfig {
    /// Local filesystem storage (for development)
    Local {
        base_path: PathBuf,
    },
    /// AWS S3 storage (for production)
    #[cfg(feature = "s3")]
    S3 {
        bucket: String,
        region: String,
        /// Optional endpoint for S3-compatible services (MinIO, LocalStack)
        endpoint: Option<String>,
        /// Prefix for all keys in this bucket
        key_prefix: Option<String>,
    },
}

impl StorageConfig {
    /// Create local storage config
    pub fn local(base_path: impl Into<PathBuf>) -> Self {
        Self::Local {
            base_path: base_path.into(),
        }
    }

    /// Create S3 storage config
    #[cfg(feature = "s3")]
    pub fn s3(bucket: String, region: String) -> Self {
        Self::S3 {
            bucket,
            region,
            endpoint: None,
            key_prefix: None,
        }
    }

    /// Create S3-compatible storage config (for MinIO, LocalStack, etc.)
    #[cfg(feature = "s3")]
    pub fn s3_compatible(bucket: String, region: String, endpoint: String) -> Self {
        Self::S3 {
            bucket,
            region,
            endpoint: Some(endpoint),
            key_prefix: None,
        }
    }

    /// Load storage config from environment variables
    pub fn from_env() -> Self {
        let provider = std::env::var("STORAGE_PROVIDER").unwrap_or_else(|_| "local".to_string());

        match provider.to_lowercase().as_str() {
            #[cfg(feature = "s3")]
            "s3" => {
                let bucket = std::env::var("S3_BUCKET").expect("S3_BUCKET required when STORAGE_PROVIDER=s3");
                let region = std::env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".to_string());
                let endpoint = std::env::var("S3_ENDPOINT").ok();
                let key_prefix = std::env::var("S3_KEY_PREFIX").ok();

                Self::S3 {
                    bucket,
                    region,
                    endpoint,
                    key_prefix,
                }
            }
            _ => {
                let base_path = std::env::var("LOCAL_STORAGE_PATH")
                    .unwrap_or_else(|_| "./data".to_string());
                Self::Local {
                    base_path: PathBuf::from(base_path),
                }
            }
        }
    }
}

/// Create a storage service from configuration
pub async fn create_storage_service(config: StorageConfig) -> Result<Box<dyn StorageService>> {
    match config {
        StorageConfig::Local { base_path } => {
            let storage = LocalStorageService::new(&base_path).await?;
            Ok(Box::new(storage))
        }
        #[cfg(feature = "s3")]
        StorageConfig::S3 { bucket, region, endpoint, key_prefix } => {
            let storage = S3StorageService::new(bucket, region, endpoint, key_prefix).await?;
            Ok(Box::new(storage))
        }
    }
}

// =============================================================================
// Local Storage Service (Development)
// =============================================================================

/// Local filesystem storage service for development
pub struct LocalStorageService {
    base_path: PathBuf,
}

impl LocalStorageService {
    /// Create a new local storage service
    pub async fn new(base_path: impl AsRef<Path>) -> Result<Self> {
        let base_path = base_path.as_ref().to_path_buf();

        // Create base directory and documents subdirectory
        fs::create_dir_all(&base_path)
            .await
            .map_err(|e| Error::Storage(format!("Failed to create storage directory: {}", e)))?;

        let docs_path = base_path.join("documents");
        fs::create_dir_all(&docs_path)
            .await
            .map_err(|e| Error::Storage(format!("Failed to create documents directory: {}", e)))?;

        Ok(Self { base_path })
    }

    /// Get the path for a storage key
    fn get_path(&self, storage_key: &str) -> PathBuf {
        self.base_path.join("documents").join(storage_key)
    }
}

#[async_trait]
impl StorageService for LocalStorageService {
    async fn upload(
        &self,
        tenant_id: &TenantId,
        filename: &str,
        data: &[u8],
        mime_type: &str,
    ) -> Result<Uuid> {
        let document_id = Uuid::new_v4();
        let storage_key = format!("{}/{}", tenant_id.as_str(), document_id);
        let path = self.get_path(&storage_key);

        // Create parent directories
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| Error::Storage(format!("Failed to create directory: {}", e)))?;
        }

        // Write file
        let mut file = fs::File::create(&path)
            .await
            .map_err(|e| Error::Storage(format!("Failed to create file: {}", e)))?;

        file.write_all(data)
            .await
            .map_err(|e| Error::Storage(format!("Failed to write file: {}", e)))?;

        tracing::debug!("Uploaded document {} to {}", document_id, path.display());

        Ok(document_id)
    }

    async fn download(&self, tenant_id: &TenantId, file_id: Uuid) -> Result<Vec<u8>> {
        let storage_key = format!("{}/{}", tenant_id.as_str(), file_id);
        let path = self.get_path(&storage_key);
        fs::read(&path)
            .await
            .map_err(|e| Error::Storage(format!("Failed to read file: {}", e)))
    }

    async fn delete(&self, tenant_id: &TenantId, file_id: Uuid) -> Result<()> {
        let storage_key = format!("{}/{}", tenant_id.as_str(), file_id);
        let path = self.get_path(&storage_key);
        fs::remove_file(&path)
            .await
            .map_err(|e| Error::Storage(format!("Failed to delete file: {}", e)))?;

        tracing::debug!("Deleted document at {}", path.display());
        Ok(())
    }

    async fn get_url(&self, _tenant_id: &TenantId, _file_id: Uuid, _expires_in_secs: u64) -> Result<String> {
        // Local storage doesn't support presigned URLs
        Err(Error::Storage("Presigned URLs not supported for local storage".to_string()))
    }

    async fn health_check(&self) -> Result<()> {
        let base_path = &self.base_path;

        if !base_path.exists() {
            return Err(Error::Storage("Storage base path does not exist".to_string()));
        }

        if !base_path.is_dir() {
            return Err(Error::Storage("Storage base path is not a directory".to_string()));
        }

        let docs_path = base_path.join("documents");
        if docs_path.exists() && !docs_path.is_dir() {
            return Err(Error::Storage("Documents path is not a directory".to_string()));
        }

        Ok(())
    }
}

/// Document repository for tracking document metadata in the database
pub struct DocumentRepositoryImpl {
    pool: std::sync::Arc<PgPool>,
}

impl DocumentRepositoryImpl {
    pub fn new(pool: std::sync::Arc<PgPool>) -> Self {
        Self { pool }
    }

    /// Create a new document record
    pub async fn create(
        &self,
        tenant_id: &TenantId,
        document_id: Uuid,
        filename: String,
        mime_type: String,
        size_bytes: u64,
        storage_key: String,
        invoice_id: Option<billforge_core::domain::InvoiceId>,
        doc_type: DocumentType,
    ) -> Result<DocumentRef> {
        let now = Utc::now();
        let doc_type_str = match doc_type {
            DocumentType::InvoiceOriginal => "invoice_original",
            DocumentType::Supporting => "supporting",
            DocumentType::TaxDocument => "tax_document",
            DocumentType::Contract => "contract",
            DocumentType::Other => "other",
        };

        sqlx::query(
            r#"INSERT INTO documents (
                id, tenant_id, filename, mime_type, size_bytes, storage_key, invoice_id, doc_type, uploaded_by, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#
        )
        .bind(document_id)
        .bind(*tenant_id.as_uuid())
        .bind(&filename)
        .bind(&mime_type)
        .bind(size_bytes as i64)
        .bind(&storage_key)
        .bind(invoice_id.as_ref().map(|id| id.0))
        .bind(doc_type_str)
        .bind(Uuid::nil()) // uploaded_by - would need to be passed in
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to create document record: {}", e)))?;

        Ok(DocumentRef {
            id: document_id,
            tenant_id: tenant_id.clone(),
            filename,
            mime_type,
            size_bytes,
            storage_key,
            invoice_id,
            doc_type,
            created_at: now,
        })
    }

    /// Get a document by ID
    pub async fn get_by_id(&self, tenant_id: &TenantId, id: Uuid) -> Result<Option<DocumentRef>> {
        let result = sqlx::query_as::<_, DocumentRow>(
            r#"SELECT id, tenant_id, filename, mime_type, size_bytes, storage_key, invoice_id, doc_type, created_at
               FROM documents WHERE id = $1 AND tenant_id = $2"#
        )
        .bind(id)
        .bind(*tenant_id.as_uuid())
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get document: {}", e)))?;

        Ok(result.map(|row| row.into_ref(tenant_id)))
    }

    /// List documents for an invoice
    pub async fn list_for_invoice(
        &self,
        tenant_id: &TenantId,
        invoice_id: &billforge_core::domain::InvoiceId,
    ) -> Result<Vec<DocumentRef>> {
        let rows = sqlx::query_as::<_, DocumentRow>(
            r#"SELECT id, tenant_id, filename, mime_type, size_bytes, storage_key, invoice_id, doc_type, created_at
               FROM documents WHERE invoice_id = $1 AND tenant_id = $2 ORDER BY created_at DESC"#
        )
        .bind(invoice_id.0)
        .bind(*tenant_id.as_uuid())
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to list documents: {}", e)))?;

        Ok(rows.into_iter().map(|row| row.into_ref(tenant_id)).collect())
    }

    /// Delete a document record
    pub async fn delete(&self, tenant_id: &TenantId, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM documents WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(*tenant_id.as_uuid())
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to delete document: {}", e)))?;

        Ok(())
    }

    /// Link a document to an invoice
    pub async fn link_to_invoice(
        &self,
        tenant_id: &TenantId,
        document_id: Uuid,
        invoice_id: &billforge_core::domain::InvoiceId,
    ) -> Result<()> {
        sqlx::query("UPDATE documents SET invoice_id = $1 WHERE id = $2 AND tenant_id = $3")
            .bind(invoice_id.0)
            .bind(document_id)
            .bind(*tenant_id.as_uuid())
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::Database(format!("Failed to link document: {}", e)))?;

        Ok(())
    }
}

/// Helper struct for mapping database rows
#[derive(sqlx::FromRow)]
struct DocumentRow {
    id: Uuid,
    tenant_id: Uuid,
    filename: String,
    mime_type: String,
    size_bytes: i64,
    storage_key: String,
    invoice_id: Option<Uuid>,
    doc_type: String,
    created_at: chrono::DateTime<Utc>,
}

impl DocumentRow {
    fn into_ref(self, tenant_id: &TenantId) -> DocumentRef {
        DocumentRef {
            id: self.id,
            tenant_id: tenant_id.clone(),
            filename: self.filename,
            mime_type: self.mime_type,
            size_bytes: self.size_bytes as u64,
            storage_key: self.storage_key,
            invoice_id: self.invoice_id.map(billforge_core::domain::InvoiceId),
            doc_type: match self.doc_type.as_str() {
                "invoice_original" => DocumentType::InvoiceOriginal,
                "supporting" => DocumentType::Supporting,
                "tax_document" => DocumentType::TaxDocument,
                "contract" => DocumentType::Contract,
                _ => DocumentType::Other,
            },
            created_at: self.created_at,
        }
    }
}

// =============================================================================
// S3 Storage Service (Production)
// =============================================================================

#[cfg(feature = "s3")]
mod s3_storage {
    use super::*;
    use aws_sdk_s3::{
        config::{Region, Builder as S3ConfigBuilder},
        primitives::ByteStream,
        Client as S3Client,
        presigning::PresigningConfig,
    };
    use std::time::Duration;

    /// AWS S3 storage service for production deployments
    pub struct S3StorageService {
        client: S3Client,
        bucket: String,
        key_prefix: Option<String>,
    }

    impl S3StorageService {
        /// Create a new S3 storage service
        pub async fn new(
            bucket: String,
            region: String,
            endpoint: Option<String>,
            key_prefix: Option<String>,
        ) -> Result<Self> {
            let mut config_builder = aws_config::defaults(aws_config::BehaviorVersion::latest())
                .region(Region::new(region));

            if let Some(endpoint_url) = endpoint {
                config_builder = config_builder.endpoint_url(endpoint_url);
            }

            let config = config_builder.load().await;
            let s3_config = S3ConfigBuilder::new().from(&config).build();
            let client = S3Client::from_conf(s3_config);

            Ok(Self {
                client,
                bucket,
                key_prefix,
            })
        }

        fn build_key(&self, storage_key: &str) -> String {
            match &self.key_prefix {
                Some(prefix) => format!("{}/{}", prefix, storage_key),
                None => storage_key.to_string(),
            }
        }
    }

    #[async_trait]
    impl StorageService for S3StorageService {
        async fn upload(
            &self,
            tenant_id: &TenantId,
            filename: &str,
            data: &[u8],
            mime_type: &str,
        ) -> Result<Uuid> {
            let document_id = Uuid::new_v4();
            let storage_key = format!("{}/{}", tenant_id.as_str(), document_id);
            let key = self.build_key(&storage_key);

            self.client
                .put_object()
                .bucket(&self.bucket)
                .key(&key)
                .body(ByteStream::from(data.to_vec()))
                .content_type(mime_type)
                .send()
                .await
                .map_err(|e| Error::Storage(format!("Failed to upload to S3: {}", e)))?;

            tracing::debug!("Uploaded document {} to S3: {}", document_id, key);
            Ok(document_id)
        }

        async fn download(&self, storage_key: &str) -> Result<Vec<u8>> {
            let key = self.build_key(storage_key);

            let output = self.client
                .get_object()
                .bucket(&self.bucket)
                .key(&key)
                .send()
                .await
                .map_err(|e| Error::Storage(format!("Failed to download from S3: {}", e)))?;

            let bytes = output.body
                .collect()
                .await
                .map_err(|e| Error::Storage(format!("Failed to read S3 response: {}", e)))?
                .into_bytes();

            Ok(bytes.to_vec())
        }

        async fn delete(&self, storage_key: &str) -> Result<()> {
            let key = self.build_key(storage_key);

            self.client
                .delete_object()
                .bucket(&self.bucket)
                .key(&key)
                .send()
                .await
                .map_err(|e| Error::Storage(format!("Failed to delete from S3: {}", e)))?;

            tracing::debug!("Deleted document from S3: {}", key);
            Ok(())
        }

        async fn health_check(&self) -> Result<()> {
            self.client
                .head_bucket()
                .bucket(&self.bucket)
                .send()
                .await
                .map_err(|e| Error::Storage(format!("S3 health check failed: {}", e)))?;

            Ok(())
        }
    }
}

#[cfg(feature = "s3")]
pub use s3_storage::S3StorageService;
