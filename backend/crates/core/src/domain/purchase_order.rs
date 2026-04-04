//! Purchase Order domain model

use crate::types::{Money, TenantId, UserId};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a purchase order
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PurchaseOrderId(pub Uuid);

impl PurchaseOrderId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for PurchaseOrderId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for PurchaseOrderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for PurchaseOrderId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Purchase order lifecycle status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum POStatus {
    /// PO created, awaiting fulfillment
    Open,
    /// Some line items received/invoiced
    PartiallyFulfilled,
    /// All line items received/invoiced
    Fulfilled,
    /// PO closed (complete)
    Closed,
    /// PO cancelled
    Cancelled,
}

impl POStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::PartiallyFulfilled => "partially_fulfilled",
            Self::Fulfilled => "fulfilled",
            Self::Closed => "closed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "open" => Some(Self::Open),
            "partially_fulfilled" => Some(Self::PartiallyFulfilled),
            "fulfilled" => Some(Self::Fulfilled),
            "closed" => Some(Self::Closed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

impl Default for POStatus {
    fn default() -> Self {
        Self::Open
    }
}

/// Core purchase order entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurchaseOrder {
    pub id: PurchaseOrderId,
    pub tenant_id: TenantId,
    pub po_number: String,
    pub vendor_id: Uuid,
    pub vendor_name: String,
    pub order_date: NaiveDate,
    pub expected_delivery: Option<NaiveDate>,
    pub status: POStatus,
    pub line_items: Vec<POLineItem>,
    pub total_amount: Money,
    pub ship_to_address: Option<String>,
    pub notes: Option<String>,
    pub created_by: UserId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Line item on a purchase order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct POLineItem {
    pub id: Uuid,
    pub line_number: u32,
    pub description: String,
    pub quantity: f64,
    pub unit_of_measure: String,
    pub unit_price: Money,
    pub total: Money,
    pub product_id: Option<String>,
    pub received_quantity: f64,
    pub invoiced_quantity: f64,
}

/// Input for creating a new purchase order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePurchaseOrderInput {
    pub po_number: String,
    pub vendor_id: Uuid,
    pub vendor_name: String,
    pub order_date: NaiveDate,
    pub expected_delivery: Option<NaiveDate>,
    pub line_items: Vec<CreatePOLineItemInput>,
    pub total_amount: Money,
    pub ship_to_address: Option<String>,
    pub notes: Option<String>,
}

/// Input for a PO line item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePOLineItemInput {
    /// External line number from EDI (PO1-01). Preserved as-is for ASN/invoice matching.
    pub line_number: Option<u32>,
    pub description: String,
    pub quantity: f64,
    pub unit_of_measure: String,
    pub unit_price: Money,
    pub total: Money,
    pub product_id: Option<String>,
}

/// Receiving record (from ASN 856 or manual receipt)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceivingRecord {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub po_id: PurchaseOrderId,
    pub received_date: NaiveDate,
    pub line_items: Vec<ReceivingLineItem>,
    pub edi_document_id: Option<Uuid>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Line item on a receiving record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceivingLineItem {
    pub id: Uuid,
    pub po_line_number: u32,
    pub quantity_received: f64,
    pub quantity_damaged: f64,
    pub product_id: Option<String>,
}

// ──────────────────────────── 3-Way Matching ────────────────────────────

/// Result of matching an invoice against PO and receiving data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchType {
    /// PO, receiving, and invoice all agree within tolerances
    Full,
    /// Some variances found but within review thresholds
    Partial,
    /// No matching PO found
    None,
    /// Invoice exceeds PO amount or quantity
    OverBilled,
}

impl MatchType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Partial => "partial",
            Self::None => "none",
            Self::OverBilled => "over_billed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "full" => Some(Self::Full),
            "partial" => Some(Self::Partial),
            "none" => Some(Self::None),
            "over_billed" => Some(Self::OverBilled),
            _ => None,
        }
    }
}

/// Stored match result linking invoice to PO
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchResult {
    pub id: Uuid,
    pub invoice_id: Uuid,
    pub po_id: Option<Uuid>,
    pub receiving_id: Option<Uuid>,
    pub match_type: MatchType,
    pub price_variance_pct: Option<f64>,
    pub quantity_variance_pct: Option<f64>,
    pub details: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// Configurable tolerances for 3-way matching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchTolerances {
    /// Max allowed price variance (e.g., 2.0 = 2%)
    pub price_variance_pct: f64,
    /// Max allowed quantity variance (e.g., 5.0 = 5%)
    pub quantity_variance_pct: f64,
    /// Auto-approve full matches under this amount (cents)
    pub auto_approve_below_cents: i64,
}

impl Default for MatchTolerances {
    fn default() -> Self {
        Self {
            price_variance_pct: 2.0,
            quantity_variance_pct: 5.0,
            auto_approve_below_cents: 100_000, // $1,000
        }
    }
}
