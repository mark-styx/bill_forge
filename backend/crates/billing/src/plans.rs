//! Subscription plan definitions

use serde::{Deserialize, Serialize};
use billforge_core::Module;

/// Plan identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanId {
    Free,
    Starter,
    Professional,
    Enterprise,
}

impl PlanId {
    pub fn as_str(&self) -> &'static str {
        match self {
            PlanId::Free => "free",
            PlanId::Starter => "starter",
            PlanId::Professional => "professional",
            PlanId::Enterprise => "enterprise",
        }
    }
}

impl std::fmt::Display for PlanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for PlanId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "free" => Ok(PlanId::Free),
            "starter" => Ok(PlanId::Starter),
            "professional" => Ok(PlanId::Professional),
            "enterprise" => Ok(PlanId::Enterprise),
            _ => Err(format!("Unknown plan: {}", s)),
        }
    }
}

/// Plan tier for comparison
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PlanTier {
    Free = 0,
    Starter = 1,
    Professional = 2,
    Enterprise = 3,
}

impl From<PlanId> for PlanTier {
    fn from(id: PlanId) -> Self {
        match id {
            PlanId::Free => PlanTier::Free,
            PlanId::Starter => PlanTier::Starter,
            PlanId::Professional => PlanTier::Professional,
            PlanId::Enterprise => PlanTier::Enterprise,
        }
    }
}

/// Features available in a plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanFeatures {
    /// Maximum number of users
    pub max_users: u32,
    /// Maximum invoices per month
    pub max_invoices_per_month: u32,
    /// Maximum vendors
    pub max_vendors: u32,
    /// Storage limit in GB
    pub storage_gb: u32,
    /// Available modules
    pub modules: Vec<Module>,
    /// Advanced OCR (AI-powered)
    pub advanced_ocr: bool,
    /// API access
    pub api_access: bool,
    /// Custom workflows
    pub custom_workflows: bool,
    /// Priority support
    pub priority_support: bool,
    /// SSO/SAML integration
    pub sso_enabled: bool,
    /// Audit log retention days
    pub audit_retention_days: u32,
    /// Custom branding
    pub custom_branding: bool,
    /// Dedicated account manager
    pub dedicated_account_manager: bool,
}

impl Default for PlanFeatures {
    fn default() -> Self {
        Self {
            max_users: 1,
            max_invoices_per_month: 50,
            max_vendors: 10,
            storage_gb: 1,
            modules: vec![Module::InvoiceCapture],
            advanced_ocr: false,
            api_access: false,
            custom_workflows: false,
            priority_support: false,
            sso_enabled: false,
            audit_retention_days: 30,
            custom_branding: false,
            dedicated_account_manager: false,
        }
    }
}

/// Subscription plan definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    /// Plan identifier
    pub id: PlanId,
    /// Display name
    pub name: String,
    /// Description
    pub description: String,
    /// Monthly price in cents
    pub monthly_price_cents: u64,
    /// Annual price in cents (typically discounted)
    pub annual_price_cents: u64,
    /// Plan features
    pub features: PlanFeatures,
    /// Stripe price ID for monthly billing
    pub stripe_monthly_price_id: Option<String>,
    /// Stripe price ID for annual billing
    pub stripe_annual_price_id: Option<String>,
    /// Is this plan publicly available?
    pub is_public: bool,
}

impl Plan {
    /// Get the Free plan
    pub fn free() -> Self {
        Self {
            id: PlanId::Free,
            name: "Free".to_string(),
            description: "Get started with basic invoice capture".to_string(),
            monthly_price_cents: 0,
            annual_price_cents: 0,
            features: PlanFeatures {
                max_users: 1,
                max_invoices_per_month: 25,
                max_vendors: 5,
                storage_gb: 1,
                modules: vec![Module::InvoiceCapture],
                advanced_ocr: false,
                api_access: false,
                custom_workflows: false,
                priority_support: false,
                sso_enabled: false,
                audit_retention_days: 7,
                custom_branding: false,
                dedicated_account_manager: false,
            },
            stripe_monthly_price_id: None,
            stripe_annual_price_id: None,
            is_public: true,
        }
    }

    /// Get the Starter plan
    pub fn starter() -> Self {
        Self {
            id: PlanId::Starter,
            name: "Starter".to_string(),
            description: "Perfect for small businesses".to_string(),
            monthly_price_cents: 4900, // $49/month
            annual_price_cents: 47000, // $470/year (~20% discount)
            features: PlanFeatures {
                max_users: 3,
                max_invoices_per_month: 200,
                max_vendors: 50,
                storage_gb: 10,
                modules: vec![Module::InvoiceCapture, Module::VendorManagement],
                advanced_ocr: false,
                api_access: false,
                custom_workflows: false,
                priority_support: false,
                sso_enabled: false,
                audit_retention_days: 30,
                custom_branding: false,
                dedicated_account_manager: false,
            },
            stripe_monthly_price_id: Some("price_starter_monthly".to_string()),
            stripe_annual_price_id: Some("price_starter_annual".to_string()),
            is_public: true,
        }
    }

    /// Get the Professional plan
    pub fn professional() -> Self {
        Self {
            id: PlanId::Professional,
            name: "Professional".to_string(),
            description: "Full AP automation for growing teams".to_string(),
            monthly_price_cents: 14900, // $149/month
            annual_price_cents: 142800, // $1428/year (~20% discount)
            features: PlanFeatures {
                max_users: 10,
                max_invoices_per_month: 1000,
                max_vendors: 500,
                storage_gb: 50,
                modules: vec![
                    Module::InvoiceCapture,
                    Module::InvoiceProcessing,
                    Module::VendorManagement,
                    Module::Reporting,
                ],
                advanced_ocr: true,
                api_access: true,
                custom_workflows: true,
                priority_support: true,
                sso_enabled: false,
                audit_retention_days: 90,
                custom_branding: true,
                dedicated_account_manager: false,
            },
            stripe_monthly_price_id: Some("price_professional_monthly".to_string()),
            stripe_annual_price_id: Some("price_professional_annual".to_string()),
            is_public: true,
        }
    }

    /// Get the Enterprise plan
    pub fn enterprise() -> Self {
        Self {
            id: PlanId::Enterprise,
            name: "Enterprise".to_string(),
            description: "Custom solutions for large organizations".to_string(),
            monthly_price_cents: 49900, // $499/month (base price)
            annual_price_cents: 479000, // $4790/year (~20% discount)
            features: PlanFeatures {
                max_users: u32::MAX,
                max_invoices_per_month: u32::MAX,
                max_vendors: u32::MAX,
                storage_gb: 500,
                modules: vec![
                    Module::InvoiceCapture,
                    Module::InvoiceProcessing,
                    Module::VendorManagement,
                    Module::Reporting,
                ],
                advanced_ocr: true,
                api_access: true,
                custom_workflows: true,
                priority_support: true,
                sso_enabled: true,
                audit_retention_days: 365,
                custom_branding: true,
                dedicated_account_manager: true,
            },
            stripe_monthly_price_id: Some("price_enterprise_monthly".to_string()),
            stripe_annual_price_id: Some("price_enterprise_annual".to_string()),
            is_public: false, // Contact sales
        }
    }

    /// Get all public plans
    pub fn all_public() -> Vec<Self> {
        vec![Self::free(), Self::starter(), Self::professional()]
    }

    /// Get all plans
    pub fn all() -> Vec<Self> {
        vec![
            Self::free(),
            Self::starter(),
            Self::professional(),
            Self::enterprise(),
        ]
    }

    /// Get a plan by ID
    pub fn by_id(id: PlanId) -> Self {
        match id {
            PlanId::Free => Self::free(),
            PlanId::Starter => Self::starter(),
            PlanId::Professional => Self::professional(),
            PlanId::Enterprise => Self::enterprise(),
        }
    }

    /// Get monthly price as dollars
    pub fn monthly_price(&self) -> f64 {
        self.monthly_price_cents as f64 / 100.0
    }

    /// Get annual price as dollars
    pub fn annual_price(&self) -> f64 {
        self.annual_price_cents as f64 / 100.0
    }

    /// Get monthly savings when paying annually
    pub fn annual_savings(&self) -> f64 {
        let monthly_total = self.monthly_price() * 12.0;
        monthly_total - self.annual_price()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_pricing() {
        let starter = Plan::starter();
        assert_eq!(starter.monthly_price(), 49.0);
        assert_eq!(starter.annual_price(), 470.0);
        assert!(starter.annual_savings() > 0.0);
    }

    #[test]
    fn test_plan_features() {
        let free = Plan::free();
        assert_eq!(free.features.max_users, 1);
        assert!(!free.features.api_access);

        let professional = Plan::professional();
        assert!(professional.features.api_access);
        assert!(professional.features.advanced_ocr);
    }

    #[test]
    fn test_plan_by_id() {
        assert_eq!(Plan::by_id(PlanId::Free).id, PlanId::Free);
        assert_eq!(Plan::by_id(PlanId::Enterprise).id, PlanId::Enterprise);
    }

    #[test]
    fn test_plan_tier_comparison() {
        assert!(PlanTier::from(PlanId::Enterprise) > PlanTier::from(PlanId::Professional));
        assert!(PlanTier::from(PlanId::Professional) > PlanTier::from(PlanId::Starter));
        assert!(PlanTier::from(PlanId::Starter) > PlanTier::from(PlanId::Free));
    }

    #[test]
    fn test_plan_id_from_str() {
        assert_eq!("free".parse::<PlanId>().unwrap(), PlanId::Free);
        assert_eq!("PROFESSIONAL".parse::<PlanId>().unwrap(), PlanId::Professional);
        assert!("invalid".parse::<PlanId>().is_err());
    }
}
