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

// =============================================================================
// Storage Factory
// =============================================================================

/// Create a storage service from configuration
pub async fn create_storage_service(config: StorageConfig) -> Result<Box<dyn StorageService>> {
    match config {
        StorageConfig::Local { base_path } => {
            Ok(Box::new(LocalStorageService::new(base_path)))
        }
        #[cfg(feature = "s3")]
        StorageConfig::S3 { bucket, region, endpoint, key_prefix } => {
            let service = S3StorageService::new(bucket, region, endpoint, key_prefix).await?;
            Ok(Box::new(service))
        }
    }
}

// =============================================================================
// Local Storage Service
// =============================================================================

/// Local filesystem storage service
pub struct LocalStorageService {
    base_path: PathBuf,
}

impl LocalStorageService {
    /// Create a new storage service with the given base path
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        Self {
            base_path: base_path.into(),
        }
    }

    /// Get the directory path for a tenant's documents
    fn tenant_dir(&self, tenant_id: &TenantId) -> PathBuf {
        self.base_path.join("documents").join(tenant_id.to_string())
    }

    /// Get the full file path for a document
    fn document_path(&self, tenant_id: &TenantId, document_id: Uuid) -> PathBuf {
        self.tenant_dir(tenant_id).join(document_id.to_string())
    }

    /// Ensure the tenant's document directory exists
    async fn ensure_tenant_dir(&self, tenant_id: &TenantId) -> Result<()> {
        let dir = self.tenant_dir(tenant_id);
        if !dir.exists() {
            fs::create_dir_all(&dir)
                .await
                .map_err(|e| Error::Storage(format!("Failed to create directory: {}", e)))?;
        }
        Ok(())
    }

    /// Detect document type from mime type
    fn detect_doc_type(mime_type: &str) -> DocumentType {
        match mime_type {
            "application/pdf" => DocumentType::InvoiceOriginal,
            "image/png" | "image/jpeg" | "image/tiff" => DocumentType::InvoiceOriginal,
            _ => DocumentType::Other,
        }
    }
}

#[async_trait]
impl StorageService for LocalStorageService {
    /// Upload a file and return its document ID
    async fn upload(
        &self,
        tenant_id: &TenantId,
        file_name: &str,
        data: &[u8],
        mime_type: &str,
    ) -> Result<Uuid> {
        self.ensure_tenant_dir(tenant_id).await?;
        
        let document_id = Uuid::new_v4();
        let file_path = self.document_path(tenant_id, document_id);
        
        // Write file to disk
        let mut file = fs::File::create(&file_path)
            .await
            .map_err(|e| Error::Storage(format!("Failed to create file: {}", e)))?;
        
        file.write_all(data)
            .await
            .map_err(|e| Error::Storage(format!("Failed to write file: {}", e)))?;
        
        file.flush()
            .await
            .map_err(|e| Error::Storage(format!("Failed to flush file: {}", e)))?;
        
        tracing::info!(
            tenant_id = %tenant_id,
            document_id = %document_id,
            file_name = %file_name,
            mime_type = %mime_type,
            size_bytes = data.len(),
            "Document uploaded"
        );
        
        Ok(document_id)
    }

    /// Download a file by its document ID
    async fn download(&self, tenant_id: &TenantId, file_id: Uuid) -> Result<Vec<u8>> {
        let file_path = self.document_path(tenant_id, file_id);
        
        if !file_path.exists() {
            return Err(Error::NotFound {
                resource_type: "Document".to_string(),
                id: file_id.to_string(),
            });
        }
        
        let data = fs::read(&file_path)
            .await
            .map_err(|e| Error::Storage(format!("Failed to read file: {}", e)))?;
        
        Ok(data)
    }

    /// Delete a file by its document ID
    async fn delete(&self, tenant_id: &TenantId, file_id: Uuid) -> Result<()> {
        let file_path = self.document_path(tenant_id, file_id);
        
        if file_path.exists() {
            fs::remove_file(&file_path)
                .await
                .map_err(|e| Error::Storage(format!("Failed to delete file: {}", e)))?;
            
            tracing::info!(
                tenant_id = %tenant_id,
                document_id = %file_id,
                "Document deleted"
            );
        }
        
        Ok(())
    }

    /// Get a URL for accessing the document (for local storage, returns a relative path)
    async fn get_url(
        &self,
        tenant_id: &TenantId,
        file_id: Uuid,
        _expires_in_secs: u64,
    ) -> Result<String> {
        // For local storage, we return a relative API path
        // The actual file will be served through the API
        Ok(format!("/api/v1/documents/{}", file_id))
    }

    /// Health check - verifies the storage directory is accessible
    async fn health_check(&self) -> Result<()> {
        let base_path = &self.base_path;

        // Check if base path exists and is a directory
        if !base_path.exists() {
            return Err(Error::Storage("Storage base path does not exist".to_string()));
        }

        if !base_path.is_dir() {
            return Err(Error::Storage("Storage base path is not a directory".to_string()));
        }

        // Try to access the documents directory
        let docs_path = base_path.join("documents");
        if docs_path.exists() && !docs_path.is_dir() {
            return Err(Error::Storage("Documents path is not a directory".to_string()));
        }

        Ok(())
    }
}

/// Document repository for tracking document metadata in the database
pub struct DocumentRepositoryImpl {
    db_manager: std::sync::Arc<crate::DatabaseManager>,
}

impl DocumentRepositoryImpl {
    pub fn new(db_manager: std::sync::Arc<crate::DatabaseManager>) -> Self {
        Self { db_manager }
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
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;
        
        let now = Utc::now();
        let doc_type_str = match doc_type {
            DocumentType::InvoiceOriginal => "invoice_original",
            DocumentType::Supporting => "supporting",
            DocumentType::TaxDocument => "tax_document",
            DocumentType::Contract => "contract",
            DocumentType::Other => "other",
        };
        
        conn.execute(
            r#"INSERT INTO documents (
                id, filename, mime_type, size_bytes, storage_key, invoice_id, doc_type, uploaded_by, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            rusqlite::params![
                document_id.to_string(),
                filename,
                mime_type,
                size_bytes as i64,
                storage_key,
                invoice_id.as_ref().map(|id| id.0.to_string()),
                doc_type_str,
                "", // uploaded_by - would need to be passed in
                now.to_rfc3339(),
            ],
        )
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
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;
        
        let mut stmt = conn
            .prepare(
                r#"SELECT id, filename, mime_type, size_bytes, storage_key, invoice_id, doc_type, created_at
                   FROM documents WHERE id = ?"#,
            )
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;
        
        let result = stmt
            .query_row(rusqlite::params![id.to_string()], |row| {
                let id_str: String = row.get(0)?;
                let invoice_id_str: Option<String> = row.get(5)?;
                let doc_type_str: String = row.get(6)?;
                
                Ok(DocumentRef {
                    id: Uuid::parse_str(&id_str).unwrap(),
                    tenant_id: tenant_id.clone(),
                    filename: row.get(1)?,
                    mime_type: row.get(2)?,
                    size_bytes: row.get::<_, i64>(3)? as u64,
                    storage_key: row.get(4)?,
                    invoice_id: invoice_id_str.and_then(|s| {
                        Uuid::parse_str(&s).ok().map(billforge_core::domain::InvoiceId)
                    }),
                    doc_type: match doc_type_str.as_str() {
                        "invoice_original" => DocumentType::InvoiceOriginal,
                        "supporting" => DocumentType::Supporting,
                        "tax_document" => DocumentType::TaxDocument,
                        "contract" => DocumentType::Contract,
                        _ => DocumentType::Other,
                    },
                    created_at: Utc::now(),
                })
            })
            .ok();
        
        Ok(result)
    }

    /// List documents for an invoice
    pub async fn list_for_invoice(
        &self,
        tenant_id: &TenantId,
        invoice_id: &billforge_core::domain::InvoiceId,
    ) -> Result<Vec<DocumentRef>> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;
        
        let mut stmt = conn
            .prepare(
                r#"SELECT id, filename, mime_type, size_bytes, storage_key, invoice_id, doc_type, created_at
                   FROM documents WHERE invoice_id = ? ORDER BY created_at DESC"#,
            )
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;
        
        let tenant_clone = tenant_id.clone();
        let rows = stmt
            .query_map(rusqlite::params![invoice_id.0.to_string()], move |row| {
                let id_str: String = row.get(0)?;
                let invoice_id_str: Option<String> = row.get(5)?;
                let doc_type_str: String = row.get(6)?;
                
                Ok(DocumentRef {
                    id: Uuid::parse_str(&id_str).unwrap(),
                    tenant_id: tenant_clone.clone(),
                    filename: row.get(1)?,
                    mime_type: row.get(2)?,
                    size_bytes: row.get::<_, i64>(3)? as u64,
                    storage_key: row.get(4)?,
                    invoice_id: invoice_id_str.and_then(|s| {
                        Uuid::parse_str(&s).ok().map(billforge_core::domain::InvoiceId)
                    }),
                    doc_type: match doc_type_str.as_str() {
                        "invoice_original" => DocumentType::InvoiceOriginal,
                        "supporting" => DocumentType::Supporting,
                        "tax_document" => DocumentType::TaxDocument,
                        "contract" => DocumentType::Contract,
                        _ => DocumentType::Other,
                    },
                    created_at: Utc::now(),
                })
            })
            .map_err(|e| Error::Database(format!("Failed to list documents: {}", e)))?;
        
        let mut docs = Vec::new();
        for row in rows {
            docs.push(row.map_err(|e| Error::Database(e.to_string()))?);
        }
        
        Ok(docs)
    }

    /// Delete a document record
    pub async fn delete(&self, tenant_id: &TenantId, id: Uuid) -> Result<()> {
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;
        
        conn.execute(
            "DELETE FROM documents WHERE id = ?",
            rusqlite::params![id.to_string()],
        )
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
        let db = self.db_manager.tenant(tenant_id).await?;
        let conn = db.connection().await;
        let conn = conn.lock().await;

        conn.execute(
            "UPDATE documents SET invoice_id = ? WHERE id = ?",
            rusqlite::params![invoice_id.0.to_string(), document_id.to_string()],
        )
        .map_err(|e| Error::Database(format!("Failed to link document: {}", e)))?;

        Ok(())
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
            let region = Region::new(region);

            // Load AWS config from environment
            let mut config_loader = aws_config::from_env().region(region.clone());

            // If custom endpoint specified (MinIO, LocalStack), configure it
            let sdk_config = config_loader.load().await;

            let mut s3_config_builder = aws_sdk_s3::config::Builder::from(&sdk_config);

            if let Some(ref endpoint_url) = endpoint {
                s3_config_builder = s3_config_builder
                    .endpoint_url(endpoint_url)
                    .force_path_style(true); // Required for MinIO/LocalStack
            }

            let client = S3Client::from_conf(s3_config_builder.build());

            tracing::info!(
                bucket = %bucket,
                endpoint = ?endpoint,
                "S3 storage service initialized"
            );

            Ok(Self {
                client,
                bucket,
                key_prefix,
            })
        }

        /// Generate the S3 key for a document
        fn object_key(&self, tenant_id: &TenantId, document_id: Uuid) -> String {
            let base_key = format!("{}/{}", tenant_id, document_id);
            match &self.key_prefix {
                Some(prefix) => format!("{}/{}", prefix.trim_end_matches('/'), base_key),
                None => base_key,
            }
        }
    }

    #[async_trait]
    impl StorageService for S3StorageService {
        async fn upload(
            &self,
            tenant_id: &TenantId,
            file_name: &str,
            data: &[u8],
            mime_type: &str,
        ) -> Result<Uuid> {
            let document_id = Uuid::new_v4();
            let key = self.object_key(tenant_id, document_id);

            self.client
                .put_object()
                .bucket(&self.bucket)
                .key(&key)
                .body(ByteStream::from(data.to_vec()))
                .content_type(mime_type)
                .metadata("original-filename", file_name)
                .send()
                .await
                .map_err(|e| Error::Storage(format!("S3 upload failed: {}", e)))?;

            tracing::info!(
                tenant_id = %tenant_id,
                document_id = %document_id,
                key = %key,
                file_name = %file_name,
                mime_type = %mime_type,
                size_bytes = data.len(),
                "Document uploaded to S3"
            );

            Ok(document_id)
        }

        async fn download(&self, tenant_id: &TenantId, file_id: Uuid) -> Result<Vec<u8>> {
            let key = self.object_key(tenant_id, file_id);

            let response = self.client
                .get_object()
                .bucket(&self.bucket)
                .key(&key)
                .send()
                .await
                .map_err(|e| {
                    // Check if it's a not found error
                    if e.to_string().contains("NoSuchKey") || e.to_string().contains("not found") {
                        Error::NotFound {
                            resource_type: "Document".to_string(),
                            id: file_id.to_string(),
                        }
                    } else {
                        Error::Storage(format!("S3 download failed: {}", e))
                    }
                })?;

            let data = response
                .body
                .collect()
                .await
                .map_err(|e| Error::Storage(format!("Failed to read S3 response body: {}", e)))?
                .into_bytes()
                .to_vec();

            Ok(data)
        }

        async fn delete(&self, tenant_id: &TenantId, file_id: Uuid) -> Result<()> {
            let key = self.object_key(tenant_id, file_id);

            self.client
                .delete_object()
                .bucket(&self.bucket)
                .key(&key)
                .send()
                .await
                .map_err(|e| Error::Storage(format!("S3 delete failed: {}", e)))?;

            tracing::info!(
                tenant_id = %tenant_id,
                document_id = %file_id,
                key = %key,
                "Document deleted from S3"
            );

            Ok(())
        }

        async fn get_url(
            &self,
            tenant_id: &TenantId,
            file_id: Uuid,
            expires_in_secs: u64,
        ) -> Result<String> {
            let key = self.object_key(tenant_id, file_id);

            let presigning_config = PresigningConfig::expires_in(Duration::from_secs(expires_in_secs))
                .map_err(|e| Error::Storage(format!("Invalid presigning duration: {}", e)))?;

            let presigned_request = self.client
                .get_object()
                .bucket(&self.bucket)
                .key(&key)
                .presigned(presigning_config)
                .await
                .map_err(|e| Error::Storage(format!("Failed to generate presigned URL: {}", e)))?;

            Ok(presigned_request.uri().to_string())
        }

        async fn health_check(&self) -> Result<()> {
            // Try to list objects with max_keys=1 to verify bucket access
            self.client
                .list_objects_v2()
                .bucket(&self.bucket)
                .max_keys(1)
                .send()
                .await
                .map_err(|e| Error::Storage(format!("S3 health check failed: {}", e)))?;

            Ok(())
        }
    }
}

#[cfg(feature = "s3")]
pub use s3_storage::S3StorageService;
