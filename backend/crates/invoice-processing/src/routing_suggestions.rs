//! Smart approver auto-routing pattern miner.
//!
//! Mines `learning_corrections` rows of type `approver_reroute` and groups
//! them by (vendor, amount-bucket) and (department, amount-bucket) to surface
//! actionable routing suggestions to admins, e.g. "80% of facilities
//! invoices over $5k are re-routed to Dana - update the rule?".
//!
//! Issue #440.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

/// Coarse amount bucket used to group reroutes for pattern detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AmountBucket {
    Under1k,
    Range1kTo5k,
    Range5kTo25k,
    Over25k,
}

impl AmountBucket {
    /// Bucket an amount expressed in cents.
    pub fn from_cents(cents: i64) -> Self {
        match cents {
            n if n < 100_000 => AmountBucket::Under1k,
            n if n < 500_000 => AmountBucket::Range1kTo5k,
            n if n < 2_500_000 => AmountBucket::Range5kTo25k,
            _ => AmountBucket::Over25k,
        }
    }

    /// Narrative label that fits the issue's phrasing
    /// ("...invoices over $5k are re-routed to Dana...").
    pub fn label(&self) -> &'static str {
        match self {
            AmountBucket::Under1k => "under $1k",
            AmountBucket::Range1kTo5k => "between $1k and $5k",
            AmountBucket::Range5kTo25k => "between $5k and $25k",
            AmountBucket::Over25k => "over $25k",
        }
    }
}

/// Key identifying the (vendor|department, amount-bucket) pattern under which
/// reroutes are grouped. Exactly one of `vendor_id` / `department` is set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingPatternKey {
    pub vendor_id: Option<Uuid>,
    pub vendor_name: Option<String>,
    pub department: Option<String>,
    pub amount_bucket: AmountBucket,
}

/// A mined suggestion the admin can act on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingPatternSuggestion {
    pub tenant_id: Uuid,
    pub pattern_key: RoutingPatternKey,
    pub dominant_approver_id: Uuid,
    pub dominant_approver_name: Option<String>,
    pub sample_size: i32,
    pub confidence_pct: i32,
    pub current_rule_approver_id: Option<Uuid>,
    pub suggested_action: SuggestedAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestedAction {
    /// A static rule already targets this segment but points at a different
    /// approver - update it.
    UpdateRule,
    /// No static rule covers this segment - create one.
    CreateRule,
}

/// One row pulled from the join of `learning_corrections` x `invoices`.
#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
struct RerouteRow {
    invoice_id: Option<Uuid>,
    vendor_id: Option<Uuid>,
    vendor_name: Option<String>,
    department: Option<String>,
    total_amount_cents: Option<i64>,
    original_value: serde_json::Value,
    corrected_value: serde_json::Value,
}

/// Aggregation key used while scanning rows.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct GroupKey {
    vendor_id: Option<Uuid>,
    department: Option<String>,
    amount_bucket: AmountBucket,
}

#[derive(Debug, Default)]
struct GroupAgg {
    sample_size: i32,
    per_approver: HashMap<Uuid, i32>,
    vendor_name: Option<String>,
}

/// Mine routing pattern suggestions for a tenant.
///
/// Looks back `lookback_days` days; keeps only groups with `>= min_sample_size`
/// reroutes and where one approver dominates with `>= min_confidence_pct`.
pub async fn mine_routing_patterns(
    pool: &PgPool,
    tenant_id: Uuid,
    lookback_days: i32,
    min_sample_size: i32,
    min_confidence_pct: i32,
) -> Result<Vec<RoutingPatternSuggestion>> {
    let lookback = lookback_days.max(1) as i64;

    // Pull every reroute correction inside the window, joined to the invoice
    // for the routing dimensions. `tenant_id = $1` is enforced on both sides
    // so a row only contributes when the invoice it points at also belongs to
    // the same tenant.
    let rows: Vec<RerouteRow> = sqlx::query_as::<_, RerouteRow>(
        r#"
        SELECT
            i.id                       AS invoice_id,
            i.vendor_id                AS vendor_id,
            i.vendor_name              AS vendor_name,
            i.department               AS department,
            i.total_amount_cents       AS total_amount_cents,
            lc.original_value          AS original_value,
            lc.corrected_value         AS corrected_value
        FROM learning_corrections lc
        LEFT JOIN invoices i
               ON i.id = lc.source_entity_id
              AND i.tenant_id = lc.tenant_id
        WHERE lc.tenant_id = $1
          AND lc.correction_type = 'approver_reroute'
          AND lc.created_at >= NOW() - ($2 || ' days')::INTERVAL
        "#,
    )
    .bind(tenant_id)
    .bind(lookback.to_string())
    .fetch_all(pool)
    .await
    .context("Failed to load reroute corrections")?;

    // Bucket every reroute under (vendor, bucket) AND (department, bucket).
    let mut groups: HashMap<GroupKey, GroupAgg> = HashMap::new();

    for row in rows {
        let Some(corrected_approver) = extract_approver_id(&row.corrected_value) else {
            continue;
        };
        let amount = row.total_amount_cents.unwrap_or(0);
        if amount <= 0 {
            continue;
        }
        let bucket = AmountBucket::from_cents(amount);

        if let Some(vid) = row.vendor_id {
            let key = GroupKey {
                vendor_id: Some(vid),
                department: None,
                amount_bucket: bucket,
            };
            let agg = groups.entry(key).or_default();
            agg.sample_size += 1;
            if agg.vendor_name.is_none() {
                agg.vendor_name = row.vendor_name.clone();
            }
            *agg.per_approver.entry(corrected_approver).or_insert(0) += 1;
        }

        if let Some(dept) = row.department.clone() {
            let key = GroupKey {
                vendor_id: None,
                department: Some(dept),
                amount_bucket: bucket,
            };
            let agg = groups.entry(key).or_default();
            agg.sample_size += 1;
            *agg.per_approver.entry(corrected_approver).or_insert(0) += 1;
        }
    }

    // Build suggestions from groups with enough samples and a dominant approver.
    let mut suggestions: Vec<RoutingPatternSuggestion> = Vec::new();
    for (key, agg) in groups.into_iter() {
        if agg.sample_size < min_sample_size {
            continue;
        }
        let Some((dom_id, dom_count)) = agg
            .per_approver
            .iter()
            .max_by_key(|(_, n)| **n)
            .map(|(k, v)| (*k, *v))
        else {
            continue;
        };
        let confidence_pct =
            ((dom_count as f64 / agg.sample_size as f64) * 100.0).round() as i32;
        if confidence_pct < min_confidence_pct {
            continue;
        }

        let dominant_name = fetch_user_name(pool, tenant_id, dom_id).await.ok().flatten();
        let current_rule_approver_id = lookup_current_rule_approver(
            pool,
            tenant_id,
            key.vendor_id,
            key.department.as_deref(),
            key.amount_bucket,
        )
        .await
        .ok()
        .flatten();

        let suggested_action = match current_rule_approver_id {
            Some(existing) if existing != dom_id => SuggestedAction::UpdateRule,
            Some(_) => continue, // rule already targets the learned approver - nothing to surface
            None => SuggestedAction::CreateRule,
        };

        suggestions.push(RoutingPatternSuggestion {
            tenant_id,
            pattern_key: RoutingPatternKey {
                vendor_id: key.vendor_id,
                vendor_name: agg.vendor_name.clone(),
                department: key.department.clone(),
                amount_bucket: key.amount_bucket,
            },
            dominant_approver_id: dom_id,
            dominant_approver_name: dominant_name,
            sample_size: agg.sample_size,
            confidence_pct,
            current_rule_approver_id,
            suggested_action,
        });
    }

    // Highest confidence first; then largest sample size as tiebreaker.
    suggestions.sort_by(|a, b| {
        b.confidence_pct
            .cmp(&a.confidence_pct)
            .then(b.sample_size.cmp(&a.sample_size))
    });

    Ok(suggestions)
}

/// Pick the strongest suggestion in `suggestions` that matches the routing
/// dimensions of a specific invoice (its vendor or department + its amount
/// bucket). Vendor-keyed patterns beat department-keyed ones; among those,
/// highest confidence then largest sample size wins. Used by the routing
/// engine to build a `LearnedRoutingHint` for the in-flight invoice.
pub fn pick_matching_suggestion<'a>(
    suggestions: &'a [RoutingPatternSuggestion],
    vendor_id: Option<Uuid>,
    department: Option<&str>,
    amount_cents: i64,
) -> Option<&'a RoutingPatternSuggestion> {
    if amount_cents <= 0 {
        return None;
    }
    let bucket = AmountBucket::from_cents(amount_cents);

    let matches = suggestions.iter().filter(|s| {
        if s.pattern_key.amount_bucket != bucket {
            return false;
        }
        match (s.pattern_key.vendor_id, s.pattern_key.department.as_deref()) {
            (Some(vid), _) => vendor_id == Some(vid),
            (None, Some(dept)) => department == Some(dept),
            (None, None) => false,
        }
    });

    matches.max_by(|a, b| {
        let vendor_rank = |s: &RoutingPatternSuggestion| s.pattern_key.vendor_id.is_some();
        vendor_rank(a)
            .cmp(&vendor_rank(b))
            .then(a.confidence_pct.cmp(&b.confidence_pct))
            .then(a.sample_size.cmp(&b.sample_size))
    })
}

fn extract_approver_id(val: &serde_json::Value) -> Option<Uuid> {
    val.get("approver_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
}

async fn fetch_user_name(
    pool: &PgPool,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as(
        r#"SELECT name FROM users WHERE id = $1 AND tenant_id = $2 LIMIT 1"#,
    )
    .bind(user_id)
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    Ok(row.map(|(name,)| name))
}

/// Best-effort lookup of the approver the static `assignment_rules` would
/// currently route this segment to. Returns `None` when no matching rule is
/// found or the rule's target shape is not a single user.
async fn lookup_current_rule_approver(
    pool: &PgPool,
    tenant_id: Uuid,
    vendor_id: Option<Uuid>,
    department: Option<&str>,
    _amount_bucket: AmountBucket,
) -> Result<Option<Uuid>> {
    // The `assignment_rules` table stores tenant_id as VARCHAR(255), conditions
    // as JSONB, and assign_to as JSONB (with shapes like {"User": <uuid>}).
    // We do a coarse pre-filter in SQL and pick the highest-priority match in
    // Rust. Anything more elaborate (operator-aware matching across all rule
    // shapes) is out of scope for the suggestion path; the admin still gets a
    // useful prompt because we mark the suggestion as `create_rule` when we
    // can't pin down an existing one.
    let tenant_id_str = tenant_id.to_string();
    let rows: Vec<(serde_json::Value, serde_json::Value, i32)> = sqlx::query_as(
        r#"
        SELECT conditions, assign_to, priority
          FROM assignment_rules
         WHERE tenant_id = $1
           AND is_active = true
         ORDER BY priority DESC
         LIMIT 50
        "#,
    )
    .bind(&tenant_id_str)
    .fetch_all(pool)
    .await
    .ok()
    .unwrap_or_default();

    for (conditions, assign_to, _prio) in rows {
        if !rule_conditions_match(&conditions, vendor_id, department) {
            continue;
        }
        if let Some(uid) = extract_assign_to_user(&assign_to) {
            return Ok(Some(uid));
        }
    }

    Ok(None)
}

fn rule_conditions_match(
    conditions: &serde_json::Value,
    vendor_id: Option<Uuid>,
    department: Option<&str>,
) -> bool {
    let Some(arr) = conditions.as_array() else {
        return false;
    };
    if arr.is_empty() {
        return false;
    }
    let target_vendor = vendor_id.map(|u| u.to_string());
    arr.iter().any(|cond| {
        let field = cond.get("field").and_then(|v| v.as_str()).unwrap_or("");
        let operator = cond
            .get("operator")
            .and_then(|v| v.as_str())
            .unwrap_or("equals");
        if operator != "equals" {
            return false;
        }
        let value = cond.get("value");
        match field {
            "vendor_id" => {
                if let (Some(vstr), Some(target)) = (
                    value.and_then(|v| v.as_str()),
                    target_vendor.as_deref(),
                ) {
                    vstr == target
                } else {
                    false
                }
            }
            "department" => {
                if let (Some(vstr), Some(target)) = (value.and_then(|v| v.as_str()), department) {
                    vstr == target
                } else {
                    false
                }
            }
            _ => false,
        }
    })
}

fn extract_assign_to_user(assign_to: &serde_json::Value) -> Option<Uuid> {
    if let Some(obj) = assign_to.as_object() {
        if let Some(u) = obj.get("User").and_then(|v| v.as_str()) {
            if let Ok(uid) = Uuid::parse_str(u) {
                return Some(uid);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amount_bucket_boundaries() {
        assert_eq!(AmountBucket::from_cents(99_999), AmountBucket::Under1k);
        assert_eq!(AmountBucket::from_cents(100_000), AmountBucket::Range1kTo5k);
        assert_eq!(AmountBucket::from_cents(499_999), AmountBucket::Range1kTo5k);
        assert_eq!(AmountBucket::from_cents(500_000), AmountBucket::Range5kTo25k);
        assert_eq!(AmountBucket::from_cents(2_499_999), AmountBucket::Range5kTo25k);
        assert_eq!(AmountBucket::from_cents(2_500_000), AmountBucket::Over25k);
    }

    #[test]
    fn extract_approver_id_handles_missing_and_invalid() {
        assert!(extract_approver_id(&serde_json::json!({})).is_none());
        assert!(extract_approver_id(&serde_json::json!({"approver_id": "not-a-uuid"})).is_none());
        let id = Uuid::new_v4();
        assert_eq!(
            extract_approver_id(&serde_json::json!({"approver_id": id.to_string()})),
            Some(id)
        );
    }

    #[test]
    fn rule_conditions_match_vendor_equals() {
        let vid = Uuid::new_v4();
        let conds = serde_json::json!([
            {"field": "vendor_id", "operator": "equals", "value": vid.to_string()}
        ]);
        assert!(rule_conditions_match(&conds, Some(vid), None));
        assert!(!rule_conditions_match(&conds, Some(Uuid::new_v4()), None));
    }

    #[test]
    fn rule_conditions_match_department_equals() {
        let conds = serde_json::json!([
            {"field": "department", "operator": "equals", "value": "facilities"}
        ]);
        assert!(rule_conditions_match(&conds, None, Some("facilities")));
        assert!(!rule_conditions_match(&conds, None, Some("marketing")));
    }

    fn make_suggestion(
        vendor_id: Option<Uuid>,
        department: Option<&str>,
        bucket: AmountBucket,
        approver: Uuid,
        confidence: i32,
        sample: i32,
    ) -> RoutingPatternSuggestion {
        RoutingPatternSuggestion {
            tenant_id: Uuid::new_v4(),
            pattern_key: RoutingPatternKey {
                vendor_id,
                vendor_name: None,
                department: department.map(|s| s.to_string()),
                amount_bucket: bucket,
            },
            dominant_approver_id: approver,
            dominant_approver_name: None,
            sample_size: sample,
            confidence_pct: confidence,
            current_rule_approver_id: None,
            suggested_action: SuggestedAction::UpdateRule,
        }
    }

    #[test]
    fn pick_matching_suggestion_prefers_vendor_over_department() {
        let dana = Uuid::new_v4();
        let other = Uuid::new_v4();
        let vendor_id = Uuid::new_v4();

        let suggestions = vec![
            make_suggestion(
                None,
                Some("facilities"),
                AmountBucket::Range5kTo25k,
                other,
                75,
                12,
            ),
            make_suggestion(
                Some(vendor_id),
                None,
                AmountBucket::Range5kTo25k,
                dana,
                92,
                25,
            ),
        ];

        let picked = pick_matching_suggestion(
            &suggestions,
            Some(vendor_id),
            Some("facilities"),
            750_000,
        )
        .expect("expected a match");
        assert_eq!(picked.dominant_approver_id, dana);
    }

    #[test]
    fn pick_matching_suggestion_falls_back_to_department() {
        let dana = Uuid::new_v4();
        let suggestions = vec![make_suggestion(
            None,
            Some("facilities"),
            AmountBucket::Range5kTo25k,
            dana,
            80,
            10,
        )];

        let picked = pick_matching_suggestion(
            &suggestions,
            None,
            Some("facilities"),
            750_000,
        )
        .expect("expected a match");
        assert_eq!(picked.dominant_approver_id, dana);
    }

    #[test]
    fn pick_matching_suggestion_returns_none_on_bucket_mismatch() {
        let dana = Uuid::new_v4();
        let suggestions = vec![make_suggestion(
            None,
            Some("facilities"),
            AmountBucket::Over25k,
            dana,
            95,
            30,
        )];

        assert!(pick_matching_suggestion(
            &suggestions,
            None,
            Some("facilities"),
            750_000,
        )
        .is_none());
    }

    #[test]
    fn extract_assign_to_user_handles_user_shape() {
        let uid = Uuid::new_v4();
        let val = serde_json::json!({"User": uid.to_string()});
        assert_eq!(extract_assign_to_user(&val), Some(uid));
        assert!(extract_assign_to_user(&serde_json::json!({"Role": "manager"})).is_none());
    }

    #[sqlx::test]
    #[ignore = "requires PostgreSQL database"]
    async fn mines_dominant_approver_for_facilities_over_5k(pool: sqlx::PgPool) {
        setup_routing_test_tables(&pool).await;

        let tenant_id = Uuid::new_v4();
        let other_tenant = Uuid::new_v4();
        let dana = Uuid::new_v4();
        let original_approver = Uuid::new_v4();
        seed_user(&pool, dana, tenant_id, "Dana").await;
        seed_user(&pool, original_approver, tenant_id, "Original").await;

        // 10 facilities invoices over $5k, 8 rerouted to Dana, 2 to original.
        for i in 0..10 {
            let invoice_id = Uuid::new_v4();
            seed_invoice(&pool, invoice_id, tenant_id, "facilities", 750_000).await;
            let to = if i < 8 { dana } else { original_approver };
            seed_reroute(&pool, tenant_id, invoice_id, original_approver, to).await;
        }

        // Tenant isolation: a different tenant has its own reroutes.
        for _ in 0..6 {
            let invoice_id = Uuid::new_v4();
            seed_invoice(&pool, invoice_id, other_tenant, "facilities", 750_000).await;
            seed_reroute(&pool, other_tenant, invoice_id, original_approver, dana).await;
        }

        let suggestions = mine_routing_patterns(&pool, tenant_id, 30, 5, 70)
            .await
            .expect("mining must succeed");

        // The facilities/over-5k group should produce a suggestion for Dana.
        let dept_suggestion = suggestions
            .iter()
            .find(|s| {
                s.pattern_key.department.as_deref() == Some("facilities")
                    && s.pattern_key.amount_bucket == AmountBucket::Range5kTo25k
            })
            .expect("expected a facilities/$5k-$25k suggestion");
        assert_eq!(dept_suggestion.dominant_approver_id, dana);
        assert_eq!(dept_suggestion.sample_size, 10);
        assert_eq!(dept_suggestion.confidence_pct, 80);

        // Other-tenant reroutes must never bleed in.
        assert!(suggestions.iter().all(|s| s.tenant_id == tenant_id));
    }

    async fn setup_routing_test_tables(pool: &sqlx::PgPool) {
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS users (
                id UUID PRIMARY KEY,
                tenant_id UUID NOT NULL,
                name TEXT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )"#,
        )
        .execute(pool)
        .await
        .expect("create users");

        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS invoices (
                id UUID PRIMARY KEY,
                tenant_id UUID NOT NULL,
                vendor_id UUID,
                vendor_name TEXT,
                department TEXT,
                total_amount_cents BIGINT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )"#,
        )
        .execute(pool)
        .await
        .expect("create invoices");

        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS learning_corrections (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                tenant_id UUID NOT NULL,
                correction_type TEXT NOT NULL,
                source_entity_id UUID,
                source_entity_type TEXT NOT NULL DEFAULT 'invoice',
                original_value JSONB NOT NULL DEFAULT '{}'::jsonb,
                corrected_value JSONB NOT NULL DEFAULT '{}'::jsonb,
                user_id UUID,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )"#,
        )
        .execute(pool)
        .await
        .expect("create learning_corrections");

        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS assignment_rules (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                tenant_id VARCHAR(255) NOT NULL,
                queue_id UUID,
                name VARCHAR(255) NOT NULL,
                priority INTEGER NOT NULL DEFAULT 0,
                is_active BOOLEAN NOT NULL DEFAULT true,
                conditions JSONB NOT NULL DEFAULT '[]',
                assign_to JSONB NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )"#,
        )
        .execute(pool)
        .await
        .expect("create assignment_rules");
    }

    async fn seed_user(pool: &sqlx::PgPool, id: Uuid, tenant_id: Uuid, name: &str) {
        sqlx::query("INSERT INTO users (id, tenant_id, name) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(tenant_id)
            .bind(name)
            .execute(pool)
            .await
            .expect("insert user");
    }

    async fn seed_invoice(
        pool: &sqlx::PgPool,
        id: Uuid,
        tenant_id: Uuid,
        department: &str,
        amount_cents: i64,
    ) {
        sqlx::query(
            r#"INSERT INTO invoices (id, tenant_id, department, total_amount_cents)
               VALUES ($1, $2, $3, $4)"#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(department)
        .bind(amount_cents)
        .execute(pool)
        .await
        .expect("insert invoice");
    }

    async fn seed_reroute(
        pool: &sqlx::PgPool,
        tenant_id: Uuid,
        invoice_id: Uuid,
        from_approver: Uuid,
        to_approver: Uuid,
    ) {
        sqlx::query(
            r#"INSERT INTO learning_corrections
                (tenant_id, correction_type, source_entity_id, source_entity_type,
                 original_value, corrected_value)
               VALUES ($1, 'approver_reroute', $2, 'invoice', $3, $4)"#,
        )
        .bind(tenant_id)
        .bind(invoice_id)
        .bind(serde_json::json!({"approver_id": from_approver.to_string()}))
        .bind(serde_json::json!({"approver_id": to_approver.to_string()}))
        .execute(pool)
        .await
        .expect("insert reroute");
    }
}
