//! Mobile API DTOs (Data Transfer Objects)
//!
//! Lightweight structures optimized for mobile bandwidth constraints

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Mobile-optimized invoice summary (50-70% smaller than full Invoice)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileInvoiceSummary {
    pub id: Uuid,
    pub vendor_name: String,
    pub invoice_number: String,
    pub total_amount_cents: i64,
    pub currency: String,
    pub due_date: Option<NaiveDate>,
    pub status: MobileInvoiceStatus,
    pub days_until_due: Option<i32>,
    pub requires_action: bool,
    pub created_at: DateTime<Utc>,
}

/// Simplified invoice status for mobile
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MobileInvoiceStatus {
    PendingReview,
    PendingApproval,
    Approved,
    Processing,
    Paid,
    Cancelled,
    Rejected,
}

impl MobileInvoiceStatus {
    pub fn from_processing_status(s: &str) -> Self {
        match s {
            "draft" => Self::PendingReview,
            "pending_approval" => Self::PendingApproval,
            "approved" => Self::Approved,
            "processing" => Self::Processing,
            "paid" => Self::Paid,
            "cancelled" => Self::Cancelled,
            "rejected" => Self::Rejected,
            _ => Self::PendingReview,
        }
    }
}

/// Mobile approval request with lightweight invoice data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileApprovalRequest {
    pub id: Uuid,
    pub invoice: MobileInvoiceSummary,
    pub requested_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub can_approve: bool,
}

/// Mobile dashboard with summary metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileDashboard {
    pub pending_approvals: u32,
    pub pending_review: u32,
    pub requires_attention: u32,
    pub upcoming_due_dates: Vec<MobileInvoiceSummary>,
    pub recent_activity: Vec<MobileActivityItem>,
}

/// Activity item for mobile dashboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileActivityItem {
    pub id: Uuid,
    pub activity_type: MobileActivityType,
    pub title: String,
    pub description: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MobileActivityType {
    InvoiceUploaded,
    ApprovalRequested,
    ApprovalCompleted,
    InvoicePaid,
    CommentAdded,
}

/// Mobile vendor summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileVendorSummary {
    pub id: Uuid,
    pub name: String,
    pub total_invoices: u32,
    pub total_amount_cents: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mobile_invoice_status_from_processing_status() {
        assert_eq!(
            MobileInvoiceStatus::from_processing_status("draft"),
            MobileInvoiceStatus::PendingReview
        );
        assert_eq!(
            MobileInvoiceStatus::from_processing_status("paid"),
            MobileInvoiceStatus::Paid
        );
        assert_eq!(
            MobileInvoiceStatus::from_processing_status("unknown"),
            MobileInvoiceStatus::PendingReview
        );
    }

    #[test]
    fn test_mobile_invoice_summary_serialization() {
        let summary = MobileInvoiceSummary {
            id: Uuid::new_v4(),
            vendor_name: "Acme Corp".to_string(),
            invoice_number: "INV-001".to_string(),
            total_amount_cents: 150050,
            currency: "USD".to_string(),
            due_date: Some(chrono::Utc::now().date_naive()),
            status: MobileInvoiceStatus::PendingApproval,
            days_until_due: Some(7),
            requires_action: true,
            created_at: Utc::now(),
        };

        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("Acme Corp"));
        assert!(json.contains("INV-001"));
        assert!(json.contains("pending_approval"));
    }

    #[test]
    fn test_mobile_dashboard() {
        let dashboard = MobileDashboard {
            pending_approvals: 5,
            pending_review: 3,
            requires_attention: 8,
            upcoming_due_dates: vec![],
            recent_activity: vec![],
        };

        assert_eq!(dashboard.pending_approvals, 5);
        assert_eq!(dashboard.requires_attention, 8);
    }
}
