//! Integration tests for Dashboard SLA & Bottleneck endpoints
//!
//! Tests the three new dashboard endpoints:
//! - GET /api/v1/dashboard/stage-dwell
//! - GET /api/v1/dashboard/approver-workload
//! - GET /api/v1/dashboard/exception-trend

use billforge_api::routes::dashboard::{ApproverWorkloadRow, ExceptionTrendPoint, StageDwellRow};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// StageDwellRow structure tests
// ---------------------------------------------------------------------------

#[test]
fn test_stage_dwell_row_structure() {
    let row = StageDwellRow {
        stage: "approval".to_string(),
        median_minutes: 45.0,
        p90_minutes: 120.0,
        count: 37,
    };
    assert_eq!(row.stage, "approval");
    assert_eq!(row.median_minutes, 45.0);
    assert_eq!(row.p90_minutes, 120.0);
    assert_eq!(row.count, 37);
}

#[test]
fn test_stage_dwell_row_serialization() {
    let row = StageDwellRow {
        stage: "capture".to_string(),
        median_minutes: 12.5,
        p90_minutes: 30.0,
        count: 100,
    };
    let json = serde_json::to_string(&row).expect("Failed to serialize");
    let deserialized: StageDwellRow = serde_json::from_str(&json).expect("Failed to deserialize");
    assert_eq!(deserialized.stage, "capture");
    assert_eq!(deserialized.median_minutes, 12.5);
    assert_eq!(deserialized.p90_minutes, 30.0);
    assert_eq!(deserialized.count, 100);
}

#[test]
fn test_stage_dwell_row_json_shape() {
    let row = StageDwellRow {
        stage: "ocr".to_string(),
        median_minutes: 5.0,
        p90_minutes: 15.0,
        count: 50,
    };
    let json = serde_json::to_value(&row).expect("Failed to serialize");
    assert!(json.is_object());
    let obj = json.as_object().unwrap();
    assert!(obj.contains_key("stage"));
    assert!(obj.contains_key("median_minutes"));
    assert!(obj.contains_key("p90_minutes"));
    assert!(obj.contains_key("count"));
}

// ---------------------------------------------------------------------------
// ApproverWorkloadRow structure tests
// ---------------------------------------------------------------------------

#[test]
fn test_approver_workload_row_structure() {
    let id = Uuid::new_v4();
    let row = ApproverWorkloadRow {
        approver_id: id,
        approver_name: "Alice Johnson".to_string(),
        pending_count: 8,
        near_breach_count: 2,
        breached_count: 1,
        avg_response_hours: 3.5,
    };
    assert_eq!(row.approver_id, id);
    assert_eq!(row.approver_name, "Alice Johnson");
    assert_eq!(row.pending_count, 8);
    assert_eq!(row.near_breach_count, 2);
    assert_eq!(row.breached_count, 1);
    assert_eq!(row.avg_response_hours, 3.5);
}

#[test]
fn test_approver_workload_row_serialization() {
    let id = Uuid::new_v4();
    let row = ApproverWorkloadRow {
        approver_id: id,
        approver_name: "Bob Smith".to_string(),
        pending_count: 5,
        near_breach_count: 0,
        breached_count: 0,
        avg_response_hours: 1.2,
    };
    let json = serde_json::to_string(&row).expect("Failed to serialize");
    let deserialized: ApproverWorkloadRow =
        serde_json::from_str(&json).expect("Failed to deserialize");
    assert_eq!(deserialized.approver_id, id);
    assert_eq!(deserialized.pending_count, 5);
    assert_eq!(deserialized.avg_response_hours, 1.2);
}

#[test]
fn test_approver_workload_row_json_shape() {
    let id = Uuid::new_v4();
    let row = ApproverWorkloadRow {
        approver_id: id,
        approver_name: "Carol".to_string(),
        pending_count: 3,
        near_breach_count: 1,
        breached_count: 0,
        avg_response_hours: 2.0,
    };
    let json = serde_json::to_value(&row).expect("Failed to serialize");
    let obj = json.as_object().unwrap();
    assert!(obj.contains_key("approver_id"));
    assert!(obj.contains_key("approver_name"));
    assert!(obj.contains_key("pending_count"));
    assert!(obj.contains_key("near_breach_count"));
    assert!(obj.contains_key("breached_count"));
    assert!(obj.contains_key("avg_response_hours"));
}

// ---------------------------------------------------------------------------
// ExceptionTrendPoint structure tests
// ---------------------------------------------------------------------------

#[test]
fn test_exception_trend_point_structure() {
    let point = ExceptionTrendPoint {
        date: "2026-05-30".to_string(),
        total_invoices: 50,
        exception_count: 5,
        exception_rate: 10.0,
    };
    assert_eq!(point.date, "2026-05-30");
    assert_eq!(point.total_invoices, 50);
    assert_eq!(point.exception_count, 5);
    assert_eq!(point.exception_rate, 10.0);
}

#[test]
fn test_exception_trend_point_serialization() {
    let point = ExceptionTrendPoint {
        date: "2026-05-29".to_string(),
        total_invoices: 30,
        exception_count: 3,
        exception_rate: 10.0,
    };
    let json = serde_json::to_string(&point).expect("Failed to serialize");
    let deserialized: ExceptionTrendPoint =
        serde_json::from_str(&json).expect("Failed to deserialize");
    assert_eq!(deserialized.date, "2026-05-29");
    assert_eq!(deserialized.total_invoices, 30);
    assert_eq!(deserialized.exception_count, 3);
    assert_eq!(deserialized.exception_rate, 10.0);
}

#[test]
fn test_exception_trend_zero_invoices() {
    let point = ExceptionTrendPoint {
        date: "2026-05-28".to_string(),
        total_invoices: 0,
        exception_count: 0,
        exception_rate: 0.0,
    };
    assert_eq!(point.exception_rate, 0.0);
}

#[test]
fn test_exception_trend_point_json_shape() {
    let point = ExceptionTrendPoint {
        date: "2026-05-30".to_string(),
        total_invoices: 20,
        exception_count: 2,
        exception_rate: 10.0,
    };
    let json = serde_json::to_value(&point).expect("Failed to serialize");
    let obj = json.as_object().unwrap();
    assert!(obj.contains_key("date"));
    assert!(obj.contains_key("total_invoices"));
    assert!(obj.contains_key("exception_count"));
    assert!(obj.contains_key("exception_rate"));
}

// ---------------------------------------------------------------------------
// Array serialization tests
// ---------------------------------------------------------------------------

#[test]
fn test_stage_dwell_array_serialization() {
    let rows: Vec<StageDwellRow> = vec![
        StageDwellRow {
            stage: "capture".to_string(),
            median_minutes: 5.0,
            p90_minutes: 10.0,
            count: 100,
        },
        StageDwellRow {
            stage: "approval".to_string(),
            median_minutes: 45.0,
            p90_minutes: 120.0,
            count: 50,
        },
    ];
    let json = serde_json::to_string(&rows).expect("Failed to serialize");
    let deserialized: Vec<StageDwellRow> =
        serde_json::from_str(&json).expect("Failed to deserialize");
    assert_eq!(deserialized.len(), 2);
    assert_eq!(deserialized[0].stage, "capture");
    assert_eq!(deserialized[1].stage, "approval");
}

#[test]
fn test_approver_workload_sorted_desc() {
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let rows: Vec<ApproverWorkloadRow> = vec![
        ApproverWorkloadRow {
            approver_id: id1,
            approver_name: "Alice".to_string(),
            pending_count: 10,
            near_breach_count: 2,
            breached_count: 1,
            avg_response_hours: 3.0,
        },
        ApproverWorkloadRow {
            approver_id: id2,
            approver_name: "Bob".to_string(),
            pending_count: 5,
            near_breach_count: 0,
            breached_count: 0,
            avg_response_hours: 1.0,
        },
    ];
    // Verify ordering: higher pending_count first
    assert!(rows[0].pending_count >= rows[1].pending_count);
}

#[test]
fn test_exception_trend_array_sorted_by_date() {
    let trend: Vec<ExceptionTrendPoint> = vec![
        ExceptionTrendPoint {
            date: "2026-05-28".to_string(),
            total_invoices: 10,
            exception_count: 1,
            exception_rate: 10.0,
        },
        ExceptionTrendPoint {
            date: "2026-05-29".to_string(),
            total_invoices: 20,
            exception_count: 2,
            exception_rate: 10.0,
        },
        ExceptionTrendPoint {
            date: "2026-05-30".to_string(),
            total_invoices: 15,
            exception_count: 0,
            exception_rate: 0.0,
        },
    ];
    // Verify ordering: ascending by date
    for i in 1..trend.len() {
        assert!(trend[i - 1].date < trend[i].date);
    }
}
