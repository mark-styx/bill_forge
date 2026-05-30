//! OCR confidence calibration backed by persistent per-field accuracy stats.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use billforge_core::types::TenantId;

/// Minimum number of extractions before calibration weights kick in.
const MIN_EXTRACTIONS_FOR_CALIBRATION: i64 = 10;

/// Trait for recording and querying OCR field-level calibration data.
#[async_trait]
pub trait OcrCalibrationStore: Send + Sync {
    /// Increment the extraction counter for each named field.
    async fn record_extraction(
        &self,
        tenant: &TenantId,
        provider: &str,
        fields: &[&str],
    ) -> billforge_core::Result<()>;

    /// Record a user correction for a single field.
    async fn record_correction(
        &self,
        tenant: &TenantId,
        provider: &str,
        field: &str,
    ) -> billforge_core::Result<()>;

    /// Return per-field accuracy weights derived from historical data.
    ///
    /// Each weight is Laplace-smoothed: (extractions - corrections + 1) / (extractions + 2).
    /// Only fields with >= `MIN_EXTRACTIONS_FOR_CALIBRATION` extractions are included.
    async fn get_field_weights(
        &self,
        tenant: &TenantId,
        provider: &str,
    ) -> billforge_core::Result<HashMap<String, f32>>;
}

/// Postgres-backed calibration store using UPSERTs.
pub struct PgOcrCalibrationStore {
    pool: Arc<sqlx::PgPool>,
}

impl PgOcrCalibrationStore {
    pub fn new(pool: Arc<sqlx::PgPool>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl OcrCalibrationStore for PgOcrCalibrationStore {
    async fn record_extraction(
        &self,
        tenant: &TenantId,
        provider: &str,
        fields: &[&str],
    ) -> billforge_core::Result<()> {
        for field in fields {
            sqlx::query(
                r#"INSERT INTO ocr_field_calibration (tenant_id, provider, field_name, extractions, corrections, last_updated)
                   VALUES ($1, $2, $3, 1, 0, NOW())
                   ON CONFLICT (tenant_id, provider, field_name)
                   DO UPDATE SET extractions = ocr_field_calibration.extractions + 1,
                                 last_updated = NOW()"#,
            )
            .bind(*tenant.as_uuid())
            .bind(provider)
            .bind(field)
            .execute(&*self.pool)
            .await
            .map_err(|e| billforge_core::Error::Database(format!("Failed to record extraction: {}", e)))?;
        }
        Ok(())
    }

    async fn record_correction(
        &self,
        tenant: &TenantId,
        provider: &str,
        field: &str,
    ) -> billforge_core::Result<()> {
        sqlx::query(
            r#"INSERT INTO ocr_field_calibration (tenant_id, provider, field_name, extractions, corrections, last_updated)
               VALUES ($1, $2, $3, 0, 1, NOW())
               ON CONFLICT (tenant_id, provider, field_name)
               DO UPDATE SET corrections = ocr_field_calibration.corrections + 1,
                             last_updated = NOW()"#,
        )
        .bind(*tenant.as_uuid())
        .bind(provider)
        .bind(field)
        .execute(&*self.pool)
        .await
        .map_err(|e| billforge_core::Error::Database(format!("Failed to record correction: {}", e)))?;
        Ok(())
    }

    async fn get_field_weights(
        &self,
        tenant: &TenantId,
        provider: &str,
    ) -> billforge_core::Result<HashMap<String, f32>> {
        let rows: Vec<(String, i64, i64)> = sqlx::query_as(
            r#"SELECT field_name, extractions, corrections
               FROM ocr_field_calibration
               WHERE tenant_id = $1 AND provider = $2
                 AND extractions >= $3"#,
        )
        .bind(*tenant.as_uuid())
        .bind(provider)
        .bind(MIN_EXTRACTIONS_FOR_CALIBRATION)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to fetch calibration weights: {}", e))
        })?;

        let mut weights = HashMap::new();
        for (field_name, extractions, corrections) in rows {
            // Laplace smoothing: (E - C + 1) / (E + 2)
            let accuracy = ((extractions - corrections + 1) as f32) / ((extractions + 2) as f32);
            weights.insert(field_name, accuracy);
        }
        Ok(weights)
    }
}

/// Compute a calibrated confidence score using empirical per-field accuracy weights.
///
/// - `raw`: slice of (field_name, raw_ocr_confidence) pairs.
/// - `weights`: per-field accuracy weights from the calibration store.
///
/// If all four tracked fields have weights, returns a weighted mean where each
/// field's raw confidence is multiplied by its empirical accuracy weight.
/// Otherwise falls back to the unweighted arithmetic mean (current behavior).
pub fn calibrated_confidence(raw: &[(&'static str, f32)], weights: &HashMap<String, f32>) -> f32 {
    if raw.is_empty() {
        return 0.0;
    }

    // Only calibrate when every tracked field has a weight entry.
    let all_have_weights = raw.iter().all(|(name, _)| weights.contains_key(*name));

    if !all_have_weights {
        // Fallback: unweighted arithmetic mean (matches old behavior).
        let sum: f32 = raw.iter().map(|(_, c)| *c).sum();
        return sum / raw.len() as f32;
    }

    // Weighted mean: each field contributes raw_confidence * accuracy_weight,
    // normalised by the sum of weights.
    let weighted_sum: f32 = raw
        .iter()
        .map(|(name, confidence)| {
            let w = weights.get(*name).copied().unwrap_or(1.0);
            confidence * w
        })
        .sum();

    let weight_total: f32 = raw
        .iter()
        .map(|(name, _)| weights.get(*name).copied().unwrap_or(1.0))
        .sum();

    if weight_total <= 0.0 {
        // Degenerate case; fall back to unweighted.
        let sum: f32 = raw.iter().map(|(_, c)| *c).sum();
        return sum / raw.len() as f32;
    }

    weighted_sum / weight_total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calibrated_confidence_equals_unweighted_when_no_weights() {
        let raw: &[(&str, f32)] = &[
            ("invoice_number", 0.95),
            ("invoice_date", 0.80),
            ("vendor_name", 0.90),
            ("total_amount", 0.85),
        ];
        let weights = HashMap::new();

        let result = calibrated_confidence(raw, &weights);
        let expected = (0.95 + 0.80 + 0.90 + 0.85) / 4.0;

        assert!(
            (result - expected).abs() < 0.001,
            "expected {}, got {}",
            expected,
            result
        );
    }

    #[test]
    fn calibrated_confidence_lower_when_vendor_name_has_many_corrections() {
        let raw: &[(&str, f32)] = &[
            ("invoice_number", 0.90),
            ("invoice_date", 0.90),
            ("vendor_name", 0.95),
            ("total_amount", 0.90),
        ];

        // Simulate: vendor_name has been corrected heavily (weight ~0.27),
        // other fields are accurate (weight ~0.86).
        let mut weights = HashMap::new();
        weights.insert("invoice_number".to_string(), 0.86);
        weights.insert("invoice_date".to_string(), 0.86);
        weights.insert("vendor_name".to_string(), 0.27);
        weights.insert("total_amount".to_string(), 0.86);

        let calibrated = calibrated_confidence(raw, &weights);
        let unweighted = (0.90 + 0.90 + 0.95 + 0.90) / 4.0f32; // 0.9125

        assert!(
            calibrated < unweighted,
            "calibrated ({}) should be less than unweighted ({})",
            calibrated,
            unweighted
        );
    }

    #[test]
    fn calibrated_confidence_uses_unweighted_when_partial_weights() {
        let raw: &[(&str, f32)] = &[
            ("invoice_number", 0.95),
            ("invoice_date", 0.80),
            ("vendor_name", 0.90),
            ("total_amount", 0.85),
        ];

        // Only 2 of 4 fields have weights - should fall back to unweighted.
        let mut weights = HashMap::new();
        weights.insert("invoice_number".to_string(), 0.50);
        weights.insert("vendor_name".to_string(), 0.50);

        let result = calibrated_confidence(raw, &weights);
        let expected = (0.95 + 0.80 + 0.90 + 0.85) / 4.0;

        assert!(
            (result - expected).abs() < 0.001,
            "expected unweighted fallback {}, got {}",
            expected,
            result
        );
    }
}
