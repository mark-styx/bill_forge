//! Vendor statement ingestion from inbound email.
//!
//! Classifies inbound emails as statements vs invoices, parses statement
//! attachment text into structured line items, persists the statement via
//! direct SQL, and runs the existing auto-match engine against the tenant
//! ledger.

use billforge_core::domain::vendor_statement::{
    auto_match_lines, InvoiceSummary, LineMatchStatus, LineType, MatchConfidence, StatementLineItem,
};
use billforge_core::types::TenantId;
use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

/// Classification of an inbound email's document type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InboundKind {
    Invoice,
    Statement,
}

/// Keyword list used to detect vendor statements.
const STATEMENT_KEYWORDS: &[&str] = &[
    "statement",
    "account summary",
    "aging report",
    "statement of account",
];

/// Classify an inbound email as Invoice or Statement based on subject line
/// and attachment filenames.
///
/// Matches case-insensitively against `STATEMENT_KEYWORDS` in the subject
/// first, then in each attachment filename. Falls back to `InboundKind::Invoice`.
pub fn classify(subject: &str, attachment_names: &[&str]) -> InboundKind {
    let subject_lower = subject.to_lowercase();
    for kw in STATEMENT_KEYWORDS {
        if subject_lower.contains(kw) {
            return InboundKind::Statement;
        }
    }

    for name in attachment_names {
        let name_lower = name.to_lowercase().replace('_', " ");
        for kw in STATEMENT_KEYWORDS {
            if name_lower.contains(kw) {
                return InboundKind::Statement;
            }
        }
    }

    InboundKind::Invoice
}

// ---------------------------------------------------------------------------
// Parsed line representation
// ---------------------------------------------------------------------------

/// A single line extracted from a vendor statement attachment.
#[derive(Debug, Clone)]
pub struct ParsedStatementLine {
    pub reference_number: String,
    pub line_date: NaiveDate,
    pub amount_cents: i64,
    pub description: String,
}

// ---------------------------------------------------------------------------
// Statement text parser
// ---------------------------------------------------------------------------

/// Header/footer phrases that indicate a non-data line.
const HEADER_FOOTER_PHRASES: &[&str] = &[
    "invoice number",
    "invoice date",
    "description",
    "total due",
    "balance forward",
    "page ",
    "continued",
    "account summary",
    "remit to",
    "thank you",
    "previous balance",
    "new charges",
    "payments applied",
];

/// Parse text content of a vendor statement attachment into structured lines.
///
/// Each line is split on whitespace. A valid data row must contain at least
/// a reference number, a date, and a monetary amount. The parser is tolerant
/// of currency symbols (`$`, `€`, `£`), thousands-separator commas, and
/// trailing description text after the amount.
///
/// Lines matching known header/footer phrases (without an amount) are skipped.
pub fn parse_statement_lines(text: &str) -> Vec<ParsedStatementLine> {
    let mut result = Vec::new();

    for raw_line in text.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let lower = trimmed.to_lowercase();

        // Skip obvious header / footer lines.
        if is_header_or_footer(&lower) {
            continue;
        }

        if let Some(parsed) = try_parse_data_line(trimmed) {
            result.push(parsed);
        }
    }

    result
}

fn is_header_or_footer(lower: &str) -> bool {
    let keyword_hits = HEADER_FOOTER_PHRASES
        .iter()
        .filter(|phrase| lower.contains(*phrase))
        .count();

    if keyword_hits >= 1 && !line_contains_amount(lower) {
        return true;
    }
    false
}

/// Returns true if the line has something that looks like a monetary amount.
fn line_contains_amount(text: &str) -> bool {
    if text.contains('$') || text.contains('€') || text.contains('£') {
        return true;
    }
    // Standalone decimal number, e.g. "123.45"
    for tok in text.split_whitespace() {
        let cleaned = tok.trim_end_matches(',');
        if cleaned.contains('.') && cleaned.parse::<f64>().is_ok() {
            return true;
        }
    }
    false
}

/// Attempt to extract `(reference, date, amount, description)` from a single
/// whitespace-split line.
fn try_parse_data_line(line: &str) -> Option<ParsedStatementLine> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 3 {
        return None;
    }

    // --- date ---
    let (date_idx, line_date) = find_date(&tokens)?;

    // --- amount ---
    let (amount_idx, amount_cents) = find_amount(&tokens)?;

    // --- reference number (first alphanumeric token that isn't date or amount) ---
    let reference_number = find_reference(&tokens, date_idx, amount_idx)?;

    // --- description: everything after the amount token ---
    let desc_start = amount_idx + 1;
    let description = if desc_start < tokens.len() {
        tokens[desc_start..].join(" ")
    } else {
        reference_number.clone()
    };

    Some(ParsedStatementLine {
        reference_number,
        line_date,
        amount_cents,
        description,
    })
}

// ---- date helpers ----

fn find_date(tokens: &[&str]) -> Option<(usize, NaiveDate)> {
    for (i, tok) in tokens.iter().enumerate() {
        let t = tok.trim_end_matches(',');
        if let Ok(d) = NaiveDate::parse_from_str(t, "%Y-%m-%d") {
            return Some((i, d));
        }
        if let Ok(d) = NaiveDate::parse_from_str(t, "%m/%d/%Y") {
            return Some((i, d));
        }
        if let Ok(d) = NaiveDate::parse_from_str(t, "%m-%d-%Y") {
            return Some((i, d));
        }
    }
    None
}

fn looks_like_date(s: &str) -> bool {
    let t = s.trim_end_matches(',');
    NaiveDate::parse_from_str(t, "%Y-%m-%d").is_ok()
        || NaiveDate::parse_from_str(t, "%m/%d/%Y").is_ok()
        || NaiveDate::parse_from_str(t, "%m-%d-%Y").is_ok()
}

// ---- amount helpers ----

fn find_amount(tokens: &[&str]) -> Option<(usize, i64)> {
    for (i, tok) in tokens.iter().enumerate() {
        if let Some(cents) = parse_amount_token(tok) {
            return Some((i, cents));
        }
    }
    None
}

/// Parse a single whitespace token as a monetary amount in cents.
/// Handles `$1,234.56`, `€100.00`, `(500.00)`, bare `1234.56`.
fn parse_amount_token(tok: &str) -> Option<i64> {
    let s = tok
        .trim_start_matches('$')
        .trim_start_matches('€')
        .trim_start_matches('£')
        .replace(',', "")
        .trim_end_matches(',')
        .to_string();

    let (digits, negate) = if s.starts_with('(') && s.ends_with(')') && s.len() > 2 {
        (&s[1..s.len() - 1], true)
    } else {
        (&s[..], false)
    };

    let val: f64 = digits.parse().ok()?;
    if val <= 0.0 {
        return None;
    }
    let cents = (val * 100.0).round() as i64;
    Some(if negate { -cents } else { cents })
}

fn looks_like_amount(s: &str) -> bool {
    parse_amount_token(s).is_some()
}

// ---- reference number helper ----

fn find_reference(tokens: &[&str], date_idx: usize, amount_idx: usize) -> Option<String> {
    for (i, tok) in tokens.iter().enumerate() {
        if i == date_idx || i == amount_idx {
            continue;
        }
        let t = tok.trim_end_matches(',');
        if t.chars().any(|c| c.is_alphanumeric()) && !looks_like_date(t) && !looks_like_amount(t) {
            return Some(t.to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Attachment text extraction
// ---------------------------------------------------------------------------

/// Extract text content from an attachment's raw bytes.
///
/// - Text-based content types (`text/plain`, `text/csv`, `application/csv`)
///   are decoded as UTF-8 directly.
/// - PDF content is processed with a lightweight heuristic that filters
///   printable character runs (suitable for text-based PDFs, not scanned
///   images which require the OCR pipeline).
pub fn extract_attachment_text(bytes: &[u8], content_type: &str) -> Option<String> {
    let ct = content_type.to_lowercase();

    if ct.contains("text/plain")
        || ct.contains("text/csv")
        || ct.contains("application/csv")
        || ct.contains("text/tab-separated-values")
    {
        return std::str::from_utf8(bytes).ok().map(|s| s.to_string());
    }

    if ct.contains("pdf") {
        return extract_pdf_text_heuristic(bytes);
    }

    None
}

/// Lightweight PDF text extraction: collects runs of printable ASCII
/// characters (code points 0x20..=0x7E) separated by short binary gaps.
/// This handles text-based PDFs but NOT scanned-image PDFs.
fn extract_pdf_text_heuristic(bytes: &[u8]) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut printable_run = 0;

    for &b in bytes {
        if (0x20..=0x7E).contains(&b) {
            current.push(b as char);
            printable_run += 1;
        } else if b == b'\n' || b == b'\r' {
            if printable_run >= 4 {
                lines.push(current.clone());
            }
            current.clear();
            printable_run = 0;
        } else {
            // Binary byte - may end the current run
            if printable_run >= 4 {
                current.push(' ');
            } else {
                current.clear();
            }
            printable_run = 0;
        }
    }
    if printable_run >= 4 && !current.trim().is_empty() {
        lines.push(current);
    }

    let text = lines.join("\n");
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

// ---------------------------------------------------------------------------
// Ingestion (persists + auto-matches)
// ---------------------------------------------------------------------------

/// Persist a vendor statement and its lines, then run auto-match against
/// the tenant ledger.
///
/// All SQL runs against the **tenant** pool. The `created_by` parameter must
/// reference a valid row in `users(id)` (FK constraint).
///
/// `source_email_id` is recorded in the `notes` field because the
/// `vendor_statements` table does not yet carry a dedicated
/// `source_email_id` column.
pub async fn ingest_statement(
    pool: &PgPool,
    tenant_id: &TenantId,
    vendor_id: Uuid,
    parsed_lines: &[ParsedStatementLine],
    source_email_id: Uuid,
    created_by: Uuid,
) -> Result<Uuid, String> {
    if parsed_lines.is_empty() {
        return Err("No statement lines to ingest".to_string());
    }

    let statement_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    // Derive period from line dates.
    let earliest = parsed_lines
        .iter()
        .map(|l| l.line_date)
        .min()
        .expect("parsed_lines is non-empty");
    let latest = parsed_lines
        .iter()
        .map(|l| l.line_date)
        .max()
        .expect("parsed_lines is non-empty");

    let closing_balance_cents: i64 = parsed_lines.iter().map(|l| l.amount_cents).sum();

    let notes = format!("Ingested from email {}", source_email_id);

    // ---- Insert statement row ----
    sqlx::query(
        r#"INSERT INTO vendor_statements
               (id, tenant_id, vendor_id, statement_number, statement_date,
                statement_period_start, statement_period_end,
                opening_balance_cents, closing_balance_cents, currency,
                status, notes, created_by, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)"#,
    )
    .bind(statement_id)
    .bind(tenant_id.as_uuid())
    .bind(vendor_id)
    .bind(Option::<String>::None) // statement_number
    .bind(latest) // statement_date
    .bind(earliest) // period_start
    .bind(latest) // period_end
    .bind(0_i64) // opening_balance_cents
    .bind(closing_balance_cents)
    .bind("USD")
    .bind("pending")
    .bind(&notes)
    .bind(created_by)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to create vendor statement: {}", e))?;

    // ---- Insert line items ----
    let mut line_ids = Vec::with_capacity(parsed_lines.len());
    for line in parsed_lines {
        let line_id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO vendor_statement_lines
                   (id, statement_id, tenant_id, line_date, description,
                    reference_number, amount_cents, line_type, match_status,
                    matched_by, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, 'invoice', 'unmatched', 'auto', $8, $9)"#,
        )
        .bind(line_id)
        .bind(statement_id)
        .bind(tenant_id.as_uuid())
        .bind(line.line_date)
        .bind(&line.description)
        .bind(&line.reference_number)
        .bind(line.amount_cents)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to create statement line: {}", e))?;

        line_ids.push(line_id);
    }

    // ---- Build in-memory StatementLineItem objects for the matcher ----
    let db_lines: Vec<StatementLineItem> = parsed_lines
        .iter()
        .zip(line_ids.iter())
        .map(|(line, &id)| StatementLineItem {
            id,
            statement_id,
            tenant_id: tenant_id.clone(),
            line_date: line.line_date,
            description: line.description.clone(),
            reference_number: Some(line.reference_number.clone()),
            amount_cents: line.amount_cents,
            line_type: LineType::Invoice,
            match_status: LineMatchStatus::Unmatched,
            matched_invoice_id: None,
            variance_cents: 0,
            matched_at: None,
            matched_by: None,
            notes: None,
            created_at: now,
            updated_at: now,
        })
        .collect();

    // ---- Query candidate invoices ----
    #[allow(clippy::type_complexity)]
    let invoice_rows: Vec<(Uuid, String, i64, Option<NaiveDate>, Option<Uuid>)> = sqlx::query_as(
        r#"SELECT id, invoice_number, total_amount_cents, invoice_date, vendor_id
               FROM invoices
               WHERE tenant_id = $1
                 AND vendor_id = $2
                 AND invoice_date >= $3
                 AND invoice_date <= $4"#,
    )
    .bind(tenant_id.as_uuid())
    .bind(vendor_id)
    .bind(earliest)
    .bind(latest)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to query invoices for matching: {}", e))?;

    let invoices: Vec<InvoiceSummary> = invoice_rows
        .into_iter()
        .map(
            |(id, invoice_number, total_amount_cents, invoice_date, vendor_id)| InvoiceSummary {
                id,
                invoice_number,
                total_amount_cents,
                invoice_date,
                vendor_id,
            },
        )
        .collect();

    // ---- Run auto-match ----
    let match_results = auto_match_lines(&db_lines, &invoices);

    // ---- Apply non-trivial results ----
    for mr in &match_results {
        if mr.confidence == MatchConfidence::NoMatch {
            continue;
        }
        sqlx::query(
            r#"UPDATE vendor_statement_lines
               SET matched_invoice_id = $1,
                   variance_cents     = $2,
                   match_status       = $3,
                   matched_by         = 'auto',
                   matched_at         = $4,
                   updated_at         = $5
               WHERE id = $6 AND tenant_id = $7"#,
        )
        .bind(mr.matched_invoice_id)
        .bind(mr.variance_cents)
        .bind(mr.match_status.as_str())
        .bind(now)
        .bind(now)
        .bind(mr.line_id)
        .bind(tenant_id.as_uuid())
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to apply match result: {}", e))?;
    }

    tracing::info!(
        statement_id = %statement_id,
        vendor_id = %vendor_id,
        lines = parsed_lines.len(),
        matched = match_results.iter().filter(|r| r.confidence != MatchConfidence::NoMatch).count(),
        "Vendor statement ingested from email"
    );

    Ok(statement_id)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ===== classify tests =====

    #[test]
    fn classify_detects_statement_from_subject() {
        assert_eq!(
            classify("April Statement of Account", &[]),
            InboundKind::Statement
        );
        assert_eq!(
            classify("Monthly account summary", &[]),
            InboundKind::Statement
        );
        assert_eq!(classify("Q1 aging report", &[]), InboundKind::Statement);
        assert_eq!(classify("Statement", &[]), InboundKind::Statement);
    }

    #[test]
    fn classify_detects_statement_from_filename() {
        assert_eq!(
            classify("Invoice", &["monthly-statement.pdf"]),
            InboundKind::Statement
        );
        assert_eq!(
            classify("Document", &["account_summary.csv"]),
            InboundKind::Statement
        );
    }

    #[test]
    fn classify_falls_back_to_invoice() {
        assert_eq!(
            classify("Invoice #1042", &["invoice.pdf"]),
            InboundKind::Invoice
        );
        assert_eq!(
            classify("Purchase order", &["po.xlsx"]),
            InboundKind::Invoice
        );
        assert_eq!(classify("", &[]), InboundKind::Invoice);
    }

    #[test]
    fn classify_is_case_insensitive() {
        assert_eq!(classify("APRIL STATEMENT", &[]), InboundKind::Statement);
        assert_eq!(
            classify("docs", &["STATEMENT_OF_ACCOUNT.PDF"]),
            InboundKind::Statement
        );
    }

    // ===== parse_statement_lines tests =====

    #[test]
    fn parse_extracts_invoice_amount_date_rows() {
        let text = "\
INV-001  2024-01-15  $1,250.00
INV-002  2024-02-10  500.00  Office supplies
INV-003  03/05/2024  €300.00";

        let lines = parse_statement_lines(text);
        assert_eq!(lines.len(), 3);

        assert_eq!(lines[0].reference_number, "INV-001");
        assert_eq!(
            lines[0].line_date,
            NaiveDate::from_ymd_opt(2024, 1, 15).unwrap()
        );
        assert_eq!(lines[0].amount_cents, 125000);

        assert_eq!(lines[1].reference_number, "INV-002");
        assert_eq!(lines[1].amount_cents, 50000);
        assert_eq!(lines[1].description, "Office supplies");

        assert_eq!(lines[2].reference_number, "INV-003");
        assert_eq!(lines[2].amount_cents, 30000);
    }

    #[test]
    fn parse_ignores_header_and_footer_lines() {
        let text = "\
Invoice Number    Date        Amount
------------------------------------
INV-001  2024-01-15  $100.00
Total Due: $100.00
Thank you for your business
Remit to: PO Box 123";

        let lines = parse_statement_lines(text);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].reference_number, "INV-001");
        assert_eq!(lines[0].amount_cents, 10000);
    }

    #[test]
    fn parse_handles_various_date_formats() {
        let text = "\
REF-A  2024-01-15  100.00
REF-B  01/20/2024  200.00
REF-C  03-25-2024  300.00";

        let lines = parse_statement_lines(text);
        assert_eq!(lines.len(), 3);

        assert_eq!(
            lines[0].line_date,
            NaiveDate::from_ymd_opt(2024, 1, 15).unwrap()
        );
        assert_eq!(
            lines[1].line_date,
            NaiveDate::from_ymd_opt(2024, 1, 20).unwrap()
        );
        assert_eq!(
            lines[2].line_date,
            NaiveDate::from_ymd_opt(2024, 3, 25).unwrap()
        );
    }

    #[test]
    fn parse_handles_currency_symbols_and_commas() {
        let text = "INV-100  2024-06-01  $12,345.67";
        let lines = parse_statement_lines(text);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].amount_cents, 1234567);
    }

    #[test]
    fn parse_returns_empty_for_gibberish() {
        let text = "blah blah blah\nfoo bar baz";
        let lines = parse_statement_lines(text);
        assert!(lines.is_empty());
    }

    // ===== extract_attachment_text tests =====

    #[test]
    fn extract_text_from_plain_text() {
        let bytes = b"INV-001  2024-01-15  $100.00".to_vec();
        let text = extract_attachment_text(&bytes, "text/plain");
        assert!(text.is_some());
        assert!(text.unwrap().contains("INV-001"));
    }

    #[test]
    fn extract_text_from_csv() {
        let bytes = b"Reference,Date,Amount\nINV-001,2024-01-15,100.00".to_vec();
        let text = extract_attachment_text(&bytes, "text/csv");
        assert!(text.is_some());
    }

    #[test]
    fn extract_text_returns_none_for_unsupported_type() {
        let bytes = b"some image data".to_vec();
        let text = extract_attachment_text(&bytes, "image/png");
        assert!(text.is_none());
    }
}
