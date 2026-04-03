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

// Allow unused variables and dead code in stub implementations (TODOs)
#![allow(unused_variables)]
#![allow(dead_code)]

pub mod manager;
pub mod metadata_db;
pub mod migrations;
pub mod pg_manager;
pub mod repositories;
pub mod seed;
pub mod storage;
pub mod tenant_db;

pub use manager::DatabaseManager;
pub use metadata_db::MetadataDatabase;
pub use pg_manager::PgManager;
pub use repositories::AuditRepositoryImpl;
#[cfg(feature = "s3")]
pub use storage::S3StorageService;
pub use storage::{
    create_storage_service, DocumentRepositoryImpl, LocalStorageService, StorageConfig,
};
