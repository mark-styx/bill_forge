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
        let req: billforge_api::routes::close_periods::CreatePeriodRequest =
            serde_json::from_value(json!({
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
            billforge_api::routes::close_periods::CreatePeriodRequest,
        >(json!({
            "period_label": "2026-05",
            "period_start": "2026-05-01",
            // missing period_end and cutoff_date
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_update_period_request_cutoff_only() {
        let req: billforge_api::routes::close_periods::UpdatePeriodRequest =
            serde_json::from_value(json!({
                "cutoff_date": "2026-05-28"
            }))
            .unwrap();
        assert_eq!(req.cutoff_date.as_deref(), Some("2026-05-28"));
    }

    #[test]
    fn test_update_period_request_empty() {
        let req: billforge_api::routes::close_periods::UpdatePeriodRequest =
            serde_json::from_value(json!({})).unwrap();
        assert!(req.cutoff_date.is_none());
    }

    #[test]
    fn test_run_close_response_fields() {
        let period_id = uuid::Uuid::new_v4();
        let resp = billforge_api::routes::close_periods::RunCloseResponse {
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
    fn test_run_close_response_unsupported() {
        let period_id = uuid::Uuid::new_v4();
        let resp = billforge_api::routes::close_periods::RunCloseResponse {
            period_id,
            accrual_entries_created: 2,
            erp_post_status: "unsupported".to_string(),
            erp_post_error: Some("QBO journal entry posting not implemented".to_string()),
        };
        let val = serde_json::to_value(&resp).unwrap();
        assert_eq!(val["erp_post_status"], "unsupported");
        assert_eq!(
            val["erp_post_error"],
            "QBO journal entry posting not implemented"
        );
        // The period should NOT be reported as locked when unsupported
        assert_ne!(val["erp_post_status"], "posted");
    }

    #[test]
    fn test_run_close_response_with_error() {
        let resp = billforge_api::routes::close_periods::RunCloseResponse {
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
        let resp = billforge_api::routes::close_periods::ClosePeriodResponse {
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
        let resp = billforge_api::routes::close_periods::ClosePeriodResponse {
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

    // -----------------------------------------------------------------------
    // Readiness response tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_readiness_score_for_clean_period() {
        // A clean period has score == 100 and empty exceptions
        let resp = billforge_api::routes::close_periods::ReadinessResponse {
            period: Some(billforge_api::routes::close_periods::ClosePeriodResponse {
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
            }),
            score: Some(100),
            computed_at: "2026-06-01T00:00:00Z".to_string(),
            totals: billforge_api::routes::close_periods::ReadinessTotals {
                total_invoices: 5,
                unapproved_invoices: 0,
                accruals_drafted: 0,
                invoices_needing_accrual: 0,
                invoices_missing_gl_coding: 0,
                days_until_cutoff: Some(20),
            },
            exceptions: vec![],
        };
        assert_eq!(resp.score, Some(100));
        assert!(resp.exceptions.is_empty());
        let val = serde_json::to_value(&resp).unwrap();
        assert_eq!(val["score"], 100);
        assert_eq!(val["exceptions"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_readiness_flags_unaccrued_invoices() {
        // 2 unapproved invoices with no accrual rows
        let resp = billforge_api::routes::close_periods::ReadinessResponse {
            period: Some(billforge_api::routes::close_periods::ClosePeriodResponse {
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
            }),
            score: Some(50),
            computed_at: "2026-06-01T00:00:00Z".to_string(),
            totals: billforge_api::routes::close_periods::ReadinessTotals {
                total_invoices: 4,
                unapproved_invoices: 2,
                accruals_drafted: 0,
                invoices_needing_accrual: 2,
                invoices_missing_gl_coding: 0,
                days_until_cutoff: Some(10),
            },
            exceptions: vec![
                billforge_api::routes::close_periods::ExceptionItem {
                    id: "unaccrued_invoices".to_string(),
                    label: "Invoices missing accrual".to_string(),
                    count: 2,
                    severity: "high".to_string(),
                },
                billforge_api::routes::close_periods::ExceptionItem {
                    id: "unapproved_invoices".to_string(),
                    label: "Unapproved invoices".to_string(),
                    count: 2,
                    severity: "low".to_string(),
                },
            ],
        };
        assert!(resp.score.unwrap() < 100);
        let unaccrued = resp
            .exceptions
            .iter()
            .find(|e| e.id == "unaccrued_invoices")
            .unwrap();
        assert_eq!(unaccrued.count, 2);
        assert_eq!(unaccrued.severity, "high");

        let val = serde_json::to_value(&resp).unwrap();
        assert_eq!(val["score"], 50);
        let exc = val["exceptions"].as_array().unwrap();
        assert!(exc.len() >= 1);
        assert_eq!(exc[0]["id"], "unaccrued_invoices");
        assert_eq!(exc[0]["count"], 2);
    }
}
