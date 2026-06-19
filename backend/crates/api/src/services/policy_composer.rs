//! Natural-language policy parser and preview engine.
//!
//! Parses common AP policy idioms into structured workflow rules using
//! deterministic regex matching (no LLM/network calls).

use billforge_core::domain::{
    ActionType, ConditionField, ConditionOperator, RuleAction, RuleCondition, WorkflowRuleType,
};
use billforge_core::TenantId;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;

/// The kind of guardrail inferred from the NL text.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GuardrailKind {
    ApprovalLimit,
    BudgetCap,
    RoutingRule,
    Block,
}

/// A proposed workflow rule parsed from natural language.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedRule {
    pub name: String,
    pub description: String,
    pub priority: i32,
    pub guardrail_kind: GuardrailKind,
    /// Serialized condition JSON for the workflow_rules table.
    pub condition_json: serde_json::Value,
    /// Serialized action JSON for the workflow_rules table.
    pub action_json: serde_json::Value,
    /// Human-readable summary of what the rule does.
    pub summary: String,
}

/// Structured error from the parser.
#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub unparseable_segments: Vec<String>,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ParseError {}

/// Preview of a proposed rule against historical invoices.
#[derive(Debug, Clone, Serialize)]
pub struct PolicyPreview {
    pub matched_count: usize,
    pub total_invoices: usize,
    pub sample_invoices: Vec<InvoiceSummaryRow>,
    pub projected_action_breakdown: serde_json::Value,
}

/// Summary row for preview.
#[derive(Debug, Clone, Serialize)]
pub struct InvoiceSummaryRow {
    pub id: String,
    pub invoice_number: Option<String>,
    pub vendor_name: Option<String>,
    pub total_amount_cents: Option<i64>,
    pub processing_status: Option<String>,
    pub invoice_date: Option<String>,
}

impl ProposedRule {
    /// Convert to the domain CreateWorkflowRuleInput for persistence.
    pub fn to_rule_type(&self) -> WorkflowRuleType {
        match self.guardrail_kind {
            GuardrailKind::ApprovalLimit => WorkflowRuleType::Approval,
            GuardrailKind::BudgetCap => WorkflowRuleType::AutoApproval,
            GuardrailKind::RoutingRule => WorkflowRuleType::Routing,
            GuardrailKind::Block => WorkflowRuleType::AutoApproval,
        }
    }

    pub fn to_conditions(&self) -> Vec<RuleCondition> {
        let conditions_val = &self.condition_json;
        let mut out = Vec::new();

        if let Some(amount) = conditions_val.get("amount_greater_than") {
            out.push(RuleCondition {
                field: ConditionField::Amount,
                operator: ConditionOperator::GreaterThanOrEqual,
                value: amount.clone(),
            });
        }
        if let Some(vendor) = conditions_val.get("vendor_name") {
            out.push(RuleCondition {
                field: ConditionField::VendorName,
                operator: ConditionOperator::Equals,
                value: vendor.clone(),
            });
        }
        if let Some(category) = conditions_val.get("category") {
            out.push(RuleCondition {
                field: ConditionField::Department,
                operator: ConditionOperator::Equals,
                value: category.clone(),
            });
        }
        if let Some(department) = conditions_val.get("department") {
            out.push(RuleCondition {
                field: ConditionField::Department,
                operator: ConditionOperator::Equals,
                value: department.clone(),
            });
        }
        if let Some(has_po) = conditions_val.get("has_po") {
            out.push(RuleCondition {
                field: ConditionField::CustomField,
                operator: ConditionOperator::Equals,
                value: has_po.clone(),
            });
        }

        out
    }

    pub fn to_actions(&self) -> Vec<RuleAction> {
        let actions_val = &self.action_json;
        vec![RuleAction {
            action_type: match self.guardrail_kind {
                GuardrailKind::ApprovalLimit => ActionType::RequireApproval,
                GuardrailKind::RoutingRule => ActionType::RouteToQueue,
                GuardrailKind::Block => ActionType::SetField,
                GuardrailKind::BudgetCap => ActionType::SetField,
            },
            params: actions_val.clone(),
        }]
    }
}

/// Parse a natural-language policy into a proposed rule.
pub fn parse_policy(text: &str) -> Result<ProposedRule, ParseError> {
    let normalized = text.to_lowercase();

    // Pattern 1: "over $X require approval from <role>"
    let re_approval =
        Regex::new(r"(?i)over\s+\$?([\d,]+(?:\.\d{2})?)\s+require\s+approval\s+from\s+(\w+)")
            .unwrap();
    if let Some(caps) = re_approval.captures(text) {
        let amount_str = caps.get(1).unwrap().as_str().replace(",", "");
        let amount: f64 = amount_str.parse().map_err(|_| ParseError {
            message: format!("Invalid amount: {}", caps.get(1).unwrap().as_str()),
            unparseable_segments: vec![caps.get(1).unwrap().as_str().to_string()],
        })?;
        let role = caps.get(2).unwrap().as_str().to_lowercase();
        let amount_cents = (amount * 100.0) as i64;

        return Ok(ProposedRule {
            name: format!("Approval threshold ${:.2}", amount),
            description: format!(
                "Invoices over ${:.2} require approval from {}",
                amount, role
            ),
            priority: 50,
            guardrail_kind: GuardrailKind::ApprovalLimit,
            condition_json: serde_json::json!({
                "amount_greater_than": amount_cents,
            }),
            action_json: serde_json::json!({
                "approval_from_role": role,
                "action": "require_approval",
            }),
            summary: format!(
                "Any invoice over ${:.2} will require approval from {}.",
                amount, role
            ),
        });
    }

    // Pattern 2: "invoices from vendor <name> need <action>"
    let re_vendor = Regex::new(r"(?i)invoices?\s+from\s+vendor\s+([^,]+?)\s+need\s+(\w+)").unwrap();
    if let Some(caps) = re_vendor.captures(text) {
        let vendor = caps.get(1).unwrap().as_str().trim().to_string();
        let action = caps.get(2).unwrap().as_str().to_lowercase();

        return Ok(ProposedRule {
            name: format!("Vendor rule: {}", vendor),
            description: format!("Invoices from vendor '{}' need {}", vendor, action),
            priority: 40,
            guardrail_kind: GuardrailKind::RoutingRule,
            condition_json: serde_json::json!({
                "vendor_name": vendor,
            }),
            action_json: serde_json::json!({
                "action": action,
            }),
            summary: format!("All invoices from vendor '{}' will be {}.", vendor, action),
        });
    }

    // Pattern 3: "block invoices over $X without PO"
    let re_block = Regex::new(
        r"(?i)block\s+invoices?\s+over\s+\$?([\d,]+(?:\.\d{2})?)\s+without\s+(?:a\s+)?PO",
    )
    .unwrap();
    if let Some(caps) = re_block.captures(text) {
        let amount_str = caps.get(1).unwrap().as_str().replace(",", "");
        let amount: f64 = amount_str.parse().map_err(|_| ParseError {
            message: format!("Invalid amount: {}", caps.get(1).unwrap().as_str()),
            unparseable_segments: vec![caps.get(1).unwrap().as_str().to_string()],
        })?;
        let amount_cents = (amount * 100.0) as i64;

        return Ok(ProposedRule {
            name: format!("Block invoices over ${:.2} without PO", amount),
            description: format!(
                "Block invoices over ${:.2} that do not have a purchase order",
                amount
            ),
            priority: 80,
            guardrail_kind: GuardrailKind::Block,
            condition_json: serde_json::json!({
                "amount_greater_than": amount_cents,
                "has_po": false,
            }),
            action_json: serde_json::json!({
                "action": "block",
                "reason": format!("Invoice exceeds ${:.2} and has no PO", amount),
            }),
            summary: format!("Invoices over ${:.2} without a PO will be blocked.", amount),
        });
    }

    // Pattern 4: "route <category> to <approver>"
    let re_route = Regex::new(r"(?i)route\s+(\w+)\s+to\s+(\w+)").unwrap();
    if let Some(caps) = re_route.captures(text) {
        let category = caps.get(1).unwrap().as_str().to_lowercase();
        let approver = caps.get(2).unwrap().as_str().to_lowercase();

        return Ok(ProposedRule {
            name: format!("Route {} to {}", category, approver),
            description: format!("Route invoices in category '{}' to {}", category, approver),
            priority: 30,
            guardrail_kind: GuardrailKind::RoutingRule,
            condition_json: serde_json::json!({
                "category": category,
            }),
            action_json: serde_json::json!({
                "action": "route",
                "approver": approver,
            }),
            summary: format!(
                "Invoices categorized as '{}' will be routed to {}.",
                category, approver
            ),
        });
    }

    // Pattern 5: "cap monthly spend on <category> at $X"
    let re_budget =
        Regex::new(r"(?i)cap\s+monthly\s+spend\s+on\s+(\w+)\s+at\s+\$?([\d,]+(?:\.\d{2})?)")
            .unwrap();
    if let Some(caps) = re_budget.captures(text) {
        let category = caps.get(1).unwrap().as_str().to_lowercase();
        let amount_str = caps.get(2).unwrap().as_str().replace(",", "");
        let amount: f64 = amount_str.parse().map_err(|_| ParseError {
            message: format!("Invalid amount: {}", caps.get(2).unwrap().as_str()),
            unparseable_segments: vec![caps.get(2).unwrap().as_str().to_string()],
        })?;
        let amount_cents = (amount * 100.0) as i64;

        return Ok(ProposedRule {
            name: format!("Monthly cap: {} at ${:.2}", category, amount),
            description: format!("Cap monthly spend on '{}' at ${:.2}", category, amount),
            priority: 70,
            guardrail_kind: GuardrailKind::BudgetCap,
            condition_json: serde_json::json!({
                "category": category,
                "monthly_cap_cents": amount_cents,
            }),
            action_json: serde_json::json!({
                "action": "budget_cap",
                "monthly_cap_cents": amount_cents,
                "category": category,
            }),
            summary: format!(
                "Monthly spending on '{}' will be capped at ${:.2}.",
                category, amount
            ),
        });
    }

    // Pattern 6: "invoices over $X from <department> go to <role>"
    // Supports `$10k` shorthand (trailing k = *1000). Used for the marquee
    // example's first clause ("Invoices over $10k from Marketing go to the CMO").
    let re_route_amount_dept = Regex::new(
        r"(?i)invoices?\s+over\s+\$?([\d,]+(?:k)?(?:\.\d{2})?)\s+from\s+([\w &]+?)\s+(?:go|are\s+routed|route)\s+to\s+(?:the\s+)?([\w-]+)",
    )
    .unwrap();
    if let Some(caps) = re_route_amount_dept.captures(text) {
        let amount_str = caps.get(1).unwrap().as_str();
        let amount_cents = parse_amount_with_k_shorthand(amount_str).map_err(|msg| ParseError {
            message: msg,
            unparseable_segments: vec![amount_str.to_string()],
        })?;
        let amount = (amount_cents as f64) / 100.0;
        let department = caps.get(2).unwrap().as_str().trim().to_lowercase();
        let role = caps.get(3).unwrap().as_str().to_lowercase();

        return Ok(ProposedRule {
            name: format!("Route {} invoices over ${:.2} to {}", department, amount, role),
            description: format!(
                "Invoices over ${:.2} from {} are routed to {}",
                amount, department, role
            ),
            priority: 35,
            guardrail_kind: GuardrailKind::RoutingRule,
            condition_json: serde_json::json!({
                "amount_greater_than": amount_cents,
                "department": department,
            }),
            action_json: serde_json::json!({
                "action": "route_to_role",
                "route_to_role": role,
            }),
            summary: format!(
                "Invoices over ${:.2} from {} will be routed to {}.",
                amount, department, role
            ),
        });
    }

    // Pattern 7: "(anything|invoices) from (a) new vendor need(s) <role> review/approval"
    // Used for the marquee example's second clause ("anything from a new vendor
    // needs Finance review before approval").
    let re_new_vendor = Regex::new(
        r"(?i)(?:anything|invoices?)\s+from\s+(?:a\s+)?new\s+vendors?\s+(?:need|require)s?\s+([\w-]+)\s+(?:review|approval)",
    )
    .unwrap();
    if let Some(caps) = re_new_vendor.captures(text) {
        let role = caps.get(1).unwrap().as_str().to_lowercase();

        return Ok(ProposedRule {
            name: format!("New vendor approval: {}", role),
            description: format!("Invoices from new vendors require {} review", role),
            priority: 60,
            guardrail_kind: GuardrailKind::ApprovalLimit,
            condition_json: serde_json::json!({
                "new_vendor": true,
            }),
            action_json: serde_json::json!({
                "action": "require_approval",
                "approval_from_role": role,
            }),
            summary: format!(
                "Any invoice from a new vendor will require {} review before approval.",
                role
            ),
        });
    }

    Err(ParseError {
        message: "Could not understand the policy. Try phrases like: \
            \"over $5000 require approval from manager\", \
            \"invoices from vendor Acme need review\", \
            \"block invoices over $10000 without PO\", \
            \"route travel to finance\", \
            \"cap monthly spend on software at $5000\", \
            \"invoices over $10k from Marketing go to the CMO\", \
            \"anything from a new vendor needs Finance review\""
            .to_string(),
        unparseable_segments: vec![text.to_string()],
    })
}

/// Parse an amount string into cents, supporting the `$10k` shorthand where a
/// trailing `k` (case-insensitive) multiplies by 1000. Commas are stripped.
fn parse_amount_with_k_shorthand(amount_str: &str) -> Result<i64, String> {
    let cleaned: String = amount_str.replace(",", "");
    let has_k = cleaned.chars().any(|c| c.eq_ignore_ascii_case(&'k'));
    let digits: String = cleaned
        .chars()
        .filter(|c| !c.eq_ignore_ascii_case(&'k'))
        .collect();
    let multiplier = if has_k { 1000.0 } else { 1.0 };
    let amount: f64 = digits
        .parse()
        .map_err(|_| format!("Invalid amount: {}", amount_str))?;
    Ok((amount * multiplier * 100.0) as i64)
}

/// Result of parsing a (possibly compound) NL policy.
#[derive(Debug, Clone, Default)]
pub struct ParsedPolicies {
    pub rules: Vec<ProposedRule>,
    pub unparseable_segments: Vec<String>,
}

/// Parse a (possibly compound) natural-language policy into typed rules.
///
/// Splits the input on clause-level conjunctions (", and ", "; ", " and ")
/// without breaking numeric strings like "$10,000" (commas are only followed
/// by digits there, never whitespace). Each segment is parsed via `parse_policy`;
/// successes are collected into `rules` and failing segments into
/// `unparseable_segments`. Returns `Ok` as long as at least one rule parses;
/// returns `Err` only when no segment parses.
pub fn parse_policies(text: &str) -> Result<ParsedPolicies, ParseError> {
    let re_split = Regex::new(r"(?i),\s+and\s+|;\s+|\s+and\s+").unwrap();
    let segments: Vec<&str> = re_split.split(text).collect();

    let mut rules = Vec::new();
    let mut unparseable_segments = Vec::new();

    for seg in segments {
        let trimmed = seg.trim();
        if trimmed.is_empty() {
            continue;
        }
        match parse_policy(trimmed) {
            Ok(rule) => rules.push(rule),
            Err(_) => unparseable_segments.push(trimmed.to_string()),
        }
    }

    if rules.is_empty() {
        Err(ParseError {
            message: "Could not understand the policy. None of the clauses parsed.".to_string(),
            unparseable_segments,
        })
    } else {
        Ok(ParsedPolicies {
            rules,
            unparseable_segments,
        })
    }
}

/// Evaluate a proposed rule against the last 90 days of invoices.
pub async fn preview_against_history(
    tenant_id: &TenantId,
    proposed: &ProposedRule,
    pool: &Arc<PgPool>,
) -> Result<PolicyPreview, billforge_core::Error> {
    #[derive(sqlx::FromRow)]
    struct InvoiceRow {
        id: uuid::Uuid,
        invoice_number: Option<String>,
        vendor_name: Option<String>,
        total_amount_cents: Option<i64>,
        processing_status: Option<String>,
        invoice_date: Option<chrono::NaiveDate>,
    }

    let rows = sqlx::query_as::<_, InvoiceRow>(
        r#"
        SELECT id, invoice_number, vendor_name, total_amount_cents,
               processing_status, invoice_date
        FROM invoices
        WHERE tenant_id = $1
          AND created_at >= NOW() - INTERVAL '90 days'
        ORDER BY created_at DESC
        "#,
    )
    .bind(*tenant_id.as_uuid())
    .fetch_all(pool.as_ref())
    .await
    .map_err(|e| billforge_core::Error::Database(format!("Failed to fetch invoices: {}", e)))?;

    let total_invoices = rows.len();
    let mut matched_count = 0usize;
    let mut matched_rows: Vec<InvoiceRow> = Vec::new();

    let conditions = &proposed.condition_json;
    let amount_threshold = conditions
        .get("amount_greater_than")
        .and_then(|v| v.as_i64());
    let vendor_filter = conditions
        .get("vendor_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_lowercase());
    let category_filter = conditions
        .get("category")
        .and_then(|v| v.as_str())
        .map(|s| s.to_lowercase());

    for row in &rows {
        let mut matches = true;

        if let Some(threshold) = amount_threshold {
            let amount = row.total_amount_cents.unwrap_or(0);
            if amount < threshold {
                matches = false;
            }
        }

        if let Some(ref vendor) = vendor_filter {
            let name = row
                .vendor_name
                .as_ref()
                .map(|s| s.to_lowercase())
                .unwrap_or_default();
            if !name.contains(vendor) {
                matches = false;
            }
        }

        if let Some(ref _cat) = category_filter {
            // Category matching would need department field; include all for preview
            // since department categorization is approximate.
            // The `department` and `new_vendor` conditions added by Patterns 6 & 7
            // are likewise approximated here: real department/new-vendor signals are
            // not present on historical invoice rows in this slice, so matched_count
            // for those rules reflects only the amount/vendor_name filters above.
        }

        if matches {
            matched_count += 1;
            matched_rows.push(InvoiceRow {
                id: row.id,
                invoice_number: row.invoice_number.clone(),
                vendor_name: row.vendor_name.clone(),
                total_amount_cents: row.total_amount_cents,
                processing_status: row.processing_status.clone(),
                invoice_date: row.invoice_date,
            });
        }
    }

    let sample_invoices: Vec<InvoiceSummaryRow> = matched_rows
        .iter()
        .take(5)
        .map(|r| InvoiceSummaryRow {
            id: r.id.to_string(),
            invoice_number: r.invoice_number.clone(),
            vendor_name: r.vendor_name.clone(),
            total_amount_cents: r.total_amount_cents,
            processing_status: r.processing_status.clone(),
            invoice_date: r.invoice_date.map(|d| d.to_string()),
        })
        .collect();

    let mut status_counts = std::collections::HashMap::new();
    for row in &matched_rows {
        let status = row
            .processing_status
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        *status_counts.entry(status).or_insert(0usize) += 1;
    }

    Ok(PolicyPreview {
        matched_count,
        total_invoices,
        sample_invoices,
        projected_action_breakdown: serde_json::json!({
            "by_status": status_counts,
            "action": proposed.action_json.get("action").unwrap_or(&serde_json::Value::String("apply".to_string())),
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_approval_threshold() {
        let rule = parse_policy("over $5000 require approval from manager").unwrap();
        assert_eq!(rule.guardrail_kind, GuardrailKind::ApprovalLimit);
        assert_eq!(rule.condition_json["amount_greater_than"], 500000);
        assert_eq!(rule.action_json["approval_from_role"], "manager");
    }

    #[test]
    fn test_parse_vendor_action() {
        let rule = parse_policy("invoices from vendor Acme Corp need review").unwrap();
        assert_eq!(rule.guardrail_kind, GuardrailKind::RoutingRule);
        assert_eq!(rule.condition_json["vendor_name"], "Acme Corp");
    }

    #[test]
    fn test_parse_block_without_po() {
        let rule = parse_policy("block invoices over $10000 without PO").unwrap();
        assert_eq!(rule.guardrail_kind, GuardrailKind::Block);
        assert_eq!(rule.condition_json["amount_greater_than"], 1000000);
        assert_eq!(rule.condition_json["has_po"], false);
    }

    #[test]
    fn test_parse_route_category() {
        let rule = parse_policy("route travel to finance").unwrap();
        assert_eq!(rule.guardrail_kind, GuardrailKind::RoutingRule);
        assert_eq!(rule.condition_json["category"], "travel");
        assert_eq!(rule.action_json["approver"], "finance");
    }

    #[test]
    fn test_parse_budget_cap() {
        let rule = parse_policy("cap monthly spend on software at $5000").unwrap();
        assert_eq!(rule.guardrail_kind, GuardrailKind::BudgetCap);
        assert_eq!(rule.condition_json["monthly_cap_cents"], 500000);
        assert_eq!(rule.condition_json["category"], "software");
    }

    #[test]
    fn test_parse_unparseable() {
        let result = parse_policy("make everything go faster");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(!err.unparseable_segments.is_empty());
    }

    #[test]
    fn test_parse_with_commas() {
        let rule = parse_policy("over $10,000 require approval from director").unwrap();
        assert_eq!(rule.condition_json["amount_greater_than"], 1000000);
    }

    #[test]
    fn test_parse_marquee_example_compound() {
        // The exact marquee example from the issue (#382).
        let text = "Invoices over $10k from Marketing go to the CMO, and anything from a new vendor needs Finance review before approval";
        let parsed = parse_policies(text).expect("marquee example must parse");

        assert_eq!(
            parsed.rules.len(),
            2,
            "marquee example must produce exactly 2 rules"
        );
        assert!(
            parsed.unparseable_segments.is_empty(),
            "no segments should be unparseable: {:?}",
            parsed.unparseable_segments
        );

        // First clause: routing rule with amount + department + route_to_role.
        let first = &parsed.rules[0];
        assert_eq!(first.guardrail_kind, GuardrailKind::RoutingRule);
        assert_eq!(first.condition_json["amount_greater_than"], 1_000_000);
        assert_eq!(first.condition_json["department"], "marketing");
        assert_eq!(first.action_json["route_to_role"], "cmo");

        // Second clause: new-vendor approval with approval_from_role.
        let second = &parsed.rules[1];
        assert_eq!(second.guardrail_kind, GuardrailKind::ApprovalLimit);
        assert_eq!(second.condition_json["new_vendor"], true);
        assert_eq!(second.action_json["approval_from_role"], "finance");
    }

    #[test]
    fn test_parse_partial_compound_returns_parsed_and_segments() {
        let text = "over $5000 require approval from manager, and frobnicate the widgets";
        let parsed = parse_policies(text).expect("partial compound should still parse");

        assert_eq!(parsed.rules.len(), 1);
        assert_eq!(parsed.rules[0].guardrail_kind, GuardrailKind::ApprovalLimit);
        assert_eq!(
            parsed.rules[0].action_json["approval_from_role"],
            "manager"
        );

        assert_eq!(parsed.unparseable_segments.len(), 1);
        assert_eq!(parsed.unparseable_segments[0], "frobnicate the widgets");
    }

    #[test]
    fn test_amount_k_shorthand() {
        let rule = parse_policy("invoices over $10k from marketing go to the cmo").unwrap();
        assert_eq!(rule.guardrail_kind, GuardrailKind::RoutingRule);
        assert_eq!(rule.condition_json["amount_greater_than"], 1_000_000);
        assert_eq!(rule.condition_json["department"], "marketing");
        assert_eq!(rule.action_json["route_to_role"], "cmo");
    }
}
