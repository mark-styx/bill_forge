//! OCR Provider Comparison and A/B Testing
//!
//! Provides tools for comparing OCR providers, A/B testing, and fallback logic.

use billforge_core::{
    domain::OcrExtractionResult,
    traits::OcrService,
    Error, Result,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// OCR provider types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OcrProvider {
    Tesseract,
    AwsTextract,
    GoogleVision,
}

impl OcrProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            OcrProvider::Tesseract => "tesseract",
            OcrProvider::AwsTextract => "aws_textract",
            OcrProvider::GoogleVision => "google_vision",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "tesseract" => Some(OcrProvider::Tesseract),
            "aws_textract" | "textract" => Some(OcrProvider::AwsTextract),
            "google_vision" | "google" => Some(OcrProvider::GoogleVision),
            _ => None,
        }
    }
}

/// OCR comparison result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrComparisonResult {
    /// Provider results
    pub providers: HashMap<String, ProviderResult>,
    /// Best provider for this document
    pub best_provider: String,
    /// Comparison metrics
    pub comparison_metrics: ComparisonMetrics,
}

/// Result from a single provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderResult {
    /// Provider name
    pub provider: String,
    /// Extraction result
    pub result: Option<OcrExtractionResult>,
    /// Processing time in milliseconds
    pub processing_time_ms: u64,
    /// Whether extraction succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Confidence score (0.0 to 1.0)
    pub confidence_score: f32,
}

/// Comparison metrics across providers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonMetrics {
    /// Fields extracted by all providers
    pub fields_in_agreement: Vec<String>,
    /// Fields with conflicting values
    pub fields_in_conflict: Vec<FieldConflict>,
    /// Overall agreement percentage
    pub agreement_percentage: f32,
}

/// Field conflict between providers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldConflict {
    pub field_name: String,
    pub values: HashMap<String, String>,
}

/// OCR comparison tool
pub struct OcrComparison {
    providers: Vec<(OcrProvider, Box<dyn OcrService>)>,
    metrics_store: Arc<RwLock<ProviderMetricsStore>>,
}

/// Stores aggregate metrics for each provider
#[derive(Debug, Default)]
struct ProviderMetricsStore {
    metrics: HashMap<OcrProvider, AggregateMetrics>,
}

#[derive(Debug, Clone)]
struct AggregateMetrics {
    total_requests: u64,
    successful_requests: u64,
    failed_requests: u64,
    total_processing_time_ms: u64,
    avg_confidence: f32,
    fields_extracted_count: HashMap<String, u64>,
}

impl OcrComparison {
    /// Create new comparison instance with multiple providers
    pub fn new(providers: Vec<(OcrProvider, Box<dyn OcrService>)>) -> Self {
        Self {
            providers,
            metrics_store: Arc::new(RwLock::new(ProviderMetricsStore::default())),
        }
    }

    /// Compare all providers on a single document
    pub async fn compare(&self, document_bytes: &[u8], mime_type: &str) -> Result<OcrComparisonResult> {
        if self.providers.is_empty() {
            return Err(Error::Ocr("No OCR providers configured for comparison".to_string()));
        }

        let start = Instant::now();
        let mut provider_results = HashMap::new();

        // Run OCR on all providers concurrently
        let futures: Vec<_> = self.providers
            .iter()
            .map(|(provider_type, provider)| {
                let provider_name = provider_type.as_str().to_string();
                let provider_clone = provider.provider_name().to_string();
                let bytes = document_bytes.to_vec();
                let mime = mime_type.to_string();

                async move {
                    let start = Instant::now();
                    let result = provider.extract(&bytes, &mime).await;
                    let processing_time_ms = start.elapsed().as_millis() as u64;

                    (
                        provider_name,
                        provider_clone,
                        result,
                        processing_time_ms,
                    )
                }
            })
            .collect();

        // Collect results
        let results = futures::future::join_all(futures).await;

        for (provider_key, provider_name, result, processing_time_ms) in results {
            let provider_result = match result {
                Ok(extraction_result) => {
                    let confidence = Self::calculate_confidence_score(&extraction_result);

                    // Update metrics
                    self.update_metrics(
                        OcrProvider::from_str(&provider_name).unwrap_or(OcrProvider::Tesseract),
                        true,
                        processing_time_ms,
                        confidence,
                        &extraction_result,
                    ).await;

                    ProviderResult {
                        provider: provider_name,
                        result: Some(extraction_result),
                        processing_time_ms,
                        success: true,
                        error: None,
                        confidence_score: confidence,
                    }
                }
                Err(e) => {
                    // Update metrics for failure
                    self.update_metrics_failure(
                        OcrProvider::from_str(&provider_name).unwrap_or(OcrProvider::Tesseract),
                        processing_time_ms,
                    ).await;

                    ProviderResult {
                        provider: provider_name,
                        result: None,
                        processing_time_ms,
                        success: false,
                        error: Some(e.to_string()),
                        confidence_score: 0.0,
                    }
                }
            };

            provider_results.insert(provider_key, provider_result);
        }

        // Determine best provider
        let best_provider = self.select_best_provider(&provider_results);

        // Calculate comparison metrics
        let comparison_metrics = self.calculate_comparison_metrics(&provider_results);

        Ok(OcrComparisonResult {
            providers: provider_results,
            best_provider,
            comparison_metrics,
        })
    }

    /// Calculate confidence score for an extraction result
    fn calculate_confidence_score(result: &OcrExtractionResult) -> f32 {
        let mut score = 0.0;
        let mut weight = 0.0;

        // Weight fields by importance
        let field_weights = [
            ("invoice_number", 2.0),
            ("total_amount", 2.0),
            ("vendor_name", 1.5),
            ("invoice_date", 1.5),
            ("due_date", 1.0),
            ("po_number", 1.0),
            ("subtotal", 1.0),
            ("tax_amount", 0.5),
            ("currency", 0.5),
        ];

        if result.invoice_number.value.is_some() {
            score += result.invoice_number.confidence * 2.0;
            weight += 2.0;
        }

        if result.total_amount.value.is_some() {
            score += result.total_amount.confidence * 2.0;
            weight += 2.0;
        }

        if result.vendor_name.value.is_some() {
            score += result.vendor_name.confidence * 1.5;
            weight += 1.5;
        }

        if result.invoice_date.value.is_some() {
            score += result.invoice_date.confidence * 1.5;
            weight += 1.5;
        }

        if result.due_date.value.is_some() {
            score += result.due_date.confidence * 1.0;
            weight += 1.0;
        }

        // Bonus for line items
        if !result.line_items.is_empty() {
            score += 1.0;
            weight += 1.0;
        }

        if weight > 0.0 {
            score / weight
        } else {
            0.0
        }
    }

    /// Select best provider based on results
    fn select_best_provider(&self, results: &HashMap<String, ProviderResult>) -> String {
        let mut best_provider = String::new();
        let mut best_score = -1.0;

        for (provider_name, result) in results {
            if !result.success {
                continue;
            }

            // Score = confidence - time_penalty
            let time_penalty = result.processing_time_ms as f32 / 10000.0; // 10 seconds = 1.0 penalty
            let score = result.confidence_score - time_penalty;

            if score > best_score {
                best_score = score;
                best_provider = provider_name.clone();
            }
        }

        if best_provider.is_empty() {
            // Fall back to first successful provider
            for (provider_name, result) in results {
                if result.success {
                    return provider_name.clone();
                }
            }

            // No successful providers
            "tesseract".to_string()
        } else {
            best_provider
        }
        .to_string()
    }

    /// Calculate comparison metrics across providers
    fn calculate_comparison_metrics(&self, results: &HashMap<String, ProviderResult>) -> ComparisonMetrics {
        let successful_results: Vec<_> = results
            .values()
            .filter(|r| r.success && r.result.is_some())
            .collect();

        if successful_results.len() < 2 {
            return ComparisonMetrics {
                fields_in_agreement: vec![],
                fields_in_conflict: vec![],
                agreement_percentage: 100.0,
            };
        }

        let mut fields_in_agreement = Vec::new();
        let mut fields_in_conflict = Vec::new();

        // Compare invoice_number
        let invoice_numbers: HashMap<String, String> = successful_results
            .iter()
            .filter_map(|r| {
                let result = r.result.as_ref()?;
                let value = result.invoice_number.value.as_ref()?;
                Some((r.provider.clone(), value.to_string()))
            })
            .collect();

        if invoice_numbers.len() >= 2 {
            let unique_values: std::collections::HashSet<_> = invoice_numbers.values().collect();
            if unique_values.len() == 1 {
                fields_in_agreement.push("invoice_number".to_string());
            } else {
                fields_in_conflict.push(FieldConflict {
                    field_name: "invoice_number".to_string(),
                    values: invoice_numbers,
                });
            }
        }

        // Compare total_amount
        let total_amounts: HashMap<String, String> = successful_results
            .iter()
            .filter_map(|r| {
                let result = r.result.as_ref()?;
                let value = result.total_amount.value?;
                Some((r.provider.clone(), format!("{:.2}", value)))
            })
            .collect();

        if total_amounts.len() >= 2 {
            let unique_values: std::collections::HashSet<_> = total_amounts.values().collect();
            if unique_values.len() == 1 {
                fields_in_agreement.push("total_amount".to_string());
            } else {
                fields_in_conflict.push(FieldConflict {
                    field_name: "total_amount".to_string(),
                    values: total_amounts,
                });
            }
        }

        // Compare vendor_name
        let vendor_names: HashMap<String, String> = successful_results
            .iter()
            .filter_map(|r| {
                let result = r.result.as_ref()?;
                let value = result.vendor_name.value.clone()?;
                Some((r.provider.clone(), value))
            })
            .collect();

        if vendor_names.len() >= 2 {
            let unique_values: std::collections::HashSet<_> = vendor_names.values().collect();
            if unique_values.len() == 1 {
                fields_in_agreement.push("vendor_name".to_string());
            } else {
                fields_in_conflict.push(FieldConflict {
                    field_name: "vendor_name".to_string(),
                    values: vendor_names,
                });
            }
        }

        let total_fields = 3;
        let agreement_count = fields_in_agreement.len();
        let agreement_percentage = if total_fields > 0 {
            (agreement_count as f32 / total_fields as f32) * 100.0
        } else {
            100.0
        };

        ComparisonMetrics {
            fields_in_agreement,
            fields_in_conflict,
            agreement_percentage,
        }
    }

    /// Update provider metrics (success)
    async fn update_metrics(
        &self,
        provider: OcrProvider,
        success: bool,
        processing_time_ms: u64,
        confidence: f32,
        result: &OcrExtractionResult,
    ) {
        let mut store = self.metrics_store.write().await;

        let metrics = store.metrics.entry(provider).or_insert_with(|| AggregateMetrics {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            total_processing_time_ms: 0,
            avg_confidence: 0.0,
            fields_extracted_count: HashMap::new(),
        });

        metrics.total_requests += 1;
        if success {
            metrics.successful_requests += 1;
        }
        metrics.total_processing_time_ms += processing_time_ms;

        // Update rolling average confidence
        let n = metrics.successful_requests as f32;
        metrics.avg_confidence = (metrics.avg_confidence * (n - 1.0) + confidence) / n;

        // Track field extraction counts
        if result.invoice_number.value.is_some() {
            *metrics.fields_extracted_count.entry("invoice_number".to_string()).or_insert(0) += 1;
        }
        if result.total_amount.value.is_some() {
            *metrics.fields_extracted_count.entry("total_amount".to_string()).or_insert(0) += 1;
        }
        if result.vendor_name.value.is_some() {
            *metrics.fields_extracted_count.entry("vendor_name".to_string()).or_insert(0) += 1;
        }
    }

    /// Update provider metrics (failure)
    async fn update_metrics_failure(&self, provider: OcrProvider, processing_time_ms: u64) {
        let mut store = self.metrics_store.write().await;

        let metrics = store.metrics.entry(provider).or_insert_with(|| AggregateMetrics {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            total_processing_time_ms: 0,
            avg_confidence: 0.0,
            fields_extracted_count: HashMap::new(),
        });

        metrics.total_requests += 1;
        metrics.failed_requests += 1;
        metrics.total_processing_time_ms += processing_time_ms;
    }

    /// Get aggregate metrics for all providers
    pub async fn get_provider_metrics(&self) -> HashMap<OcrProvider, ProviderMetricsSnapshot> {
        let store = self.metrics_store.read().await;

        store.metrics
            .iter()
            .map(|(provider, metrics)| {
                (
                    *provider,
                    ProviderMetricsSnapshot {
                        total_requests: metrics.total_requests,
                        successful_requests: metrics.successful_requests,
                        failed_requests: metrics.failed_requests,
                        avg_processing_time_ms: if metrics.successful_requests > 0 {
                            metrics.total_processing_time_ms / metrics.successful_requests
                        } else {
                            0
                        },
                        avg_confidence: metrics.avg_confidence,
                        success_rate: if metrics.total_requests > 0 {
                            (metrics.successful_requests as f64 / metrics.total_requests as f64) * 100.0
                        } else {
                            0.0
                        },
                        fields_extracted_count: metrics.fields_extracted_count.clone(),
                    },
                )
            })
            .collect()
    }
}

/// Snapshot of provider metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMetricsSnapshot {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub avg_processing_time_ms: u64,
    pub avg_confidence: f32,
    pub success_rate: f64,
    pub fields_extracted_count: HashMap<String, u64>,
}

/// OCR provider with fallback logic
pub struct OcrWithFallback {
    primary: Box<dyn OcrService>,
    fallback: Box<dyn OcrService>,
    fallback_threshold_ms: u64,
}

impl OcrWithFallback {
    /// Create OCR with fallback provider
    pub fn new(primary: Box<dyn OcrService>, fallback: Box<dyn OcrService>) -> Self {
        Self {
            primary,
            fallback,
            fallback_threshold_ms: 5000, // 5 seconds
        }
    }

    /// Set fallback threshold in milliseconds
    pub fn with_fallback_threshold(mut self, threshold_ms: u64) -> Self {
        self.fallback_threshold_ms = threshold_ms;
        self
    }

    /// Extract using primary, fall back if fails or too slow
    pub async fn extract_with_fallback(&self, document_bytes: &[u8], mime_type: &str) -> Result<OcrExtractionResult> {
        let start = Instant::now();

        // Try primary provider
        match self.primary.extract(document_bytes, mime_type).await {
            Ok(result) => {
                let elapsed_ms = start.elapsed().as_millis() as u64;

                // If primary was too slow, log warning
                if elapsed_ms > self.fallback_threshold_ms {
                    tracing::warn!(
                        primary_provider = %self.primary.provider_name(),
                        processing_time_ms = elapsed_ms,
                        threshold_ms = self.fallback_threshold_ms,
                        "Primary OCR provider exceeded time threshold"
                    );
                }

                Ok(result)
            }
            Err(e) => {
                tracing::warn!(
                    primary_provider = %self.primary.provider_name(),
                    error = %e,
                    "Primary OCR provider failed, falling back to secondary"
                );

                // Fall back to secondary provider
                self.fallback.extract(document_bytes, mime_type).await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_from_str() {
        assert_eq!(OcrProvider::from_str("tesseract"), Some(OcrProvider::Tesseract));
        assert_eq!(OcrProvider::from_str("aws_textract"), Some(OcrProvider::AwsTextract));
        assert_eq!(OcrProvider::from_str("textract"), Some(OcrProvider::AwsTextract));
        assert_eq!(OcrProvider::from_str("google_vision"), Some(OcrProvider::GoogleVision));
        assert_eq!(OcrProvider::from_str("unknown"), None);
    }

    #[test]
    fn test_provider_as_str() {
        assert_eq!(OcrProvider::Tesseract.as_str(), "tesseract");
        assert_eq!(OcrProvider::AwsTextract.as_str(), "aws_textract");
        assert_eq!(OcrProvider::GoogleVision.as_str(), "google_vision");
    }
}
