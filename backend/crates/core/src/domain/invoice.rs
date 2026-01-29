//! Invoice domain model

use crate::types::{Money, TenantId, UserId};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for an invoice
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InvoiceId(pub Uuid);

impl InvoiceId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for InvoiceId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for InvoiceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for InvoiceId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Status of an invoice in the capture phase
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureStatus {
    /// Just uploaded, not yet processed
    Pending,
    /// OCR processing in progress
    Processing,
    /// OCR complete, awaiting review
    ReadyForReview,
    /// User has reviewed and corrected data
    Reviewed,
    /// Data extraction failed
    Failed,
}

impl CaptureStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::ReadyForReview => "ready_for_review",
            Self::Reviewed => "reviewed",
            Self::Failed => "failed",
        }
    }
    
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "pending" => Some(Self::Pending),
            "processing" => Some(Self::Processing),
            "ready_for_review" => Some(Self::ReadyForReview),
            "reviewed" => Some(Self::Reviewed),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

impl Default for CaptureStatus {
    fn default() -> Self {
        Self::Pending
    }
}

/// Status of an invoice in the processing workflow
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingStatus {
    /// Invoice captured but not yet submitted for processing
    Draft,
    /// Submitted for processing, in initial review
    Submitted,
    /// Awaiting approval(s)
    PendingApproval,
    /// All approvals received
    Approved,
    /// Rejected by approver or AP user
    Rejected,
    /// On hold for clarification
    OnHold,
    /// Ready to be paid
    ReadyForPayment,
    /// Payment has been issued
    Paid,
    /// Invoice was voided/cancelled
    Voided,
}

impl ProcessingStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Submitted => "submitted",
            Self::PendingApproval => "pending_approval",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::OnHold => "on_hold",
            Self::ReadyForPayment => "ready_for_payment",
            Self::Paid => "paid",
            Self::Voided => "voided",
        }
    }
    
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "draft" => Some(Self::Draft),
            "submitted" => Some(Self::Submitted),
            "pending_approval" => Some(Self::PendingApproval),
            "approved" => Some(Self::Approved),
            "rejected" => Some(Self::Rejected),
            "on_hold" => Some(Self::OnHold),
            "ready_for_payment" => Some(Self::ReadyForPayment),
            "paid" => Some(Self::Paid),
            "voided" => Some(Self::Voided),
            _ => None,
        }
    }
}

impl Default for ProcessingStatus {
    fn default() -> Self {
        Self::Draft
    }
}

/// Core invoice entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invoice {
    pub id: InvoiceId,
    pub tenant_id: TenantId,
    
    // Vendor information
    pub vendor_id: Option<Uuid>,
    pub vendor_name: String,
    
    // Invoice details
    pub invoice_number: String,
    pub invoice_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    pub po_number: Option<String>,
    
    // Financial
    pub subtotal: Option<Money>,
    pub tax_amount: Option<Money>,
    pub total_amount: Money,
    pub currency: String,
    
    // Line items
    pub line_items: Vec<InvoiceLineItem>,
    
    // Status tracking
    pub capture_status: CaptureStatus,
    pub processing_status: ProcessingStatus,
    
    // Queue tracking
    /// Current queue the invoice is in (None = not in workflow yet)
    pub current_queue_id: Option<Uuid>,
    /// User currently assigned to this invoice
    pub assigned_to: Option<UserId>,
    
    // Document reference
    /// Primary document (PDF/image) for this invoice
    pub document_id: Uuid,
    /// Additional supporting documents
    pub supporting_documents: Vec<Uuid>,
    pub ocr_confidence: Option<f32>,
    
    // Department/GL coding
    pub department: Option<String>,
    pub gl_code: Option<String>,
    pub cost_center: Option<String>,
    
    // Metadata
    pub notes: Option<String>,
    pub tags: Vec<String>,
    pub custom_fields: serde_json::Value,
    
    // Audit
    pub created_by: UserId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Line item on an invoice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceLineItem {
    pub id: Uuid,
    pub line_number: u32,
    pub description: String,
    pub quantity: Option<f64>,
    pub unit_price: Option<Money>,
    pub amount: Money,
    pub gl_code: Option<String>,
    pub department: Option<String>,
    pub project: Option<String>,
}

/// OCR extraction result for an invoice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrExtractionResult {
    pub invoice_number: ExtractedField<String>,
    pub invoice_date: ExtractedField<NaiveDate>,
    pub due_date: ExtractedField<NaiveDate>,
    pub vendor_name: ExtractedField<String>,
    pub vendor_address: ExtractedField<String>,
    pub subtotal: ExtractedField<f64>,
    pub tax_amount: ExtractedField<f64>,
    pub total_amount: ExtractedField<f64>,
    pub currency: ExtractedField<String>,
    pub po_number: ExtractedField<String>,
    pub line_items: Vec<ExtractedLineItem>,
    pub raw_text: String,
    pub processing_time_ms: u64,
}

/// A field extracted by OCR with confidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedField<T> {
    pub value: Option<T>,
    pub confidence: f32,
    pub bounding_box: Option<BoundingBox>,
    pub source_text: Option<String>,
}

impl<T> ExtractedField<T> {
    pub fn empty() -> Self {
        Self {
            value: None,
            confidence: 0.0,
            bounding_box: None,
            source_text: None,
        }
    }

    pub fn with_value(value: T, confidence: f32) -> Self {
        Self {
            value: Some(value),
            confidence,
            bounding_box: None,
            source_text: None,
        }
    }
}

/// Line item extracted by OCR
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedLineItem {
    pub description: ExtractedField<String>,
    pub quantity: ExtractedField<f64>,
    pub unit_price: ExtractedField<f64>,
    pub amount: ExtractedField<f64>,
}

/// Bounding box for extracted text
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub page: u32,
}

/// Input for creating a new invoice from OCR
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateInvoiceInput {
    pub document_id: Uuid,
    pub vendor_id: Option<Uuid>,
    pub vendor_name: String,
    pub invoice_number: String,
    pub invoice_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    pub po_number: Option<String>,
    pub subtotal: Option<Money>,
    pub tax_amount: Option<Money>,
    pub total_amount: Money,
    pub currency: String,
    pub line_items: Vec<CreateLineItemInput>,
    pub ocr_confidence: Option<f32>,
    pub department: Option<String>,
    pub gl_code: Option<String>,
    pub cost_center: Option<String>,
    pub notes: Option<String>,
    pub tags: Vec<String>,
}

/// Input for updating an invoice (all fields optional)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateInvoiceInput {
    pub vendor_id: Option<Option<Uuid>>,
    pub vendor_name: Option<String>,
    pub invoice_number: Option<String>,
    pub invoice_date: Option<Option<NaiveDate>>,
    pub due_date: Option<Option<NaiveDate>>,
    pub po_number: Option<Option<String>>,
    pub subtotal: Option<Option<Money>>,
    pub tax_amount: Option<Option<Money>>,
    pub total_amount: Option<Money>,
    pub currency: Option<String>,
    pub line_items: Option<Vec<CreateLineItemInput>>,
    pub department: Option<Option<String>>,
    pub gl_code: Option<Option<String>>,
    pub cost_center: Option<Option<String>>,
    pub notes: Option<Option<String>>,
    pub tags: Option<Vec<String>>,
    pub custom_fields: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateLineItemInput {
    pub description: String,
    pub quantity: Option<f64>,
    pub unit_price: Option<Money>,
    pub amount: Money,
    pub gl_code: Option<String>,
    pub department: Option<String>,
    pub project: Option<String>,
}

/// Filters for querying invoices
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InvoiceFilters {
    pub vendor_id: Option<Uuid>,
    pub capture_status: Option<CaptureStatus>,
    pub processing_status: Option<ProcessingStatus>,
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
    pub amount_min: Option<f64>,
    pub amount_max: Option<f64>,
    pub search: Option<String>,
    pub tags: Vec<String>,
}
