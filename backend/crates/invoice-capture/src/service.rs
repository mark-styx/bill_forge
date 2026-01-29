//! Invoice capture service

use crate::ocr;
use billforge_core::{
    domain::{CaptureStatus, CreateInvoiceInput, CreateLineItemInput, Invoice, OcrExtractionResult},
    traits::{InvoiceRepository, OcrService, StorageService},
    types::{Money, TenantId, UserId},
    Result,
};
use std::sync::Arc;
use uuid::Uuid;

/// Service for capturing and processing invoices
pub struct InvoiceCaptureService {
    ocr_provider: Box<dyn OcrService>,
    invoice_repo: Arc<dyn InvoiceRepository>,
    storage: Arc<dyn StorageService>,
}

impl InvoiceCaptureService {
    pub fn new(
        ocr_provider_name: &str,
        invoice_repo: Arc<dyn InvoiceRepository>,
        storage: Arc<dyn StorageService>,
    ) -> Self {
        Self {
            ocr_provider: ocr::create_provider(ocr_provider_name),
            invoice_repo,
            storage,
        }
    }

    /// Upload and process a new invoice document
    pub async fn upload_invoice(
        &self,
        tenant_id: &TenantId,
        user_id: &UserId,
        file_name: &str,
        file_bytes: &[u8],
        mime_type: &str,
    ) -> Result<Invoice> {
        // 1. Store the document
        let document_id = self
            .storage
            .upload(tenant_id, file_name, file_bytes, mime_type)
            .await?;

        // 2. Run OCR
        let ocr_result = self.ocr_provider.extract(file_bytes, mime_type).await?;

        // 3. Create invoice from OCR result
        let invoice = self
            .create_invoice_from_ocr(tenant_id, user_id, document_id, &ocr_result)
            .await?;

        Ok(invoice)
    }

    /// Create an invoice record from OCR extraction result
    async fn create_invoice_from_ocr(
        &self,
        tenant_id: &TenantId,
        user_id: &UserId,
        document_id: Uuid,
        ocr_result: &OcrExtractionResult,
    ) -> Result<Invoice> {
        // Calculate overall confidence
        let confidence = self.calculate_confidence(ocr_result);

        let input = CreateInvoiceInput {
            document_id,
            vendor_id: None,
            vendor_name: ocr_result
                .vendor_name
                .value
                .clone()
                .unwrap_or_else(|| "Unknown Vendor".to_string()),
            invoice_number: ocr_result
                .invoice_number
                .value
                .clone()
                .unwrap_or_else(|| format!("UNKNOWN-{}", Uuid::new_v4())),
            invoice_date: ocr_result.invoice_date.value,
            due_date: ocr_result.due_date.value,
            po_number: ocr_result.po_number.value.clone(),
            subtotal: ocr_result
                .subtotal
                .value
                .map(|v| Money::usd(v)),
            tax_amount: ocr_result
                .tax_amount
                .value
                .map(|v| Money::usd(v)),
            total_amount: Money::usd(ocr_result.total_amount.value.unwrap_or(0.0)),
            currency: ocr_result
                .currency
                .value
                .clone()
                .unwrap_or_else(|| "USD".to_string()),
            line_items: ocr_result
                .line_items
                .iter()
                .map(|item| CreateLineItemInput {
                    description: item
                        .description
                        .value
                        .clone()
                        .unwrap_or_default(),
                    quantity: item.quantity.value,
                    unit_price: item.unit_price.value.map(|v| Money::usd(v)),
                    amount: Money::usd(item.amount.value.unwrap_or(0.0)),
                    gl_code: None,
                    department: None,
                    project: None,
                })
                .collect(),
            ocr_confidence: Some(confidence),
            department: None,
            gl_code: None,
            cost_center: None,
            notes: None,
            tags: Vec::new(),
        };

        self.invoice_repo.create(tenant_id, input, user_id).await
    }

    /// Calculate overall confidence score from OCR result
    fn calculate_confidence(&self, ocr_result: &OcrExtractionResult) -> f32 {
        let fields = [
            ocr_result.invoice_number.confidence,
            ocr_result.invoice_date.confidence,
            ocr_result.vendor_name.confidence,
            ocr_result.total_amount.confidence,
        ];

        let sum: f32 = fields.iter().sum();
        sum / fields.len() as f32
    }

    /// Reprocess OCR for an existing invoice
    pub async fn reprocess_ocr(
        &self,
        tenant_id: &TenantId,
        invoice_id: &billforge_core::domain::InvoiceId,
    ) -> Result<OcrExtractionResult> {
        // Get the invoice
        let invoice = self
            .invoice_repo
            .get_by_id(tenant_id, invoice_id)
            .await?
            .ok_or_else(|| billforge_core::Error::NotFound {
                resource_type: "Invoice".to_string(),
                id: invoice_id.to_string(),
            })?;

        // Download the document
        let document_bytes = self.storage.download(tenant_id, invoice.document_id).await?;

        // Update status
        self.invoice_repo
            .update_capture_status(tenant_id, invoice_id, CaptureStatus::Processing)
            .await?;

        // Run OCR
        let ocr_result = self
            .ocr_provider
            .extract(&document_bytes, "application/pdf")
            .await?;

        // Update status
        self.invoice_repo
            .update_capture_status(tenant_id, invoice_id, CaptureStatus::ReadyForReview)
            .await?;

        Ok(ocr_result)
    }
}
