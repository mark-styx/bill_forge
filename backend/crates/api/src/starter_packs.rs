//! Industry starter packs for tenant onboarding.
//!
//! New tenants select an industry vertical during onboarding. This module
//! provides pre-built configuration packs (GL accounts, vendor categories,
//! approval workflows, policy thresholds) that seed the tenant workspace
//! with realistic defaults. Packs are code-as-config: no template tables,
//! just static Rust data applied at provision time.

use billforge_core::TenantId;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fmt;
use std::str::FromStr;

// ---------------------------------------------------------------------------
// Industry enum
// ---------------------------------------------------------------------------

/// Supported industry verticals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Industry {
    Construction,
    ProfessionalServices,
    Retail,
    Generic,
}

impl Industry {
    /// Human-readable label for UI display.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Construction => "Construction",
            Self::ProfessionalServices => "Professional Services",
            Self::Retail => "Retail",
            Self::Generic => "Generic",
        }
    }
}

impl FromStr for Industry {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "construction" => Ok(Self::Construction),
            "professional_services" => Ok(Self::ProfessionalServices),
            "retail" => Ok(Self::Retail),
            "generic" => Ok(Self::Generic),
            other => Err(format!(
                "Unknown industry '{}'. Valid options: construction, professional_services, retail, generic",
                other
            )),
        }
    }
}

impl fmt::Display for Industry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Construction => write!(f, "construction"),
            Self::ProfessionalServices => write!(f, "professional_services"),
            Self::Retail => write!(f, "retail"),
            Self::Generic => write!(f, "generic"),
        }
    }
}

// ---------------------------------------------------------------------------
// Seed structs
// ---------------------------------------------------------------------------

pub struct GlAccountSeed {
    pub code: &'static str,
    pub name: &'static str,
    pub account_type: &'static str,
}

pub struct VendorCategorySeed {
    pub name: &'static str,
    pub default_gl_code: &'static str,
    pub description: &'static str,
}

pub struct ApprovalRuleSeed {
    pub name: &'static str,
    pub description: &'static str,
    pub rule_type: &'static str,
    pub priority: i32,
    pub conditions: serde_json::Value,
    pub actions: serde_json::Value,
}

pub struct PolicyThresholdSeed {
    pub name: &'static str,
    pub threshold_type: &'static str,
    pub value: i64,
    pub applies_to: &'static str,
}

pub struct StarterPack {
    pub gl_accounts: Vec<GlAccountSeed>,
    pub vendor_categories: Vec<VendorCategorySeed>,
    pub approval_rules: Vec<ApprovalRuleSeed>,
    pub policy_thresholds: Vec<PolicyThresholdSeed>,
}

// ---------------------------------------------------------------------------
// Pack data per vertical
// ---------------------------------------------------------------------------

/// Return the hard-coded starter pack for the given industry.
pub fn pack_for(industry: Industry) -> StarterPack {
    match industry {
        Industry::Construction => construction_pack(),
        Industry::ProfessionalServices => professional_services_pack(),
        Industry::Retail => retail_pack(),
        Industry::Generic => StarterPack {
            gl_accounts: vec![],
            vendor_categories: vec![],
            approval_rules: vec![],
            policy_thresholds: vec![],
        },
    }
}

fn construction_pack() -> StarterPack {
    StarterPack {
        gl_accounts: vec![
            GlAccountSeed { code: "5100", name: "Subcontractor Labor", account_type: "expense" },
            GlAccountSeed { code: "5200", name: "Materials & Supplies", account_type: "expense" },
            GlAccountSeed { code: "5300", name: "Equipment Rental", account_type: "expense" },
            GlAccountSeed { code: "5400", name: "Permits & Fees", account_type: "expense" },
            GlAccountSeed { code: "6000", name: "Job Site Overhead", account_type: "expense" },
            GlAccountSeed { code: "6100", name: "Project Management", account_type: "expense" },
            GlAccountSeed { code: "7000", name: "Insurance", account_type: "expense" },
            GlAccountSeed { code: "7100", name: "Depreciation - Equipment", account_type: "expense" },
        ],
        vendor_categories: vec![
            VendorCategorySeed { name: "Subcontractors", default_gl_code: "5100", description: "Trade subcontractors and specialty labor" },
            VendorCategorySeed { name: "Material Suppliers", default_gl_code: "5200", description: "Building materials and supplies" },
            VendorCategorySeed { name: "Equipment Rental", default_gl_code: "5300", description: "Heavy equipment and tool rental" },
            VendorCategorySeed { name: "Permit Services", default_gl_code: "5400", description: "Permit expediting and inspection services" },
            VendorCategorySeed { name: "Insurance Providers", default_gl_code: "7000", description: "Builder's risk and liability insurance" },
            VendorCategorySeed { name: "Professional Services", default_gl_code: "6100", description: "Architects, engineers, and consultants" },
        ],
        approval_rules: vec![
            ApprovalRuleSeed {
                name: "Subcontractor Payment Approval",
                description: "Subcontractor payments over $25,000 require controller approval",
                rule_type: "approval",
                priority: 100,
                conditions: json!([
                    {"field": "vendor_category", "operator": "equals", "value": "Subcontractors"},
                    {"field": "total_amount_cents", "operator": "greater_than", "value": 2_500_000}
                ]),
                actions: json!([
                    {"type": "require_approval", "role": "controller"}
                ]),
            },
            ApprovalRuleSeed {
                name: "Materials Auto-Approve",
                description: "Material purchases under $10,000 auto-approve",
                rule_type: "approval",
                priority: 50,
                conditions: json!([
                    {"field": "vendor_category", "operator": "equals", "value": "Material Suppliers"},
                    {"field": "total_amount_cents", "operator": "less_than", "value": 1_000_000}
                ]),
                actions: json!([
                    {"type": "auto_approve"}
                ]),
            },
        ],
        policy_thresholds: vec![
            PolicyThresholdSeed { name: "Max Subcontractor Payment", threshold_type: "max_amount", value: 50_000_00, applies_to: "Subcontractors" },
            PolicyThresholdSeed { name: "Auto-Approve Materials", threshold_type: "auto_approve", value: 5_000_00, applies_to: "Material Suppliers" },
            PolicyThresholdSeed { name: "Retention Percentage", threshold_type: "percentage", value: 10, applies_to: "Subcontractors" },
        ],
    }
}

fn professional_services_pack() -> StarterPack {
    StarterPack {
        gl_accounts: vec![
            GlAccountSeed { code: "4100", name: "Consulting Revenue", account_type: "revenue" },
            GlAccountSeed { code: "5000", name: "Professional Fees", account_type: "expense" },
            GlAccountSeed { code: "5100", name: "Subcontractor Services", account_type: "expense" },
            GlAccountSeed { code: "5200", name: "Travel & Entertainment", account_type: "expense" },
            GlAccountSeed { code: "6000", name: "Office Expenses", account_type: "expense" },
            GlAccountSeed { code: "6100", name: "Software Subscriptions", account_type: "expense" },
            GlAccountSeed { code: "7000", name: "Marketing", account_type: "expense" },
            GlAccountSeed { code: "7100", name: "Insurance", account_type: "expense" },
        ],
        vendor_categories: vec![
            VendorCategorySeed { name: "Contractors & Consultants", default_gl_code: "5100", description: "Independent contractors and freelance consultants" },
            VendorCategorySeed { name: "Travel Providers", default_gl_code: "5200", description: "Airlines, hotels, and travel agencies" },
            VendorCategorySeed { name: "Software Vendors", default_gl_code: "6100", description: "SaaS tools and software licenses" },
            VendorCategorySeed { name: "Office Suppliers", default_gl_code: "6000", description: "Office supplies and equipment" },
            VendorCategorySeed { name: "Marketing Agencies", default_gl_code: "7000", description: "Advertising, PR, and creative agencies" },
            VendorCategorySeed { name: "Insurance Providers", default_gl_code: "7100", description: "Professional liability and E&O insurance" },
        ],
        approval_rules: vec![
            ApprovalRuleSeed {
                name: "Consulting Fee Approval",
                description: "External consultant payments over $15,000 require partner approval",
                rule_type: "approval",
                priority: 100,
                conditions: json!([
                    {"field": "vendor_category", "operator": "equals", "value": "Contractors & Consultants"},
                    {"field": "total_amount_cents", "operator": "greater_than", "value": 1_500_000}
                ]),
                actions: json!([
                    {"type": "require_approval", "role": "partner"}
                ]),
            },
            ApprovalRuleSeed {
                name: "Expense Auto-Approve",
                description: "Office and software expenses under $2,500 auto-approve",
                rule_type: "approval",
                priority: 50,
                conditions: json!([
                    {"field": "vendor_category", "operator": "in", "value": ["Office Suppliers", "Software Vendors"]},
                    {"field": "total_amount_cents", "operator": "less_than", "value": 250_000}
                ]),
                actions: json!([
                    {"type": "auto_approve"}
                ]),
            },
        ],
        policy_thresholds: vec![
            PolicyThresholdSeed { name: "Max Consulting Fee", threshold_type: "max_amount", value: 25_000_00, applies_to: "Contractors & Consultants" },
            PolicyThresholdSeed { name: "Auto-Approve Expenses", threshold_type: "auto_approve", value: 2_500_00, applies_to: "Office Suppliers" },
            PolicyThresholdSeed { name: "Billable Threshold", threshold_type: "min_amount", value: 50_00, applies_to: "Travel Providers" },
        ],
    }
}

fn retail_pack() -> StarterPack {
    StarterPack {
        gl_accounts: vec![
            GlAccountSeed { code: "4100", name: "Sales Revenue", account_type: "revenue" },
            GlAccountSeed { code: "5000", name: "Cost of Goods Sold", account_type: "expense" },
            GlAccountSeed { code: "5100", name: "Inventory Purchases", account_type: "expense" },
            GlAccountSeed { code: "5200", name: "Freight & Shipping", account_type: "expense" },
            GlAccountSeed { code: "6000", name: "Store Operations", account_type: "expense" },
            GlAccountSeed { code: "6100", name: "Marketing & Advertising", account_type: "expense" },
            GlAccountSeed { code: "7000", name: "Utilities", account_type: "expense" },
            GlAccountSeed { code: "7100", name: "Maintenance & Repairs", account_type: "expense" },
        ],
        vendor_categories: vec![
            VendorCategorySeed { name: "Suppliers & Distributors", default_gl_code: "5100", description: "Product suppliers and wholesale distributors" },
            VendorCategorySeed { name: "Freight Carriers", default_gl_code: "5200", description: "Shipping and freight companies" },
            VendorCategorySeed { name: "Marketing Vendors", default_gl_code: "6100", description: "Advertising, signage, and promotional vendors" },
            VendorCategorySeed { name: "Maintenance Services", default_gl_code: "7100", description: "Facility maintenance and repair services" },
            VendorCategorySeed { name: "Utility Providers", default_gl_code: "7000", description: "Electric, gas, water, and telecom" },
            VendorCategorySeed { name: "Store Fixtures", default_gl_code: "6000", description: "Shelving, displays, and store equipment" },
        ],
        approval_rules: vec![
            ApprovalRuleSeed {
                name: "Large Purchase Order Approval",
                description: "Purchase orders over $50,000 require VP approval",
                rule_type: "approval",
                priority: 100,
                conditions: json!([
                    {"field": "vendor_category", "operator": "equals", "value": "Suppliers & Distributors"},
                    {"field": "total_amount_cents", "operator": "greater_than", "value": 5_000_000}
                ]),
                actions: json!([
                    {"type": "require_approval", "role": "vp_operations"}
                ]),
            },
            ApprovalRuleSeed {
                name: "Reorder Auto-Approve",
                description: "Standard reorders under $10,000 auto-approve",
                rule_type: "approval",
                priority: 50,
                conditions: json!([
                    {"field": "vendor_category", "operator": "equals", "value": "Suppliers & Distributors"},
                    {"field": "total_amount_cents", "operator": "less_than", "value": 1_000_000}
                ]),
                actions: json!([
                    {"type": "auto_approve"}
                ]),
            },
        ],
        policy_thresholds: vec![
            PolicyThresholdSeed { name: "Max Purchase Order", threshold_type: "max_amount", value: 100_000_00, applies_to: "Suppliers & Distributors" },
            PolicyThresholdSeed { name: "Auto-Approve Reorder", threshold_type: "auto_approve", value: 10_000_00, applies_to: "Suppliers & Distributors" },
            PolicyThresholdSeed { name: "Freight Tolerance", threshold_type: "percentage", value: 15, applies_to: "Freight Carriers" },
        ],
    }
}

// ---------------------------------------------------------------------------
// Pack application
// ---------------------------------------------------------------------------

/// Apply a starter pack to a freshly provisioned tenant database.
///
/// Creates supporting tables (gl_accounts, vendor_categories, policy_thresholds)
/// if they do not exist, then inserts seed rows into those tables plus the
/// existing workflow_rules table.  Idempotent: skips if any gl_accounts rows
/// already exist for the tenant.
pub async fn apply_pack(
    pool: &sqlx::PgPool,
    tenant_id: &TenantId,
    industry: Industry,
) -> Result<(), sqlx::Error> {
    let pack = pack_for(industry);
    if pack.gl_accounts.is_empty() {
        return Ok(());
    }

    let tid = tenant_id.to_string();

    // Ensure supporting tables exist
    ensure_starter_pack_tables(pool).await?;

    // Idempotency: skip if GL accounts already seeded for this tenant
    let existing: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM gl_accounts WHERE tenant_id = $1",
    )
    .bind(&tid)
    .fetch_one(pool)
    .await?;

    if existing.0 > 0 {
        tracing::info!(
            tenant_id = %tid,
            industry = %industry,
            "Starter pack already applied, skipping"
        );
        return Ok(());
    }

    // GL accounts
    for acct in &pack.gl_accounts {
        sqlx::query(
            r#"
            INSERT INTO gl_accounts (tenant_id, account_code, account_name, account_type)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (tenant_id, account_code) DO NOTHING
            "#,
        )
        .bind(&tid)
        .bind(acct.code)
        .bind(acct.name)
        .bind(acct.account_type)
        .execute(pool)
        .await?;
    }

    // Vendor categories
    for cat in &pack.vendor_categories {
        sqlx::query(
            r#"
            INSERT INTO vendor_categories (tenant_id, category_name, default_gl_code, description)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (tenant_id, category_name) DO NOTHING
            "#,
        )
        .bind(&tid)
        .bind(cat.name)
        .bind(cat.default_gl_code)
        .bind(cat.description)
        .execute(pool)
        .await?;
    }

    // Approval workflow rules (into existing workflow_rules table)
    for rule in &pack.approval_rules {
        sqlx::query(
            r#"
            INSERT INTO workflow_rules (tenant_id, name, description, priority, rule_type, conditions, actions)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(&tid)
        .bind(rule.name)
        .bind(rule.description)
        .bind(rule.priority)
        .bind(rule.rule_type)
        .bind(&rule.conditions)
        .bind(&rule.actions)
        .execute(pool)
        .await?;
    }

    // Policy thresholds
    for pt in &pack.policy_thresholds {
        sqlx::query(
            r#"
            INSERT INTO policy_thresholds (tenant_id, threshold_name, threshold_type, threshold_value, applies_to)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (tenant_id, threshold_name) DO NOTHING
            "#,
        )
        .bind(&tid)
        .bind(pt.name)
        .bind(pt.threshold_type)
        .bind(pt.value)
        .bind(pt.applies_to)
        .execute(pool)
        .await?;
    }

    tracing::info!(
        tenant_id = %tid,
        industry = %industry,
        gl_accounts = pack.gl_accounts.len(),
        vendor_categories = pack.vendor_categories.len(),
        approval_rules = pack.approval_rules.len(),
        policy_thresholds = pack.policy_thresholds.len(),
        "Starter pack applied"
    );

    Ok(())
}

/// Create gl_accounts, vendor_categories, and policy_thresholds tables if they
/// do not already exist in the tenant database.
async fn ensure_starter_pack_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS gl_accounts (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id VARCHAR(255) NOT NULL,
            account_code VARCHAR(50) NOT NULL,
            account_name VARCHAR(255) NOT NULL,
            account_type VARCHAR(50) NOT NULL DEFAULT 'expense',
            is_active BOOLEAN NOT NULL DEFAULT true,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE(tenant_id, account_code)
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS vendor_categories (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id VARCHAR(255) NOT NULL,
            category_name VARCHAR(255) NOT NULL,
            default_gl_code VARCHAR(50),
            description TEXT,
            is_active BOOLEAN NOT NULL DEFAULT true,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE(tenant_id, category_name)
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS policy_thresholds (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id VARCHAR(255) NOT NULL,
            threshold_name VARCHAR(255) NOT NULL,
            threshold_type VARCHAR(50) NOT NULL,
            threshold_value BIGINT NOT NULL,
            currency VARCHAR(10) NOT NULL DEFAULT 'USD',
            applies_to VARCHAR(100),
            is_active BOOLEAN NOT NULL DEFAULT true,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE(tenant_id, threshold_name)
        )
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn industry_from_str_valid() {
        assert_eq!(Industry::from_str("construction").unwrap(), Industry::Construction);
        assert_eq!(Industry::from_str("professional_services").unwrap(), Industry::ProfessionalServices);
        assert_eq!(Industry::from_str("retail").unwrap(), Industry::Retail);
        assert_eq!(Industry::from_str("generic").unwrap(), Industry::Generic);
    }

    #[test]
    fn industry_from_str_invalid() {
        assert!(Industry::from_str("healthcare").is_err());
        assert!(Industry::from_str("").is_err());
        assert!(Industry::from_str("Construction").is_err()); // case-sensitive
    }

    #[test]
    fn industry_serde_round_trip() {
        let json = serde_json::to_string(&Industry::Construction).unwrap();
        assert_eq!(json, "\"construction\"");
        let back: Industry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Industry::Construction);
    }

    #[test]
    fn industry_display() {
        assert_eq!(format!("{}", Industry::ProfessionalServices), "professional_services");
        assert_eq!(format!("{}", Industry::Generic), "generic");
    }

    #[test]
    fn industry_label() {
        assert_eq!(Industry::Construction.label(), "Construction");
        assert_eq!(Industry::ProfessionalServices.label(), "Professional Services");
        assert_eq!(Industry::Retail.label(), "Retail");
        assert_eq!(Industry::Generic.label(), "Generic");
    }

    #[test]
    fn generic_pack_is_empty() {
        let pack = pack_for(Industry::Generic);
        assert!(pack.gl_accounts.is_empty());
        assert!(pack.vendor_categories.is_empty());
        assert!(pack.approval_rules.is_empty());
        assert!(pack.policy_thresholds.is_empty());
    }

    #[test]
    fn construction_pack_has_expected_size() {
        let pack = pack_for(Industry::Construction);
        assert_eq!(pack.gl_accounts.len(), 8);
        assert_eq!(pack.vendor_categories.len(), 6);
        assert_eq!(pack.approval_rules.len(), 2);
        assert_eq!(pack.policy_thresholds.len(), 3);

        // Verify a known GL account
        let sub = pack.gl_accounts.iter().find(|a| a.code == "5100").unwrap();
        assert_eq!(sub.name, "Subcontractor Labor");
    }

    #[test]
    fn professional_services_pack_has_expected_size() {
        let pack = pack_for(Industry::ProfessionalServices);
        assert_eq!(pack.gl_accounts.len(), 8);
        assert_eq!(pack.vendor_categories.len(), 6);
        assert_eq!(pack.approval_rules.len(), 2);
        assert_eq!(pack.policy_thresholds.len(), 3);
    }

    #[test]
    fn retail_pack_has_expected_size() {
        let pack = pack_for(Industry::Retail);
        assert_eq!(pack.gl_accounts.len(), 8);
        assert_eq!(pack.vendor_categories.len(), 6);
        assert_eq!(pack.approval_rules.len(), 2);
        assert_eq!(pack.policy_thresholds.len(), 3);
    }

    #[test]
    fn all_packs_have_consistent_gl_codes() {
        // Every vendor category references a GL code that exists in the pack
        for industry in [Industry::Construction, Industry::ProfessionalServices, Industry::Retail] {
            let pack = pack_for(industry);
            let codes: std::collections::HashSet<&str> =
                pack.gl_accounts.iter().map(|a| a.code).collect();
            for cat in &pack.vendor_categories {
                assert!(
                    codes.contains(cat.default_gl_code),
                    "Industry {:?}: vendor category '{}' references unknown GL code '{}'",
                    industry,
                    cat.name,
                    cat.default_gl_code
                );
            }
        }
    }
}
