//! EDI document types (normalized JSON, not raw X12)
//!
//! These types represent EDI documents after the middleware has parsed
//! the raw X12 into structured JSON. Field names map to X12 segments
//! but use Rust conventions.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ──────────────────────────── Document Envelope ────────────────────────────

/// EDI document type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EdiDocumentType {
    /// X12 810 - Invoice
    #[serde(rename = "invoice_810")]
    Invoice810,
    /// X12 850 - Purchase Order
    #[serde(rename = "purchase_order_850")]
    PurchaseOrder850,
    /// X12 856 - Advance Ship Notice
    #[serde(rename = "ship_notice_856")]
    ShipNotice856,
    /// X12 997 - Functional Acknowledgment
    #[serde(rename = "functional_ack_997")]
    FunctionalAck997,
}

impl std::fmt::Display for EdiDocumentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invoice810 => write!(f, "invoice_810"),
            Self::PurchaseOrder850 => write!(f, "purchase_order_850"),
            Self::ShipNotice856 => write!(f, "ship_notice_856"),
            Self::FunctionalAck997 => write!(f, "functional_ack_997"),
        }
    }
}

/// Direction of an EDI document
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EdiDirection {
    Inbound,
    Outbound,
}

/// Processing status for an EDI document
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EdiDocumentStatus {
    Received,
    Processing,
    Mapped,
    Failed,
}

/// Functional acknowledgment status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AckStatus {
    Pending,
    Accepted,
    AcceptedWithErrors,
    Rejected,
}

// ──────────────────────────── 810 Invoice ────────────────────────────

/// EDI Invoice (normalized from X12 810)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdiInvoice {
    /// ISA sender qualifier + ID
    pub sender_id: String,
    /// ISA receiver qualifier + ID
    pub receiver_id: String,
    /// Interchange control number (ICN) for tracking
    pub interchange_control: String,
    /// Group control number
    pub group_control: Option<String>,
    /// Invoice number (BIG02)
    pub invoice_number: String,
    /// Invoice date (BIG01)
    pub invoice_date: NaiveDate,
    /// PO number reference (BIG04)
    pub po_number: Option<String>,
    /// Vendor/seller party (N1*SE)
    pub vendor: EdiParty,
    /// Bill-to party (N1*BT)
    pub bill_to: Option<EdiParty>,
    /// Remit-to party (N1*RI)
    pub remit_to: Option<EdiParty>,
    /// Ship-to party (N1*ST)
    pub ship_to: Option<EdiParty>,
    /// Invoice line items (IT1 segments)
    #[serde(default)]
    pub line_items: Vec<EdiLineItem>,
    /// Total invoice amount in cents (TDS01)
    pub total_amount_cents: i64,
    /// Currency code (default USD)
    pub currency: String,
    /// Payment terms
    pub terms: Option<EdiPaymentTerms>,
    /// Due date (if specified via DTM)
    pub due_date: Option<NaiveDate>,
    /// Freight/shipping charges (SAC segments)
    #[serde(default)]
    pub charges: Vec<EdiCharge>,
    /// Tax amount in cents
    pub tax_amount_cents: Option<i64>,
}

/// EDI party (N1 loop - name, address, identifiers)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdiParty {
    /// Party name (N1-02)
    pub name: String,
    /// Identification code qualifier (N1-03): DUNS, EIN, etc.
    pub id_qualifier: Option<String>,
    /// Identification code (N1-04)
    pub id_code: Option<String>,
    /// Address line 1 (N3-01)
    pub address_line1: Option<String>,
    /// Address line 2 (N3-02)
    pub address_line2: Option<String>,
    /// City (N4-01)
    pub city: Option<String>,
    /// State (N4-02)
    pub state: Option<String>,
    /// Postal code (N4-03)
    pub postal_code: Option<String>,
    /// Country (N4-04)
    pub country: Option<String>,
}

/// EDI line item (IT1 segment)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdiLineItem {
    /// Line number (IT1-01)
    pub line_number: u32,
    /// Quantity invoiced (IT1-02)
    pub quantity: f64,
    /// Unit of measure (IT1-03): EA, CS, LB, etc.
    pub unit_of_measure: String,
    /// Unit price in cents (IT1-04)
    pub unit_price_cents: i64,
    /// Product/service ID qualifier (IT1-06): UP=UPC, SK=SKU, VP=Vendor Part
    pub product_id_qualifier: Option<String>,
    /// Product/service ID (IT1-07)
    pub product_id: Option<String>,
    /// Description (PID segment)
    pub description: String,
    /// Line total in cents
    pub total_cents: i64,
}

/// EDI payment terms (ITD segment)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdiPaymentTerms {
    /// Terms type: "basic", "discount", "net"
    pub terms_type: String,
    /// Discount percentage (e.g., 2.0 for 2/10 net 30)
    pub discount_percent: Option<f64>,
    /// Discount due days
    pub discount_days: Option<u32>,
    /// Net due days
    pub net_days: Option<u32>,
    /// Description (e.g., "2/10 Net 30")
    pub description: Option<String>,
}

/// EDI charge/allowance (SAC segment)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdiCharge {
    /// Allowance or charge: "A" = allowance (discount), "C" = charge
    pub indicator: String,
    /// Code (e.g., "D240" = freight, "C310" = discount)
    pub code: Option<String>,
    /// Amount in cents
    pub amount_cents: i64,
    /// Description
    pub description: Option<String>,
}

// ──────────────────────────── 997 Functional Ack ────────────────────────────

/// Functional acknowledgment (X12 997)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdiFunctionalAck {
    /// Acknowledged group control number
    pub group_control: String,
    /// Acknowledged transaction set control number
    pub transaction_control: Option<String>,
    /// Overall status
    pub status: AckStatus,
    /// Error codes (if rejected)
    pub errors: Vec<EdiAckError>,
}

/// Ack error detail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdiAckError {
    /// Error code
    pub code: String,
    /// Segment position
    pub segment_position: Option<u32>,
    /// Description
    pub description: String,
}

// ──────────────────────────── Webhook Payload ────────────────────────────

/// Inbound webhook payload from EDI middleware
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdiWebhookPayload {
    /// Event type: "document.received", "ack.received"
    pub event_type: String,
    /// Document type
    pub document_type: EdiDocumentType,
    /// Direction
    pub direction: EdiDirection,
    /// The parsed document (specific type depends on document_type)
    pub payload: serde_json::Value,
    /// Middleware-assigned document ID
    pub middleware_id: Option<String>,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

// ──────────────────────────── Stored Document ────────────────────────────

/// EDI document record (stored in database)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdiDocument {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub document_type: EdiDocumentType,
    pub direction: EdiDirection,
    pub interchange_control: Option<String>,
    pub sender_id: Option<String>,
    pub receiver_id: Option<String>,
    pub status: EdiDocumentStatus,
    pub invoice_id: Option<Uuid>,
    pub raw_payload: serde_json::Value,
    pub mapped_data: Option<serde_json::Value>,
    pub error_message: Option<String>,
    pub ack_status: Option<AckStatus>,
    pub ack_received_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub processed_at: Option<DateTime<Utc>>,
}

/// Trading partner record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdiTradingPartner {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub edi_qualifier: Option<String>,
    pub edi_id: String,
    pub vendor_id: Option<Uuid>,
    pub is_active: bool,
    pub settings: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
