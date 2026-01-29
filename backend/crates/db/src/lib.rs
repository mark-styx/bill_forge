//! BillForge Database Layer
//!
//! Provides multi-tenant database management with complete isolation:
//! - SQLite for metadata, auth, and tenant registry
//! - Per-tenant SQLite databases for analytical data (invoices, vendors, etc.)
//! - Local file storage for documents

pub mod manager;
pub mod migrations;
pub mod tenant_db;
pub mod metadata_db;
pub mod repositories;
pub mod storage;

pub use manager::DatabaseManager;
pub use tenant_db::TenantDatabase;
pub use metadata_db::MetadataDatabase;
pub use storage::{LocalStorageService, DocumentRepositoryImpl};
pub use repositories::AuditRepositoryImpl;
