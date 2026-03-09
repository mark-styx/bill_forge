//! Workflow condition evaluation logic
//!
//! Provides reusable condition evaluation for workflow rules across the system.

use crate::domain::{ConditionField, ConditionOperator, Invoice, RuleCondition};

/// Evaluate all conditions against an invoice (AND logic)
pub fn evaluate_conditions(invoice: &Invoice, conditions: &[RuleCondition]) -> bool {
    // All conditions must match (AND logic)
    for condition in conditions {
        if !evaluate_single_condition(invoice, condition) {
            return false;
        }
    }
    true
}

/// Evaluate a single condition against an invoice
pub fn evaluate_single_condition(invoice: &Invoice, condition: &RuleCondition) -> bool {
    // Extract the field value from the invoice
    let field_value = match condition.field {
        ConditionField::Amount => {
            serde_json::to_value(invoice.total_amount.amount).ok()
        }
        ConditionField::VendorId => {
            invoice.vendor_id.as_ref().and_then(|v| serde_json::to_value(v).ok())
        }
        ConditionField::VendorName => {
            Some(serde_json::Value::String(invoice.vendor_name.clone()))
        }
        ConditionField::Department => {
            invoice.department.as_ref().map(|d| serde_json::Value::String(d.clone()))
        }
        ConditionField::GlCode => {
            invoice.gl_code.as_ref().map(|g| serde_json::Value::String(g.clone()))
        }
        ConditionField::InvoiceDate => {
            invoice.invoice_date.and_then(|d| serde_json::to_value(d.to_string()).ok())
        }
        ConditionField::DueDate => {
            invoice.due_date.and_then(|d| serde_json::to_value(d.to_string()).ok())
        }
        ConditionField::Tag => {
            if invoice.tags.is_empty() {
                None
            } else {
                serde_json::to_value(&invoice.tags).ok()
            }
        }
        ConditionField::CustomField => {
            // For custom fields, the condition value should specify which field
            if let serde_json::Value::Object(ref map) = condition.value {
                if let Some(field_name) = map.get("field").and_then(|v| v.as_str()) {
                    invoice.custom_fields.get(field_name).cloned()
                } else {
                    None
                }
            } else {
                invoice.custom_fields.clone().into()
            }
        }
    };

    // Apply the operator
    match &field_value {
        None => {
            // Field is null/missing
            matches!(condition.operator, ConditionOperator::IsNull)
        }
        Some(fv) => {
            apply_operator(fv, &condition.operator, &condition.value)
        }
    }
}

/// Apply a comparison operator to field and condition values
pub fn apply_operator(
    field_value: &serde_json::Value,
    operator: &ConditionOperator,
    condition_value: &serde_json::Value,
) -> bool {
    match operator {
        ConditionOperator::Equals => field_value == condition_value,
        ConditionOperator::NotEquals => field_value != condition_value,
        ConditionOperator::GreaterThan => {
            compare_values(field_value, condition_value) == Some(std::cmp::Ordering::Greater)
        }
        ConditionOperator::GreaterThanOrEqual => {
            matches!(compare_values(field_value, condition_value), Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal))
        }
        ConditionOperator::LessThan => {
            compare_values(field_value, condition_value) == Some(std::cmp::Ordering::Less)
        }
        ConditionOperator::LessThanOrEqual => {
            matches!(compare_values(field_value, condition_value), Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal))
        }
        ConditionOperator::Contains => {
            // String contains or array contains
            match (field_value, condition_value) {
                (serde_json::Value::String(s), serde_json::Value::String(pattern)) => {
                    s.contains(pattern)
                }
                (serde_json::Value::Array(arr), _) => {
                    arr.contains(condition_value)
                }
                _ => false,
            }
        }
        ConditionOperator::StartsWith => {
            match (field_value, condition_value) {
                (serde_json::Value::String(s), serde_json::Value::String(prefix)) => {
                    s.starts_with(prefix)
                }
                _ => false,
            }
        }
        ConditionOperator::EndsWith => {
            match (field_value, condition_value) {
                (serde_json::Value::String(s), serde_json::Value::String(suffix)) => {
                    s.ends_with(suffix)
                }
                _ => false,
            }
        }
        ConditionOperator::In => {
            // Field value is in the condition value (which should be an array)
            match condition_value {
                serde_json::Value::Array(arr) => arr.contains(field_value),
                _ => false,
            }
        }
        ConditionOperator::NotIn => {
            // Field value is NOT in the condition value (which should be an array)
            match condition_value {
                serde_json::Value::Array(arr) => !arr.contains(field_value),
                _ => true,
            }
        }
        ConditionOperator::Between => {
            // Condition value should be [min, max]
            match condition_value {
                serde_json::Value::Array(arr) if arr.len() == 2 => {
                    let min_ok = compare_values(field_value, &arr[0])
                        .map(|o| o == std::cmp::Ordering::Greater || o == std::cmp::Ordering::Equal)
                        .unwrap_or(false);
                    let max_ok = compare_values(field_value, &arr[1])
                        .map(|o| o == std::cmp::Ordering::Less || o == std::cmp::Ordering::Equal)
                        .unwrap_or(false);
                    min_ok && max_ok
                }
                _ => false,
            }
        }
        ConditionOperator::IsNull => {
            // Already handled in evaluate_single_condition
            false
        }
        ConditionOperator::IsNotNull => {
            // Already handled in evaluate_single_condition (field_value is Some)
            true
        }
    }
}

/// Compare two JSON values (numeric, string, or boolean comparison)
pub fn compare_values(
    a: &serde_json::Value,
    b: &serde_json::Value,
) -> Option<std::cmp::Ordering> {
    use serde_json::Value;

    match (a, b) {
        // Numeric comparison (handle both i64 and f64)
        (Value::Number(a_num), Value::Number(b_num)) => {
            let a_val = a_num.as_f64()?;
            let b_val = b_num.as_f64()?;
            a_val.partial_cmp(&b_val)
        }
        // String comparison
        (Value::String(a_str), Value::String(b_str)) => {
            Some(a_str.cmp(b_str))
        }
        // Boolean comparison
        (Value::Bool(a_bool), Value::Bool(b_bool)) => {
            Some(a_bool.cmp(b_bool))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Money, TenantId, UserId};
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    fn create_test_invoice() -> Invoice {
        Invoice {
            id: crate::domain::InvoiceId::new(),
            tenant_id: TenantId::new(),
            vendor_id: Some(Uuid::new_v4()),
            vendor_name: "Test Vendor".to_string(),
            invoice_number: "INV-001".to_string(),
            invoice_date: Some(chrono::NaiveDate::from_ymd_opt(2026, 3, 6).unwrap()),
            due_date: Some(chrono::NaiveDate::from_ymd_opt(2026, 4, 6).unwrap()),
            po_number: None,
            subtotal: Some(Money { amount: 10000, currency: "USD".to_string() }),
            tax_amount: Some(Money { amount: 800, currency: "USD".to_string() }),
            total_amount: Money { amount: 10800, currency: "USD".to_string() },
            currency: "USD".to_string(),
            line_items: vec![],
            capture_status: crate::domain::CaptureStatus::Reviewed,
            processing_status: crate::domain::ProcessingStatus::Draft,
            current_queue_id: None,
            assigned_to: None,
            document_id: Uuid::new_v4(),
            supporting_documents: vec![],
            ocr_confidence: Some(0.95),
            department: Some("Engineering".to_string()),
            gl_code: Some("5000".to_string()),
            cost_center: None,
            notes: None,
            tags: vec!["urgent".to_string(), "approved".to_string()],
            custom_fields: json!({"priority": "high", "project": "alpha"}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            created_by: UserId(Uuid::new_v4()),
        }
    }

    #[test]
    fn test_evaluate_equals() {
        let invoice = create_test_invoice();

        let condition = RuleCondition {
            field: ConditionField::Amount,
            operator: ConditionOperator::Equals,
            value: json!(10800),
        };

        assert!(evaluate_single_condition(&invoice, &condition));
    }

    #[test]
    fn test_evaluate_greater_than() {
        let invoice = create_test_invoice();

        let condition = RuleCondition {
            field: ConditionField::Amount,
            operator: ConditionOperator::GreaterThan,
            value: json!(10000),
        };

        assert!(evaluate_single_condition(&invoice, &condition));
    }

    #[test]
    fn test_evaluate_string_contains() {
        let invoice = create_test_invoice();

        let condition = RuleCondition {
            field: ConditionField::VendorName,
            operator: ConditionOperator::Contains,
            value: json!("Test"),
        };

        assert!(evaluate_single_condition(&invoice, &condition));
    }

    #[test]
    fn test_evaluate_in_array() {
        let invoice = create_test_invoice();

        let condition = RuleCondition {
            field: ConditionField::Department,
            operator: ConditionOperator::In,
            value: json!(["Engineering", "Sales", "Marketing"]),
        };

        assert!(evaluate_single_condition(&invoice, &condition));
    }

    #[test]
    fn test_evaluate_between() {
        let invoice = create_test_invoice();

        let condition = RuleCondition {
            field: ConditionField::Amount,
            operator: ConditionOperator::Between,
            value: json!([5000, 15000]),
        };

        assert!(evaluate_single_condition(&invoice, &condition));
    }

    #[test]
    fn test_evaluate_all_conditions_and_logic() {
        let invoice = create_test_invoice();

        let conditions = vec![
            RuleCondition {
                field: ConditionField::Amount,
                operator: ConditionOperator::GreaterThan,
                value: json!(10000),
            },
            RuleCondition {
                field: ConditionField::Department,
                operator: ConditionOperator::Equals,
                value: json!("Engineering"),
            },
        ];

        assert!(evaluate_conditions(&invoice, &conditions));

        // Add a failing condition
        let conditions_with_fail = vec![
            RuleCondition {
                field: ConditionField::Amount,
                operator: ConditionOperator::GreaterThan,
                value: json!(10000),
            },
            RuleCondition {
                field: ConditionField::Department,
                operator: ConditionOperator::Equals,
                value: json!("Sales"), // This doesn't match
            },
        ];

        assert!(!evaluate_conditions(&invoice, &conditions_with_fail));
    }
}
