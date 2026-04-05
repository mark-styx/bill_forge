//! Vendor statement reconciliation domain model

use crate::types::TenantId;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a vendor statement
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VendorStatementId(pub Uuid);

impl VendorStatementId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for VendorStatementId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for VendorStatementId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for VendorStatementId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Status of a vendor statement in the reconciliation workflow
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatementStatus {
    Pending,
    InReview,
    Reconciled,
    Disputed,
}

impl StatementStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InReview => "in_review",
            Self::Reconciled => "reconciled",
            Self::Disputed => "disputed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "in_review" => Some(Self::InReview),
            "reconciled" => Some(Self::Reconciled),
            "disputed" => Some(Self::Disputed),
            _ => None,
        }
    }
}

impl Default for StatementStatus {
    fn default() -> Self {
        Self::Pending
    }
}

/// Match status of a statement line item
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineMatchStatus {
    Unmatched,
    Matched,
    Discrepancy,
    Ignored,
}

impl LineMatchStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unmatched => "unmatched",
            Self::Matched => "matched",
            Self::Discrepancy => "discrepancy",
            Self::Ignored => "ignored",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "unmatched" => Some(Self::Unmatched),
            "matched" => Some(Self::Matched),
            "discrepancy" => Some(Self::Discrepancy),
            "ignored" => Some(Self::Ignored),
            _ => None,
        }
    }
}

impl Default for LineMatchStatus {
    fn default() -> Self {
        Self::Unmatched
    }
}

/// Type of a statement line item
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineType {
    Invoice,
    Credit,
    Payment,
    Adjustment,
}

impl LineType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Invoice => "invoice",
            Self::Credit => "credit",
            Self::Payment => "payment",
            Self::Adjustment => "adjustment",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "invoice" => Some(Self::Invoice),
            "credit" => Some(Self::Credit),
            "payment" => Some(Self::Payment),
            "adjustment" => Some(Self::Adjustment),
            _ => None,
        }
    }
}

impl Default for LineType {
    fn default() -> Self {
        Self::Invoice
    }
}

/// Confidence level of an auto-match result
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchConfidence {
    Exact,
    AmountOnly,
    NoMatch,
}

/// Vendor statement entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorStatement {
    pub id: VendorStatementId,
    pub tenant_id: TenantId,
    pub vendor_id: Uuid,
    pub statement_number: Option<String>,
    pub statement_date: Option<NaiveDate>,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub opening_balance_cents: i64,
    pub closing_balance_cents: i64,
    pub currency: String,
    pub status: StatementStatus,
    pub reconciled_by: Option<Uuid>,
    pub reconciled_at: Option<DateTime<Utc>>,
    pub notes: Option<String>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Vendor statement line item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatementLineItem {
    pub id: Uuid,
    pub statement_id: Uuid,
    pub tenant_id: TenantId,
    pub line_date: NaiveDate,
    pub description: String,
    pub reference_number: Option<String>,
    pub amount_cents: i64,
    pub line_type: LineType,
    pub match_status: LineMatchStatus,
    pub matched_invoice_id: Option<Uuid>,
    pub variance_cents: i64,
    pub matched_at: Option<DateTime<Utc>>,
    pub matched_by: Option<String>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Input for creating a new vendor statement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateStatementInput {
    pub vendor_id: Uuid,
    pub statement_number: Option<String>,
    pub statement_date: Option<NaiveDate>,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub opening_balance_cents: i64,
    pub closing_balance_cents: i64,
    pub currency: Option<String>,
    pub notes: Option<String>,
    pub lines: Vec<CreateStatementLineInput>,
}

/// Input for a single line item on a statement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateStatementLineInput {
    pub line_date: NaiveDate,
    pub description: String,
    pub reference_number: Option<String>,
    pub amount_cents: i64,
    pub line_type: Option<LineType>,
}

/// Input for manually updating a line's match
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateLineMatchInput {
    pub match_status: LineMatchStatus,
    pub matched_invoice_id: Option<Uuid>,
    pub notes: Option<String>,
}

/// Summary of reconciliation progress for a statement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconciliationSummary {
    pub total_lines: i64,
    pub matched: i64,
    pub unmatched: i64,
    pub discrepancies: i64,
    pub ignored: i64,
    pub total_variance_cents: i64,
}

/// Result of auto-matching a single line
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchResult {
    pub line_id: Uuid,
    pub confidence: MatchConfidence,
    pub matched_invoice_id: Option<Uuid>,
    pub variance_cents: i64,
    pub match_status: LineMatchStatus,
}

/// Minimal invoice representation used by the matching engine
#[derive(Debug, Clone)]
pub struct InvoiceSummary {
    pub id: Uuid,
    pub invoice_number: String,
    pub total_amount_cents: i64,
    pub invoice_date: Option<NaiveDate>,
    pub vendor_id: Option<Uuid>,
}

/// Auto-match statement lines against known invoices.
///
/// Strategy (prioritized per line):
/// 1. Exact match: reference_number == invoice_number AND amount_cents == total_amount_cents
/// 2. Discrepancy: reference_number == invoice_number BUT amount differs
/// 3. Amount-only match: amount_cents == total_amount_cents (no reference match)
/// 4. No match
///
/// Each invoice can only be matched to one statement line. Once consumed,
/// the invoice is removed from the candidate pool for subsequent lines.
pub fn auto_match_lines(
    lines: &[StatementLineItem],
    invoices: &[InvoiceSummary],
) -> Vec<MatchResult> {
    let mut results = Vec::with_capacity(lines.len());
    // Track consumed invoices by their id to prevent duplicate matching.
    let mut consumed: std::collections::HashSet<Uuid> = std::collections::HashSet::new();

    for line in lines {
        // Skip already matched/ignored lines
        if line.match_status != LineMatchStatus::Unmatched {
            continue;
        }

        let mut best_exact: Option<&InvoiceSummary> = None;
        let mut best_amount: Option<&InvoiceSummary> = None;
        let mut discrepancy: Option<&InvoiceSummary> = None;

        let ref_num = line.reference_number.as_deref().unwrap_or("");

        for inv in invoices {
            // Skip invoices already matched to a previous line
            if consumed.contains(&inv.id) {
                continue;
            }

            // Check reference number match (case-insensitive)
            let ref_matches = !ref_num.is_empty()
                && ref_num.eq_ignore_ascii_case(&inv.invoice_number);

            if ref_matches {
                if line.amount_cents == inv.total_amount_cents {
                    // Exact match on number + amount
                    best_exact = Some(inv);
                    break;
                } else {
                    // Number matches but amount differs - discrepancy
                    discrepancy = Some(inv);
                }
            } else if line.amount_cents == inv.total_amount_cents && best_amount.is_none() {
                // Amount-only match (pick first unconsumed)
                best_amount = Some(inv);
            }
        }

        let result = if let Some(inv) = best_exact {
            consumed.insert(inv.id);
            MatchResult {
                line_id: line.id,
                confidence: MatchConfidence::Exact,
                matched_invoice_id: Some(inv.id),
                variance_cents: 0,
                match_status: LineMatchStatus::Matched,
            }
        } else if let Some(inv) = discrepancy {
            consumed.insert(inv.id);
            MatchResult {
                line_id: line.id,
                confidence: MatchConfidence::Exact,
                matched_invoice_id: Some(inv.id),
                variance_cents: line.amount_cents - inv.total_amount_cents,
                match_status: LineMatchStatus::Discrepancy,
            }
        } else if let Some(inv) = best_amount {
            consumed.insert(inv.id);
            MatchResult {
                line_id: line.id,
                confidence: MatchConfidence::AmountOnly,
                matched_invoice_id: Some(inv.id),
                variance_cents: 0,
                match_status: LineMatchStatus::Matched,
            }
        } else {
            MatchResult {
                line_id: line.id,
                confidence: MatchConfidence::NoMatch,
                matched_invoice_id: None,
                variance_cents: 0,
                match_status: LineMatchStatus::Unmatched,
            }
        };

        results.push(result);
    }

    results
}

/// Compute a reconciliation summary from line items
pub fn compute_reconciliation_summary(lines: &[StatementLineItem]) -> ReconciliationSummary {
    let total_lines = lines.len() as i64;
    let matched = lines.iter().filter(|l| l.match_status == LineMatchStatus::Matched).count() as i64;
    let unmatched = lines.iter().filter(|l| l.match_status == LineMatchStatus::Unmatched).count() as i64;
    let discrepancies = lines.iter().filter(|l| l.match_status == LineMatchStatus::Discrepancy).count() as i64;
    let ignored = lines.iter().filter(|l| l.match_status == LineMatchStatus::Ignored).count() as i64;
    let total_variance_cents: i64 = lines
        .iter()
        .map(|l| l.variance_cents.abs())
        .sum();

    ReconciliationSummary {
        total_lines,
        matched,
        unmatched,
        discrepancies,
        ignored,
        total_variance_cents,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn make_line(id: &str, ref_num: Option<&str>, amount: i64) -> StatementLineItem {
        StatementLineItem {
            id: Uuid::parse_str(id).unwrap(),
            statement_id: Uuid::new_v4(),
            tenant_id: TenantId::new(),
            line_date: NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
            description: "Test line".to_string(),
            reference_number: ref_num.map(|s| s.to_string()),
            amount_cents: amount,
            line_type: LineType::Invoice,
            match_status: LineMatchStatus::Unmatched,
            matched_invoice_id: None,
            variance_cents: 0,
            matched_at: None,
            matched_by: None,
            notes: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn make_invoice(id: &str, number: &str, amount: i64) -> InvoiceSummary {
        InvoiceSummary {
            id: Uuid::parse_str(id).unwrap(),
            invoice_number: number.to_string(),
            total_amount_cents: amount,
            invoice_date: Some(NaiveDate::from_ymd_opt(2024, 1, 10).unwrap()),
            vendor_id: Some(Uuid::new_v4()),
        }
    }

    #[test]
    fn test_exact_match_number_and_amount() {
        let lines = vec![make_line("00000000-0000-0000-0000-000000000001", Some("INV-001"), 10000)];
        let invoices = vec![make_invoice("11111111-1111-1111-1111-111111111111", "INV-001", 10000)];

        let results = auto_match_lines(&lines, &invoices);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].confidence, MatchConfidence::Exact);
        assert_eq!(results[0].match_status, LineMatchStatus::Matched);
        assert_eq!(results[0].variance_cents, 0);
        assert!(results[0].matched_invoice_id.is_some());
    }

    #[test]
    fn test_discrepancy_number_matches_amount_differs() {
        let lines = vec![make_line("00000000-0000-0000-0000-000000000001", Some("INV-001"), 12000)];
        let invoices = vec![make_invoice("11111111-1111-1111-1111-111111111111", "INV-001", 10000)];

        let results = auto_match_lines(&lines, &invoices);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].match_status, LineMatchStatus::Discrepancy);
        assert_eq!(results[0].variance_cents, 2000);
    }

    #[test]
    fn test_amount_only_match() {
        let lines = vec![make_line("00000000-0000-0000-0000-000000000001", Some("UNKNOWN-REF"), 10000)];
        let invoices = vec![make_invoice("11111111-1111-1111-1111-111111111111", "INV-001", 10000)];

        let results = auto_match_lines(&lines, &invoices);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].confidence, MatchConfidence::AmountOnly);
        assert_eq!(results[0].match_status, LineMatchStatus::Matched);
    }

    #[test]
    fn test_no_match() {
        let lines = vec![make_line("00000000-0000-0000-0000-000000000001", Some("NOPE-001"), 99999)];
        let invoices = vec![make_invoice("11111111-1111-1111-1111-111111111111", "INV-001", 10000)];

        let results = auto_match_lines(&lines, &invoices);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].confidence, MatchConfidence::NoMatch);
        assert_eq!(results[0].match_status, LineMatchStatus::Unmatched);
        assert!(results[0].matched_invoice_id.is_none());
    }

    #[test]
    fn test_skips_already_matched_lines() {
        let mut line = make_line("00000000-0000-0000-0000-000000000001", Some("INV-001"), 10000);
        line.match_status = LineMatchStatus::Matched;
        let lines = vec![line];
        let invoices = vec![make_invoice("11111111-1111-1111-1111-111111111111", "INV-001", 10000)];

        let results = auto_match_lines(&lines, &invoices);
        assert!(results.is_empty());
    }

    #[test]
    fn test_no_duplicate_invoice_matching() {
        // Three lines each for $500, three distinct $500 invoices.
        // Each line should match a different invoice, not all match the same one.
        let lines = vec![
            make_line("00000000-0000-0000-0000-000000000001", Some("REF-A"), 500),
            make_line("00000000-0000-0000-0000-000000000002", Some("REF-B"), 500),
            make_line("00000000-0000-0000-0000-000000000003", Some("REF-C"), 500),
        ];
        let invoices = vec![
            make_invoice("11111111-1111-1111-1111-111111111111", "INV-001", 500),
            make_invoice("22222222-2222-2222-2222-222222222222", "INV-002", 500),
            make_invoice("33333333-3333-3333-3333-333333333333", "INV-003", 500),
        ];

        let results = auto_match_lines(&lines, &invoices);
        assert_eq!(results.len(), 3);

        // Each line should match a distinct invoice (amount-only match)
        let matched_ids: std::collections::HashSet<Uuid> = results
            .iter()
            .filter_map(|r| r.matched_invoice_id)
            .collect();
        assert_eq!(matched_ids.len(), 3, "Each line should match a different invoice");
    }

    #[test]
    fn test_consumed_invoices_prevent_reuse() {
        // Two lines with same reference, two invoices. First line gets exact match,
        // second should not re-match the consumed invoice.
        let lines = vec![
            make_line("00000000-0000-0000-0000-000000000001", Some("INV-001"), 10000),
            make_line("00000000-0000-0000-0000-000000000002", Some("INV-001"), 10000),
        ];
        let invoices = vec![
            make_invoice("11111111-1111-1111-1111-111111111111", "INV-001", 10000),
        ];

        let results = auto_match_lines(&lines, &invoices);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].confidence, MatchConfidence::Exact);
        assert_eq!(results[1].confidence, MatchConfidence::NoMatch);
    }

    #[test]
    fn test_compute_summary() {
        let lines = vec![
            {
                let mut l = make_line("00000000-0000-0000-0000-000000000001", None, 100);
                l.match_status = LineMatchStatus::Matched;
                l
            },
            {
                let mut l = make_line("00000000-0000-0000-0000-000000000002", None, 200);
                l.match_status = LineMatchStatus::Unmatched;
                l
            },
            {
                let mut l = make_line("00000000-0000-0000-0000-000000000003", None, 300);
                l.match_status = LineMatchStatus::Discrepancy;
                l.variance_cents = -50;
                l
            },
            {
                let mut l = make_line("00000000-0000-0000-0000-000000000004", None, 400);
                l.match_status = LineMatchStatus::Ignored;
                l
            },
        ];

        let summary = compute_reconciliation_summary(&lines);
        assert_eq!(summary.total_lines, 4);
        assert_eq!(summary.matched, 1);
        assert_eq!(summary.unmatched, 1);
        assert_eq!(summary.discrepancies, 1);
        assert_eq!(summary.ignored, 1);
        assert_eq!(summary.total_variance_cents, 50); // abs(-50)
    }
}
