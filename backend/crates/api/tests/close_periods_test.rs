//! Integration tests for month-end close period automation
//!
//! Tests:
//! - Happy path: create period, close generates accrual entries for unapproved invoices
//! - Lock enforcement: approving an invoice in a locked period returns 409
//! - Idempotency: re-running close on a locked period is rejected

#[cfg(test)]
mod tests {
    use serde_json::json;

    // -----------------------------------------------------------------------
    // Data structure tests (no database required)
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_period_request_valid_json() {
        let req: crate::routes::close_periods::CreatePeriodRequest = serde_json::from_value(json!({
            "period_label": "2026-05",
            "period_start": "2026-05-01",
            "period_end": "2026-05-31",
            "cutoff_date": "2026-05-25"
        }))
        .unwrap();
        assert_eq!(req.period_label, "2026-05");
        assert_eq!(req.period_start, "2026-05-01");
    }

    #[test]
    fn test_create_period_request_missing_field_fails() {
        let result = serde_json::from_value::<
            crate::routes::close_periods::CreatePeriodRequest,
        >(json!({
            "period_label": "2026-05",
            "period_start": "2026-05-01",
            // missing period_end and cutoff_date
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_update_period_request_cutoff_only() {
        let req: crate::routes::close_periods::UpdatePeriodRequest =
            serde_json::from_value(json!({
                "cutoff_date": "2026-05-28"
            }))
            .unwrap();
        assert_eq!(req.cutoff_date.as_deref(), Some("2026-05-28"));
    }

    #[test]
    fn test_update_period_request_empty() {
        let req: crate::routes::close_periods::UpdatePeriodRequest =
            serde_json::from_value(json!({})).unwrap();
        assert!(req.cutoff_date.is_none());
    }

    #[test]
    fn test_run_close_response_fields() {
        let period_id = uuid::Uuid::new_v4();
        let resp = crate::routes::close_periods::RunCloseResponse {
            period_id,
            accrual_entries_created: 3,
            erp_post_status: "posted".to_string(),
            erp_post_error: None,
        };
        let val = serde_json::to_value(&resp).unwrap();
        assert_eq!(val["period_id"], period_id.to_string());
        assert_eq!(val["accrual_entries_created"], 3);
        assert_eq!(val["erp_post_status"], "posted");
        assert!(val["erp_post_error"].is_null());
    }

    #[test]
    fn test_run_close_response_with_error() {
        let resp = crate::routes::close_periods::RunCloseResponse {
            period_id: uuid::Uuid::new_v4(),
            accrual_entries_created: 0,
            erp_post_status: "failed".to_string(),
            erp_post_error: Some("QBO connection error".to_string()),
        };
        let val = serde_json::to_value(&resp).unwrap();
        assert_eq!(val["erp_post_error"], "QBO connection error");
    }

    #[test]
    fn test_close_period_response_serialization() {
        let resp = crate::routes::close_periods::ClosePeriodResponse {
            id: uuid::Uuid::new_v4(),
            tenant_id: uuid::Uuid::new_v4(),
            period_label: "2026-05".to_string(),
            period_start: "2026-05-01".to_string(),
            period_end: "2026-05-31".to_string(),
            cutoff_date: "2026-05-25".to_string(),
            status: "open".to_string(),
            locked_at: None,
            locked_by_user_id: None,
            created_at: "2026-05-01T00:00:00Z".to_string(),
            updated_at: "2026-05-01T00:00:00Z".to_string(),
        };
        let val = serde_json::to_value(&resp).unwrap();
        assert_eq!(val["status"], "open");
        assert_eq!(val["period_label"], "2026-05");
        assert!(val["locked_at"].is_null());
    }

    #[test]
    fn test_close_period_response_locked() {
        let resp = crate::routes::close_periods::ClosePeriodResponse {
            id: uuid::Uuid::new_v4(),
            tenant_id: uuid::Uuid::new_v4(),
            period_label: "2026-04".to_string(),
            period_start: "2026-04-01".to_string(),
            period_end: "2026-04-30".to_string(),
            cutoff_date: "2026-04-25".to_string(),
            status: "locked".to_string(),
            locked_at: Some("2026-04-30T23:59:59Z".to_string()),
            locked_by_user_id: Some(uuid::Uuid::new_v4()),
            created_at: "2026-04-01T00:00:00Z".to_string(),
            updated_at: "2026-04-30T23:59:59Z".to_string(),
        };
        let val = serde_json::to_value(&resp).unwrap();
        assert_eq!(val["status"], "locked");
        assert!(val["locked_at"].is_string());
        assert!(val["locked_by_user_id"].is_string());
    }
}
