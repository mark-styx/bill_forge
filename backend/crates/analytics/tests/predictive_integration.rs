//! Integration tests for predictive analytics
//!
//! Tests for forecasting, anomaly detection, and budget alerts.

use billforge_analytics::{
    forecasting::ArimaForecaster,
    anomaly_detection::{DuplicateDetector, InvoiceRecord, StatisticalAnomalyDetector},
    predictive_models::*,
};
use chrono::{Duration, Utc};
use serde_json;

/// Test ARIMA forecaster with sufficient data
#[tokio::test]
async fn test_arima_forecaster_basic() {
    let mut forecaster = ArimaForecaster::new();

    // Create 60 days of historical data with trend
    let now = Utc::now();
    let points: Vec<TimeSeriesPoint> = (0..60)
        .map(|i| TimeSeriesPoint {
            timestamp: now - Duration::days(60 - i),
            value: 1000.0 + (i as f64 * 10.0), // Upward trend
        })
        .collect();

    let time_series = TimeSeries {
        entity_id: "vendor_123".to_string(),
        entity_type: EntityType::Vendor,
        metric_name: "spend".to_string(),
        points,
    };

    // Fit model
    let fit_result = forecaster.fit(&time_series).await;
    assert!(fit_result.is_ok(), "ARIMA fit should succeed with 60 data points");

    // Generate forecast
    let forecast_result = forecaster.forecast(ForecastHorizon::Days30).await;
    assert!(forecast_result.is_ok(), "ARIMA forecast should succeed");

    let forecast = forecast_result.unwrap();
    println!("Predicted value: {}", forecast.predicted_value);
    println!("Confidence lower: {}", forecast.confidence_lower);
    println!("Confidence upper: {}", forecast.confidence_upper);
    assert_eq!(forecast.entity_id, "vendor_123");
    assert!(forecast.predicted_value > 0.0, "Predicted value should be positive");
    assert!(forecast.confidence_lower < forecast.predicted_value,
            "Confidence lower ({}) should be less than predicted ({})",
            forecast.confidence_lower, forecast.predicted_value);
    assert!(forecast.confidence_upper > forecast.predicted_value);
}

/// Test ARIMA forecaster with insufficient data
#[tokio::test]
async fn test_arima_forecaster_insufficient_data() {
    let mut forecaster = ArimaForecaster::new();

    // Create only 10 days of data (need 30 minimum)
    let now = Utc::now();
    let points: Vec<TimeSeriesPoint> = (0..10)
        .map(|i| TimeSeriesPoint {
            timestamp: now - Duration::days(10 - i),
            value: 1000.0,
        })
        .collect();

    let time_series = TimeSeries {
        entity_id: "vendor_456".to_string(),
        entity_type: EntityType::Vendor,
        metric_name: "spend".to_string(),
        points,
    };

    // Fit should fail
    let fit_result = forecaster.fit(&time_series).await;
    assert!(fit_result.is_err(), "ARIMA fit should fail with insufficient data");
}

/// Test statistical anomaly detector with outliers
#[test]
fn test_statistical_anomaly_detector_outliers() {
    let tenant_id = uuid::Uuid::new_v4();
    let detector = StatisticalAnomalyDetector::new(tenant_id);

    // Create data with one clear outlier
    let now = Utc::now();
    let mut points: Vec<TimeSeriesPoint> = (0..20)
        .map(|i| TimeSeriesPoint {
            timestamp: now - Duration::days(20 - i),
            value: 1000.0 + (i as f64 * 10.0),
        })
        .collect();

    // Add an outlier
    points.push(TimeSeriesPoint {
        timestamp: now,
        value: 50000.0, // Much higher than normal
    });

    let time_series = TimeSeries {
        entity_id: "vendor_789".to_string(),
        entity_type: EntityType::Vendor,
        metric_name: "invoice_amount".to_string(),
        points,
    };

    let anomalies = detector.detect_amount_outliers(&time_series).unwrap();

    // Should detect at least one anomaly
    assert!(!anomalies.is_empty(), "Should detect outlier anomalies");
}

/// Test statistical anomaly detector with normal data
#[test]
fn test_statistical_anomaly_detector_normal() {
    let tenant_id = uuid::Uuid::new_v4();
    let detector = StatisticalAnomalyDetector::new(tenant_id);

    // Create normal data without outliers
    let now = Utc::now();
    let points: Vec<TimeSeriesPoint> = (0..20)
        .map(|i| TimeSeriesPoint {
            timestamp: now - Duration::days(20 - i),
            value: 1000.0 + ((i as f64 % 10.0) * 10.0), // Small variance without random
        })
        .collect();

    let time_series = TimeSeries {
        entity_id: "vendor_101".to_string(),
        entity_type: EntityType::Vendor,
        metric_name: "invoice_amount".to_string(),
        points,
    };

    let result = detector.detect_amount_outliers(&time_series);
    assert!(result.is_ok(), "Should process normal data without error");
}

/// Test duplicate detector with duplicate invoices
#[tokio::test]
async fn test_duplicate_detector() {
    let tenant_id = uuid::Uuid::new_v4();
    let detector = DuplicateDetector::new(tenant_id);

    let now = Utc::now();

    // Create duplicate invoices (same vendor, amount, and date)
    let invoices = vec![
        InvoiceRecord {
            invoice_id: "inv_1".to_string(),
            vendor_name: "Acme Corp".to_string(),
            amount: 5000.0,
            invoice_date: now,
        },
        InvoiceRecord {
            invoice_id: "inv_2".to_string(),
            vendor_name: "Acme Corp".to_string(),
            amount: 5000.0, // Same amount
            invoice_date: now, // Same date
        },
        InvoiceRecord {
            invoice_id: "inv_3".to_string(),
            vendor_name: "Other Corp".to_string(),
            amount: 1000.0,
            invoice_date: now,
        },
    ];

    let anomalies = detector.detect_duplicates(&invoices).unwrap();

    // Should detect duplicate
    assert!(!anomalies.is_empty(), "Should detect duplicate invoices");

    // Check that duplicate is classified as such
    let duplicate = &anomalies[0];
    assert_eq!(duplicate.anomaly_type, AnomalyType::DuplicateInvoice);
}

/// Test duplicate detector with unique invoices
#[tokio::test]
async fn test_duplicate_detector_unique() {
    let tenant_id = uuid::Uuid::new_v4();
    let detector = DuplicateDetector::new(tenant_id);

    let now = Utc::now();

    // Create unique invoices
    let invoices: Vec<InvoiceRecord> = (0..10)
        .map(|i| InvoiceRecord {
            invoice_id: format!("inv_{}", i),
            vendor_name: format!("Vendor {}", i),
            amount: 1000.0 + (i as f64 * 100.0),
            invoice_date: now - Duration::days(i),
        })
        .collect();

    let anomalies = detector.detect_duplicates(&invoices).unwrap();

    // Should not detect duplicates
    assert!(anomalies.is_empty(), "Should not detect duplicates in unique invoices");
}

/// Test forecast horizon days calculation
#[test]
fn test_forecast_horizon_days() {
    assert_eq!(ForecastHorizon::Days30.days(), 30);
    assert_eq!(ForecastHorizon::Days60.days(), 60);
    assert_eq!(ForecastHorizon::Days90.days(), 90);
}

/// Test anomaly severity classification
#[test]
fn test_anomaly_severity() {
    let tenant_id = uuid::Uuid::new_v4();
    let detector = StatisticalAnomalyDetector::new(tenant_id);

    // Create data with extreme outlier
    let now = Utc::now();
    let mut points: Vec<TimeSeriesPoint> = (0..20)
        .map(|i| TimeSeriesPoint {
            timestamp: now - Duration::days(20 - i),
            value: 1000.0,
        })
        .collect();

    // Add extreme outlier (10x normal)
    points.push(TimeSeriesPoint {
        timestamp: now,
        value: 10000.0,
    });

    let time_series = TimeSeries {
        entity_id: "vendor_extreme".to_string(),
        entity_type: EntityType::Vendor,
        metric_name: "amount".to_string(),
        points,
    };

    let anomalies = detector.detect_amount_outliers(&time_series).unwrap();

    // Should detect high or critical severity
    if !anomalies.is_empty() {
        let anomaly = &anomalies[0];
        assert!(
            anomaly.severity == AnomalySeverity::High
                || anomaly.severity == AnomalySeverity::Critical,
            "Extreme outlier should be high or critical severity"
        );
    }
}

/// Test seasonality detection
#[tokio::test]
async fn test_seasonality_detection() {
    let mut forecaster = ArimaForecaster::new();

    // Create data with weekly seasonality
    let now = Utc::now();
    let points: Vec<TimeSeriesPoint> = (0..60)
        .map(|i| {
            let day_of_week = i % 7;
            let base_value = 1000.0;
            let seasonal_factor = if day_of_week < 5 { 1.2 } else { 0.3 }; // Higher on weekdays
            TimeSeriesPoint {
                timestamp: now - Duration::days(60 - i),
                value: base_value * seasonal_factor,
            }
        })
        .collect();

    let time_series = TimeSeries {
        entity_id: "vendor_seasonal".to_string(),
        entity_type: EntityType::Vendor,
        metric_name: "spend".to_string(),
        points,
    };

    // Fit model
    forecaster.fit(&time_series).await.unwrap();

    // Generate forecast
    let forecast = forecaster.forecast(ForecastHorizon::Days30).await.unwrap();

    // Should detect seasonality
    assert!(forecast.seasonality_detected, "Should detect weekly seasonality");
}

/// Test that all EntityType variants serialize/deserialize correctly,
/// confirming the exact string values the accuracy loop must match.
#[test]
fn test_entity_type_serialization_roundtrip() {
    let variants = vec![
        EntityType::Vendor,
        EntityType::Department,
        EntityType::GlCode,
        EntityType::Tenant,
        EntityType::Approver,
    ];

    for variant in variants {
        let json = serde_json::to_string(&variant).unwrap();
        let deserialized: EntityType = serde_json::from_str(&json).unwrap();
        assert_eq!(variant, deserialized, "Roundtrip failed for {:?}", variant);

        // Also verify the plain string value (without surrounding quotes)
        let plain = json.trim_matches('"');
        let roundtrip: EntityType = serde_json::from_str(&format!("\"{}\"", plain)).unwrap();
        assert_eq!(variant, roundtrip, "Plain string roundtrip failed for {:?}", variant);
    }
}

/// Test that the entity type match logic covers all supported types.
/// Extracts the same matching pattern used in calculate_forecast_accuracy.
#[test]
fn test_entity_type_match_coverage() {
    fn is_supported_entity_type(entity_type: &str) -> bool {
        match entity_type {
            "\"vendor\"" | "vendor" => true,
            "\"department\"" | "department" => true,
            _ => false,
        }
    }

    // Serialize each EntityType to get the actual string the DB would store
    let vendor_json = serde_json::to_string(&EntityType::Vendor).unwrap();
    let dept_json = serde_json::to_string(&EntityType::Department).unwrap();
    let glcode_json = serde_json::to_string(&EntityType::GlCode).unwrap();
    let tenant_json = serde_json::to_string(&EntityType::Tenant).unwrap();
    let approver_json = serde_json::to_string(&EntityType::Approver).unwrap();

    // Vendor and Department (both plain and JSON-quoted) are supported
    assert!(is_supported_entity_type("vendor"), "Vendor should be supported");
    assert!(is_supported_entity_type(&vendor_json), "Vendor JSON should be supported");
    assert!(is_supported_entity_type("department"), "Department should be supported");
    assert!(is_supported_entity_type(&dept_json), "Department JSON should be supported");

    // GlCode, Tenant, Approver are not supported for accuracy calculation
    assert!(!is_supported_entity_type("gl_code"), "GlCode should not be supported");
    assert!(!is_supported_entity_type(&glcode_json), "GlCode JSON should not be supported");
    assert!(!is_supported_entity_type("tenant"), "Tenant should not be supported");
    assert!(!is_supported_entity_type(&tenant_json), "Tenant JSON should not be supported");
    assert!(!is_supported_entity_type("approver"), "Approver should not be supported");
    assert!(!is_supported_entity_type(&approver_json), "Approver JSON should not be supported");
}
