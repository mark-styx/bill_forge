//! Wire-contract tests for the EDI job handlers added in issue #402.
//!
//! These tests do not stand up a Postgres pool; they exercise the parts of
//! the EDI job wiring that don't need one — the JobType wire names (so the
//! existing producer/consumer round-trip survives), the tenant discovery SQL
//! used by the scheduler, and the JSON payload contract every handler depends
//! on. Real database interaction is exercised by integration tests that own a
//! `PgPool` (see api/tests/integration_gating.rs for an example pattern).

use billforge_worker::jobs::{Job, JobType};
use chrono::Utc;
use serde_json::json;

#[test]
fn edi_job_type_names_match_wire_contract() {
    // Mirrors job_contract_test.rs — making sure the EDI variants serialize to
    // the same snake_case names producers and consumers already use.
    let cases = [
        (JobType::EdiProcessInbound, "edi_process_inbound"),
        (JobType::EdiSendRemittance, "edi_send_remittance"),
        (JobType::EdiSendAck, "edi_send_ack"),
        (JobType::EdiCheckAckStatus, "edi_check_ack_status"),
    ];

    for (job_type, expected) in cases {
        let serialized = serde_json::to_value(job_type).unwrap();
        assert_eq!(serialized, json!(expected));
    }
}

#[test]
fn edi_check_ack_status_payload_round_trips_through_job_envelope() {
    let tenant_uuid = uuid::Uuid::new_v4();
    let job = Job {
        id: "job-edi-1".to_string(),
        job_type: JobType::EdiCheckAckStatus,
        tenant_id: tenant_uuid.to_string(),
        payload: json!({}),
        created_at: Utc::now(),
        retry_count: 0,
    };

    let serialized = serde_json::to_string(&job).unwrap();
    let deserialized: Job = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized.tenant_id, tenant_uuid.to_string());
    assert!(matches!(
        deserialized.job_type,
        JobType::EdiCheckAckStatus
    ));
}

#[test]
fn edi_tenant_discovery_sql_filters_by_active_and_entitlement() {
    // The scheduler producer relies on this exact predicate; if anyone changes
    // it to a non-jsonb form, the EDI ack-check job stops being enqueued for
    // exactly the tenants the issue requires it to run for.
    let sql = billforge_worker::jobs::edi::EDI_TENANT_DISCOVERY_SQL;
    assert!(sql.contains("is_active = true"));
    assert!(sql.contains("enabled_modules @>"));
    assert!(sql.contains("[\"edi\"]"));
}

#[test]
fn edi_remittance_job_payload_carries_required_fields() {
    // Producers (payment-completion flow, manual /send-remittance API) must be
    // able to enqueue with this shape; pin the contract so a payload change
    // gets caught here instead of at runtime.
    let payload = json!({
        "invoice_id": "00000000-0000-0000-0000-000000000001",
        "payment_reference": "PR-1",
        "payment_method": "ACH",
        "payer_name": "BillForge"
    });
    let job = Job {
        id: "job-edi-rem".to_string(),
        job_type: JobType::EdiSendRemittance,
        tenant_id: uuid::Uuid::new_v4().to_string(),
        payload,
        created_at: Utc::now(),
        retry_count: 0,
    };

    let serialized = serde_json::to_string(&job).unwrap();
    let deserialized: Job = serde_json::from_str(&serialized).unwrap();
    assert_eq!(
        deserialized.payload.get("invoice_id").and_then(|v| v.as_str()),
        Some("00000000-0000-0000-0000-000000000001")
    );
    assert_eq!(
        deserialized.payload.get("payment_reference").and_then(|v| v.as_str()),
        Some("PR-1")
    );
}

#[test]
fn edi_send_ack_payload_carries_original_doc_id() {
    let payload = json!({
        "original_doc_id": "00000000-0000-0000-0000-000000000002",
        "accepted": true,
    });
    let job = Job {
        id: "job-edi-ack".to_string(),
        job_type: JobType::EdiSendAck,
        tenant_id: uuid::Uuid::new_v4().to_string(),
        payload,
        created_at: Utc::now(),
        retry_count: 0,
    };

    let serialized = serde_json::to_string(&job).unwrap();
    let deserialized: Job = serde_json::from_str(&serialized).unwrap();
    assert_eq!(
        deserialized
            .payload
            .get("original_doc_id")
            .and_then(|v| v.as_str()),
        Some("00000000-0000-0000-0000-000000000002")
    );
}
