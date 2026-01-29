//! Vendor management service

use billforge_core::{
    domain::{CreateVendorInput, TaxDocument, UpdateVendorInput, Vendor, VendorFilters, VendorId},
    traits::{StorageService, TaxDocumentRepository, VendorRepository},
    types::{PaginatedResponse, Pagination, TenantId},
    Result,
};
use std::sync::Arc;

/// Service for vendor management
pub struct VendorService {
    vendor_repo: Arc<dyn VendorRepository>,
    tax_doc_repo: Arc<dyn TaxDocumentRepository>,
    storage: Arc<dyn StorageService>,
}

impl VendorService {
    pub fn new(
        vendor_repo: Arc<dyn VendorRepository>,
        tax_doc_repo: Arc<dyn TaxDocumentRepository>,
        storage: Arc<dyn StorageService>,
    ) -> Self {
        Self {
            vendor_repo,
            tax_doc_repo,
            storage,
        }
    }

    /// Create a new vendor
    pub async fn create_vendor(
        &self,
        tenant_id: &TenantId,
        input: CreateVendorInput,
    ) -> Result<Vendor> {
        // Check for duplicate
        if let Some(existing) = self.vendor_repo.find_by_name(tenant_id, &input.name).await? {
            return Err(billforge_core::Error::AlreadyExists {
                resource_type: "Vendor".to_string(),
            });
        }

        self.vendor_repo.create(tenant_id, input).await
    }

    /// Update an existing vendor
    pub async fn update_vendor(
        &self,
        tenant_id: &TenantId,
        vendor_id: &VendorId,
        input: UpdateVendorInput,
    ) -> Result<Vendor> {
        self.vendor_repo.update(tenant_id, vendor_id, input).await
    }

    /// Get a vendor by ID
    pub async fn get_vendor(
        &self,
        tenant_id: &TenantId,
        vendor_id: &VendorId,
    ) -> Result<Option<Vendor>> {
        self.vendor_repo.get_by_id(tenant_id, vendor_id).await
    }

    /// List vendors with filtering and pagination
    pub async fn list_vendors(
        &self,
        tenant_id: &TenantId,
        filters: &VendorFilters,
        pagination: &Pagination,
    ) -> Result<PaginatedResponse<Vendor>> {
        self.vendor_repo.list(tenant_id, filters, pagination).await
    }

    /// Upload a tax document for a vendor
    pub async fn upload_tax_document(
        &self,
        tenant_id: &TenantId,
        vendor_id: &VendorId,
        doc: TaxDocument,
        file_bytes: &[u8],
        mime_type: &str,
    ) -> Result<TaxDocument> {
        // Store the file
        let file_id = self
            .storage
            .upload(tenant_id, &doc.file_name, file_bytes, mime_type)
            .await?;

        // Create document record with the file ID
        let mut doc = doc;
        doc.file_id = file_id;

        self.tax_doc_repo.create(tenant_id, doc).await
    }

    /// Get tax documents for a vendor
    pub async fn get_tax_documents(
        &self,
        tenant_id: &TenantId,
        vendor_id: &VendorId,
    ) -> Result<Vec<TaxDocument>> {
        self.tax_doc_repo.list_for_vendor(tenant_id, vendor_id).await
    }
}
