//! 3-way match engine: PO (850) vs Receiving (856) vs Invoice (810)
//!
//! Compares invoice line items against the original purchase order and
//! receiving records to determine match quality. Results drive the
//! approval workflow:
//!
//! - FullMatch    -> auto-approve (if under tolerance thresholds)
//! - PartialMatch -> route to exception queue for review
//! - NoMatch      -> flag for manual PO assignment
//! - OverBilled   -> block, require manager approval

use billforge_core::domain::{
    MatchTolerances, MatchType, POLineItem, ReceivingLineItem,
};
use serde::{Deserialize, Serialize};

/// Per-line match detail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineMatchDetail {
    pub po_line_number: u32,
    pub po_quantity: f64,
    pub po_unit_price_cents: i64,
    pub received_quantity: f64,
    pub invoiced_quantity: f64,
    pub invoiced_unit_price_cents: i64,
    pub quantity_variance_pct: f64,
    pub price_variance_pct: f64,
    pub line_match: MatchType,
}

/// Overall match result from the engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchOutput {
    pub match_type: MatchType,
    pub overall_price_variance_pct: f64,
    pub overall_quantity_variance_pct: f64,
    pub line_details: Vec<LineMatchDetail>,
    pub unmatched_invoice_lines: Vec<u32>,
    pub unmatched_po_lines: Vec<u32>,
}

/// Lightweight representation of an invoice line for matching purposes.
/// Avoids coupling the match engine to the full Invoice/InvoiceLineItem types.
#[derive(Debug, Clone)]
pub struct InvoiceLineForMatch {
    pub line_number: u32,
    pub quantity: f64,
    pub unit_price_cents: i64,
    pub product_id: Option<String>,
}

/// The match engine performs 3-way matching.
pub struct MatchEngine;

impl MatchEngine {
    /// Run a 3-way match: PO lines vs receiving lines vs invoice lines.
    ///
    /// If `receiving_lines` is empty, falls back to 2-way (PO vs Invoice).
    pub fn run(
        po_lines: &[POLineItem],
        receiving_lines: &[ReceivingLineItem],
        invoice_lines: &[InvoiceLineForMatch],
        tolerances: &MatchTolerances,
    ) -> MatchOutput {
        let mut line_details = Vec::new();
        let mut matched_invoice_indices: Vec<usize> = Vec::new();
        let mut unmatched_po_lines = Vec::new();
        let has_receiving = !receiving_lines.is_empty();

        for po_line in po_lines {
            // Find corresponding invoice line (one-to-one: skip already-matched)
            let inv_match = find_invoice_line(invoice_lines, po_line, &matched_invoice_indices);

            let received_qty = if !has_receiving {
                // No receiving data: 2-way match, assume received = ordered
                po_line.quantity
            } else {
                receiving_lines
                    .iter()
                    .filter(|r| r.po_line_number == po_line.line_number)
                    .map(|r| r.quantity_received)
                    .sum()
            };

            // When receiving data exists, use received_qty as the quantity
            // baseline instead of the PO ordered quantity. This prevents
            // auto-approving invoices for goods that haven't been received.
            let qty_baseline = if has_receiving { received_qty } else { po_line.quantity };

            match inv_match {
                Some((idx, il)) => {
                    let qty_var = variance_pct(qty_baseline, il.quantity);
                    let price_var = variance_pct(
                        po_line.unit_price.amount as f64,
                        il.unit_price_cents as f64,
                    );

                    let line_match = classify_line(qty_var, price_var, il.quantity, qty_baseline, tolerances);

                    line_details.push(LineMatchDetail {
                        po_line_number: po_line.line_number,
                        po_quantity: po_line.quantity,
                        po_unit_price_cents: po_line.unit_price.amount,
                        received_quantity: received_qty,
                        invoiced_quantity: il.quantity,
                        invoiced_unit_price_cents: il.unit_price_cents,
                        quantity_variance_pct: qty_var,
                        price_variance_pct: price_var,
                        line_match,
                    });

                    matched_invoice_indices.push(idx);
                }
                None => {
                    unmatched_po_lines.push(po_line.line_number);
                }
            }
        }

        let unmatched_invoice_lines: Vec<u32> = invoice_lines
            .iter()
            .enumerate()
            .filter(|(i, _)| !matched_invoice_indices.contains(i))
            .map(|(_, il)| il.line_number)
            .collect();

        // Determine overall match type
        let (overall_match, overall_price_var, overall_qty_var) =
            classify_overall(&line_details, &unmatched_po_lines, &unmatched_invoice_lines, tolerances);

        MatchOutput {
            match_type: overall_match,
            overall_price_variance_pct: overall_price_var,
            overall_quantity_variance_pct: overall_qty_var,
            line_details,
            unmatched_invoice_lines,
            unmatched_po_lines,
        }
    }
}

/// Find the best matching invoice line for a PO line.
/// Matches first by product_id, then falls back to line_number.
/// Returns (index, line) and skips already-matched indices for one-to-one matching.
fn find_invoice_line<'a>(
    invoice_lines: &'a [InvoiceLineForMatch],
    po_line: &POLineItem,
    matched_indices: &[usize],
) -> Option<(usize, &'a InvoiceLineForMatch)> {
    // Try product_id match first
    if let Some(ref po_product) = po_line.product_id {
        if let Some((idx, il)) = invoice_lines.iter().enumerate().find(|(i, il)| {
            !matched_indices.contains(i)
                && il.product_id.as_deref() == Some(po_product.as_str())
        }) {
            return Some((idx, il));
        }
    }
    // Fall back to line number match
    invoice_lines
        .iter()
        .enumerate()
        .find(|(i, il)| !matched_indices.contains(i) && il.line_number == po_line.line_number)
}

/// Calculate percentage variance between expected and actual values.
/// Returns 0.0 if expected is 0.
fn variance_pct(expected: f64, actual: f64) -> f64 {
    if expected == 0.0 {
        if actual == 0.0 {
            return 0.0;
        }
        return 100.0;
    }
    ((actual - expected) / expected * 100.0).abs()
}

/// Classify a single line match
fn classify_line(
    qty_var: f64,
    price_var: f64,
    invoiced_qty: f64,
    po_qty: f64,
    tolerances: &MatchTolerances,
) -> MatchType {
    // Over-billed: invoiced quantity or price exceeds PO by more than tolerance
    if invoiced_qty > po_qty && qty_var > tolerances.quantity_variance_pct {
        return MatchType::OverBilled;
    }

    if qty_var <= tolerances.quantity_variance_pct
        && price_var <= tolerances.price_variance_pct
    {
        MatchType::Full
    } else {
        MatchType::Partial
    }
}

/// Classify the overall match result across all lines
fn classify_overall(
    line_details: &[LineMatchDetail],
    unmatched_po: &[u32],
    unmatched_inv: &[u32],
    _tolerances: &MatchTolerances,
) -> (MatchType, f64, f64) {
    if line_details.is_empty() {
        return (MatchType::None, 0.0, 0.0);
    }

    // If there are unmatched lines on either side, it's at best partial
    let has_unmatched = !unmatched_po.is_empty() || !unmatched_inv.is_empty();

    let avg_price_var = if line_details.is_empty() {
        0.0
    } else {
        line_details.iter().map(|d| d.price_variance_pct).sum::<f64>()
            / line_details.len() as f64
    };

    let avg_qty_var = if line_details.is_empty() {
        0.0
    } else {
        line_details.iter().map(|d| d.quantity_variance_pct).sum::<f64>()
            / line_details.len() as f64
    };

    // Check if any line is over-billed
    if line_details.iter().any(|d| d.line_match == MatchType::OverBilled) {
        return (MatchType::OverBilled, avg_price_var, avg_qty_var);
    }

    if has_unmatched {
        return (MatchType::Partial, avg_price_var, avg_qty_var);
    }

    // All lines matched - check if all are full matches
    if line_details.iter().all(|d| d.line_match == MatchType::Full) {
        (MatchType::Full, avg_price_var, avg_qty_var)
    } else {
        (MatchType::Partial, avg_price_var, avg_qty_var)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use billforge_core::Money;

    fn make_po_line(num: u32, qty: f64, price_cents: i64, product_id: Option<&str>) -> POLineItem {
        POLineItem {
            id: uuid::Uuid::new_v4(),
            line_number: num,
            description: format!("Item {}", num),
            quantity: qty,
            unit_of_measure: "EA".to_string(),
            unit_price: Money::new(price_cents, "USD"),
            total: Money::new((qty as i64) * price_cents, "USD"),
            product_id: product_id.map(|s| s.to_string()),
            received_quantity: 0.0,
            invoiced_quantity: 0.0,
        }
    }

    fn make_inv_line(num: u32, qty: f64, price_cents: i64, product_id: Option<&str>) -> InvoiceLineForMatch {
        InvoiceLineForMatch {
            line_number: num,
            quantity: qty,
            unit_price_cents: price_cents,
            product_id: product_id.map(|s| s.to_string()),
        }
    }

    fn make_recv_line(po_line: u32, qty: f64) -> ReceivingLineItem {
        ReceivingLineItem {
            id: uuid::Uuid::new_v4(),
            po_line_number: po_line,
            quantity_received: qty,
            quantity_damaged: 0.0,
            product_id: None,
        }
    }

    #[test]
    fn test_full_match() {
        let po = vec![
            make_po_line(1, 100.0, 1500, Some("SKU-A")),
            make_po_line(2, 50.0, 2500, Some("SKU-B")),
        ];
        let recv = vec![make_recv_line(1, 100.0), make_recv_line(2, 50.0)];
        let inv = vec![
            make_inv_line(1, 100.0, 1500, Some("SKU-A")),
            make_inv_line(2, 50.0, 2500, Some("SKU-B")),
        ];

        let result = MatchEngine::run(&po, &recv, &inv, &MatchTolerances::default());
        assert_eq!(result.match_type, MatchType::Full);
        assert!(result.unmatched_po_lines.is_empty());
        assert!(result.unmatched_invoice_lines.is_empty());
    }

    #[test]
    fn test_partial_match_price_variance() {
        let tolerances = MatchTolerances {
            price_variance_pct: 2.0,
            quantity_variance_pct: 5.0,
            auto_approve_below_cents: 100_000,
        };

        let po = vec![make_po_line(1, 100.0, 1000, None)];
        let inv = vec![make_inv_line(1, 100.0, 1050, None)]; // 5% price increase

        let result = MatchEngine::run(&po, &[], &inv, &tolerances);
        assert_eq!(result.match_type, MatchType::Partial);
        assert!(result.line_details[0].price_variance_pct > 2.0);
    }

    #[test]
    fn test_over_billed() {
        let tolerances = MatchTolerances::default();
        let po = vec![make_po_line(1, 100.0, 1000, None)];
        // Invoiced 120 units when PO was for 100 - 20% over
        let inv = vec![make_inv_line(1, 120.0, 1000, None)];

        let result = MatchEngine::run(&po, &[], &inv, &tolerances);
        assert_eq!(result.match_type, MatchType::OverBilled);
    }

    #[test]
    fn test_no_match_empty_po() {
        let inv = vec![make_inv_line(1, 100.0, 1000, None)];
        let result = MatchEngine::run(&[], &[], &inv, &MatchTolerances::default());
        assert_eq!(result.match_type, MatchType::None);
    }

    #[test]
    fn test_unmatched_invoice_lines() {
        let po = vec![make_po_line(1, 100.0, 1000, None)];
        let inv = vec![
            make_inv_line(1, 100.0, 1000, None),
            make_inv_line(2, 50.0, 500, None), // extra line not on PO
        ];

        let result = MatchEngine::run(&po, &[], &inv, &MatchTolerances::default());
        assert_eq!(result.match_type, MatchType::Partial);
        assert_eq!(result.unmatched_invoice_lines, vec![2]);
    }

    #[test]
    fn test_within_tolerance() {
        let tolerances = MatchTolerances {
            price_variance_pct: 5.0,
            quantity_variance_pct: 5.0,
            auto_approve_below_cents: 100_000,
        };

        let po = vec![make_po_line(1, 100.0, 1000, None)];
        // 3% quantity under, 2% price over - both within 5% tolerance
        let inv = vec![make_inv_line(1, 97.0, 1020, None)];

        let result = MatchEngine::run(&po, &[], &inv, &tolerances);
        assert_eq!(result.match_type, MatchType::Full);
    }

    #[test]
    fn test_3way_with_receiving() {
        let po = vec![make_po_line(1, 100.0, 1000, None)];
        let recv = vec![make_recv_line(1, 100.0)];
        let inv = vec![make_inv_line(1, 100.0, 1000, None)];

        let result = MatchEngine::run(&po, &recv, &inv, &MatchTolerances::default());
        assert_eq!(result.match_type, MatchType::Full);
        assert_eq!(result.line_details[0].received_quantity, 100.0);
    }

    #[test]
    fn test_partial_received_prevents_full_match() {
        // PO for 100, only 50 received, invoice for 100
        // Should be OverBilled because invoiced qty (100) > received qty (50)
        let po = vec![make_po_line(1, 100.0, 1000, None)];
        let recv = vec![make_recv_line(1, 50.0)];
        let inv = vec![make_inv_line(1, 100.0, 1000, None)];

        let result = MatchEngine::run(&po, &recv, &inv, &MatchTolerances::default());
        // Invoicing for 100 when only 50 received = 100% over received qty
        assert_eq!(result.match_type, MatchType::OverBilled);
    }

    #[test]
    fn test_one_to_one_matching_no_double_match() {
        // Two PO lines for the same SKU, but only one invoice line
        let po = vec![
            make_po_line(1, 50.0, 1000, Some("SKU-X")),
            make_po_line(2, 50.0, 1000, Some("SKU-X")),
        ];
        let inv = vec![make_inv_line(1, 50.0, 1000, Some("SKU-X"))];

        let result = MatchEngine::run(&po, &[], &inv, &MatchTolerances::default());
        // One PO line matched, one unmatched
        assert_eq!(result.unmatched_po_lines.len(), 1);
        assert!(result.unmatched_invoice_lines.is_empty());
        assert_eq!(result.match_type, MatchType::Partial);
    }
}
