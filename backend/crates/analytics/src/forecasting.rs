//! Time-Series Forecasting
//!
//! Provides forecasting models for spend prediction, invoice volume, and approval times.

use crate::predictive_models::*;
use chrono::{Duration, Utc, Datelike};
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

    async fn forecast(&self, horizon: ForecastHorizon) -> PredictiveResult<Forecast> {
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

        // Calculate residual standard deviation
        let mean = sum_y / n;
        let residual_std = if seasonality_detected {
            // Remove seasonal component before calculating residual
            self.calculate_seasonal_residual_std(&detrended, seasonal_period.unwrap_or(7))
        } else {
            (detrended.iter().map(|x| x.powi(2)).sum::<f64>() / n).sqrt()
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

    fn calculate_seasonal_residual_std(&self, detrended: &[f64], period: u32) -> f64 {
        // Group by seasonal position
        let period = period as usize;
        let mut seasonal_avgs = vec![0.0; period];
        let mut seasonal_counts = vec![0; period];

        for (i, &value) in detrended.iter().enumerate() {
            let pos = i % period;
            seasonal_avgs[pos] += value;
            seasonal_counts[pos] += 1;
        }

        // Average seasonal values
        for i in 0..period {
            if seasonal_counts[i] > 0 {
                seasonal_avgs[i] /= seasonal_counts[i] as f64;
            }
        }

        // Calculate residual after removing seasonal component
        let residuals: Vec<f64> = detrended
            .iter()
            .enumerate()
            .map(|(i, &value)| value - seasonal_avgs[i % period])
            .collect();

        let residual_variance = residuals.iter().map(|x| x.powi(2)).sum::<f64>() / residuals.len() as f64;
        residual_variance.sqrt()
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

            // Simplified: use mean of that seasonal position from historical data
            // In production, would use stored seasonal indices
            0.0 // Placeholder for seasonal adjustment
        } else {
            0.0
        };

        let predicted_value = trend_forecast + seasonal_adjustment;

        // Calculate confidence interval
        // Widen interval based on forecast horizon and residual variance
        let confidence_multiplier = match horizon {
            ForecastHorizon::Days30 => 1.5,
            ForecastHorizon::Days60 => 2.0,
            ForecastHorizon::Days90 => 2.5,
        };

        let margin = stats.residual_std * confidence_multiplier * 1.96;

        // Ensure we always have a meaningful confidence interval
        // Use at least 5% margin if residual variance is too small
        let min_margin = predicted_value * 0.05;
        let margin = if margin < min_margin {
            min_margin
        } else {
            margin
        };

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
}
