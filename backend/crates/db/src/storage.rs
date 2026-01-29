//! Local file storage service for documents
//!
//! Stores files on the local filesystem organized by tenant.
//! Each tenant's files are stored in: {base_path}/documents/{tenant_id}/{document_id}

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
