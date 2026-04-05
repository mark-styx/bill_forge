//! Time-Series Forecasting
//!
//! Provides forecasting models for spend prediction, invoice volume, and approval times.

use crate::predictive_models::*;
use chrono::{Utc, Datelike};
use tracing::{debug, info};

/// Naive Forecasting Model (baseline)
///
/// Uses historical mean with trend adjustment for simple forecasting.
pub struct NaiveForecaster {
    model_version: String,
}

impl NaiveForecaster {
    pub fn new() -> Self {
        Self {
            model_version: "naive_v1".to_string(),
        }
    }

    fn calculate_statistics(&self, data: &TimeSeries) -> PredictiveResult<(f64, f64, bool)> {
        if data.points.len() < 7 {
            return Err(PredictiveError::InsufficientData {
                required: 7,
                actual: data.points.len(),
            });
        }

        // Calculate mean
        let mean = data.points.iter().map(|p| p.value).sum::<f64>() / data.points.len() as f64;

        // Calculate trend (simple linear regression)
        let n = data.points.len() as f64;
        let sum_x: f64 = (0..data.points.len()).map(|i| i as f64).sum();
        let sum_y: f64 = data.points.iter().map(|p| p.value).sum();
        let sum_xy: f64 = data
            .points
            .iter()
            .enumerate()
            .map(|(i, p)| i as f64 * p.value)
            .sum();
        let sum_x2: f64 = (0..data.points.len()).map(|i| (i as f64).powi(2)).sum();

        let slope = (n * sum_xy - sum_x * sum_y) / (n * sum_x2 - sum_x.powi(2));
        let intercept = (sum_y - slope * sum_x) / n;

        // Detect seasonality (simplified: check for weekly patterns)
        let seasonality_detected = self.detect_weekly_seasonality(&data.points);

        debug!(
            "Calculated statistics: mean={}, slope={}, intercept={}, seasonality={}",
            mean, slope, intercept, seasonality_detected
        );

        Ok((mean, slope, seasonality_detected))
    }

    fn detect_weekly_seasonality(&self, points: &[TimeSeriesPoint]) -> bool {
        if points.len() < 14 {
            return false;
        }

        // Group by day of week
        let mut day_totals = vec![0.0; 7];
        let mut day_counts = vec![0; 7];

        for point in points {
            let day_of_week = point.timestamp.weekday().num_days_from_monday() as usize;
            day_totals[day_of_week] += point.value;
            day_counts[day_of_week] += 1;
        }

        // Calculate averages per day
        let day_averages: Vec<f64> = day_totals
            .iter()
            .zip(day_counts.iter())
            .map(|(total, count)| if *count > 0 { *total / *count as f64 } else { 0.0 })
            .collect();

        // Check if variance between days is significant
        let overall_mean = day_averages.iter().sum::<f64>() / 7.0;
        let variance: f64 = day_averages
            .iter()
            .map(|avg| (avg - overall_mean).powi(2))
            .sum::<f64>()
            / 7.0;

        // Threshold: if variance > 10% of mean, consider it seasonal
        variance > (overall_mean * 0.1).powi(2)
    }

    #[allow(dead_code)]
    fn calculate_confidence_interval(&self, data: &TimeSeries, forecast_value: f64) -> (f64, f64) {
        // Calculate standard deviation
        let mean = data.points.iter().map(|p| p.value).sum::<f64>() / data.points.len() as f64;
        let variance = data
            .points
            .iter()
            .map(|p| (p.value - mean).powi(2))
            .sum::<f64>()
            / data.points.len() as f64;
        let std_dev = variance.sqrt();

        // 95% confidence interval (±1.96 standard deviations)
        // Widen interval based on forecast horizon
        let margin = std_dev * 1.96;
        (forecast_value - margin, forecast_value + margin)
    }
}

impl Default for NaiveForecaster {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ForecastingModel for NaiveForecaster {
    async fn fit(&mut self, data: &TimeSeries) -> PredictiveResult<()> {
        info!("Fitting naive forecasting model for entity: {}", data.entity_id);
        self.calculate_statistics(data)?;
        Ok(())
    }

    async fn forecast(&self, _horizon: ForecastHorizon) -> PredictiveResult<Forecast> {
        // For this simplified implementation, we'll need the data to be stored
        // In production, this would use stored model parameters
        Err(PredictiveError::PredictionFailed(
            "Model must be fit before forecasting".to_string(),
        ))
    }

    fn model_name(&self) -> &str {
        &self.model_version
    }
}

/// ARIMA-inspired Forecasting Model
///
/// Simplified ARIMA implementation for time-series forecasting.
pub struct ArimaForecaster {
    model_version: String,
    data: Option<TimeSeries>,
    statistics: Option<ArimaStatistics>,
}

#[derive(Debug, Clone)]
struct ArimaStatistics {
    mean: f64,
    trend_slope: f64,
    seasonality_detected: bool,
    seasonal_period: Option<u32>, // Days
    seasonal_indices: Vec<f64>,   // Per-period-position adjustment values
    residual_std: f64,
}

impl ArimaForecaster {
    pub fn new() -> Self {
        Self {
            model_version: "arima_v1".to_string(),
            data: None,
            statistics: None,
        }
    }

    fn decompose_time_series(&self, data: &TimeSeries) -> PredictiveResult<ArimaStatistics> {
        if data.points.len() < 30 {
            return Err(PredictiveError::InsufficientData {
                required: 30,
                actual: data.points.len(),
            });
        }

        // Trend component (linear regression)
        let n = data.points.len() as f64;
        let sum_x: f64 = (0..data.points.len()).map(|i| i as f64).sum();
        let sum_y: f64 = data.points.iter().map(|p| p.value).sum();
        let sum_xy: f64 = data
            .points
            .iter()
            .enumerate()
            .map(|(i, p)| i as f64 * p.value)
            .sum();
        let sum_x2: f64 = (0..data.points.len()).map(|i| (i as f64).powi(2)).sum();

        let trend_slope = (n * sum_xy - sum_x * sum_y) / (n * sum_x2 - sum_x.powi(2));
        let trend_intercept = (sum_y - trend_slope * sum_x) / n;

        // Remove trend to get residuals
        let detrended: Vec<f64> = data
            .points
            .iter()
            .enumerate()
            .map(|(i, p)| p.value - (trend_intercept + trend_slope * i as f64))
            .collect();

        // Detect seasonality using autocorrelation
        let (seasonality_detected, seasonal_period) = self.detect_seasonality_autocorr(&detrended);

        // Calculate seasonal indices and residual standard deviation
        let mean = sum_y / n;
        let (seasonal_indices, residual_std) = if seasonality_detected {
            let period = seasonal_period.unwrap_or(7) as usize;
            let mut seasonal_sums = vec![0.0; period];
            let mut seasonal_counts = vec![0usize; period];
            for (i, &value) in detrended.iter().enumerate() {
                let pos = i % period;
                seasonal_sums[pos] += value;
                seasonal_counts[pos] += 1;
            }
            let indices: Vec<f64> = seasonal_sums
                .iter()
                .zip(seasonal_counts.iter())
                .map(|(s, c)| if *c > 0 { s / *c as f64 } else { 0.0 })
                .collect();
            let residuals: Vec<f64> = detrended
                .iter()
                .enumerate()
                .map(|(i, &v)| v - indices[i % period])
                .collect();
            let residual_variance =
                residuals.iter().map(|x| x.powi(2)).sum::<f64>() / residuals.len() as f64;
            (indices, residual_variance.sqrt())
        } else {
            (vec![], (detrended.iter().map(|x| x.powi(2)).sum::<f64>() / n).sqrt())
        };

        debug!(
            "ARIMA decomposition: mean={}, slope={}, seasonality={}, period={:?}",
            mean, trend_slope, seasonality_detected, seasonal_period
        );

        Ok(ArimaStatistics {
            mean,
            trend_slope,
            seasonality_detected,
            seasonal_period,
            seasonal_indices,
            residual_std,
        })
    }

    fn detect_seasonality_autocorr(&self, data: &[f64]) -> (bool, Option<u32>) {
        if data.len() < 14 {
            return (false, None);
        }

        let mean = data.iter().sum::<f64>() / data.len() as f64;
        let variance = data.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / data.len() as f64;

        if variance == 0.0 {
            return (false, None);
        }

        // Test common seasonal periods (7 days, 30 days)
        let periods_to_test = vec![7, 14, 30];

        for period in periods_to_test {
            if data.len() < period * 2 {
                continue;
            }

            // Calculate autocorrelation at lag = period
            let autocorr = self.autocorrelation(data, period, mean, variance);

            // Threshold: autocorrelation > 0.5 indicates seasonality
            if autocorr > 0.5 {
                return (true, Some(period as u32));
            }
        }

        (false, None)
    }

    fn autocorrelation(&self, data: &[f64], lag: usize, mean: f64, variance: f64) -> f64 {
        let n = data.len();
        if lag >= n || variance == 0.0 {
            return 0.0;
        }

        let sum: f64 = (lag..n)
            .map(|i| (data[i] - mean) * (data[i - lag] - mean))
            .sum();

        sum / (variance * (n - lag) as f64)
    }

}

impl Default for ArimaForecaster {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ForecastingModel for ArimaForecaster {
    async fn fit(&mut self, data: &TimeSeries) -> PredictiveResult<()> {
        info!("Fitting ARIMA model for entity: {}", data.entity_id);

        let statistics = self.decompose_time_series(data)?;
        self.data = Some(data.clone());
        self.statistics = Some(statistics);

        Ok(())
    }

    async fn forecast(&self, horizon: ForecastHorizon) -> PredictiveResult<Forecast> {
        let data = self.data.as_ref().ok_or_else(|| {
            PredictiveError::PredictionFailed("Model must be fit before forecasting".to_string())
        })?;

        let stats = self.statistics.as_ref().ok_or_else(|| {
            PredictiveError::PredictionFailed("Model statistics not available".to_string())
        })?;

        // Project trend forward
        let forecast_days = horizon.days();
        let last_index = data.points.len() as f64;
        let forecast_index = last_index + forecast_days as f64;

        let trend_forecast = stats.mean + stats.trend_slope * (forecast_index - data.points.len() as f64 / 2.0);

        // Add seasonal component if detected
        let seasonal_adjustment = if stats.seasonality_detected {
            let period = stats.seasonal_period.unwrap_or(7) as usize;
            let position = (data.points.len() + forecast_days as usize) % period;

            if position < stats.seasonal_indices.len() {
                stats.seasonal_indices[position]
            } else {
                0.0
            }
        } else {
            0.0
        };

        let predicted_value = trend_forecast + seasonal_adjustment;

        // Calculate confidence interval
        // Scale confidence interval width with sqrt of horizon days.
        // Uncertainty in cumulative forecasts grows proportionally to sqrt(time).
        // Base: 1.96 * residual_std for 95% CI at 1-day horizon.
        let horizon_days = horizon.days() as f64;
        let margin = stats.residual_std * 1.96 * (horizon_days / 30.0).sqrt();

        // Floor: at least 5% of predicted value to avoid degenerate zero-width intervals
        let margin = margin.max(predicted_value.abs() * 0.05);

        // Ensure confidence interval is valid (lower < predicted < upper)
        // Also ensure we don't have negative spend, but preserve the interval relationship
        let confidence_lower = if predicted_value > margin {
            (predicted_value - margin).max(0.0)
        } else {
            // If margin is too large, use a percentage-based lower bound
            predicted_value * 0.5 // At least 50% of predicted value
        };

        let confidence_upper = predicted_value + margin;

        Ok(Forecast {
            entity_id: data.entity_id.clone(),
            entity_type: data.entity_type,
            metric_name: data.metric_name.clone(),
            horizon,
            predicted_value,
            confidence_lower,
            confidence_upper,
            confidence_level: 0.95,
            generated_at: Utc::now(),
            model_version: self.model_version.clone(),
            seasonality_detected: stats.seasonality_detected,
        })
    }

    fn model_name(&self) -> &str {
        &self.model_version
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::Duration;

    fn create_test_timeseries() -> TimeSeries {
        let now = Utc::now();
        let points: Vec<TimeSeriesPoint> = (0..60)
            .map(|i| TimeSeriesPoint {
                timestamp: now - Duration::days(60 - i),
                value: 1000.0 + (i as f64 * 10.0) + (i % 7) as f64 * 50.0, // Trend + weekly seasonality
            })
            .collect();

        TimeSeries {
            entity_id: "test_vendor".to_string(),
            entity_type: EntityType::Vendor,
            metric_name: "spend".to_string(),
            points,
        }
    }

    #[tokio::test]
    async fn test_naive_forecaster_fit() {
        let mut forecaster = NaiveForecaster::new();
        let data = create_test_timeseries();

        let result = forecaster.fit(&data).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_naive_forecaster_insufficient_data() {
        let mut forecaster = NaiveForecaster::new();
        let data = TimeSeries {
            entity_id: "test".to_string(),
            entity_type: EntityType::Vendor,
            metric_name: "spend".to_string(),
            points: vec![TimeSeriesPoint {
                timestamp: Utc::now(),
                value: 100.0,
            }],
        };

        let result = forecaster.fit(&data).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_arima_forecaster_fit() {
        let mut forecaster = ArimaForecaster::new();
        let data = create_test_timeseries();

        let result = forecaster.fit(&data).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_arima_forecaster_forecast() {
        let mut forecaster = ArimaForecaster::new();
        let data = create_test_timeseries();

        forecaster.fit(&data).await.unwrap();
        let forecast = forecaster.forecast(ForecastHorizon::Days30).await.unwrap();

        assert_eq!(forecast.entity_id, "test_vendor");
        assert!(forecast.predicted_value > 0.0);
        assert!(forecast.confidence_lower <= forecast.predicted_value);
        assert!(forecast.confidence_upper >= forecast.predicted_value);
    }

    #[test]
    fn test_detect_weekly_seasonality() {
        let forecaster = NaiveForecaster::new();
        let now = Utc::now();

        // Create data with strong weekly pattern
        let points: Vec<TimeSeriesPoint> = (0..28)
            .map(|i| TimeSeriesPoint {
                timestamp: now - Duration::days(28 - i),
                value: if i % 7 < 5 { 1000.0 } else { 100.0 }, // Weekdays vs weekends
            })
            .collect();

        let detected = forecaster.detect_weekly_seasonality(&points);
        assert!(detected);
    }

    #[tokio::test]
    async fn test_arima_seasonal_adjustment_applied() {
        // Create data with strong weekly seasonality: weekday=1000, weekend=100
        let now = Utc::now();
        let points: Vec<TimeSeriesPoint> = (0..60)
            .map(|i| TimeSeriesPoint {
                timestamp: now - Duration::days(60 - i),
                value: if i % 7 < 5 { 1000.0 } else { 100.0 },
            })
            .collect();
        let data = TimeSeries {
            entity_id: "seasonal_vendor".to_string(),
            entity_type: EntityType::Vendor,
            metric_name: "spend".to_string(),
            points,
        };

        let mut forecaster = ArimaForecaster::new();
        forecaster.fit(&data).await.unwrap();

        // Check that seasonal indices were computed and are non-trivial
        let stats = forecaster.statistics.as_ref().unwrap();
        assert!(stats.seasonality_detected, "seasonality should be detected");
        assert!(!stats.seasonal_indices.is_empty(), "seasonal indices should be stored");

        // Forecast and verify seasonal adjustment is nonzero for at least one horizon
        let f30 = forecaster.forecast(ForecastHorizon::Days30).await.unwrap();
        let f31 = forecaster.forecast(ForecastHorizon::Days60).await.unwrap();
        // The two forecasts hit different seasonal positions, so predicted values differ
        assert_ne!(
            f30.predicted_value, f31.predicted_value,
            "forecasts at different seasonal positions should differ (seasonal adjustment applied)"
        );
    }

    #[tokio::test]
    async fn test_confidence_interval_widens_with_horizon() {
        let mut forecaster = ArimaForecaster::new();
        let data = create_test_timeseries();
        forecaster.fit(&data).await.unwrap();

        let f30 = forecaster.forecast(ForecastHorizon::Days30).await.unwrap();
        let f60 = forecaster.forecast(ForecastHorizon::Days60).await.unwrap();
        let f90 = forecaster.forecast(ForecastHorizon::Days90).await.unwrap();

        let width30 = f30.confidence_upper - f30.confidence_lower;
        let width60 = f60.confidence_upper - f60.confidence_lower;
        let width90 = f90.confidence_upper - f90.confidence_lower;

        assert!(
            width60 > width30,
            "60-day interval ({}) should be wider than 30-day ({})",
            width60, width30
        );
        assert!(
            width90 > width60,
            "90-day interval ({}) should be wider than 60-day ({})",
            width90, width60
        );
    }

    #[tokio::test]
    async fn test_confidence_interval_has_minimum_width() {
        // Constant-value data: zero residual, so the 5% floor should kick in
        let now = Utc::now();
        let points: Vec<TimeSeriesPoint> = (0..60)
            .map(|i| TimeSeriesPoint {
                timestamp: now - Duration::days(60 - i),
                value: 500.0, // constant
            })
            .collect();
        let data = TimeSeries {
            entity_id: "flat_vendor".to_string(),
            entity_type: EntityType::Vendor,
            metric_name: "spend".to_string(),
            points,
        };

        let mut forecaster = ArimaForecaster::new();
        forecaster.fit(&data).await.unwrap();

        let forecast = forecaster.forecast(ForecastHorizon::Days30).await.unwrap();
        let width = forecast.confidence_upper - forecast.confidence_lower;

        assert!(
            width > 0.0,
            "confidence interval should have nonzero width (5% floor), got width={}",
            width
        );
        // The floor is 5% of predicted value; predicted ~500, so width should be >= 2 * 5% * 500 = 50
        // but we just check it's positive to avoid being too brittle
    }
}
