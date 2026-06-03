//! Tests verifying that OcrComparison is wired into InvoiceCaptureService.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use billforge_core::domain::{
    CaptureStatus, CreateInvoiceInput, ExtractedField, Invoice, InvoiceFilters,
    InvoiceId, InvoiceLineItem, OcrExtractionResult, ProcessingStatus,
};
use billforge_core::traits::{InvoiceRepository, OcrService, StorageService};
use billforge_core::types::{Money, PaginatedResponse, Pagination, TenantId, UserId};
use billforge_core::Result;
use billforge_invoice_capture::{InvoiceCaptureService, OcrComparison, OcrProvider};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Fakes (mirroring service_test.rs patterns)
// ---------------------------------------------------------------------------

struct StubOcr {
    name: &'static str,
    result: OcrExtractionResult,
}

impl StubOcr {
    fn new(name: &'static str, result: OcrExtractionResult) -> Self {
        Self { name, result }
    }
}

#[async_trait]
impl OcrService for StubOcr {
    async fn extract(
        &self,
        _document_bytes: &[u8],
        _mime_type: &str,
    ) -> Result<OcrExtractionResult> {
        Ok(self.result.clone())
    }

    fn supported_formats(&self) -> Vec<&'static str> {
        vec!["application/pdf"]
    }

    fn provider_name(&self) -> &'static str {
        self.name
    }
}

struct FakeStorage {
    uploads: Mutex<HashMap<Uuid, Vec<u8>>>,
    download_count: Mutex<usize>,
}

impl FakeStorage {
    fn new() -> Self {
        Self {
            uploads: Mutex::new(HashMap::new()),
            download_count: Mutex::new(0),
        }
    }

    fn upload_count(&self) -> usize {
        self.uploads.lock().unwrap().len()
    }

    fn download_count(&self) -> usize {
        *self.download_count.lock().unwrap()
    }

    fn seed(&self, id: Uuid, data: Vec<u8>) {
        self.uploads.lock().unwrap().insert(id, data);
    }
}

#[async_trait]
impl StorageService for FakeStorage {
    async fn upload(
        &self,
        _tenant_id: &TenantId,
        _file_name: &str,
        data: &[u8],
        _mime_type: &str,
    ) -> Result<Uuid> {
        let id = Uuid::new_v4();
        self.uploads.lock().unwrap().insert(id, data.to_vec());
        Ok(id)
    }

    async fn download(&self, _tenant_id: &TenantId, file_id: Uuid) -> Result<Vec<u8>> {
        *self.download_count.lock().unwrap() += 1;
        self.uploads
            .lock()
            .unwrap()
            .get(&file_id)
            .cloned()
            .ok_or_else(|| billforge_core::Error::FileNotFound(file_id.to_string()))
    }

    async fn delete(&self, _tenant_id: &TenantId, _file_id: Uuid) -> Result<()> {
        Ok(())
    }

    async fn get_url(
        &self,
        _tenant_id: &TenantId,
        _file_id: Uuid,
        _expires_in_secs: u64,
    ) -> Result<String> {
        Ok(String::new())
    }

    async fn health_check(&self) -> Result<()> {
        Ok(())
    }
}

struct FakeInvoiceRepo {
    invoices: Mutex<HashMap<InvoiceId, Invoice>>,
    status_transitions: Mutex<Vec<CaptureStatus>>,
}

impl FakeInvoiceRepo {
    fn new() -> Self {
        Self {
            invoices: Mutex::new(HashMap::new()),
            status_transitions: Mutex::new(Vec::new()),
        }
    }

    fn seed(&self, invoice: Invoice) {
        self.invoices
            .lock()
            .unwrap()
            .insert(invoice.id.clone(), invoice);
    }
}

#[async_trait]
impl InvoiceRepository for FakeInvoiceRepo {
    async fn create(
        &self,
        _tenant_id: &TenantId,
        input: CreateInvoiceInput,
        created_by: Option<&UserId>,
    ) -> Result<Invoice> {
        let id = InvoiceId::new();
        let now = chrono::Utc::now();
        let line_items: Vec<InvoiceLineItem> = input
            .line_items
            .into_iter()
            .enumerate()
            .map(|(i, li)| InvoiceLineItem {
                id: Uuid::new_v4(),
                line_number: i as u32 + 1,
                description: li.description,
                quantity: li.quantity,
                unit_price: li.unit_price,
                amount: li.amount,
                gl_code: li.gl_code,
                department: li.department,
                project: li.project,
            })
            .collect();

        let invoice = Invoice {
            id: id.clone(),
            tenant_id: _tenant_id.clone(),
            vendor_id: input.vendor_id,
            vendor_name: input.vendor_name,
            invoice_number: input.invoice_number,
            invoice_date: input.invoice_date,
            due_date: input.due_date,
            po_number: input.po_number,
            subtotal: input.subtotal,
            tax_amount: input.tax_amount,
            total_amount: input.total_amount,
            currency: input.currency,
            line_items,
            capture_status: CaptureStatus::Pending,
            processing_status: ProcessingStatus::Draft,
            current_queue_id: None,
            assigned_to: None,
            document_id: input.document_id,
            supporting_documents: Vec::new(),
            ocr_confidence: input.ocr_confidence,
            categorization_confidence: None,
            department: input.department,
            gl_code: input.gl_code,
            cost_center: input.cost_center,
            notes: input.notes,
            tags: input.tags,
            custom_fields: serde_json::Value::Null,
            created_by: created_by.cloned(),
            created_at: now,
            updated_at: now,
        };
        self.invoices
            .lock()
            .unwrap()
            .insert(id.clone(), invoice.clone());
        Ok(invoice)
    }

    async fn get_by_id(&self, _tenant_id: &TenantId, id: &InvoiceId) -> Result<Option<Invoice>> {
        Ok(self.invoices.lock().unwrap().get(id).cloned())
    }

    async fn update_capture_status(
        &self,
        _tenant_id: &TenantId,
        _id: &InvoiceId,
        status: CaptureStatus,
    ) -> Result<()> {
        self.status_transitions.lock().unwrap().push(status);
        if let Some(inv) = self.invoices.lock().unwrap().get_mut(_id) {
            inv.capture_status = status;
        }
        Ok(())
    }

    async fn list(
        &self,
        _tenant_id: &TenantId,
        _filters: &InvoiceFilters,
        _pagination: &Pagination,
    ) -> Result<PaginatedResponse<Invoice>> {
        unimplemented!()
    }

    async fn update(
        &self,
        _tenant_id: &TenantId,
        _id: &InvoiceId,
        _updates: serde_json::Value,
    ) -> Result<Invoice> {
        unimplemented!()
    }

    async fn delete(&self, _tenant_id: &TenantId, _id: &InvoiceId) -> Result<()> {
        unimplemented!()
    }

    async fn update_processing_status(
        &self,
        _tenant_id: &TenantId,
        _id: &InvoiceId,
        _status: ProcessingStatus,
    ) -> Result<()> {
        unimplemented!()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Low-confidence OCR result (simulates a weak provider).
fn low_confidence_result() -> OcrExtractionResult {
    OcrExtractionResult {
        invoice_number: ExtractedField::with_value("LOW-INV".to_string(), 0.3),
        invoice_date: ExtractedField::empty(),
        due_date: ExtractedField::empty(),
        vendor_name: ExtractedField::with_value("LowCorp".to_string(), 0.2),
        vendor_address: ExtractedField::empty(),
        subtotal: ExtractedField::empty(),
        tax_amount: ExtractedField::empty(),
        total_amount: ExtractedField::with_value(10.0, 0.25),
        currency: ExtractedField::with_value("USD".to_string(), 0.5),
        po_number: ExtractedField::empty(),
        line_items: vec![],
        raw_text: String::new(),
        processing_time_ms: 50,
    }
}

/// High-confidence OCR result (simulates a strong provider).
fn high_confidence_result() -> OcrExtractionResult {
    OcrExtractionResult {
        invoice_number: ExtractedField::with_value("HIGH-INV-99".to_string(), 0.98),
        invoice_date: ExtractedField::empty(),
        due_date: ExtractedField::empty(),
        vendor_name: ExtractedField::with_value("HighCorp".to_string(), 0.97),
        vendor_address: ExtractedField::empty(),
        subtotal: ExtractedField::empty(),
        tax_amount: ExtractedField::empty(),
        total_amount: ExtractedField::with_value(9999.0, 0.96),
        currency: ExtractedField::with_value("USD".to_string(), 0.99),
        po_number: ExtractedField::empty(),
        line_items: vec![],
        raw_text: String::new(),
        processing_time_ms: 80,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn upload_invoice_uses_comparison_best_provider() {
    let low_stub = StubOcr::new("tesseract", low_confidence_result());
    let _high_stub = StubOcr::new("aws_textract", high_confidence_result());

    // Build comparison with both providers; the high-confidence one should win.
    let comparison = Arc::new(OcrComparison::new(vec![
        (OcrProvider::Tesseract, Box::new(StubOcr::new("tesseract", low_confidence_result()))),
        (OcrProvider::AwsTextract, Box::new(StubOcr::new("aws_textract", high_confidence_result()))),
    ]));

    let storage = Arc::new(FakeStorage::new());
    let repo = Arc::new(FakeInvoiceRepo::new());

    // Service default provider is the low stub, but comparison should override.
    let service = InvoiceCaptureService::with_provider(
        Box::new(low_stub),
        repo.clone(),
        storage.clone(),
    )
    .with_comparison(comparison);

    let tenant_id = TenantId::new();
    let user_id = UserId::new();

    let invoice = service
        .upload_invoice(
            &tenant_id,
            &user_id,
            "test.pdf",
            b"pdf-bytes",
            "application/pdf",
        )
        .await
        .expect("upload_invoice should succeed");

    // The invoice should contain the high-confidence provider's values,
    // not the low-confidence default provider.
    assert_eq!(invoice.vendor_name, "HighCorp");
    assert_eq!(invoice.invoice_number, "HIGH-INV-99");
    assert_eq!(invoice.total_amount, Money::usd(9999.0));

    // Sanity: storage received the upload
    assert_eq!(storage.upload_count(), 1);
}

#[tokio::test]
async fn reprocess_ocr_uses_comparison_best_provider() {
    let low_stub = StubOcr::new("tesseract", low_confidence_result());
    let high_stub_for_comparison = StubOcr::new("aws_textract", high_confidence_result());

    let comparison = Arc::new(OcrComparison::new(vec![
        (OcrProvider::Tesseract, Box::new(StubOcr::new("tesseract", low_confidence_result()))),
        (OcrProvider::AwsTextract, Box::new(high_stub_for_comparison)),
    ]));

    let storage = Arc::new(FakeStorage::new());
    let repo = Arc::new(FakeInvoiceRepo::new());

    // Pre-seed an invoice with a stored document
    let doc_id = Uuid::new_v4();
    let invoice_id = InvoiceId::new();
    let tenant_id = TenantId::new();

    storage.seed(doc_id, b"stored-pdf".to_vec());

    let seed_invoice = Invoice {
        id: invoice_id.clone(),
        tenant_id: tenant_id.clone(),
        vendor_id: None,
        vendor_name: "OldVendor".to_string(),
        invoice_number: "OLD-1".to_string(),
        invoice_date: None,
        due_date: None,
        po_number: None,
        subtotal: None,
        tax_amount: None,
        total_amount: Money::usd(0.0),
        currency: "USD".to_string(),
        line_items: vec![],
        capture_status: CaptureStatus::Reviewed,
        processing_status: ProcessingStatus::Draft,
        current_queue_id: None,
        assigned_to: None,
        document_id: doc_id,
        supporting_documents: vec![],
        ocr_confidence: None,
        categorization_confidence: None,
        department: None,
        gl_code: None,
        cost_center: None,
        notes: None,
        tags: vec![],
        custom_fields: serde_json::Value::Null,
        created_by: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    repo.seed(seed_invoice);

    let service = InvoiceCaptureService::with_provider(
        Box::new(low_stub),
        repo.clone(),
        storage.clone(),
    )
    .with_comparison(comparison);

    let result = service
        .reprocess_ocr(&tenant_id, &invoice_id)
        .await
        .expect("reprocess_ocr should succeed");

    // The result should come from the high-confidence comparison provider.
    assert_eq!(result.invoice_number.value.as_deref(), Some("HIGH-INV-99"));
    assert_eq!(result.vendor_name.value.as_deref(), Some("HighCorp"));
    assert_eq!(result.total_amount.value, Some(9999.0));

    // Storage download was called
    assert_eq!(storage.download_count(), 1);
}
