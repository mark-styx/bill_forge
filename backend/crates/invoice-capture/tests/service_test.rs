//! Integration tests for InvoiceCaptureService orchestration

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use billforge_core::domain::{
    CaptureStatus, CreateInvoiceInput, ExtractedField, ExtractedLineItem, Invoice, InvoiceFilters,
    InvoiceId, InvoiceLineItem, OcrExtractionResult, ProcessingStatus,
};
use billforge_core::traits::{InvoiceRepository, OcrService, StorageService};
use billforge_core::types::{Money, PaginatedResponse, Pagination, TenantId, UserId};
use billforge_core::Result;
use billforge_invoice_capture::service::InvoiceCaptureService;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------------

struct FakeOcr {
    result: OcrExtractionResult,
    call_count: Arc<Mutex<usize>>,
}

impl FakeOcr {
    fn new(result: OcrExtractionResult, call_count: Arc<Mutex<usize>>) -> Self {
        Self { result, call_count }
    }
}

#[async_trait]
impl OcrService for FakeOcr {
    async fn extract(&self, _document_bytes: &[u8], _mime_type: &str) -> Result<OcrExtractionResult> {
        *self.call_count.lock().unwrap() += 1;
        Ok(self.result.clone())
    }

    fn supported_formats(&self) -> Vec<&'static str> {
        vec!["application/pdf"]
    }

    fn provider_name(&self) -> &'static str {
        "fake"
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

    fn status_transitions(&self) -> Vec<CaptureStatus> {
        self.status_transitions.lock().unwrap().clone()
    }
}

#[async_trait]
impl InvoiceRepository for FakeInvoiceRepo {
    async fn create(
        &self,
        _tenant_id: &TenantId,
        input: CreateInvoiceInput,
        created_by: &UserId,
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
            created_by: created_by.clone(),
            created_at: now,
            updated_at: now,
        };
        self.invoices
            .lock()
            .unwrap()
            .insert(id.clone(), invoice.clone());
        Ok(invoice)
    }

    async fn get_by_id(
        &self,
        _tenant_id: &TenantId,
        id: &InvoiceId,
    ) -> Result<Option<Invoice>> {
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

fn make_service() -> (
    InvoiceCaptureService,
    Arc<Mutex<usize>>,
    Arc<FakeStorage>,
    Arc<FakeInvoiceRepo>,
) {
    let call_count = Arc::new(Mutex::new(0usize));
    let ocr = FakeOcr::new(default_ocr_result(), call_count.clone());
    let storage = Arc::new(FakeStorage::new());
    let repo = Arc::new(FakeInvoiceRepo::new());

    let service = InvoiceCaptureService::with_provider(
        Box::new(ocr),
        repo.clone(),
        storage.clone(),
    );

    (service, call_count, storage, repo)
}

fn default_ocr_result() -> OcrExtractionResult {
    OcrExtractionResult {
        invoice_number: ExtractedField::with_value("INV-1".to_string(), 0.95),
        invoice_date: ExtractedField::empty(),
        due_date: ExtractedField::empty(),
        vendor_name: ExtractedField::with_value("ACME".to_string(), 0.90),
        vendor_address: ExtractedField::empty(),
        subtotal: ExtractedField::empty(),
        tax_amount: ExtractedField::empty(),
        total_amount: ExtractedField::with_value(150.00, 0.88),
        currency: ExtractedField::with_value("USD".to_string(), 0.99),
        po_number: ExtractedField::empty(),
        line_items: vec![
            ExtractedLineItem {
                description: ExtractedField::with_value("Widget A".to_string(), 0.92),
                quantity: ExtractedField::with_value(2.0, 0.95),
                unit_price: ExtractedField::with_value(50.0, 0.93),
                amount: ExtractedField::with_value(100.0, 0.97),
            },
            ExtractedLineItem {
                description: ExtractedField::with_value("Widget B".to_string(), 0.91),
                quantity: ExtractedField::with_value(1.0, 0.94),
                unit_price: ExtractedField::with_value(50.0, 0.90),
                amount: ExtractedField::with_value(50.0, 0.96),
            },
        ],
        raw_text: String::new(),
        processing_time_ms: 100,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn upload_invoice_happy_path() {
    let (service, ocr_calls, storage, _repo) = make_service();

    let tenant_id = TenantId::new();
    let user_id = UserId::new();

    let invoice = service
        .upload_invoice(&tenant_id, &user_id, "invoice.pdf", b"fake-pdf-bytes", "application/pdf")
        .await
        .expect("upload_invoice should succeed");

    // Storage received one upload
    assert_eq!(storage.upload_count(), 1);

    // OCR extract was called once
    assert_eq!(*ocr_calls.lock().unwrap(), 1);

    // Invoice fields match the canned OCR result
    assert_eq!(invoice.vendor_name, "ACME");
    assert_eq!(invoice.invoice_number, "INV-1");
    assert_eq!(invoice.total_amount, Money::usd(150.00));
    assert_eq!(invoice.line_items.len(), 2);

    // ocr_confidence = average of four field confidences:
    // invoice_number=0.95, invoice_date=0.0, vendor_name=0.90, total_amount=0.88
    let expected_confidence = (0.95f32 + 0.0 + 0.90 + 0.88) / 4.0;
    let actual = invoice.ocr_confidence.expect("should have ocr_confidence");
    assert!(
        (actual - expected_confidence).abs() < 0.001,
        "expected confidence ~{}, got {}",
        expected_confidence,
        actual,
    );
}

#[tokio::test]
async fn upload_invoice_missing_fields_uses_defaults() {
    let ocr_result = OcrExtractionResult {
        invoice_number: ExtractedField::empty(),
        invoice_date: ExtractedField::empty(),
        due_date: ExtractedField::empty(),
        vendor_name: ExtractedField::empty(),
        vendor_address: ExtractedField::empty(),
        subtotal: ExtractedField::empty(),
        tax_amount: ExtractedField::empty(),
        total_amount: ExtractedField::empty(),
        currency: ExtractedField::empty(),
        po_number: ExtractedField::empty(),
        line_items: vec![],
        raw_text: String::new(),
        processing_time_ms: 50,
    };

    let ocr_call_count = Arc::new(Mutex::new(0usize));
    let ocr = FakeOcr::new(ocr_result, ocr_call_count.clone());
    let storage = Arc::new(FakeStorage::new());
    let repo = Arc::new(FakeInvoiceRepo::new());

    let service = InvoiceCaptureService::with_provider(
        Box::new(ocr),
        repo.clone(),
        storage.clone(),
    );

    let tenant_id = TenantId::new();
    let user_id = UserId::new();

    let invoice = service
        .upload_invoice(&tenant_id, &user_id, "blank.pdf", b"empty", "application/pdf")
        .await
        .expect("upload_invoice should succeed with empty OCR");

    // Fallback vendor name
    assert_eq!(invoice.vendor_name, "Unknown Vendor");

    // Fallback invoice number starts with UNKNOWN- prefix
    assert!(
        invoice.invoice_number.starts_with("UNKNOWN-"),
        "expected UNKNOWN-<uuid>, got {}",
        invoice.invoice_number,
    );

    // Fallback total is 0.00 USD
    assert_eq!(invoice.total_amount, Money::usd(0.0));
    assert_eq!(invoice.currency, "USD");
    assert!(invoice.line_items.is_empty());
}

#[tokio::test]
async fn reprocess_ocr_transitions_status() {
    let ocr_call_count = Arc::new(Mutex::new(0usize));
    let ocr = FakeOcr::new(default_ocr_result(), ocr_call_count.clone());
    let storage = Arc::new(FakeStorage::new());
    let repo = Arc::new(FakeInvoiceRepo::new());

    // Seed a pre-existing invoice and its document bytes
    let doc_id = Uuid::new_v4();
    let invoice_id = InvoiceId::new();
    let tenant_id = TenantId::new();
    let user_id = UserId::new();

    storage.seed(doc_id, b"stored-pdf-data".to_vec());

    let seed_invoice = Invoice {
        id: invoice_id.clone(),
        tenant_id: tenant_id.clone(),
        vendor_id: None,
        vendor_name: "Old Vendor".to_string(),
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
        created_by: user_id,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    repo.seed(seed_invoice);

    let service = InvoiceCaptureService::with_provider(
        Box::new(ocr),
        repo.clone(),
        storage.clone(),
    );

    let result = service
        .reprocess_ocr(&tenant_id, &invoice_id)
        .await
        .expect("reprocess_ocr should succeed");

    // OCR was called
    assert_eq!(*ocr_call_count.lock().unwrap(), 1);

    // Storage download was called
    assert_eq!(storage.download_count(), 1);

    // Result is returned
    assert_eq!(result.vendor_name.value.as_deref(), Some("ACME"));

    // Status transitions: Processing -> ReadyForReview (in that order)
    let transitions = repo.status_transitions();
    assert_eq!(
        transitions,
        vec![CaptureStatus::Processing, CaptureStatus::ReadyForReview],
        "expected [Processing, ReadyForReview], got {:?}",
        transitions,
    );
}
