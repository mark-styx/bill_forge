//! Tesseract OCR implementation using command-line tesseract
//!
//! This implementation calls the `tesseract` command-line tool, which must be
//! installed on the system. For development, you can install it via:
//! - macOS: `brew install tesseract`
//! - Ubuntu: `apt-get install tesseract-ocr`
//! - Windows: Download from https://github.com/UB-Mannheim/tesseract/wiki
//!
//! For production, consider using AWS Textract or Google Cloud Vision for
//! better accuracy and structured data extraction.

use async_trait::async_trait;
use billforge_core::{
    domain::{ExtractedField, ExtractedLineItem, OcrExtractionResult},
    traits::OcrService,
    Error, Result,
};
use chrono::NaiveDate;
use regex::Regex;
use std::io::Write;
use std::process::Command;
use std::time::Instant;
use tempfile::NamedTempFile;
use tokio::process::Command as TokioCommand;

/// Tesseract OCR provider using command-line interface
pub struct TesseractOcr {
    /// Path to tesseract binary (default: "tesseract")
    tesseract_path: String,
    /// Language(s) for OCR (default: "eng")
    language: String,
}

impl TesseractOcr {
    pub fn new() -> Self {
        Self {
            tesseract_path: std::env::var("TESSERACT_PATH")
                .unwrap_or_else(|_| "tesseract".to_string()),
            language: std::env::var("TESSERACT_LANG")
                .unwrap_or_else(|_| "eng".to_string()),
        }
    }

    /// Check if tesseract is available on the system
    pub fn is_available() -> bool {
        Command::new("tesseract")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Run OCR on an image file and return the text
    async fn ocr_image(&self, image_data: &[u8], mime_type: &str) -> Result<String> {
        // Determine file extension
        let extension = match mime_type {
            "image/png" => "png",
            "image/jpeg" | "image/jpg" => "jpg",
            "image/tiff" => "tiff",
            "application/pdf" => "pdf",
            _ => "png", // Default to png
        };

        // Write image to temp file
        let mut input_file = NamedTempFile::with_suffix(&format!(".{}", extension))
            .map_err(|e| Error::Ocr(format!("Failed to create temp file: {}", e)))?;

        input_file.write_all(image_data)
            .map_err(|e| Error::Ocr(format!("Failed to write temp file: {}", e)))?;

        let input_path = input_file.path().to_str()
            .ok_or_else(|| Error::Ocr("Invalid temp file path".to_string()))?;

        // Create output file
        let output_file = NamedTempFile::new()
            .map_err(|e| Error::Ocr(format!("Failed to create output file: {}", e)))?;
        let output_base = output_file.path().to_str()
            .ok_or_else(|| Error::Ocr("Invalid output path".to_string()))?;

        // Run tesseract
        let output = TokioCommand::new(&self.tesseract_path)
            .arg(input_path)
            .arg(output_base)
            .arg("-l")
            .arg(&self.language)
            .arg("--psm")
            .arg("3") // Fully automatic page segmentation
            .output()
            .await
            .map_err(|e| Error::Ocr(format!("Failed to run tesseract: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // If tesseract is not found, return helpful message
            if stderr.contains("not found") || stderr.contains("command not found") {
                return Err(Error::Ocr(
                    "Tesseract not found. Please install tesseract-ocr. \
                     On macOS: brew install tesseract. \
                     On Ubuntu: apt-get install tesseract-ocr".to_string()
                ));
            }
            return Err(Error::Ocr(format!("Tesseract failed: {}", stderr)));
        }

        // Read output text file
        let output_path = format!("{}.txt", output_base);
        let text = tokio::fs::read_to_string(&output_path)
            .await
            .map_err(|e| Error::Ocr(format!("Failed to read OCR output: {}", e)))?;

        // Clean up output file
        let _ = tokio::fs::remove_file(&output_path).await;

        Ok(text)
    }

    /// Parse extracted text to find invoice fields
    fn parse_invoice_data(&self, raw_text: &str) -> OcrExtractionResult {
        let lines: Vec<&str> = raw_text.lines().collect();

        OcrExtractionResult {
            invoice_number: self.extract_invoice_number(raw_text),
            invoice_date: self.extract_date(raw_text, &["invoice date", "date:", "inv date", "bill date"]),
            due_date: self.extract_date(raw_text, &["due date", "payment due", "due:", "pay by"]),
            vendor_name: self.extract_vendor_name(&lines),
            vendor_address: ExtractedField::empty(),
            subtotal: self.extract_amount(raw_text, &["subtotal", "sub total", "sub-total"]),
            tax_amount: self.extract_amount(raw_text, &["tax", "vat", "gst", "sales tax"]),
            total_amount: self.extract_amount(raw_text, &["total", "amount due", "grand total", "total due", "balance due"]),
            currency: self.extract_currency(raw_text),
            po_number: self.extract_po_number(raw_text),
            line_items: self.extract_line_items(raw_text),
            raw_text: raw_text.to_string(),
            processing_time_ms: 0,
        }
    }

    /// Extract invoice number using common patterns
    fn extract_invoice_number(&self, text: &str) -> ExtractedField<String> {
        let patterns = [
            r"(?i)invoice\s*#?\s*:?\s*([A-Z0-9\-]+)",
            r"(?i)inv\s*#?\s*:?\s*([A-Z0-9\-]+)",
            r"(?i)invoice\s+number\s*:?\s*([A-Z0-9\-]+)",
            r"(?i)bill\s*#?\s*:?\s*([A-Z0-9\-]+)",
            r"#\s*([A-Z0-9\-]{4,})",
        ];

        for pattern in patterns {
            if let Ok(re) = Regex::new(pattern) {
                if let Some(caps) = re.captures(text) {
                    if let Some(m) = caps.get(1) {
                        let value = m.as_str().trim().to_string();
                        if value.len() >= 3 {
                            return ExtractedField::with_value(value, 0.85);
                        }
                    }
                }
            }
        }

        ExtractedField::empty()
    }

    /// Extract date from text using common patterns
    fn extract_date(&self, text: &str, keywords: &[&str]) -> ExtractedField<NaiveDate> {
        let text_lower = text.to_lowercase();

        for keyword in keywords {
            if let Some(pos) = text_lower.find(keyword) {
                // Look for date pattern after the keyword
                let after_keyword = &text[pos..];
                if let Some(date) = self.parse_date_from_text(after_keyword) {
                    return ExtractedField::with_value(date, 0.8);
                }
            }
        }

        // Try to find any date in the text
        if let Some(date) = self.parse_date_from_text(text) {
            return ExtractedField::with_value(date, 0.5);
        }

        ExtractedField::empty()
    }

    /// Parse a date from text using common formats
    fn parse_date_from_text(&self, text: &str) -> Option<NaiveDate> {
        // Common date patterns
        let patterns = [
            // MM/DD/YYYY, MM-DD-YYYY
            (r"(\d{1,2})[/\-](\d{1,2})[/\-](\d{4})", "%m/%d/%Y"),
            // YYYY-MM-DD
            (r"(\d{4})[/\-](\d{1,2})[/\-](\d{1,2})", "%Y-%m-%d"),
        ];

        for (pattern, _format) in patterns {
            if let Ok(re) = Regex::new(pattern) {
                if let Some(caps) = re.captures(text) {
                    let date_str = caps.get(0).map(|m| m.as_str()).unwrap_or("");
                    // Try multiple formats
                    let formats = [
                        "%m/%d/%Y", "%m-%d-%Y", "%Y-%m-%d", "%Y/%m/%d",
                        "%d/%m/%Y", "%d-%m-%Y",
                    ];
                    for fmt in formats {
                        if let Ok(date) = NaiveDate::parse_from_str(date_str, fmt) {
                            return Some(date);
                        }
                    }
                }
            }
        }

        None
    }

    /// Extract vendor name (usually at the top of the invoice)
    fn extract_vendor_name(&self, lines: &[&str]) -> ExtractedField<String> {
        // The vendor name is typically in the first few non-empty lines
        let mut candidates = Vec::new();

        for (i, line) in lines.iter().take(10).enumerate() {
            let trimmed = line.trim();
            // Skip empty lines and lines that look like addresses or numbers
            if trimmed.is_empty() { continue; }
            if trimmed.chars().all(|c| c.is_numeric() || c == '-' || c == '/') { continue; }
            if trimmed.to_lowercase().contains("invoice") { continue; }
            if trimmed.to_lowercase().contains("bill to") { continue; }
            if trimmed.starts_with('$') { continue; }

            // Prefer lines that look like company names (contains letters, not too long)
            if trimmed.len() > 3 && trimmed.len() < 60 {
                let has_upper = trimmed.chars().any(|c| c.is_uppercase());
                let has_lower = trimmed.chars().any(|c| c.is_lowercase());
                if has_upper || has_lower {
                    candidates.push((i, trimmed.to_string()));
                }
            }
        }

        if let Some((_, name)) = candidates.first() {
            return ExtractedField::with_value(name.clone(), 0.7);
        }

        ExtractedField::empty()
    }

    /// Extract a monetary amount near specific keywords
    fn extract_amount(&self, text: &str, keywords: &[&str]) -> ExtractedField<f64> {
        let text_lower = text.to_lowercase();

        for keyword in keywords {
            if let Some(pos) = text_lower.find(keyword) {
                // Look for amount pattern after the keyword
                let after_keyword = &text[pos..];
                if let Some(amount) = self.parse_amount_from_text(after_keyword) {
                    return ExtractedField::with_value(amount, 0.75);
                }
            }
        }

        ExtractedField::empty()
    }

    /// Parse a monetary amount from text
    fn parse_amount_from_text(&self, text: &str) -> Option<f64> {
        // Match currency amounts like $1,234.56 or 1234.56 or USD 1,234.56
        let patterns = [
            r"\$\s*([\d,]+\.?\d*)",
            r"USD\s*([\d,]+\.?\d*)",
            r"([\d,]+\.\d{2})",
        ];

        for pattern in patterns {
            if let Ok(re) = Regex::new(pattern) {
                if let Some(caps) = re.captures(text) {
                    if let Some(m) = caps.get(1) {
                        let amount_str = m.as_str().replace(',', "");
                        if let Ok(amount) = amount_str.parse::<f64>() {
                            return Some(amount);
                        }
                    }
                }
            }
        }

        None
    }

    /// Extract currency code
    fn extract_currency(&self, text: &str) -> ExtractedField<String> {
        let text_upper = text.to_uppercase();

        // Look for common currency indicators
        if text.contains('$') || text_upper.contains("USD") {
            return ExtractedField::with_value("USD".to_string(), 0.9);
        }
        if text.contains('€') || text_upper.contains("EUR") {
            return ExtractedField::with_value("EUR".to_string(), 0.9);
        }
        if text.contains('£') || text_upper.contains("GBP") {
            return ExtractedField::with_value("GBP".to_string(), 0.9);
        }

        // Default to USD for US-centric AP systems
        ExtractedField::with_value("USD".to_string(), 0.5)
    }

    /// Extract PO number
    fn extract_po_number(&self, text: &str) -> ExtractedField<String> {
        let patterns = [
            r"(?i)P\.?O\.?\s*#?\s*:?\s*([A-Z0-9\-]+)",
            r"(?i)purchase\s+order\s*#?\s*:?\s*([A-Z0-9\-]+)",
            r"(?i)PO\s+number\s*:?\s*([A-Z0-9\-]+)",
        ];

        for pattern in patterns {
            if let Ok(re) = Regex::new(pattern) {
                if let Some(caps) = re.captures(text) {
                    if let Some(m) = caps.get(1) {
                        let value = m.as_str().trim().to_string();
                        if value.len() >= 3 {
                            return ExtractedField::with_value(value, 0.8);
                        }
                    }
                }
            }
        }

        ExtractedField::empty()
    }

    /// Extract line items (basic implementation)
    fn extract_line_items(&self, text: &str) -> Vec<ExtractedLineItem> {
        // Line item extraction is complex and would benefit from
        // structured extraction (AWS Textract, Google Document AI)
        // This is a basic implementation that looks for patterns like:
        // "Description    Qty    Price    Amount"

        let mut items = Vec::new();

        // Look for lines that have description + amount pattern
        let line_pattern = Regex::new(r"^(.+?)\s+(\d+(?:\.\d+)?)\s+\$?([\d,]+\.?\d*)\s+\$?([\d,]+\.?\d*)$").ok();

        if let Some(re) = line_pattern {
            for line in text.lines() {
                if let Some(caps) = re.captures(line.trim()) {
                    let description = caps.get(1).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
                    let qty = caps.get(2).and_then(|m| m.as_str().parse::<f64>().ok());
                    let unit_price = caps.get(3).and_then(|m| m.as_str().replace(',', "").parse::<f64>().ok());
                    let amount = caps.get(4).and_then(|m| m.as_str().replace(',', "").parse::<f64>().ok());

                    if description.len() > 2 {
                        items.push(ExtractedLineItem {
                            description: ExtractedField::with_value(description, 0.6),
                            quantity: qty.map(|q| ExtractedField::with_value(q, 0.7)).unwrap_or_else(ExtractedField::empty),
                            unit_price: unit_price.map(|p| ExtractedField::with_value(p, 0.7)).unwrap_or_else(ExtractedField::empty),
                            amount: amount.map(|a| ExtractedField::with_value(a, 0.7)).unwrap_or_else(ExtractedField::empty),
                        });
                    }
                }
            }
        }

        items
    }
}

impl Default for TesseractOcr {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl OcrService for TesseractOcr {
    async fn extract(&self, document_bytes: &[u8], mime_type: &str) -> Result<OcrExtractionResult> {
        let start = Instant::now();

        // Check if tesseract is available
        if !Self::is_available() {
            tracing::warn!("Tesseract not available, using mock extraction");
            // Return mock data for development when tesseract isn't installed
            let result = OcrExtractionResult {
                invoice_number: ExtractedField::with_value(format!("MOCK-{}", uuid::Uuid::new_v4().to_string()[..8].to_uppercase()), 0.5),
                invoice_date: ExtractedField::empty(),
                due_date: ExtractedField::empty(),
                vendor_name: ExtractedField::with_value("Mock Vendor (Tesseract not installed)".to_string(), 0.3),
                vendor_address: ExtractedField::empty(),
                subtotal: ExtractedField::empty(),
                tax_amount: ExtractedField::empty(),
                total_amount: ExtractedField::with_value(0.0, 0.1),
                currency: ExtractedField::with_value("USD".to_string(), 0.9),
                po_number: ExtractedField::empty(),
                line_items: Vec::new(),
                raw_text: "Tesseract OCR is not installed. Install with: brew install tesseract (macOS) or apt-get install tesseract-ocr (Ubuntu)".to_string(),
                processing_time_ms: start.elapsed().as_millis() as u64,
            };
            return Ok(result);
        }

        // Run OCR
        let raw_text = self.ocr_image(document_bytes, mime_type).await?;

        // Parse the extracted text
        let mut result = self.parse_invoice_data(&raw_text);
        result.processing_time_ms = start.elapsed().as_millis() as u64;

        tracing::info!(
            processing_time_ms = result.processing_time_ms,
            text_length = raw_text.len(),
            invoice_number = ?result.invoice_number.value,
            vendor_name = ?result.vendor_name.value,
            total_amount = ?result.total_amount.value,
            "OCR extraction completed"
        );

        Ok(result)
    }

    fn supported_formats(&self) -> Vec<&'static str> {
        vec!["application/pdf", "image/png", "image/jpeg", "image/tiff", "image/bmp"]
    }

    fn provider_name(&self) -> &'static str {
        "tesseract"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_invoice_number() {
        let ocr = TesseractOcr::new();

        let text = "INVOICE #12345\nDate: 01/15/2024";
        let result = ocr.extract_invoice_number(text);
        assert_eq!(result.value, Some("12345".to_string()));

        let text = "Invoice Number: ABC-789";
        let result = ocr.extract_invoice_number(text);
        assert_eq!(result.value, Some("ABC-789".to_string()));
    }

    #[test]
    fn test_parse_amount() {
        let ocr = TesseractOcr::new();

        let amount = ocr.parse_amount_from_text("Total: $1,234.56");
        assert_eq!(amount, Some(1234.56));

        let amount = ocr.parse_amount_from_text("Amount Due: 5678.90");
        assert_eq!(amount, Some(5678.90));
    }

    #[test]
    fn test_extract_po_number() {
        let ocr = TesseractOcr::new();

        let text = "P.O. #: PO-2024-001";
        let result = ocr.extract_po_number(text);
        assert_eq!(result.value, Some("PO-2024-001".to_string()));

        let text = "Purchase Order Number: ABC123";
        let result = ocr.extract_po_number(text);
        assert_eq!(result.value, Some("ABC123".to_string()));
    }
}
