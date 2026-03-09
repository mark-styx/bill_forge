//! Analytics Background Jobs
//!
//! Scheduled jobs for analytics aggregation and cleanup.

pub mod daily_aggregation;

pub use daily_aggregation::DailyAggregationJob;
