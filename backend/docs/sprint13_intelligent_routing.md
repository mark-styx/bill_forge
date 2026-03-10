# Sprint 13 Feature #7: Intelligent Approval Routing

## Overview

The Intelligent Approval Routing system transforms static approval workflows into dynamic, AI-powered routing that considers multiple factors to optimize approval speed and quality.

## Problem Statement

Traditional approval routing uses static rules that don't adapt to:
- Team workload imbalances
- Approver availability (OOO, vacation, meetings)
- Historical performance patterns
- Vendor/department expertise

This results in:
- Bottlenecks when key approvers are overloaded
- Delayed approvals when approvers are unavailable
- Suboptimal routing that doesn't leverage expertise
- "Stuck" approvals waiting >7 days

## Solution

Multi-factor intelligent routing that balances:
1. **Workload Distribution** (40% weight)
   - Tracks active/pending approvals per approver
   - Calculates workload scores (0-100 scale)
   - Routes to least loaded eligible approver

2. **Expertise Matching** (30% weight)
   - Learns vendor/department/GL code expertise
   - Tracks approval speed and accuracy per area
   - Scores expertise from 0.0-1.0 based on outcomes

3. **Availability Detection** (30% weight)
   - Calendar integration (Google, Outlook)
   - Working hours configuration
   - Auto-delegation for OOO approvers

## Architecture

### Core Components

#### 1. Intelligent Routing Engine (`intelligent_routing.rs`)
```rust
pub struct IntelligentRoutingEngine {
    config: RoutingConfig,
}

impl IntelligentRoutingEngine {
    pub fn route_invoice(
        &self,
        invoice: &Invoice,
        eligible_approvers: &[UserId],
        workloads: &HashMap<UserId, ApproverWorkload>,
        availabilities: &[ApproverAvailability],
        expertise: &[ApproverExpertise],
    ) -> RoutingDecision;
}
```

**Key Features:**
- Combines 3 scoring factors with configurable weights
- Supports 5 routing strategies (least loaded, round-robin, expert-based, availability-based, hybrid)
- Auto-delegation when approvers are unavailable
- Detailed decision logging with candidate scores

#### 2. Workload Balancer (`workload_balancer.rs`)
```rust
pub struct WorkloadBalancer {
    config: WorkloadBalancerConfig,
}

impl WorkloadBalancer {
    pub fn calculate_workload_score(
        &self,
        active_approvals: i32,
        pending_approvals: i32,
        avg_approval_time_hours: Option<f64>,
    ) -> f64;

    pub fn suggest_redistribution(
        &self,
        workloads: &[ApproverWorkload],
    ) -> Vec<RedistributionSuggestion>;
}
```

**Key Features:**
- Real-time workload scoring (0 = no load, 100 = max)
- Speed bonuses for fast approvers
- Automatic redistribution suggestions when imbalanced
- Distribution statistics (variance, overloaded/underloaded counts)

#### 3. Availability Manager (`approver_availability.rs`)
```rust
pub struct AvailabilityManager {
    calendar_integrations: HashMap<String, Box<dyn CalendarIntegration>>,
}

impl AvailabilityManager {
    pub fn check_availability(
        &self,
        availability_records: &[ApproverAvailability],
        working_hours: &WorkingHoursConfig,
    ) -> AvailabilityStatus;

    pub async fn sync_calendar_events(
        &self,
        user_id: &UserId,
        calendar_source: &str,
    ) -> Result<Vec<ApproverAvailability>>;
}
```

**Key Features:**
- Google Calendar / Microsoft Outlook integration
- Working hours and timezone support
- OOO detection with auto-delegation
- Future availability prediction

#### 4. Background Job: Routing Optimization (`routing_optimization.rs`)
```rust
pub async fn run_routing_optimization(pool: &PgPool) -> Result<()>;
```

**Scheduled Tasks:**
- Update workload scores every 5 minutes
- Update expertise scores from recent approvals (daily)
- Check workload balance and generate alerts
- Clean up old routing logs (90 days retention)

### Database Schema

#### `approver_workload_tracking`
Tracks real-time workload metrics for each approver.

**Key Columns:**
- `active_approvals`: Currently assigned approvals
- `pending_approvals`: Awaiting response
- `workload_score`: Composite score (0-100)
- `avg_approval_time_hours`: Performance metric

#### `routing_optimization_log`
Audit trail for all routing decisions.

**Key Columns:**
- `routing_strategy`: Which strategy was used
- `selected_approver_id`: Final choice
- `candidate_approvers`: All candidates with scores
- `routing_factors`: Weights and invoice details
- `outcome`: tracked result (approved/rejected/escalated)
- `was_optimal`: Did it meet SLA?

#### `approver_availability`
Calendar and availability tracking.

**Key Columns:**
- `status`: available/busy/out_of_office/vacation
- `delegate_id`: Auto-delegate when unavailable
- `calendar_source`: Integration source
- `start_at`, `end_at`: Time window

#### `approver_expertise`
Learned expertise scores by area.

**Key Columns:**
- `expertise_type`: vendor/department/gl_code/amount_range
- `expertise_key`: Specific value (e.g., vendor_id)
- `expertise_score`: 0.0-1.0 based on outcomes
- `total_approved`, `total_rejected`: Volume stats
- `avg_time_hours`: Speed metric

#### `routing_configuration`
Tenant-level routing settings.

**Key Columns:**
- `workload_weight`, `expertise_weight`, `availability_weight`: Factor weights (should sum to 1.0)
- `enable_auto_delegation`: Auto-delegate for OOO
- `enable_pattern_learning`: Learn from outcomes
- `working_hours_start`, `working_hours_end`: Time config
- `working_days`: Mon-Fri as `[1,2,3,4,5]`

## API Endpoints

### 1. Get Routing Decision
```http
POST /api/v1/invoices/{id}/route
```

Returns intelligent routing recommendation for an invoice.

**Response:**
```json
{
  "approver_id": "uuid",
  "strategy": "hybrid",
  "score": 0.87,
  "candidates": [
    {
      "user_id": "uuid",
      "score": 0.87,
      "workload_score": 0.9,
      "expertise_score": 0.95,
      "availability_score": 1.0,
      "reason": "highest_expertise"
    }
  ],
  "factors": {
    "workload_weight": 0.4,
    "expertise_weight": 0.3,
    "availability_weight": 0.3
  }
}
```

### 2. Get Workload Distribution
```http
GET /api/v1/routing/workload
```

Returns workload statistics for all approvers.

**Response:**
```json
{
  "average_workload": 45.5,
  "max_workload": 85.0,
  "min_workload": 15.0,
  "std_deviation": 20.3,
  "variance_coefficient": 44.6,
  "overloaded_count": 2,
  "underloaded_count": 3,
  "suggestions": [
    {
      "from_user_id": "uuid",
      "to_user_id": "uuid",
      "suggested_transfers": 3,
      "reason": "Redistribute from 85.0% loaded to 15.0% loaded",
      "priority": "high"
    }
  ]
}
```

### 3. Update Availability
```http
POST /api/v1/users/{id}/availability
```

Set availability status for an approver.

**Request:**
```json
{
  "status": "out_of_office",
  "start_at": "2026-03-15T00:00:00Z",
  "end_at": "2026-03-22T23:59:59Z",
  "delegate_id": "uuid",
  "reason": "Vacation"
}
```

### 4. Configure Routing
```http
PUT /api/v1/routing/config
```

Update routing configuration for tenant.

**Request:**
```json
{
  "workload_weight": 0.5,
  "expertise_weight": 0.3,
  "availability_weight": 0.2,
  "enable_auto_delegation": true,
  "enable_pattern_learning": true
}
```

## Success Metrics

### Target Goals (Sprint 13)
- **35% reduction in average approval time** (from 48 hours to 31 hours)
- **90% elimination of stuck approvals** (>7 days pending)
- **Workload variance < 20%** across approvers
- **80% routing decision confidence** (score >= 0.8)

### Measurement
- Track approval times before/after routing
- Monitor workload distribution daily
- Analyze routing log outcomes
- Survey approver satisfaction

## Configuration

### Default Weights
```rust
workload_weight: 0.4    // 40% priority on load balancing
expertise_weight: 0.3   // 30% priority on expertise
availability_weight: 0.3 // 30% priority on availability
```

### Thresholds
```rust
max_workload_score: 100.0      // Maximum score before "overloaded"
min_expertise_score: 0.3       // Minimum expertise to consider
overloaded_threshold: 80.0     // Workload score >= 80 = overloaded
underloaded_threshold: 20.0    // Workload score <= 20 = underloaded
```

### Working Hours
```rust
working_hours_start: "09:00:00"  // 9 AM
working_hours_end: "17:00:00"    // 5 PM
working_timezone: "UTC"
working_days: [1, 2, 3, 4, 5]    // Mon-Fri
```

## Future Enhancements

### Sprint 14+
1. **Machine Learning Model** (instead of heuristic scoring)
   - Train on historical outcomes
   - Feature engineering for invoice characteristics
   - Continuous learning from corrections

2. **Advanced Calendar Integration**
   - Real-time Google Calendar sync
   - Microsoft Graph API integration
   - Slack/Teams status integration

3. **Routing Strategy A/B Testing**
   - Test different weight combinations
   - Measure impact on approval times
   - Automatic strategy optimization

4. **Predictive Routing**
   - Predict future workload based on pipeline
   - Pre-assign approvers for predictable vendors
   - Time-based routing (avoid lunch, evenings)

## Testing

### Unit Tests
All core modules include comprehensive unit tests:
- `intelligent_routing.rs`: 3 tests (least loaded, delegation, expertise)
- `workload_balancer.rs`: 4 tests (score calc, finding, stats, redistribution)
- `approver_availability.rs`: 5 tests (working hours, OOO, availability checks)

### Integration Tests
Run with: `cargo test --lib`

### Manual Testing
```bash
# Get routing decision for invoice
curl -X POST http://localhost:8080/api/v1/invoices/{id}/route

# Check workload distribution
curl http://localhost:8080/api/v1/routing/workload

# Set availability
curl -X POST http://localhost:8080/api/v1/users/{id}/availability \
  -H "Content-Type: application/json" \
  -d '{"status":"out_of_office","start_at":"...","delegate_id":"..."}'
```

## Migration

### Database Migration
```bash
# Run migration
sqlx migrate run

# Verify tables created
psql -d billforge -c "\dt approver_*"
psql -d billforge -c "\dt routing_*"
```

### Backfill Data
```sql
-- Initialize workload tracking for existing approvers
INSERT INTO approver_workload_tracking (id, tenant_id, user_id)
SELECT
    uuid_generate_v4(),
    tenant_id,
    user_id
FROM users
WHERE role = 'approver';

-- Seed initial expertise from historical approvals
INSERT INTO approver_expertise (...)
SELECT ... FROM approval_requests WHERE ...;
```

## Monitoring

### Key Metrics
- `routing_decisions_total{strategy, outcome}`: Routing decisions by strategy
- `approval_time_hours`: Histogram of approval times
- `workload_score{user_id}`: Gauge per approver
- `expertise_score{user_id, type, key}`: Expertise metrics

### Alerts
- Workload variance > 30%
- Approver stuck > 7 days
- Routing decision score < 0.5
- Delegation loop detected

## Dependencies

### New Dependencies
- `chrono-tz = "0.8"`: Timezone support for working hours

### Existing Dependencies
- `chrono`: Date/time handling
- `serde`, `serde_json`: Serialization
- `uuid`: ID generation
- `sqlx`: Database operations
- `tracing`: Logging

## Files Changed

### New Files
- `crates/core/src/intelligent_routing.rs` (590 lines)
- `crates/core/src/workload_balancer.rs` (330 lines)
- `crates/core/src/approver_availability.rs` (470 lines)
- `crates/worker/src/jobs/routing_optimization.rs` (460 lines)
- `migrations/051_add_intelligent_routing.sql` (150 lines)

### Modified Files
- `crates/core/src/lib.rs`: Added module exports
- `crates/core/Cargo.toml`: Added `chrono-tz` dependency
- `crates/worker/src/jobs/mod.rs`: Added routing optimization job

## References

- [Sprint 13 Feature Plan](../sprint_13_feature_plan.md#p0-7-intelligent-approval-routing-3-days)
- [Workflow Domain Model](../crates/core/src/domain/workflow.rs)
- [Assignment Rules](../crates/core/src/domain/workflow.rs#L424-L497)
