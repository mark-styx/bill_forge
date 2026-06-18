//! OCR confidence calibration backed by persistent per-field accuracy stats.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use billforge_core::types::TenantId;

/// Minimum number of extractions before calibration weights kick in.
const MIN_EXTRACTIONS_FOR_CALIBRATION: i64 = 10;

/// Minimum total extractions required before first-pass accuracy is considered
/// reliable enough for the OCR completion gate.
pub const MIN_EXTRACTIONS_FOR_ACCURACY: i64 = 30;

/// First-pass accuracy threshold (90%) that OCR phase completion requires.
pub const OCR_FIRST_PASS_ACCURACY_THRESHOLD: f64 = 0.90;

/// Per-field accuracy breakdown used in [`FirstPassAccuracy`].
#[derive(Debug, Clone, Default)]
pub struct FieldAccuracy {
    pub field_name: String,
    pub extractions: i64,
    pub corrections: i64,
    pub accuracy: Option<f64>,
}

/// Aggregated first-pass accuracy for a tenant (optionally filtered by provider).
///
/// Accuracy is `1 - (total_corrections / total_extractions)` once enough
/// extractions have accumulated. Below the minimum sample floor the accuracy
/// is `None` and `sufficient_sample` is `false`.
#[derive(Debug, Clone, Default)]
pub struct FirstPassAccuracy {
    pub accuracy: Option<f64>,
    pub total_extractions: i64,
    pub total_corrections: i64,
    pub per_field: Vec<FieldAccuracy>,
    pub sufficient_sample: bool,
}

/// Pure computation: aggregate per-field extraction/correction rows into a
/// [`FirstPassAccuracy`].
///
/// This is separated from the async DB function so it can be unit-tested
/// without a database.
pub fn compute_first_pass_accuracy(field_rows: &[(String, i64, i64)]) -> FirstPassAccuracy {
    let total_extractions: i64 = field_rows.iter().map(|(_, e, _)| *e).sum();
    let total_corrections: i64 = field_rows.iter().map(|(_, _, c)| *c).sum();
    let sufficient_sample = total_extractions >= MIN_EXTRACTIONS_FOR_ACCURACY;
    let accuracy = if sufficient_sample && total_extractions > 0 {
        Some(1.0 - (total_corrections as f64 / total_extractions as f64))
    } else {
        None
    };
    let per_field: Vec<FieldAccuracy> = field_rows
        .iter()
        .map(|(name, ext, cor)| FieldAccuracy {
            field_name: name.clone(),
            extractions: *ext,
            corrections: *cor,
            accuracy: if *ext > 0 {
                Some(1.0 - (*cor as f64 / *ext as f64))
            } else {
                None
            },
        })
        .collect();
    FirstPassAccuracy {
        accuracy,
        total_extractions,
        total_corrections,
        per_field,
        sufficient_sample,
    }
}

/// Fetch aggregated first-pass accuracy for a tenant from the
/// `ocr_field_calibration` table.
///
/// When `provider` is `None`, aggregates across all providers.
pub async fn tenant_first_pass_accuracy(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    provider: Option<&str>,
) -> billforge_core::Result<FirstPassAccuracy> {
    let rows: Vec<(String, i64, i64)> = sqlx::query_as(
        r#"SELECT field_name, SUM(extractions) AS extractions, SUM(corrections) AS corrections
           FROM ocr_field_calibration
           WHERE tenant_id = $1 AND ($2::text IS NULL OR provider = $2)
           GROUP BY field_name"#,
    )
    .bind(*tenant_id.as_uuid())
    .bind(provider)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        billforge_core::Error::Database(format!("Failed to fetch first-pass accuracy: {}", e))
    })?;

    Ok(compute_first_pass_accuracy(&rows))
}

/// Minimum number of bucket samples before bucket-based calibration overrides
/// the field-weight path.
const MIN_BUCKET_SAMPLES: u64 = 20;

/// Per-bucket calibration data: how many extractions fell into this bucket and
/// how many of those were later corrected.
#[derive(Debug, Clone, Default)]
pub struct BucketCalibration {
    pub extractions: u64,
    pub corrections: u64,
}

/// Map a raw confidence value in [0.0, 1.0] to a bucket index 0..=9.
///
/// Bucket 0 = [0.0, 0.1), bucket 9 = [0.9, 1.0]. Values outside [0.0, 1.0]
/// are clamped.
pub fn bucket_for(raw_confidence: f64) -> u8 {
    let clamped = raw_confidence.clamp(0.0, 1.0);
    let bucket = (clamped * 10.0).floor() as u8;
    bucket.min(9)
}

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

    /// Fetch per-bucket calibration data for the given fields.
    ///
    /// Returns a map keyed by (field_name, bucket) with extraction/correction counts.
    async fn get_field_buckets(
        &self,
        tenant: &TenantId,
        provider: &str,
        fields: &[&str],
    ) -> billforge_core::Result<HashMap<(String, u8), BucketCalibration>>;

    /// Record an extraction or correction outcome for a specific confidence bucket.
    ///
    /// If `was_corrected` is true, increments corrections; otherwise increments extractions.
    async fn record_field_outcome(
        &self,
        tenant: &TenantId,
        provider: &str,
        field: &str,
        bucket: u8,
        was_corrected: bool,
    ) -> billforge_core::Result<()>;

    /// Persist the confidence bucket for a specific (tenant, provider, invoice, field)
    /// at extraction time so the correction handler can look it up later.
    async fn record_pending_correction(
        &self,
        tenant: &TenantId,
        provider: &str,
        invoice_id: &uuid::Uuid,
        field: &str,
        bucket: u8,
    ) -> billforge_core::Result<()>;

    /// Look up and consume the pending bucket for a corrected field.
    ///
    /// Returns the bucket index if a pending record was found (and deletes it),
    /// or `None` if no pending record exists for this (tenant, provider, invoice, field).
    async fn consume_pending_correction(
        &self,
        tenant: &TenantId,
        provider: &str,
        invoice_id: &uuid::Uuid,
        field: &str,
    ) -> billforge_core::Result<Option<u8>>;
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

    async fn get_field_buckets(
        &self,
        tenant: &TenantId,
        provider: &str,
        fields: &[&str],
    ) -> billforge_core::Result<HashMap<(String, u8), BucketCalibration>> {
        let rows: Vec<(String, i16, i64, i64)> = sqlx::query_as(
            r#"SELECT field_name, bucket, extractions, corrections
               FROM ocr_field_calibration_bucket
               WHERE tenant_id = $1 AND provider = $2"#,
        )
        .bind(*tenant.as_uuid())
        .bind(provider)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to fetch calibration buckets: {}", e))
        })?;

        let field_set: Vec<&str> = fields.to_vec();
        let mut result = HashMap::new();
        for (field_name, bucket, extractions, corrections) in rows {
            if field_set.contains(&field_name.as_str()) {
                result.insert(
                    (field_name, bucket as u8),
                    BucketCalibration {
                        extractions: extractions as u64,
                        corrections: corrections as u64,
                    },
                );
            }
        }
        Ok(result)
    }

    async fn record_field_outcome(
        &self,
        tenant: &TenantId,
        provider: &str,
        field: &str,
        bucket: u8,
        was_corrected: bool,
    ) -> billforge_core::Result<()> {
        let (ext_inc, cor_inc) = if was_corrected {
            (0i64, 1i64)
        } else {
            (1i64, 0i64)
        };
        sqlx::query(
            r#"INSERT INTO ocr_field_calibration_bucket
                  (tenant_id, provider, field_name, bucket, extractions, corrections, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, NOW())
               ON CONFLICT (tenant_id, provider, field_name, bucket)
               DO UPDATE SET extractions = ocr_field_calibration_bucket.extractions + $5,
                             corrections = ocr_field_calibration_bucket.corrections + $6,
                             updated_at = NOW()"#,
        )
        .bind(*tenant.as_uuid())
        .bind(provider)
        .bind(field)
        .bind(bucket as i16)
        .bind(ext_inc)
        .bind(cor_inc)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to record field bucket outcome: {}", e))
        })?;
        Ok(())
    }

    async fn record_pending_correction(
        &self,
        tenant: &TenantId,
        provider: &str,
        invoice_id: &uuid::Uuid,
        field: &str,
        bucket: u8,
    ) -> billforge_core::Result<()> {
        sqlx::query(
            r#"INSERT INTO ocr_pending_correction
                  (tenant_id, provider, invoice_id, field_name, bucket, created_at)
               VALUES ($1, $2, $3, $4, $5, NOW())"#,
        )
        .bind(*tenant.as_uuid())
        .bind(provider)
        .bind(invoice_id)
        .bind(field)
        .bind(bucket as i16)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to record pending correction: {}", e))
        })?;
        Ok(())
    }

    async fn consume_pending_correction(
        &self,
        tenant: &TenantId,
        provider: &str,
        invoice_id: &uuid::Uuid,
        field: &str,
    ) -> billforge_core::Result<Option<u8>> {
        let row: Option<(i16,)> = sqlx::query_as(
            r#"DELETE FROM ocr_pending_correction
               WHERE tenant_id = $1 AND provider = $2 AND invoice_id = $3 AND field_name = $4
               RETURNING bucket"#,
        )
        .bind(*tenant.as_uuid())
        .bind(provider)
        .bind(invoice_id)
        .bind(field)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| {
            billforge_core::Error::Database(format!("Failed to consume pending correction: {}", e))
        })?;
        Ok(row.map(|(b,)| b as u8))
    }
}

/// Compute a calibrated confidence score using bucket-based calibration with
/// fallback to per-field accuracy weights, then raw arithmetic mean.
///
/// Precedence (per field):
/// 1. **Bucket evidence**: if the (field, confidence-bucket) has >= `MIN_BUCKET_SAMPLES`
///    extractions, use the Laplace-smoothed observed correctness rate as that field's
///    calibrated value: `(extractions - corrections + 1) / (extractions + 2)`.
/// 2. **Field-weight path**: if no bucket has enough samples but all tracked fields
///    have aggregate weights, fall back to the weighted-mean of raw confidences.
/// 3. **Raw fallback**: otherwise use the unweighted arithmetic mean of raw confidences.
///
/// After resolving each field's calibrated (or raw) value, the overall score is the
/// arithmetic mean across all fields.
pub fn calibrated_confidence(
    raw: &[(&'static str, f32)],
    weights: &HashMap<String, f32>,
    buckets: &HashMap<(String, u8), BucketCalibration>,
) -> f32 {
    if raw.is_empty() {
        return 0.0;
    }

    // Phase 1: try bucket-based calibration per field.
    let mut bucket_results: Vec<f32> = Vec::new();
    let mut fields_without_buckets: Vec<(&str, f32)> = Vec::new();

    for (name, confidence) in raw {
        let b = bucket_for(*confidence as f64);
        let key = (name.to_string(), b);
        if let Some(bucket) = buckets.get(&key) {
            if bucket.extractions >= MIN_BUCKET_SAMPLES {
                // Laplace-smoothed observed correctness rate.
                let calibrated = ((bucket.extractions - bucket.corrections + 1) as f32)
                    / ((bucket.extractions + 2) as f32);
                bucket_results.push(calibrated);
                continue;
            }
        }
        fields_without_buckets.push((*name, *confidence));
    }

    // If every field resolved via buckets, average the bucket values.
    if fields_without_buckets.is_empty() && !bucket_results.is_empty() {
        return bucket_results.iter().sum::<f32>() / bucket_results.len() as f32;
    }

    // If some fields used buckets and some didn't, we can't mix the two
    // strategies meaningfully; treat it as partial coverage and fall
    // through to the weight or raw path for all fields.
    let _ = bucket_results;

    // Phase 2: weighted-mean fallback using aggregate field weights.
    let all_have_weights = raw.iter().all(|(name, _)| weights.contains_key(*name));

    if all_have_weights {
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

        if weight_total > 0.0 {
            return weighted_sum / weight_total;
        }
    }

    // Phase 3: unweighted arithmetic mean (raw fallback).
    let sum: f32 = raw.iter().map(|(_, c)| *c).sum();
    sum / raw.len() as f32
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
        let buckets = HashMap::new();

        let result = calibrated_confidence(raw, &weights, &buckets);
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

        let buckets = HashMap::new();

        let calibrated = calibrated_confidence(raw, &weights, &buckets);
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

        let buckets = HashMap::new();

        let result = calibrated_confidence(raw, &weights, &buckets);
        let expected = (0.95 + 0.80 + 0.90 + 0.85) / 4.0;

        assert!(
            (result - expected).abs() < 0.001,
            "expected unweighted fallback {}, got {}",
            expected,
            result
        );
    }

    #[test]
    fn bucket_for_maps_confidence_to_bucket_index() {
        assert_eq!(bucket_for(0.0), 0);
        assert_eq!(bucket_for(0.05), 0);
        assert_eq!(bucket_for(0.1), 1);
        assert_eq!(bucket_for(0.5), 5);
        assert_eq!(bucket_for(0.95), 9);
        assert_eq!(bucket_for(1.0), 9);
        assert_eq!(bucket_for(1.5), 9);
        assert_eq!(bucket_for(-0.1), 0);
    }

    #[test]
    fn bucket_calibration_overrides_weighted_mean_when_evidence_sufficient() {
        // High raw confidence (0.95) for all fields, but the bucket for
        // vendor_name has many extractions and many corrections, producing a
        // low observed correctness rate. All four fields have bucket evidence.
        let raw: &[(&str, f32)] = &[
            ("invoice_number", 0.95),
            ("invoice_date", 0.95),
            ("vendor_name", 0.95),
            ("total_amount", 0.95),
        ];

        let mut buckets = HashMap::new();
        // invoice_number bucket 9: 100 extractions, 5 corrections => (100-5+1)/(100+2) ≈ 0.941
        buckets.insert(
            ("invoice_number".to_string(), 9),
            BucketCalibration {
                extractions: 100,
                corrections: 5,
            },
        );
        // invoice_date bucket 9: 100 extractions, 3 corrections => 0.961
        buckets.insert(
            ("invoice_date".to_string(), 9),
            BucketCalibration {
                extractions: 100,
                corrections: 3,
            },
        );
        // vendor_name bucket 9: 100 extractions, 80 corrections => (100-80+1)/(100+2) ≈ 0.206
        buckets.insert(
            ("vendor_name".to_string(), 9),
            BucketCalibration {
                extractions: 100,
                corrections: 80,
            },
        );
        // total_amount bucket 9: 100 extractions, 2 corrections => 0.970
        buckets.insert(
            ("total_amount".to_string(), 9),
            BucketCalibration {
                extractions: 100,
                corrections: 2,
            },
        );

        // Also provide weights so we can prove bucket path takes precedence.
        let mut weights = HashMap::new();
        weights.insert("invoice_number".to_string(), 0.99);
        weights.insert("invoice_date".to_string(), 0.99);
        weights.insert("vendor_name".to_string(), 0.99);
        weights.insert("total_amount".to_string(), 0.99);

        let result = calibrated_confidence(raw, &weights, &buckets);

        // Expected: mean of [0.941, 0.961, 0.206, 0.970] ≈ 0.770
        // The weighted-mean path with weight 0.99 for all fields would give
        // 0.95, so result should be materially below 0.95.
        assert!(
            result < 0.85,
            "bucket-based result ({}) should be well below raw confidence (0.95)",
            result
        );

        let inv_num_cal = (100u64 - 5 + 1) as f32 / (100 + 2) as f32;
        let inv_date_cal = (100 - 3 + 1) as f32 / (100 + 2) as f32;
        let vendor_cal = (100 - 80 + 1) as f32 / (100 + 2) as f32;
        let total_cal = (100 - 2 + 1) as f32 / (100 + 2) as f32;
        let expected = (inv_num_cal + inv_date_cal + vendor_cal + total_cal) / 4.0;
        assert!(
            (result - expected).abs() < 0.001,
            "expected {}, got {}",
            expected,
            result
        );
    }

    #[test]
    fn bucket_calibration_falls_back_to_weights_when_samples_insufficient() {
        // Same setup as above but bucket has only 5 extractions (< MIN_BUCKET_SAMPLES=20),
        // so the bucket path should be skipped and the weighted-mean path used instead.
        let raw: &[(&str, f32)] = &[
            ("invoice_number", 0.90),
            ("invoice_date", 0.90),
            ("vendor_name", 0.95),
            ("total_amount", 0.90),
        ];

        let mut buckets = HashMap::new();
        // Only 5 extractions - below threshold, should be ignored.
        buckets.insert(
            ("vendor_name".to_string(), 9),
            BucketCalibration {
                extractions: 5,
                corrections: 4,
            },
        );

        let mut weights = HashMap::new();
        weights.insert("invoice_number".to_string(), 0.86);
        weights.insert("invoice_date".to_string(), 0.86);
        weights.insert("vendor_name".to_string(), 0.27);
        weights.insert("total_amount".to_string(), 0.86);

        let result = calibrated_confidence(raw, &weights, &buckets);
        let unweighted = (0.90 + 0.90 + 0.95 + 0.90) / 4.0f32;

        // Should match the weighted-mean path (same as the existing test), not
        // the bucket path and not the raw unweighted mean.
        assert!(
            result < unweighted,
            "calibrated ({}) should use weights, not unweighted ({})",
            result,
            unweighted
        );

        // Compute expected weighted result manually.
        let weighted_sum = 0.90 * 0.86 + 0.90 * 0.86 + 0.95 * 0.27 + 0.90 * 0.86;
        let weight_total = 0.86 + 0.86 + 0.27 + 0.86;
        let expected = weighted_sum / weight_total;
        assert!(
            (result - expected).abs() < 0.001,
            "expected weighted fallback {}, got {}",
            expected,
            result
        );
    }

    #[test]
    fn first_pass_accuracy_insufficient_sample_below_floor() {
        // 20 extractions total (< 30 floor) => no accuracy, insufficient sample
        let rows: Vec<(String, i64, i64)> = vec![
            ("invoice_number".to_string(), 10, 1),
            ("total_amount".to_string(), 10, 0),
        ];
        let result = compute_first_pass_accuracy(&rows);
        assert!(!result.sufficient_sample);
        assert!(result.accuracy.is_none());
        assert_eq!(result.total_extractions, 20);
        assert_eq!(result.total_corrections, 1);
        // Per-field accuracy is still computed individually
        assert_eq!(result.per_field.len(), 2);
        assert_eq!(result.per_field[0].field_name, "invoice_number");
        assert!(result.per_field[0].accuracy.unwrap() > 0.0);
    }

    #[test]
    fn first_pass_accuracy_meets_threshold() {
        // 30 extractions, 2 corrections => accuracy = 1 - 2/30 ≈ 0.933 >= 0.90
        let rows: Vec<(String, i64, i64)> = vec![
            ("invoice_number".to_string(), 15, 1),
            ("total_amount".to_string(), 15, 1),
        ];
        let result = compute_first_pass_accuracy(&rows);
        assert!(result.sufficient_sample);
        let acc = result.accuracy.expect("accuracy should be computed");
        assert!(acc >= OCR_FIRST_PASS_ACCURACY_THRESHOLD);
        assert!((acc - (1.0 - 2.0 / 30.0)).abs() < 0.001);
    }

    #[test]
    fn first_pass_accuracy_below_threshold() {
        // 30 extractions, 4 corrections => accuracy = 1 - 4/30 ≈ 0.867 < 0.90
        let rows: Vec<(String, i64, i64)> = vec![
            ("invoice_number".to_string(), 10, 2),
            ("vendor_name".to_string(), 10, 1),
            ("total_amount".to_string(), 10, 1),
        ];
        let result = compute_first_pass_accuracy(&rows);
        assert!(result.sufficient_sample);
        let acc = result.accuracy.expect("accuracy should be computed");
        assert!(acc < OCR_FIRST_PASS_ACCURACY_THRESHOLD);
    }

    #[test]
    fn first_pass_accuracy_empty_rows() {
        let rows: Vec<(String, i64, i64)> = vec![];
        let result = compute_first_pass_accuracy(&rows);
        assert!(!result.sufficient_sample);
        assert!(result.accuracy.is_none());
        assert_eq!(result.total_extractions, 0);
        assert!(result.per_field.is_empty());
    }
}
