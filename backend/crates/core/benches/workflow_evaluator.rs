use billforge_core::{
    evaluate_conditions, CaptureStatus, ConditionField, ConditionOperator, Invoice, InvoiceId,
    Money, ProcessingStatus, RuleCondition, TenantId, UserId,
};
use chrono::Utc;
use criterion::{criterion_group, criterion_main, Criterion};
use serde_json::json;
use uuid::Uuid;

fn sample_invoice() -> Invoice {
    Invoice {
        id: InvoiceId::new(),
        tenant_id: TenantId::new(),
        vendor_id: Some(Uuid::new_v4()),
        vendor_name: "Meridian Office Supply".to_string(),
        invoice_number: "INV-READY-001".to_string(),
        invoice_date: Some(chrono::NaiveDate::from_ymd_opt(2026, 5, 1).unwrap()),
        due_date: Some(chrono::NaiveDate::from_ymd_opt(2026, 5, 31).unwrap()),
        po_number: Some("PO-9001".to_string()),
        subtotal: Some(Money::new(124_000, "USD")),
        tax_amount: Some(Money::new(9_920, "USD")),
        total_amount: Money::new(133_920, "USD"),
        currency: "USD".to_string(),
        line_items: Vec::new(),
        capture_status: CaptureStatus::Reviewed,
        processing_status: ProcessingStatus::PendingApproval,
        current_queue_id: None,
        assigned_to: Some(UserId(Uuid::new_v4())),
        document_id: Uuid::new_v4(),
        supporting_documents: Vec::new(),
        ocr_confidence: Some(0.97),
        categorization_confidence: Some(0.94),
        department: Some("Operations".to_string()),
        gl_code: Some("6100".to_string()),
        cost_center: Some("DET-OPS".to_string()),
        notes: None,
        tags: vec!["pilot".to_string(), "priority".to_string()],
        custom_fields: json!({ "region": "midwest", "risk": "low" }),
        created_by: Some(UserId(Uuid::new_v4())),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn representative_conditions() -> Vec<RuleCondition> {
    vec![
        RuleCondition {
            field: ConditionField::Amount,
            operator: ConditionOperator::GreaterThanOrEqual,
            value: json!(100_000),
        },
        RuleCondition {
            field: ConditionField::Department,
            operator: ConditionOperator::Equals,
            value: json!("Operations"),
        },
        RuleCondition {
            field: ConditionField::GlCode,
            operator: ConditionOperator::StartsWith,
            value: json!("61"),
        },
        RuleCondition {
            field: ConditionField::Tag,
            operator: ConditionOperator::Contains,
            value: json!("priority"),
        },
        RuleCondition {
            field: ConditionField::DueDate,
            operator: ConditionOperator::LessThanOrEqual,
            value: json!("2026-06-15"),
        },
    ]
}

fn bench_workflow_condition_evaluation(c: &mut Criterion) {
    let invoice = sample_invoice();
    let conditions = representative_conditions();

    c.bench_function("workflow_evaluator_representative_conditions", |b| {
        b.iter(|| evaluate_conditions(&invoice, &conditions))
    });
}

criterion_group!(benches, bench_workflow_condition_evaluation);
criterion_main!(benches);
