//! Invoice capture service

use crate::calibration::{calibrated_confidence, OcrCalibrationStore};
use crate::ocr::{self, OcrComparison};
use billforge_core::{
    domain::{
        CaptureStatus, CreateInvoiceInput, CreateLineItemInput, Invoice, OcrExtractionResult,
    },
    traits::{InvoiceRepository, OcrService, StorageService},
    types::{Money, TenantId, TenantSettings, UserId},
    Result,
};
use std::sync::Arc;
use uuid::Uuid;

/// OCR confidence required before an invoice can continue through straight-through processing.
pub const OCR_EXCEPTION_REVIEW_CONFIDENCE_THRESHOLD: f32 = 0.90;
pub const OCR_HARD_FAIL_CONFIDENCE_THRESHOLD: f32 = 0.30;
pub const LOCAL_OCR_PROVIDER: &str = "tesseract";

/// Field names tracked for OCR confidence calibration.
pub const OCR_CALIBRATED_FIELDS: &[&str] = &[
    "invoice_number",
    "invoice_date",
    "vendor_name",
    "total_amount",
];

/// Whitelist of field names accepted by the correction endpoint.
const VALID_CORRECTION_FIELDS: &[&str] = &[
    "invoice_number",
    "invoice_date",
    "vendor_name",
    "total_amount",
];

/// Resolve the OCR provider for a tenant, enforcing local-only privacy settings.
pub fn resolve_ocr_provider_name(global_provider: &str, settings: &TenantSettings) -> String {
    if settings.features.local_ocr_required {
        LOCAL_OCR_PROVIDER.to_string()
    } else {
        settings
            .ocr_provider
            .as_deref()
            .filter(|provider| !provider.trim().is_empty())
            .unwrap_or(global_provider)
            .to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrRoutingDecision {
    Error,
    ExceptionReview,
    StraightThrough,
}

pub fn ocr_routing_decision(confidence: Option<f32>) -> OcrRoutingDecision {
    match confidence {
        Some(confidence) if confidence < OCR_HARD_FAIL_CONFIDENCE_THRESHOLD => {
            OcrRoutingDecision::Error
        }
        Some(confidence) if confidence >= OCR_EXCEPTION_REVIEW_CONFIDENCE_THRESHOLD => {
            OcrRoutingDecision::StraightThrough
        }
        _ => OcrRoutingDecision::ExceptionReview,
    }
}

/// Service for capturing and processing invoices
pub struct InvoiceCaptureService {
    ocr_provider: Box<dyn OcrService>,
    invoice_repo: Arc<dyn InvoiceRepository>,
    storage: Arc<dyn StorageService>,
    calibration: Option<Arc<dyn OcrCalibrationStore>>,
    comparison: Option<Arc<OcrComparison>>,
}

impl InvoiceCaptureService {
    pub fn new(
        ocr_provider_name: &str,
        invoice_repo: Arc<dyn InvoiceRepository>,
        storage: Arc<dyn StorageService>,
    ) -> Self {
        Self {
            ocr_provider: ocr::create_provider(ocr_provider_name),
            invoice_repo,
            storage,
            calibration: None,
            comparison: None,
        }
    }

    /// Construct with an injected OCR provider (for testing)
    pub fn with_provider(
        ocr_provider: Box<dyn OcrService>,
        invoice_repo: Arc<dyn InvoiceRepository>,
        storage: Arc<dyn StorageService>,
    ) -> Self {
        Self {
            ocr_provider,
            invoice_repo,
            storage,
            calibration: None,
            comparison: None,
        }
    }

    /// Attach a calibration store for weighted confidence scoring.
    pub fn with_calibration(mut self, store: Arc<dyn OcrCalibrationStore>) -> Self {
        self.calibration = Some(store);
        self
    }

    /// Attach an OCR comparison engine for multi-provider extraction.
    pub fn with_comparison(mut self, comparison: Arc<OcrComparison>) -> Self {
        self.comparison = Some(comparison);
        self
    }

    /// Run OCR extraction, using multi-provider comparison when configured.
    ///
    /// Returns the extraction result and the name of the provider that produced it.
    async fn run_ocr(&self, bytes: &[u8], mime: &str) -> Result<(OcrExtractionResult, String)> {
        if let Some(ref comparison) = self.comparison {
            let cmp_result = comparison.compare(bytes, mime).await?;
            let best_key = &cmp_result.best_provider;
            if let Some(pr) = cmp_result.providers.get(best_key) {
                if let Some(ref extraction) = pr.result {
                    tracing::info!(
                        best_provider = %best_key,
                        "OCR comparison selected best provider"
                    );
                    return Ok((extraction.clone(), best_key.clone()));
                }
            }
            // All providers in comparison failed; fall back to default provider
            tracing::warn!("All comparison providers failed, falling back to default provider");
            let extraction = self.ocr_provider.extract(bytes, mime).await?;
            Ok((extraction, self.ocr_provider.provider_name().to_string()))
        } else {
            let extraction = self.ocr_provider.extract(bytes, mime).await?;
            Ok((extraction, self.ocr_provider.provider_name().to_string()))
        }
    }

    /// Upload and process a new invoice document
    pub async fn upload_invoice(
        &self,
        tenant_id: &TenantId,
        user_id: &UserId,
        file_name: &str,
        file_bytes: &[u8],
        mime_type: &str,
    ) -> Result<Invoice> {
        // 1. Store the document
        let document_id = self
            .storage
            .upload(tenant_id, file_name, file_bytes, mime_type)
            .await?;

        // 2. Run OCR (single provider or multi-provider comparison)
        let (ocr_result, _best_provider) = self.run_ocr(file_bytes, mime_type).await?;

        // 3. Create invoice from OCR result
        let invoice = self
            .create_invoice_from_ocr(tenant_id, user_id, document_id, &ocr_result)
            .await?;

        Ok(invoice)
    }

    /// Create an invoice record from OCR extraction result
    async fn create_invoice_from_ocr(
        &self,
        tenant_id: &TenantId,
        user_id: &UserId,
        document_id: Uuid,
        ocr_result: &OcrExtractionResult,
    ) -> Result<Invoice> {
        // Calculate overall confidence (calibrated if store is available)
        let confidence = self
            .calculate_confidence_with_calibration(tenant_id, ocr_result)
            .await;

        let input = CreateInvoiceInput {
            document_id,
            vendor_id: None,
            vendor_name: ocr_result
                .vendor_name
                .value
                .clone()
                .unwrap_or_else(|| "Unknown Vendor".to_string()),
            invoice_number: ocr_result
                .invoice_number
                .value
                .clone()
                .unwrap_or_else(|| format!("UNKNOWN-{}", Uuid::new_v4())),
            invoice_date: ocr_result.invoice_date.value,
            due_date: ocr_result.due_date.value,
            po_number: ocr_result.po_number.value.clone(),
            subtotal: ocr_result.subtotal.value.map(Money::usd),
            tax_amount: ocr_result.tax_amount.value.map(Money::usd),
            total_amount: Money::usd(ocr_result.total_amount.value.unwrap_or(0.0)),
            currency: ocr_result
                .currency
                .value
                .clone()
                .unwrap_or_else(|| "USD".to_string()),
            line_items: ocr_result
                .line_items
                .iter()
                .map(|item| CreateLineItemInput {
                    description: item.description.value.clone().unwrap_or_default(),
                    quantity: item.quantity.value,
                    unit_price: item.unit_price.value.map(Money::usd),
                    amount: Money::usd(item.amount.value.unwrap_or(0.0)),
                    gl_code: None,
                    department: None,
                    project: None,
                })
                .collect(),
            ocr_confidence: Some(confidence),
            department: None,
            gl_code: None,
            cost_center: None,
            notes: None,
            tags: Vec::new(),
        };

        let invoice = self
            .invoice_repo
            .create(tenant_id, input, Some(user_id))
            .await?;

        // Persist per-field buckets so future corrections can debit the right bucket.
        if let Some(ref store) = self.calibration {
            let fields: [(&str, f32); 4] = [
                ("invoice_number", ocr_result.invoice_number.confidence),
                ("invoice_date", ocr_result.invoice_date.confidence),
                ("vendor_name", ocr_result.vendor_name.confidence),
                ("total_amount", ocr_result.total_amount.confidence),
            ];
            for (field, conf) in &fields {
                let b = crate::calibration::bucket_for(*conf as f64);
                if let Err(e) = store
                    .record_pending_correction(
                        tenant_id,
                        self.ocr_provider.provider_name(),
                        invoice.id.as_uuid(),
                        field,
                        b,
                    )
                    .await
                {
                    tracing::warn!(
                        error = %e,
                        field = field,
                        bucket = b,
                        "Failed to record pending correction bucket"
                    );
                }
            }
        }

        Ok(invoice)
    }

    /// Calculate overall confidence score from OCR result (unweighted, for backwards compat)
    #[allow(dead_code)]
    fn calculate_confidence(&self, ocr_result: &OcrExtractionResult) -> f32 {
        let fields = [
            ocr_result.invoice_number.confidence,
            ocr_result.invoice_date.confidence,
            ocr_result.vendor_name.confidence,
            ocr_result.total_amount.confidence,
        ];

        let sum: f32 = fields.iter().sum();
        sum / fields.len() as f32
    }

    /// Calculate calibrated confidence using persisted field-level accuracy weights.
    ///
    /// Falls back to the unweighted mean when the calibration store is absent
    /// or has insufficient data for all four tracked fields.
    async fn calculate_confidence_with_calibration(
        &self,
        tenant_id: &TenantId,
        ocr_result: &OcrExtractionResult,
    ) -> f32 {
        let raw: &[(&str, f32)] = &[
            ("invoice_number", ocr_result.invoice_number.confidence),
            ("invoice_date", ocr_result.invoice_date.confidence),
            ("vendor_name", ocr_result.vendor_name.confidence),
            ("total_amount", ocr_result.total_amount.confidence),
        ];

        if let Some(ref store) = self.calibration {
            // Record this extraction event so the calibration data grows.
            if let Err(e) = store
                .record_extraction(
                    tenant_id,
                    self.ocr_provider.provider_name(),
                    OCR_CALIBRATED_FIELDS,
                )
                .await
            {
                tracing::warn!(error = %e, "Failed to record extraction for calibration");
            }

            // Record per-bucket extraction outcomes for bucket-based calibration.
            for (field, conf) in raw {
                let b = crate::calibration::bucket_for(*conf as f64);
                if let Err(e) = store
                    .record_field_outcome(
                        tenant_id,
                        self.ocr_provider.provider_name(),
                        field,
                        b,
                        false,
                    )
                    .await
                {
                    tracing::warn!(
                        error = %e,
                        field = field,
                        bucket = b,
                        "Failed to record bucket extraction outcome"
                    );
                }
            }

            match store
                .get_field_weights(tenant_id, self.ocr_provider.provider_name())
                .await
            {
                Ok(weights) => {
                    let bucket_result = store
                        .get_field_buckets(
                            tenant_id,
                            self.ocr_provider.provider_name(),
                            OCR_CALIBRATED_FIELDS,
                        )
                        .await
                        .unwrap_or_default();
                    return calibrated_confidence(raw, &weights, &bucket_result);
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to fetch calibration weights, falling back to unweighted");
                }
            }
        }

        // Fallback: unweighted arithmetic mean.
        let sum: f32 = raw.iter().map(|(_, c)| *c).sum();
        sum / raw.len() as f32
    }

    /// Record that a user corrected an OCR-extracted field.
    ///
    /// Persists the correction signal into the calibration store so that future
    /// confidence calculations reflect the provider's empirical accuracy.
    /// Also records the bucket-level correction so per-bucket calibration
    /// tracks observed correctness rates.
    pub async fn record_field_correction(
        &self,
        tenant_id: &TenantId,
        invoice_id: &billforge_core::domain::InvoiceId,
        field_name: &str,
    ) -> Result<()> {
        // Whitelist the field name.
        if !VALID_CORRECTION_FIELDS.contains(&field_name) {
            return Err(billforge_core::Error::Validation(format!(
                "Invalid OCR field name '{}'. Must be one of: {}",
                field_name,
                VALID_CORRECTION_FIELDS.join(", ")
            )));
        }

        if let Some(ref store) = self.calibration {
            // Record correction on the aggregate table (existing path).
            store
                .record_correction(tenant_id, self.ocr_provider.provider_name(), field_name)
                .await?;

            // Look up the pending bucket recorded at extraction time and
            // debit the bucket calibration table.
            match store
                .consume_pending_correction(
                    tenant_id,
                    self.ocr_provider.provider_name(),
                    invoice_id.as_uuid(),
                    field_name,
                )
                .await
            {
                Ok(Some(bucket)) => {
                    if let Err(e) = store
                        .record_field_outcome(
                            tenant_id,
                            self.ocr_provider.provider_name(),
                            field_name,
                            bucket,
                            true, // was_corrected
                        )
                        .await
                    {
                        tracing::warn!(
                            error = %e,
                            field = field_name,
                            bucket = bucket,
                            "Failed to record bucket correction outcome"
                        );
                    }
                }
                Ok(None) => {
                    tracing::debug!(
                        field = field_name,
                        "No pending correction bucket found; bucket calibration not updated"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        field = field_name,
                        "Failed to look up pending correction bucket"
                    );
                }
            }
        }

        Ok(())
    }

    /// Reprocess OCR for an existing invoice
    pub async fn reprocess_ocr(
        &self,
        tenant_id: &TenantId,
        invoice_id: &billforge_core::domain::InvoiceId,
    ) -> Result<OcrExtractionResult> {
        // Get the invoice
        let invoice = self
            .invoice_repo
            .get_by_id(tenant_id, invoice_id)
            .await?
            .ok_or_else(|| billforge_core::Error::NotFound {
                resource_type: "Invoice".to_string(),
                id: invoice_id.to_string(),
            })?;

        // Download the document
        let document_bytes = self
            .storage
            .download(tenant_id, invoice.document_id)
            .await?;

        // Update status
        self.invoice_repo
            .update_capture_status(tenant_id, invoice_id, CaptureStatus::Processing)
            .await?;

        // Run OCR (single provider or multi-provider comparison)
        let (ocr_result, _best_provider) = self.run_ocr(&document_bytes, "application/pdf").await?;

        let confidence = self
            .calculate_confidence_with_calibration(tenant_id, &ocr_result)
            .await;
        let status = if confidence < 0.3 {
            CaptureStatus::Failed
        } else {
            CaptureStatus::ReadyForReview
        };

        self.invoice_repo
            .update_capture_status(tenant_id, invoice_id, status)
            .await?;

        Ok(ocr_result)
    }
}
