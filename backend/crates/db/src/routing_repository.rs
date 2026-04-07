//! Routing data repository
//!
//! Provides a concrete implementation of `RoutingDataProvider` that reads
//! routing context (workloads, availability, expertise) from PostgreSQL.

use async_trait::async_trait;
use billforge_core::{
    intelligent_routing::{
        ApproverAvailability, ApproverExpertise, ApproverWorkload, AvailabilityStatus,
        ExpertiseType, RoutingConfig, RoutingContext, RoutingDataProvider,
    },
    types::TenantId,
    Error, Result, UserId,
};
use chrono::{DateTime, NaiveTime, Utc};
use num_traits::ToPrimitive;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

/// Database-backed routing data provider
pub struct RoutingRepository {
    pool: PgPool,
}

impl RoutingRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Load routing configuration for a tenant, or return defaults
    pub async fn get_routing_config(&self, tenant_id: &TenantId) -> Result<RoutingConfig> {
        let row = sqlx::query_as::<_, RoutingConfigRow>(
            r#"SELECT
                tenant_id, workload_weight, expertise_weight, availability_weight,
                max_workload_score, min_expertise_score,
                enable_auto_delegation, enable_pattern_learning, enable_calendar_sync,
                working_hours_start, working_hours_end, working_timezone, working_days
            FROM routing_configuration
            WHERE tenant_id = $1"#,
        )
        .bind(tenant_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to get routing config: {}", e)))?;

        match row {
            Some(r) => Ok(r.into_config(tenant_id)),
            None => Ok(RoutingConfig {
                tenant_id: tenant_id.clone(),
                ..Default::default()
            }),
        }
    }

    /// Upsert routing configuration for a tenant
    pub async fn upsert_routing_config(&self, config: &RoutingConfig) -> Result<()> {
        let working_days_json = serde_json::to_value(&config.working_days)
            .map_err(|e| Error::Database(format!("Failed to serialize working_days: {}", e)))?;

        sqlx::query(
            r#"INSERT INTO routing_configuration (
                id, tenant_id, workload_weight, expertise_weight, availability_weight,
                max_workload_score, min_expertise_score,
                enable_auto_delegation, enable_pattern_learning, enable_calendar_sync,
                working_hours_start, working_hours_end, working_timezone, working_days,
                updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, NOW())
            ON CONFLICT (tenant_id) DO UPDATE SET
                workload_weight = EXCLUDED.workload_weight,
                expertise_weight = EXCLUDED.expertise_weight,
                availability_weight = EXCLUDED.availability_weight,
                max_workload_score = EXCLUDED.max_workload_score,
                min_expertise_score = EXCLUDED.min_expertise_score,
                enable_auto_delegation = EXCLUDED.enable_auto_delegation,
                enable_pattern_learning = EXCLUDED.enable_pattern_learning,
                enable_calendar_sync = EXCLUDED.enable_calendar_sync,
                working_hours_start = EXCLUDED.working_hours_start,
                working_hours_end = EXCLUDED.working_hours_end,
                working_timezone = EXCLUDED.working_timezone,
                working_days = EXCLUDED.working_days,
                updated_at = NOW()"#,
        )
        .bind(Uuid::new_v4())
        .bind(config.tenant_id.as_str())
        .bind(config.workload_weight)
        .bind(config.expertise_weight)
        .bind(config.availability_weight)
        .bind(config.max_workload_score)
        .bind(config.min_expertise_score)
        .bind(config.enable_auto_delegation)
        .bind(config.enable_pattern_learning)
        .bind(config.enable_calendar_sync)
        .bind(config.working_hours_start)
        .bind(config.working_hours_end)
        .bind(&config.working_timezone)
        .bind(&working_days_json)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to upsert routing config: {}", e)))?;

        Ok(())
    }

    /// Set approver availability
    pub async fn set_availability(&self, tenant_id: &TenantId, input: &SetAvailabilityInput) -> Result<()> {
        // Delete any existing availability for this user in overlapping time windows
        sqlx::query(
            r#"DELETE FROM approver_availability
            WHERE tenant_id = $1 AND user_id = $2
                AND start_at < $4 AND end_at > $3"#,
        )
        .bind(tenant_id.as_str())
        .bind(input.user_id)
        .bind(input.start_at)
        .bind(input.end_at)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to clear overlapping availability: {}", e)))?;

        sqlx::query(
            r#"INSERT INTO approver_availability (
                id, tenant_id, user_id, status, start_at, end_at,
                delegate_id, calendar_source, reason, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())"#,
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id.as_str())
        .bind(input.user_id)
        .bind(input.status.as_str())
        .bind(input.start_at)
        .bind(input.end_at)
        .bind(input.delegate_id)
        .bind("manual")
        .bind(&input.reason)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to set availability: {}", e)))?;

        Ok(())
    }

    /// Fetch workload data for all approvers in a tenant
    pub async fn get_workloads(&self, tenant_id: &TenantId) -> Result<Vec<ApproverWorkload>> {
        let rows = sqlx::query_as::<_, WorkloadRow>(
            r#"SELECT
                user_id, active_approvals, pending_approvals, completed_this_week,
                avg_approval_time_hours, workload_score, last_assignment_at
            FROM approver_workload_tracking
            WHERE tenant_id = $1"#,
        )
        .bind(tenant_id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to fetch workloads: {}", e)))?;

        Ok(rows.into_iter().map(|r| r.into_workload()).collect())
    }

    /// Log a routing decision for later analysis
    pub async fn log_routing_decision(
        &self,
        tenant_id: &TenantId,
        invoice_id: Uuid,
        queue_id: Uuid,
        decision: &billforge_core::intelligent_routing::RoutingDecision,
    ) -> Result<()> {
        let candidates_json = serde_json::to_value(&decision.candidates)
            .map_err(|e| Error::Database(format!("Failed to serialize candidates: {}", e)))?;
        let factors_json = serde_json::to_value(&decision.factors)
            .map_err(|e| Error::Database(format!("Failed to serialize factors: {}", e)))?;

        sqlx::query(
            r#"INSERT INTO routing_optimization_log (
                id, tenant_id, invoice_id, queue_id, routing_strategy,
                selected_approver_id, candidate_approvers, routing_factors
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id.as_str())
        .bind(invoice_id)
        .bind(queue_id)
        .bind(format!("{:?}", decision.strategy).to_lowercase())
        .bind(decision.approver_id.as_ref().map(|u| u.0))
        .bind(&candidates_json)
        .bind(&factors_json)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to log routing decision: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl RoutingDataProvider for RoutingRepository {
    async fn get_routing_context(&self, tenant_id: &TenantId) -> Result<RoutingContext> {
        // Fetch workloads
        let workload_rows = sqlx::query_as::<_, WorkloadRow>(
            r#"SELECT
                user_id, active_approvals, pending_approvals, completed_this_week,
                avg_approval_time_hours, workload_score, last_assignment_at
            FROM approver_workload_tracking
            WHERE tenant_id = $1"#,
        )
        .bind(tenant_id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to fetch workloads: {}", e)))?;

        let mut workloads = HashMap::new();
        let mut eligible_approvers = Vec::new();
        for row in workload_rows {
            let w = row.into_workload();
            eligible_approvers.push(w.user_id.clone());
            workloads.insert(w.user_id.clone(), w);
        }

        // Fetch active availabilities (current time window)
        let availability_rows = sqlx::query_as::<_, AvailabilityRow>(
            r#"SELECT
                user_id, status, delegate_id, start_at, end_at, reason
            FROM approver_availability
            WHERE tenant_id = $1 AND start_at <= NOW() AND end_at > NOW()"#,
        )
        .bind(tenant_id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to fetch availability: {}", e)))?;

        let availabilities: Vec<ApproverAvailability> =
            availability_rows.into_iter().map(|r| r.into_availability()).collect();

        // Fetch expertise records
        let expertise_rows = sqlx::query_as::<_, ExpertiseRow>(
            r#"SELECT
                user_id, expertise_type, expertise_key,
                total_approved, total_rejected, avg_time_hours,
                expertise_score, last_used_at
            FROM approver_expertise
            WHERE tenant_id = $1"#,
        )
        .bind(tenant_id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to fetch expertise: {}", e)))?;

        let expertise: Vec<ApproverExpertise> =
            expertise_rows.into_iter().map(|r| r.into_expertise()).collect();

        Ok(RoutingContext {
            eligible_approvers,
            workloads,
            availabilities,
            expertise,
        })
    }
}

// --- Row types for sqlx deserialization ---

#[derive(sqlx::FromRow)]
struct RoutingConfigRow {
    tenant_id: String,
    workload_weight: sqlx::types::BigDecimal,
    expertise_weight: sqlx::types::BigDecimal,
    availability_weight: sqlx::types::BigDecimal,
    max_workload_score: sqlx::types::BigDecimal,
    min_expertise_score: sqlx::types::BigDecimal,
    enable_auto_delegation: bool,
    enable_pattern_learning: bool,
    enable_calendar_sync: bool,
    working_hours_start: NaiveTime,
    working_hours_end: NaiveTime,
    working_timezone: String,
    working_days: serde_json::Value,
}

impl RoutingConfigRow {
    fn into_config(self, tenant_id: &TenantId) -> RoutingConfig {
        let working_days: Vec<i32> = serde_json::from_value(self.working_days).unwrap_or_else(|_| vec![1, 2, 3, 4, 5]);
        RoutingConfig {
            tenant_id: tenant_id.clone(),
            workload_weight: self.workload_weight.to_f64().unwrap_or(0.4),
            expertise_weight: self.expertise_weight.to_f64().unwrap_or(0.3),
            availability_weight: self.availability_weight.to_f64().unwrap_or(0.3),
            max_workload_score: self.max_workload_score.to_f64().unwrap_or(100.0),
            min_expertise_score: self.min_expertise_score.to_f64().unwrap_or(0.3),
            enable_auto_delegation: self.enable_auto_delegation,
            enable_pattern_learning: self.enable_pattern_learning,
            enable_calendar_sync: self.enable_calendar_sync,
            working_hours_start: self.working_hours_start,
            working_hours_end: self.working_hours_end,
            working_timezone: self.working_timezone,
            working_days,
        }
    }
}

#[derive(sqlx::FromRow)]
struct WorkloadRow {
    user_id: Uuid,
    active_approvals: i32,
    pending_approvals: i32,
    completed_this_week: i32,
    avg_approval_time_hours: Option<sqlx::types::BigDecimal>,
    workload_score: sqlx::types::BigDecimal,
    last_assignment_at: Option<DateTime<Utc>>,
}

impl WorkloadRow {
    fn into_workload(self) -> ApproverWorkload {
        ApproverWorkload {
            user_id: UserId(self.user_id),
            active_approvals: self.active_approvals,
            pending_approvals: self.pending_approvals,
            completed_this_week: self.completed_this_week,
            avg_approval_time_hours: self.avg_approval_time_hours.and_then(|d| d.to_f64()),
            workload_score: self.workload_score.to_f64().unwrap_or(0.0),
            last_assignment_at: self.last_assignment_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct AvailabilityRow {
    user_id: Uuid,
    status: String,
    delegate_id: Option<Uuid>,
    start_at: DateTime<Utc>,
    end_at: DateTime<Utc>,
    reason: Option<String>,
}

impl AvailabilityRow {
    fn into_availability(self) -> ApproverAvailability {
        ApproverAvailability {
            user_id: UserId(self.user_id),
            status: match self.status.as_str() {
                "busy" => AvailabilityStatus::Busy,
                "out_of_office" => AvailabilityStatus::OutOfOffice,
                "vacation" => AvailabilityStatus::Vacation,
                _ => AvailabilityStatus::Available,
            },
            delegate_id: self.delegate_id.map(UserId),
            start_at: self.start_at,
            end_at: self.end_at,
            reason: self.reason,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ExpertiseRow {
    user_id: Uuid,
    expertise_type: String,
    expertise_key: String,
    total_approved: i32,
    total_rejected: i32,
    avg_time_hours: Option<sqlx::types::BigDecimal>,
    expertise_score: sqlx::types::BigDecimal,
    last_used_at: Option<DateTime<Utc>>,
}

impl ExpertiseRow {
    fn into_expertise(self) -> ApproverExpertise {
        ApproverExpertise {
            user_id: UserId(self.user_id),
            expertise_type: self.expertise_type.parse().unwrap_or(ExpertiseType::Vendor),
            expertise_key: self.expertise_key,
            total_approved: self.total_approved,
            total_rejected: self.total_rejected,
            avg_time_hours: self.avg_time_hours.and_then(|d| d.to_f64()),
            expertise_score: self.expertise_score.to_f64().unwrap_or(0.5),
            last_used_at: self.last_used_at,
        }
    }
}

/// Input for setting approver availability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetAvailabilityInput {
    pub user_id: Uuid,
    pub status: AvailabilityStatusInput,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub delegate_id: Option<Uuid>,
    pub reason: Option<String>,
}

/// Availability status for API input
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AvailabilityStatusInput {
    Available,
    Busy,
    OutOfOffice,
    Vacation,
}

impl AvailabilityStatusInput {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Available => "available",
            Self::Busy => "busy",
            Self::OutOfOffice => "out_of_office",
            Self::Vacation => "vacation",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use billforge_core::intelligent_routing::{AvailabilityStatus, ExpertiseType};

    #[test]
    fn availability_status_input_as_str() {
        assert_eq!(AvailabilityStatusInput::Available.as_str(), "available");
        assert_eq!(AvailabilityStatusInput::Busy.as_str(), "busy");
        assert_eq!(AvailabilityStatusInput::OutOfOffice.as_str(), "out_of_office");
        assert_eq!(AvailabilityStatusInput::Vacation.as_str(), "vacation");
    }

    #[test]
    fn availability_status_roundtrip() {
        // Verify string representations match AvailabilityRow parsing
        let cases = vec![
            ("available", AvailabilityStatus::Available),
            ("busy", AvailabilityStatus::Busy),
            ("out_of_office", AvailabilityStatus::OutOfOffice),
            ("vacation", AvailabilityStatus::Vacation),
            ("unknown_value", AvailabilityStatus::Available), // default fallback
        ];

        for (s, expected) in cases {
            let row = AvailabilityRow {
                user_id: Uuid::new_v4(),
                status: s.to_string(),
                delegate_id: None,
                start_at: Utc::now(),
                end_at: Utc::now() + chrono::Duration::hours(1),
                reason: None,
            };
            let availability = row.into_availability();
            assert_eq!(availability.status, expected, "Status mismatch for '{}'", s);
        }
    }

    #[test]
    fn expertise_type_parse_roundtrip() {
        let types = vec![
            ("vendor", ExpertiseType::Vendor),
            ("department", ExpertiseType::Department),
            ("gl_code", ExpertiseType::GlCode),
            ("amount_range", ExpertiseType::AmountRange),
        ];

        for (s, expected) in types {
            let parsed: ExpertiseType = s.parse().expect("should parse");
            assert_eq!(parsed, expected);
            assert_eq!(parsed.to_string(), s);
        }
    }

    #[test]
    fn availability_input_serde_roundtrip() {
        let input = SetAvailabilityInput {
            user_id: Uuid::new_v4(),
            status: AvailabilityStatusInput::OutOfOffice,
            start_at: Utc::now(),
            end_at: Utc::now() + chrono::Duration::days(3),
            delegate_id: Some(Uuid::new_v4()),
            reason: Some("Vacation".to_string()),
        };

        let json = serde_json::to_string(&input).expect("serialize");
        let deserialized: SetAvailabilityInput = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.status.as_str(), "out_of_office");
        assert!(deserialized.delegate_id.is_some());
    }

    #[test]
    fn availability_input_without_delegate() {
        let input = SetAvailabilityInput {
            user_id: Uuid::new_v4(),
            status: AvailabilityStatusInput::Busy,
            start_at: Utc::now(),
            end_at: Utc::now() + chrono::Duration::hours(2),
            delegate_id: None,
            reason: None,
        };

        let json = serde_json::to_string(&input).expect("serialize");
        let deserialized: SetAvailabilityInput = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.status.as_str(), "busy");
        assert!(deserialized.delegate_id.is_none());
        assert!(deserialized.reason.is_none());
    }

    #[test]
    fn default_routing_config_has_correct_values() {
        let config = RoutingConfig::default();
        assert_eq!(config.workload_weight, 0.4);
        assert_eq!(config.expertise_weight, 0.3);
        assert_eq!(config.availability_weight, 0.3);
        assert_eq!(config.max_workload_score, 100.0);
        assert_eq!(config.min_expertise_score, 0.3);
        assert!(config.enable_auto_delegation);
        assert!(config.enable_pattern_learning);
        assert!(!config.enable_calendar_sync);
        assert_eq!(config.working_days, vec![1, 2, 3, 4, 5]);
    }
}
