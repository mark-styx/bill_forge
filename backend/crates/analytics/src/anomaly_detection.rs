//! Anomaly Detection
//!
//! Detects anomalies in invoice amounts, approval times, vendor behavior, and more.

use crate::predictive_models::*;
use chrono::Utc;
use tracing::{debug, info};
use uuid::Uuid;

/// Statistical Anomaly Detector
///
/// Uses z-score and IQR methods to detect outliers in time-series data.
pub struct StatisticalAnomalyDetector {
    tenant_id: Uuid,
    zscore_threshold: f64,
    iqr_multiplier: f64,
}

impl StatisticalAnomalyDetector {
    pub fn new(tenant_id: Uuid) -> Self {
        Self {
            tenant_id,
            zscore_threshold: 3.0, // 3 standard deviations
            iqr_multiplier: 1.5,   // Standard IQR outlier threshold
        }
    }

    /// Detect invoice amount outliers using z-score and IQR methods
    pub fn detect_amount_outliers(&self, data: &TimeSeries) -> PredictiveResult<Vec<Anomaly>> {
        if data.points.len() < 10 {
            return Err(PredictiveError::InsufficientData {
                required: 10,
                actual: data.points.len(),
            });
        }

        let values: Vec<f64> = data.points.iter().map(|p| p.value).collect();
        let mut anomalies = Vec::new();

        // Calculate statistics
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let variance = values.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / values.len() as f64;
        let std_dev = variance.sqrt();

        // Calculate IQR
        let mut sorted_values = values.clone();
        sorted_values.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let q1 = sorted_values[sorted_values.len() / 4];
        let q3 = sorted_values[3 * sorted_values.len() / 4];
        let iqr = q3 - q1;

        debug!(
            "Amount statistics: mean={}, std_dev={}, q1={}, q3={}, iqr={}",
            mean, std_dev, q1, q3, iqr
        );

        // Check each point for anomalies
        for (i, point) in data.points.iter().enumerate() {
            let mut is_zscore_anomaly = false;
            let mut is_iqr_anomaly = false;
            let mut deviation_score: f64 = 0.0;

            // Z-score method
            if std_dev > 0.0 {
                let zscore = (point.value - mean).abs() / std_dev;
                if zscore > self.zscore_threshold {
                    is_zscore_anomaly = true;
                    deviation_score = deviation_score.max(zscore);
                    debug!("Z-score anomaly at index {}: zscore={}", i, zscore);
                }
            }

            // IQR method
            let lower_bound = q1 - self.iqr_multiplier * iqr;
            let upper_bound = q3 + self.iqr_multiplier * iqr;

            if point.value < lower_bound || point.value > upper_bound {
                is_iqr_anomaly = true;
                let deviation = if point.value > upper_bound {
                    (point.value - upper_bound) / iqr
                } else {
                    (lower_bound - point.value) / iqr
                };
                deviation_score = deviation_score.max(deviation);
                debug!(
                    "IQR anomaly at index {}: value={}, bounds=[{}, {}]",
                    i, point.value, lower_bound, upper_bound
                );
            }

            // Record anomaly if detected by either method
            if is_zscore_anomaly || is_iqr_anomaly {
                let severity = self.classify_severity(deviation_score);

                anomalies.push(Anomaly {
                    id: Uuid::new_v4(),
                    tenant_id: self.tenant_id,
                    anomaly_type: AnomalyType::InvoiceAmountOutlier,
                    entity_id: data.entity_id.clone(),
                    entity_type: data.entity_type,
                    severity,
                    detected_value: point.value,
                    expected_range: (lower_bound, upper_bound),
                    deviation_score,
                    detected_at: Utc::now(),
                    metadata: serde_json::json!({
                        "timestamp": point.timestamp,
                        "index": i,
                        "mean": mean,
                        "std_dev": std_dev,
                        "zscore_anomaly": is_zscore_anomaly,
                        "iqr_anomaly": is_iqr_anomaly,
                    }),
                    acknowledged: false,
                    acknowledged_at: None,
                    acknowledged_by: None,
                });
            }
        }

        info!(
            "Detected {} amount outliers for entity {}",
            anomalies.len(),
            data.entity_id
        );

        Ok(anomalies)
    }

    /// Detect sudden volume spikes for vendors
    pub fn detect_vendor_volume_spikes(&self, data: &TimeSeries) -> PredictiveResult<Vec<Anomaly>> {
        if data.points.len() < 14 {
            return Err(PredictiveError::InsufficientData {
                required: 14,
                actual: data.points.len(),
            });
        }

        // Compare recent volume (last 7 days) to historical average (previous 7+ days)
        let recent_count = 7;
        let historical_count = data.points.len() - recent_count;

        let recent_avg: f64 = data.points[recent_count..]
            .iter()
            .map(|p| p.value)
            .sum::<f64>()
            / recent_count as f64;
        let historical_avg: f64 = data.points[..recent_count]
            .iter()
            .map(|p| p.value)
            .sum::<f64>()
            / historical_count as f64;

        if historical_avg == 0.0 {
            return Ok(Vec::new());
        }

        let increase_ratio = recent_avg / historical_avg;
        let mut anomalies = Vec::new();

        // Threshold: > 2x increase in volume
        if increase_ratio > 2.0 {
            let severity = self.classify_severity(increase_ratio);

            anomalies.push(Anomaly {
                id: Uuid::new_v4(),
                tenant_id: self.tenant_id,
                anomaly_type: AnomalyType::VendorVolumeSpike,
                entity_id: data.entity_id.clone(),
                entity_type: data.entity_type,
                severity,
                detected_value: recent_avg,
                expected_range: (historical_avg * 0.5, historical_avg * 1.5),
                deviation_score: increase_ratio,
                detected_at: Utc::now(),
                metadata: serde_json::json!({
                    "recent_avg": recent_avg,
                    "historical_avg": historical_avg,
                    "increase_ratio": increase_ratio,
                    "recent_days": recent_count,
                    "historical_days": historical_count,
                }),
                acknowledged: false,
                acknowledged_at: None,
                acknowledged_by: None,
            });

            info!(
                "Detected vendor volume spike for {}: {:.1}x increase",
                data.entity_id, increase_ratio
            );
        }

        Ok(anomalies)
    }

    /// Detect approval time anomalies
    pub fn detect_approval_time_anomalies(
        &self,
        data: &TimeSeries,
    ) -> PredictiveResult<Vec<Anomaly>> {
        if data.points.len() < 10 {
            return Err(PredictiveError::InsufficientData {
                required: 10,
                actual: data.points.len(),
            });
        }

        let values: Vec<f64> = data.points.iter().map(|p| p.value).collect();
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let variance = values.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / values.len() as f64;
        let std_dev = variance.sqrt();

        let mut anomalies = Vec::new();

        // Check for unusually long approval times (> 2 standard deviations)
        let threshold = mean + 2.0 * std_dev;

        for (i, point) in data.points.iter().enumerate() {
            if point.value > threshold {
                let deviation = (point.value - mean) / std_dev;
                let severity = if point.value > mean + 4.0 * std_dev {
                    AnomalySeverity::Critical
                } else if point.value > mean + 3.0 * std_dev {
                    AnomalySeverity::High
                } else {
                    AnomalySeverity::Medium
                };

                anomalies.push(Anomaly {
                    id: Uuid::new_v4(),
                    tenant_id: self.tenant_id,
                    anomaly_type: AnomalyType::ApprovalTimeAnomaly,
                    entity_id: data.entity_id.clone(),
                    entity_type: data.entity_type,
                    severity,
                    detected_value: point.value,
                    expected_range: (0.0, threshold),
                    deviation_score: deviation,
                    detected_at: Utc::now(),
                    metadata: serde_json::json!({
                        "timestamp": point.timestamp,
                        "index": i,
                        "mean_approval_time": mean,
                        "std_dev": std_dev,
                    }),
                    acknowledged: false,
                    acknowledged_at: None,
                    acknowledged_by: None,
                });
            }
        }

        info!(
            "Detected {} approval time anomalies for entity {}",
            anomalies.len(),
            data.entity_id
        );

        Ok(anomalies)
    }

    fn classify_severity(&self, deviation_score: f64) -> AnomalySeverity {
        if deviation_score > 4.0 {
            AnomalySeverity::Critical
        } else if deviation_score > 3.0 {
            AnomalySeverity::High
        } else if deviation_score > 2.0 {
            AnomalySeverity::Medium
        } else {
            AnomalySeverity::Low
        }
    }
}

/// Duplicate Invoice Detector
///
/// Detects potential duplicate invoices using fuzzy matching.
pub struct DuplicateDetector {
    tenant_id: Uuid,
    amount_tolerance: f64,    // Percentage tolerance for amount matching
    date_tolerance_days: i64, // Days tolerance for date matching
}

impl DuplicateDetector {
    pub fn new(tenant_id: Uuid) -> Self {
        Self {
            tenant_id,
            amount_tolerance: 0.01,  // 1% tolerance
            date_tolerance_days: 30, // Within 30 days
        }
    }

    /// Check for potential duplicates in invoice data
    pub fn detect_duplicates(&self, invoices: &[InvoiceRecord]) -> PredictiveResult<Vec<Anomaly>> {
        let mut anomalies = Vec::new();
        let mut checked_pairs = std::collections::HashSet::new();

        for i in 0..invoices.len() {
            for j in (i + 1)..invoices.len() {
                let pair_key = if i < j {
                    format!("{}-{}", i, j)
                } else {
                    format!("{}-{}", j, i)
                };

                if checked_pairs.contains(&pair_key) {
                    continue;
                }
                checked_pairs.insert(pair_key);

                let inv1 = &invoices[i];
                let inv2 = &invoices[j];

                // Check if potential duplicate
                let similarity_score = self.calculate_similarity(inv1, inv2);

                if similarity_score > 0.8 {
                    // High similarity threshold
                    let severity = if similarity_score > 0.95 {
                        AnomalySeverity::Critical
                    } else if similarity_score > 0.90 {
                        AnomalySeverity::High
                    } else {
                        AnomalySeverity::Medium
                    };

                    anomalies.push(Anomaly {
                        id: Uuid::new_v4(),
                        tenant_id: self.tenant_id,
                        anomaly_type: AnomalyType::DuplicateInvoice,
                        entity_id: inv1.invoice_id.clone(),
                        entity_type: EntityType::Vendor,
                        severity,
                        detected_value: similarity_score,
                        expected_range: (0.0, 0.8),
                        deviation_score: similarity_score,
                        detected_at: Utc::now(),
                        metadata: serde_json::json!({
                            "invoice1": {
                                "id": inv1.invoice_id,
                                "vendor": inv1.vendor_name,
                                "amount": inv1.amount,
                                "date": inv1.invoice_date,
                            },
                            "invoice2": {
                                "id": inv2.invoice_id,
                                "vendor": inv2.vendor_name,
                                "amount": inv2.amount,
                                "date": inv2.invoice_date,
                            },
                            "similarity_score": similarity_score,
                        }),
                        acknowledged: false,
                        acknowledged_at: None,
                        acknowledged_by: None,
                    });

                    debug!(
                        "Detected potential duplicate: {} and {} (similarity: {:.2})",
                        inv1.invoice_id, inv2.invoice_id, similarity_score
                    );
                }
            }
        }

        info!("Detected {} potential duplicate invoices", anomalies.len());

        Ok(anomalies)
    }

    fn calculate_similarity(&self, inv1: &InvoiceRecord, inv2: &InvoiceRecord) -> f64 {
        let mut score = 0.0;
        let mut weights = 0.0;

        // Vendor name similarity (weight: 0.3)
        let vendor_sim = self.string_similarity(&inv1.vendor_name, &inv2.vendor_name);
        score += vendor_sim * 0.3;
        weights += 0.3;

        // Amount similarity (weight: 0.4)
        if inv1.amount > 0.0 && inv2.amount > 0.0 {
            let amount_diff = (inv1.amount - inv2.amount).abs() / inv1.amount.max(inv2.amount);
            if amount_diff <= self.amount_tolerance {
                score += 1.0 * 0.4;
            } else {
                score += (1.0 - amount_diff.min(1.0)) * 0.4;
            }
            weights += 0.4;
        }

        // Date proximity (weight: 0.3)
        let date_diff = (inv1.invoice_date - inv2.invoice_date).num_days().abs();
        if date_diff <= self.date_tolerance_days {
            let date_sim = 1.0 - (date_diff as f64 / self.date_tolerance_days as f64);
            score += date_sim * 0.3;
        }
        weights += 0.3;

        if weights > 0.0 {
            score / weights
        } else {
            0.0
        }
    }

    fn string_similarity(&self, s1: &str, s2: &str) -> f64 {
        // Simplified Jaccard similarity on words
        let s1_lower = s1.to_lowercase();
        let s2_lower = s2.to_lowercase();
        let words1: std::collections::HashSet<&str> = s1_lower.split_whitespace().collect();
        let words2: std::collections::HashSet<&str> = s2_lower.split_whitespace().collect();

        if words1.is_empty() && words2.is_empty() {
            return 1.0;
        }

        let intersection = words1.intersection(&words2).count();
        let union = words1.union(&words2).count();

        if union == 0 {
            0.0
        } else {
            intersection as f64 / union as f64
        }
    }
}

/// Invoice record for duplicate detection
#[derive(Debug, Clone)]
pub struct InvoiceRecord {
    pub invoice_id: String,
    pub vendor_name: String,
    pub amount: f64,
    pub invoice_date: chrono::DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn create_test_timeseries() -> TimeSeries {
        let now = Utc::now();
        let points: Vec<TimeSeriesPoint> = (0..30)
            .map(|i| TimeSeriesPoint {
                timestamp: now - Duration::days(30 - i),
                value: if i == 15 {
                    5000.0
                } else {
                    1000.0 + (i as f64 * 10.0)
                }, // Outlier at index 15
            })
            .collect();

        TimeSeries {
            entity_id: "test_vendor".to_string(),
            entity_type: EntityType::Vendor,
            metric_name: "invoice_amount".to_string(),
            points,
        }
    }

    #[test]
    fn test_detect_amount_outliers() {
        let detector = StatisticalAnomalyDetector::new(Uuid::new_v4());
        let data = create_test_timeseries();

        let anomalies = detector.detect_amount_outliers(&data).unwrap();
        assert!(!anomalies.is_empty());

        // Should detect the outlier at index 15 (value 5000.0)
        let outlier = anomalies.iter().find(|a| a.detected_value == 5000.0);
        assert!(outlier.is_some());
    }

    #[test]
    fn test_detect_vendor_volume_spike() {
        let detector = StatisticalAnomalyDetector::new(Uuid::new_v4());
        let now = Utc::now();

        // Create data with recent volume spike
        let points: Vec<TimeSeriesPoint> = (0..14)
            .map(|i| TimeSeriesPoint {
                timestamp: now - Duration::days(14 - i),
                value: if i >= 7 { 100.0 } else { 10.0 }, // Recent week 10x higher
            })
            .collect();

        let data = TimeSeries {
            entity_id: "vendor_with_spike".to_string(),
            entity_type: EntityType::Vendor,
            metric_name: "invoice_count".to_string(),
            points,
        };

        let anomalies = detector.detect_vendor_volume_spikes(&data).unwrap();
        assert!(!anomalies.is_empty());
        assert_eq!(anomalies[0].anomaly_type, AnomalyType::VendorVolumeSpike);
    }

    #[test]
    fn test_detect_approval_time_anomalies() {
        let detector = StatisticalAnomalyDetector::new(Uuid::new_v4());
        let now = Utc::now();

        // Create data with some slow approvals
        let points: Vec<TimeSeriesPoint> = (0..20)
            .map(|i| TimeSeriesPoint {
                timestamp: now - Duration::days(20 - i),
                value: if i == 5 || i == 12 { 10.0 } else { 2.0 }, // Two slow approvals (5x normal)
            })
            .collect();

        let data = TimeSeries {
            entity_id: "approver_123".to_string(),
            entity_type: EntityType::Approver,
            metric_name: "approval_time_days".to_string(),
            points,
        };

        let anomalies = detector.detect_approval_time_anomalies(&data).unwrap();
        assert!(!anomalies.is_empty());
    }

    #[test]
    fn test_detect_duplicate_invoices() {
        let detector = DuplicateDetector::new(Uuid::new_v4());
        let now = Utc::now();

        let invoices = vec![
            InvoiceRecord {
                invoice_id: "INV-001".to_string(),
                vendor_name: "Acme Corp".to_string(),
                amount: 1500.00,
                invoice_date: now - Duration::days(10),
            },
            InvoiceRecord {
                invoice_id: "INV-002".to_string(),
                vendor_name: "Acme Corp".to_string(), // Identical name
                amount: 1500.00,                      // Same amount
                invoice_date: now - Duration::days(8), // Close date
            },
            InvoiceRecord {
                invoice_id: "INV-003".to_string(),
                vendor_name: "Different Vendor".to_string(),
                amount: 2000.00,
                invoice_date: now - Duration::days(5),
            },
        ];

        let anomalies = detector.detect_duplicates(&invoices).unwrap();
        assert!(
            !anomalies.is_empty(),
            "Should detect at least one duplicate"
        );

        // Should detect INV-001 and INV-002 as duplicates
        let duplicate = anomalies
            .iter()
            .find(|a| a.anomaly_type == AnomalyType::DuplicateInvoice);
        assert!(
            duplicate.is_some(),
            "Should find a DuplicateInvoice anomaly"
        );
    }

    #[test]
    fn test_string_similarity() {
        let detector = DuplicateDetector::new(Uuid::new_v4());

        let sim1 = detector.string_similarity("Acme Corp", "Acme Corporation");
        assert!(sim1 > 0.3); // Jaccard similarity: "acme" in both = 1/3 ≈ 0.33

        let sim2 = detector.string_similarity("Acme Corp", "Different Vendor");
        assert!(sim2 < 0.3);
    }
}
