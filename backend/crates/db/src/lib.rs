//! BillForge Database Layer
//!
//! Provides multi-tenant database management with complete isolation:
//! - SQLite for metadata, auth, and tenant registry
//! - Per-tenant SQLite databases for analytical data (invoices, vendors, etc.)
//! - Local file storage for development, S3 for production
//!
//! # Storage Configuration
//!
//! The storage layer supports both local filesystem and AWS S3:
//!
//! ```rust,ignore
//! // Local storage (development)
//! let config = StorageConfig::local("./data");
//!
//! // S3 storage (production)
//! let config = StorageConfig::s3("my-bucket".into(), "us-east-1".into());
//!
//! // S3-compatible (MinIO, LocalStack)
//! let config = StorageConfig::s3_compatible(
//!     "my-bucket".into(),
//!     "us-east-1".into(),
//!     "http://localhost:9000".into(),
//! );
//!
//! // Create service from config
//! let storage = create_storage_service(config).await?;
//! ```

pub mod manager;
pub mod migrations;
pub mod tenant_db;
pub mod metadata_db;
pub mod repositories;
pub mod storage;

pub use manager::DatabaseManager;
pub use tenant_db::TenantDatabase;
pub use metadata_db::MetadataDatabase;
pub use storage::{LocalStorageService, DocumentRepositoryImpl, StorageConfig, create_storage_service};
#[cfg(feature = "s3")]
pub use storage::S3StorageService;
pub use repositories::AuditRepositoryImpl;
