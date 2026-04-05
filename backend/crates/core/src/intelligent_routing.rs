//! Intelligent Approval Routing System
//!
//! This module implements AI-powered dynamic routing for invoice approvals,
//! considering multiple factors:
//! - Approver workload balance
//! - Availability and out-of-office status
//! - Historical approval patterns and expertise
//! - Vendor/department specialization
//!
//! The system learns from outcomes to improve routing decisions over time.

use crate::{
    domain::Invoice,
    types::TenantId,
    Error, Result, UserId,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Bundle of routing data needed to call `route_invoice()`.
///
/// The `RoutingDataProvider` trait assembles this from the database so
/// callers do not need to gather each collection individually.
#[derive(Debug, Clone, Default)]
pub struct RoutingContext {
    pub eligible_approvers: Vec<UserId>,
    pub workloads: HashMap<UserId, ApproverWorkload>,
    pub availabilities: Vec<ApproverAvailability>,
    pub expertise: Vec<ApproverExpertise>,
}

impl RoutingContext {
    /// Convenience: delegate to `IntelligentRoutingEngine::route_invoice`
    /// using the data stored in this context.
    pub fn route(&self, engine: &IntelligentRoutingEngine, invoice: &Invoice) -> RoutingDecision {
        engine.route_invoice(
            invoice,
            &self.eligible_approvers,
            &self.workloads,
            &self.availabilities,
            &self.expertise,
        )
    }
}

/// Trait for fetching the routing data a tenant needs for intelligent approval routing.
///
/// A concrete implementation that hits the database will be provided in a follow-up;
/// for now tests use mocks.
#[async_trait::async_trait]
pub trait RoutingDataProvider: Send + Sync {
    async fn get_routing_context(&self, tenant_id: &TenantId) -> Result<RoutingContext>;
}

/// Routing decision with reasoning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingDecision {
    /// Selected approver (if any available)
    pub approver_id: Option<UserId>,
    /// Routing strategy used
    pub strategy: RoutingStrategy,
    /// Score for the selected approver
    pub score: f64,
    /// All candidates considered with their scores
    pub candidates: Vec<CandidateScore>,
    /// Factors that influenced the decision
    pub factors: RoutingFactors,
    /// Whether delegation was applied
    pub delegated_from: Option<UserId>,
}

/// Strategy used for routing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingStrategy {
    /// Route to least loaded approver
    LeastLoaded,
    /// Round-robin distribution
    RoundRobin,
    /// Route based on vendor/department expertise
    ExpertBased,
    /// Route based on availability (OOO, working hours)
    AvailabilityBased,
    /// Hybrid approach combining all factors
    Hybrid,
    /// Fallback when no intelligent routing possible
    Fallback,
}

/// Score breakdown for a candidate approver
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateScore {
    pub user_id: UserId,
    /// Overall score (0.0 to 1.0)
    pub score: f64,
    /// Workload score component
    pub workload_score: f64,
    /// Expertise score component
    pub expertise_score: f64,
    /// Availability score component
    pub availability_score: f64,
    /// Reason for this score
    pub reason: String,
}

/// Factors used in routing decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingFactors {
    pub workload_weight: f64,
    pub expertise_weight: f64,
    pub availability_weight: f64,
    pub invoice_amount: i64,
    pub vendor_id: Option<Uuid>,
    pub department: Option<String>,
}

/// Approver workload metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApproverWorkload {
    pub user_id: UserId,
    pub active_approvals: i32,
    pub pending_approvals: i32,
    pub completed_this_week: i32,
    pub avg_approval_time_hours: Option<f64>,
    /// Workload score (0 = unloaded, 100 = max loaded)
    pub workload_score: f64,
    pub last_assignment_at: Option<DateTime<Utc>>,
}

/// Approver availability status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApproverAvailability {
    pub user_id: UserId,
    pub status: AvailabilityStatus,
    pub delegate_id: Option<UserId>,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub reason: Option<String>,
}

/// Availability status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AvailabilityStatus {
    Available,
    Busy,
    OutOfOffice,
    Vacation,
}

/// Approver expertise in a specific area
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApproverExpertise {
    pub user_id: UserId,
    pub expertise_type: ExpertiseType,
    pub expertise_key: String,
    pub total_approved: i32,
    pub total_rejected: i32,
    pub avg_time_hours: Option<f64>,
    /// Expertise score (0.0 to 1.0)
    pub expertise_score: f64,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// Type of expertise
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpertiseType {
    Vendor,
    Department,
    GlCode,
    AmountRange,
}

impl std::fmt::Display for ExpertiseType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Vendor => write!(f, "vendor"),
            Self::Department => write!(f, "department"),
            Self::GlCode => write!(f, "gl_code"),
            Self::AmountRange => write!(f, "amount_range"),
        }
    }
}

impl std::str::FromStr for ExpertiseType {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "vendor" => Ok(Self::Vendor),
            "department" => Ok(Self::Department),
            "gl_code" => Ok(Self::GlCode),
            "amount_range" => Ok(Self::AmountRange),
            _ => Err(Error::Validation(format!(
                "Invalid expertise type: {}",
                s
            ))),
        }
    }
}

/// Routing configuration for a tenant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfig {
    pub tenant_id: TenantId,
    pub workload_weight: f64,
    pub expertise_weight: f64,
    pub availability_weight: f64,
    pub max_workload_score: f64,
    pub min_expertise_score: f64,
    pub enable_auto_delegation: bool,
    pub enable_pattern_learning: bool,
    pub enable_calendar_sync: bool,
    pub working_hours_start: chrono::NaiveTime,
    pub working_hours_end: chrono::NaiveTime,
    pub working_timezone: String,
    pub working_days: Vec<i32>,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            tenant_id: TenantId::new(),
            workload_weight: 0.4,
            expertise_weight: 0.3,
            availability_weight: 0.3,
            max_workload_score: 100.0,
            min_expertise_score: 0.3,
            enable_auto_delegation: true,
            enable_pattern_learning: true,
            enable_calendar_sync: false,
            working_hours_start: chrono::NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            working_hours_end: chrono::NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
            working_timezone: "UTC".to_string(),
            working_days: vec![1, 2, 3, 4, 5], // Mon-Fri
        }
    }
}

/// Intelligent routing engine
pub struct IntelligentRoutingEngine {
    config: RoutingConfig,
}

impl IntelligentRoutingEngine {
    /// Create a new routing engine with configuration
    pub fn new(config: RoutingConfig) -> Self {
        Self { config }
    }

    /// Calculate the best approver for an invoice
    ///
    /// # Arguments
    /// * `invoice` - The invoice to route
    /// * `eligible_approvers` - List of approver IDs who can approve this invoice
    /// * `workloads` - Current workload metrics for each approver
    /// * `availabilities` - Availability status for each approver
    /// * `expertise` - Expertise records for approvers
    ///
    /// # Returns
    /// A routing decision with the selected approver and reasoning
    pub fn route_invoice(
        &self,
        invoice: &Invoice,
        eligible_approvers: &[UserId],
        workloads: &HashMap<UserId, ApproverWorkload>,
        availabilities: &[ApproverAvailability],
        expertise: &[ApproverExpertise],
    ) -> RoutingDecision {
        // If no eligible approvers, return fallback
        if eligible_approvers.is_empty() {
            return RoutingDecision {
                approver_id: None,
                strategy: RoutingStrategy::Fallback,
                score: 0.0,
                candidates: vec![],
                factors: self.build_factors(invoice),
                delegated_from: None,
            };
        }

        // Score all candidates
        let candidates: Vec<CandidateScore> = eligible_approvers
            .iter()
            .map(|user_id| {
                self.score_candidate(user_id, invoice, workloads, availabilities, expertise)
            })
            .collect();

        // Partition into available (availability_score > 0.0) vs unavailable
        let available: Vec<&CandidateScore> = candidates
            .iter()
            .filter(|c| c.availability_score > 0.0)
            .collect();

        // Pick best from available pool; fall back to full pool only if all are unavailable
        let (pool, strategy_override): (Vec<&CandidateScore>, Option<RoutingStrategy>) =
            if available.is_empty() {
                (
                    candidates.iter().collect(),
                    Some(RoutingStrategy::Fallback),
                )
            } else {
                (available, None)
            };

        // Find the best candidate from the selected pool
        let best = pool
            .iter()
            .max_by(|a, b| {
                a.score
                    .partial_cmp(&b.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .expect("At least one candidate exists");

        // Check if delegation applies
        let (final_approver, delegated_from) = self.apply_delegation(
            best.user_id.clone(),
            availabilities,
        );

        // Determine strategy used (respect override when all unavailable)
        let strategy = strategy_override
            .unwrap_or_else(|| self.determine_strategy(&pool.iter().map(|cs| (*cs).clone()).collect::<Vec<_>>()));

        RoutingDecision {
            approver_id: Some(final_approver),
            strategy,
            score: best.score,
            candidates,
            factors: self.build_factors(invoice),
            delegated_from,
        }
    }

    /// Score a candidate approver
    fn score_candidate(
        &self,
        user_id: &UserId,
        invoice: &Invoice,
        workloads: &HashMap<UserId, ApproverWorkload>,
        availabilities: &[ApproverAvailability],
        expertise: &[ApproverExpertise],
    ) -> CandidateScore {
        // Calculate workload score (lower is better, invert for final score)
        let workload = workloads.get(user_id);
        let raw_workload_score = workload
            .map(|w| w.workload_score / self.config.max_workload_score)
            .unwrap_or(0.0);
        let workload_score = 1.0 - raw_workload_score.min(1.0);

        // Calculate expertise score
        let expertise_score = self.calculate_expertise_score(user_id, invoice, expertise);

        // Calculate availability score
        let availability_score = self.calculate_availability_score(user_id, availabilities);

        // Combine scores with weights
        let score = (workload_score * self.config.workload_weight)
            + (expertise_score * self.config.expertise_weight)
            + (availability_score * self.config.availability_weight);

        // Determine reason
        let reason = self.determine_reason(workload_score, expertise_score, availability_score);

        CandidateScore {
            user_id: user_id.clone(),
            score,
            workload_score,
            expertise_score,
            availability_score,
            reason,
        }
    }

    /// Calculate expertise score for an approver on this invoice
    fn calculate_expertise_score(
        &self,
        user_id: &UserId,
        invoice: &Invoice,
        expertise: &[ApproverExpertise],
    ) -> f64 {
        let mut scores: Vec<f64> = vec![];

        // Check vendor expertise
        if let Some(vendor_id) = invoice.vendor_id {
            let vendor_expertise = expertise.iter().find(|e| {
                e.user_id == *user_id
                    && e.expertise_type == ExpertiseType::Vendor
                    && e.expertise_key == vendor_id.to_string()
            });
            if let Some(exp) = vendor_expertise {
                scores.push(exp.expertise_score);
            }
        }

        // Check department expertise
        if let Some(ref dept) = invoice.department {
            let dept_expertise = expertise.iter().find(|e| {
                e.user_id == *user_id
                    && e.expertise_type == ExpertiseType::Department
                    && e.expertise_key == *dept
            });
            if let Some(exp) = dept_expertise {
                scores.push(exp.expertise_score);
            }
        }

        // Check GL code expertise
        if let Some(ref gl_code) = invoice.gl_code {
            let gl_expertise = expertise.iter().find(|e| {
                e.user_id == *user_id
                    && e.expertise_type == ExpertiseType::GlCode
                    && e.expertise_key == *gl_code
            });
            if let Some(exp) = gl_expertise {
                scores.push(exp.expertise_score);
            }
        }

        // Average all matching expertise scores
        if scores.is_empty() {
            0.5 // Default neutral score
        } else {
            scores.iter().sum::<f64>() / scores.len() as f64
        }
    }

    /// Calculate availability score for an approver
    fn calculate_availability_score(
        &self,
        user_id: &UserId,
        availabilities: &[ApproverAvailability],
    ) -> f64 {
        let now = Utc::now();

        // Find active availability record
        let active_availability = availabilities.iter().find(|a| {
            a.user_id == *user_id && a.start_at <= now && a.end_at > now
        });

        match active_availability {
            Some(availability) => {
                match availability.status {
                    AvailabilityStatus::Available => 1.0,
                    AvailabilityStatus::Busy => 0.3,
                    AvailabilityStatus::OutOfOffice => 0.0,
                    AvailabilityStatus::Vacation => 0.0,
                }
            }
            None => {
                // No explicit availability record - check working hours
                if self.is_within_working_hours(now) {
                    1.0
                } else {
                    0.5 // Outside working hours but no explicit OOO
                }
            }
        }
    }

    /// Check if a timestamp is within working hours
    fn is_within_working_hours(&self, timestamp: DateTime<Utc>) -> bool {
        use chrono::Datelike;

        // Check day of week (1 = Monday, 7 = Sunday)
        let weekday = timestamp.weekday().number_from_monday() as i32;
        if !self.config.working_days.contains(&weekday) {
            return false;
        }

        // Check time of day
        let time = timestamp.time();
        time >= self.config.working_hours_start && time <= self.config.working_hours_end
    }

    /// Apply delegation if approver is unavailable
    fn apply_delegation(
        &self,
        approver_id: UserId,
        availabilities: &[ApproverAvailability],
    ) -> (UserId, Option<UserId>) {
        if !self.config.enable_auto_delegation {
            return (approver_id, None);
        }

        let now = Utc::now();

        // Check if approver is unavailable
        let unavailable = availabilities.iter().find(|a| {
            a.user_id == approver_id
                && a.start_at <= now
                && a.end_at > now
                && matches!(
                    a.status,
                    AvailabilityStatus::OutOfOffice | AvailabilityStatus::Vacation
                )
                && a.delegate_id.is_some()
        });

        match unavailable {
            Some(availability) => {
                // Delegate to the specified person
                let delegate = availability.delegate_id.clone().unwrap();
                (delegate, Some(approver_id))
            }
            None => (approver_id, None),
        }
    }

    /// Determine which routing strategy was primarily used
    fn determine_strategy(&self, candidates: &[CandidateScore]) -> RoutingStrategy {
        if candidates.is_empty() {
            return RoutingStrategy::Fallback;
        }

        // Check if expertise was a major differentiator
        let expertise_range = candidates.iter().map(|c| c.expertise_score).fold(
            (f64::MAX, f64::MIN),
            |(min, max), score| (min.min(score), max.max(score)),
        );

        if expertise_range.1 - expertise_range.0 > 0.3 {
            return RoutingStrategy::ExpertBased;
        }

        // Check if workload was a major differentiator
        let workload_range = candidates.iter().map(|c| c.workload_score).fold(
            (f64::MAX, f64::MIN),
            |(min, max), score| (min.min(score), max.max(score)),
        );

        if workload_range.1 - workload_range.0 > 0.3 {
            return RoutingStrategy::LeastLoaded;
        }

        // Default to hybrid
        RoutingStrategy::Hybrid
    }

    /// Build routing factors from invoice
    fn build_factors(&self, invoice: &Invoice) -> RoutingFactors {
        RoutingFactors {
            workload_weight: self.config.workload_weight,
            expertise_weight: self.config.expertise_weight,
            availability_weight: self.config.availability_weight,
            invoice_amount: invoice.total_amount.amount,
            vendor_id: invoice.vendor_id,
            department: invoice.department.clone(),
        }
    }

    /// Determine the reason for a candidate's score
    fn determine_reason(
        &self,
        workload_score: f64,
        expertise_score: f64,
        availability_score: f64,
    ) -> String {
        let max_score = workload_score.max(expertise_score).max(availability_score);

        if (workload_score - max_score).abs() < 0.01 {
            "lowest_workload".to_string()
        } else if (expertise_score - max_score).abs() < 0.01 {
            "highest_expertise".to_string()
        } else if (availability_score - max_score).abs() < 0.01 {
            "best_availability".to_string()
        } else {
            "balanced".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{CaptureStatus, ProcessingStatus};
    use crate::types::Money;
    use chrono::NaiveDate;
    use uuid::Uuid;

    fn create_test_invoice() -> Invoice {
        Invoice {
            id: crate::domain::InvoiceId::new(),
            tenant_id: TenantId::new(),
            vendor_id: Some(Uuid::new_v4()),
            vendor_name: "Test Vendor".to_string(),
            invoice_number: "INV-001".to_string(),
            invoice_date: Some(NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()),
            due_date: Some(NaiveDate::from_ymd_opt(2026, 4, 10).unwrap()),
            po_number: None,
            subtotal: Some(Money {
                amount: 10000,
                currency: "USD".to_string(),
            }),
            tax_amount: Some(Money {
                amount: 800,
                currency: "USD".to_string(),
            }),
            total_amount: Money {
                amount: 10800,
                currency: "USD".to_string(),
            },
            currency: "USD".to_string(),
            line_items: vec![],
            capture_status: CaptureStatus::Reviewed,
            processing_status: ProcessingStatus::Draft,
            current_queue_id: None,
            assigned_to: None,
            document_id: Uuid::new_v4(),
            supporting_documents: vec![],
            ocr_confidence: Some(0.95),
            categorization_confidence: None,
            department: Some("Engineering".to_string()),
            gl_code: Some("5000".to_string()),
            cost_center: None,
            notes: None,
            tags: vec![],
            custom_fields: serde_json::json!({}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            created_by: UserId(Uuid::new_v4()),
        }
    }

    #[test]
    fn test_route_invoice_least_loaded() {
        let config = RoutingConfig {
            workload_weight: 0.8,
            expertise_weight: 0.1,
            availability_weight: 0.1,
            ..Default::default()
        };
        let engine = IntelligentRoutingEngine::new(config);

        let invoice = create_test_invoice();
        let approver1 = UserId(Uuid::new_v4());
        let approver2 = UserId(Uuid::new_v4());

        let workloads = HashMap::from([
            (
                approver1.clone(),
                ApproverWorkload {
                    user_id: approver1.clone(),
                    active_approvals: 10,
                    pending_approvals: 5,
                    completed_this_week: 20,
                    avg_approval_time_hours: Some(24.0),
                    workload_score: 80.0,
                    last_assignment_at: Some(Utc::now()),
                },
            ),
            (
                approver2.clone(),
                ApproverWorkload {
                    user_id: approver2.clone(),
                    active_approvals: 2,
                    pending_approvals: 1,
                    completed_this_week: 5,
                    avg_approval_time_hours: Some(12.0),
                    workload_score: 20.0,
                    last_assignment_at: Some(Utc::now()),
                },
            ),
        ]);

        let decision = engine.route_invoice(
            &invoice,
            &[approver1.clone(), approver2.clone()],
            &workloads,
            &[],
            &[],
        );

        assert!(decision.approver_id.is_some());
        assert_eq!(decision.approver_id.unwrap(), approver2); // Lower workload
        assert_eq!(decision.strategy, RoutingStrategy::LeastLoaded);
    }

    #[test]
    fn test_route_invoice_with_delegation() {
        let config = RoutingConfig {
            enable_auto_delegation: true,
            ..Default::default()
        };
        let engine = IntelligentRoutingEngine::new(config);

        let invoice = create_test_invoice();
        let approver1 = UserId(Uuid::new_v4());
        let delegate = UserId(Uuid::new_v4());

        let decision = engine.route_invoice(
            &invoice,
            &[approver1.clone()],
            &HashMap::new(),
            &[ApproverAvailability {
                user_id: approver1.clone(),
                status: AvailabilityStatus::OutOfOffice,
                delegate_id: Some(delegate.clone()),
                start_at: Utc::now() - chrono::Duration::hours(1),
                end_at: Utc::now() + chrono::Duration::hours(24),
                reason: Some("Vacation".to_string()),
            }],
            &[],
        );

        assert!(decision.approver_id.is_some());
        assert_eq!(decision.approver_id.unwrap(), delegate);
        assert_eq!(decision.delegated_from, Some(approver1));
    }

    #[test]
    fn test_expertise_scoring() {
        let config = RoutingConfig::default();
        let engine = IntelligentRoutingEngine::new(config);

        let vendor_id = Uuid::new_v4();
        let invoice = Invoice {
            vendor_id: Some(vendor_id),
            department: Some("Engineering".to_string()),
            ..create_test_invoice()
        };

        let approver1 = UserId(Uuid::new_v4());
        let approver2 = UserId(Uuid::new_v4());

        let expertise = vec![
            ApproverExpertise {
                user_id: approver1.clone(),
                expertise_type: ExpertiseType::Vendor,
                expertise_key: vendor_id.to_string(),
                total_approved: 50,
                total_rejected: 2,
                avg_time_hours: Some(8.0),
                expertise_score: 0.95,
                last_used_at: Some(Utc::now()),
            },
            ApproverExpertise {
                user_id: approver2.clone(),
                expertise_type: ExpertiseType::Vendor,
                expertise_key: vendor_id.to_string(),
                total_approved: 10,
                total_rejected: 5,
                avg_time_hours: Some(48.0),
                expertise_score: 0.45,
                last_used_at: Some(Utc::now()),
            },
        ];

        let decision = engine.route_invoice(
            &invoice,
            &[approver1.clone(), approver2.clone()],
            &HashMap::new(),
            &[],
            &expertise,
        );

        assert!(decision.approver_id.is_some());
        // Approver 1 should win due to higher expertise score
        // (assuming availability and workload are equal)
    }

    #[test]
    fn test_available_approver_preferred_over_unavailable_expert() {
        // approver1: available but low expertise; approver2: out-of-office with high expertise
        let config = RoutingConfig {
            workload_weight: 0.1,
            expertise_weight: 0.8,
            availability_weight: 0.1,
            ..Default::default()
        };
        let engine = IntelligentRoutingEngine::new(config);

        let vendor_id = Uuid::new_v4();
        let invoice = Invoice {
            vendor_id: Some(vendor_id),
            ..create_test_invoice()
        };

        let approver1 = UserId(Uuid::new_v4()); // available, low expertise
        let approver2 = UserId(Uuid::new_v4()); // out-of-office, high expertise

        let expertise = vec![
            ApproverExpertise {
                user_id: approver1.clone(),
                expertise_type: ExpertiseType::Vendor,
                expertise_key: vendor_id.to_string(),
                total_approved: 5,
                total_rejected: 1,
                avg_time_hours: Some(24.0),
                expertise_score: 0.2,
                last_used_at: Some(Utc::now()),
            },
            ApproverExpertise {
                user_id: approver2.clone(),
                expertise_type: ExpertiseType::Vendor,
                expertise_key: vendor_id.to_string(),
                total_approved: 100,
                total_rejected: 2,
                avg_time_hours: Some(4.0),
                expertise_score: 0.95,
                last_used_at: Some(Utc::now()),
            },
        ];

        let availabilities = vec![ApproverAvailability {
            user_id: approver2.clone(),
            status: AvailabilityStatus::OutOfOffice,
            delegate_id: None,
            start_at: Utc::now() - chrono::Duration::hours(1),
            end_at: Utc::now() + chrono::Duration::hours(24),
            reason: Some("Sick leave".to_string()),
        }];

        let decision = engine.route_invoice(
            &invoice,
            &[approver1.clone(), approver2.clone()],
            &HashMap::new(),
            &availabilities,
            &expertise,
        );

        assert!(decision.approver_id.is_some());
        assert_eq!(decision.approver_id.unwrap(), approver1);
        assert_ne!(decision.strategy, RoutingStrategy::Fallback);
    }

    #[test]
    fn test_all_unavailable_returns_fallback_strategy() {
        let config = RoutingConfig {
            enable_auto_delegation: true,
            ..Default::default()
        };
        let engine = IntelligentRoutingEngine::new(config);

        let invoice = create_test_invoice();
        let approver1 = UserId(Uuid::new_v4());
        let approver2 = UserId(Uuid::new_v4());

        let availabilities = vec![
            ApproverAvailability {
                user_id: approver1.clone(),
                status: AvailabilityStatus::Vacation,
                delegate_id: None,
                start_at: Utc::now() - chrono::Duration::hours(1),
                end_at: Utc::now() + chrono::Duration::hours(168),
                reason: Some("Vacation".to_string()),
            },
            ApproverAvailability {
                user_id: approver2.clone(),
                status: AvailabilityStatus::OutOfOffice,
                delegate_id: None,
                start_at: Utc::now() - chrono::Duration::hours(1),
                end_at: Utc::now() + chrono::Duration::hours(24),
                reason: Some("Sick leave".to_string()),
            },
        ];

        let decision = engine.route_invoice(
            &invoice,
            &[approver1.clone(), approver2.clone()],
            &HashMap::new(),
            &availabilities,
            &[],
        );

        // Should still pick someone (best of the unavailable)
        assert!(decision.approver_id.is_some());
        assert_eq!(decision.strategy, RoutingStrategy::Fallback);
        // Delegation was attempted (no delegates configured, so delegated_from is None)
        assert_eq!(decision.delegated_from, None);
    }

    #[test]
    fn test_routing_context_route_delegates_to_engine() {
        let engine = IntelligentRoutingEngine::new(RoutingConfig {
            workload_weight: 0.8,
            expertise_weight: 0.1,
            availability_weight: 0.1,
            ..Default::default()
        });

        let approver = UserId(Uuid::new_v4());

        let ctx = RoutingContext {
            eligible_approvers: vec![approver.clone()],
            workloads: HashMap::from([(
                approver.clone(),
                ApproverWorkload {
                    user_id: approver.clone(),
                    active_approvals: 0,
                    pending_approvals: 0,
                    completed_this_week: 10,
                    avg_approval_time_hours: Some(4.0),
                    workload_score: 5.0,
                    last_assignment_at: None,
                },
            )]),
            availabilities: vec![],
            expertise: vec![],
        };

        let invoice = create_test_invoice();
        let decision = ctx.route(&engine, &invoice);

        assert!(decision.approver_id.is_some());
        assert_eq!(decision.approver_id.unwrap(), approver);
    }

    #[test]
    fn test_routing_context_empty_approvers_returns_fallback() {
        let engine = IntelligentRoutingEngine::new(RoutingConfig::default());
        let ctx = RoutingContext::default();
        let invoice = create_test_invoice();

        let decision = ctx.route(&engine, &invoice);
        assert!(decision.approver_id.is_none());
        assert_eq!(decision.strategy, RoutingStrategy::Fallback);
    }
}
