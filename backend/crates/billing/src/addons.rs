//! A-la-carte module add-on catalog, pricing quotes, and effective feature resolution.
//!
//! Each `Module` variant that is NOT already bundled in a subscriber's base plan can be
//! purchased individually. This module provides the pure pricing/entitlement core that
//! downstream integrations (API, UI, Stripe) will consume.

use billforge_core::Module;
use crate::plans::{Plan, PlanFeatures, PlanId};

/// A purchasable add-on module with its own pricing independent of base plans.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModuleAddOn {
    pub module: Module,
    pub name: String,
    pub description: String,
    pub monthly_price_cents: u64,
    pub annual_price_cents: u64,
    pub stripe_monthly_price_id: Option<String>,
    pub stripe_annual_price_id: Option<String>,
}

impl ModuleAddOn {
    pub fn invoice_capture() -> Self {
        Self {
            module: Module::InvoiceCapture,
            name: "Invoice Capture".to_string(),
            description: "OCR-based invoice scanning and data extraction".to_string(),
            monthly_price_cents: 1900,  // $19/mo
            annual_price_cents: 18200,  // $182/yr
            stripe_monthly_price_id: None,
            stripe_annual_price_id: None,
        }
    }

    pub fn invoice_processing() -> Self {
        Self {
            module: Module::InvoiceProcessing,
            name: "Invoice Processing".to_string(),
            description: "Automated invoice coding, approval routing, and GL posting".to_string(),
            monthly_price_cents: 3900,  // $39/mo
            annual_price_cents: 37400,  // $374/yr
            stripe_monthly_price_id: None,
            stripe_annual_price_id: None,
        }
    }

    pub fn vendor_management() -> Self {
        Self {
            module: Module::VendorManagement,
            name: "Vendor Management".to_string(),
            description: "Vendor portal, 1099 tracking, and compliance management".to_string(),
            monthly_price_cents: 2900,  // $29/mo
            annual_price_cents: 27800,  // $278/yr
            stripe_monthly_price_id: None,
            stripe_annual_price_id: None,
        }
    }

    pub fn reporting() -> Self {
        Self {
            module: Module::Reporting,
            name: "Reporting & Analytics".to_string(),
            description: "Dashboards, spend analytics, and custom report builder".to_string(),
            monthly_price_cents: 2500,  // $25/mo
            annual_price_cents: 24000,  // $240/yr
            stripe_monthly_price_id: None,
            stripe_annual_price_id: None,
        }
    }

    /// Returns the full add-on catalog (one entry per `Module` variant).
    pub fn catalog() -> Vec<Self> {
        vec![
            Self::invoice_capture(),
            Self::invoice_processing(),
            Self::vendor_management(),
            Self::reporting(),
        ]
    }

    /// Look up a catalog add-on by its module variant.
    pub fn for_module(m: Module) -> Self {
        match m {
            Module::InvoiceCapture => Self::invoice_capture(),
            Module::InvoiceProcessing => Self::invoice_processing(),
            Module::VendorManagement => Self::vendor_management(),
            Module::Reporting => Self::reporting(),
        }
    }
}

/// A pricing quote that breaks down base plan cost + add-on module costs.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SubscriptionQuote {
    pub base_plan: PlanId,
    pub base_monthly_cents: u64,
    pub base_annual_cents: u64,
    pub addon_modules: Vec<Module>,
    pub addon_monthly_cents: u64,
    pub addon_annual_cents: u64,
    pub total_monthly_cents: u64,
    pub total_annual_cents: u64,
}

/// Calculate a pricing quote for a base plan plus requested add-on modules.
///
/// Modules already bundled in the base plan are silently skipped so the
/// subscriber is never double-charged.
pub fn quote_subscription(plan_id: PlanId, requested_addons: &[Module]) -> SubscriptionQuote {
    let plan = Plan::by_id(plan_id);
    let bundled: Vec<Module> = plan.features.modules.clone();

    // Filter out modules already included in the base plan.
    let chargeable: Vec<Module> = requested_addons
        .iter()
        .copied()
        .filter(|m| !bundled.contains(m))
        .collect();

    let addon_monthly_cents: u64 = chargeable
        .iter()
        .map(|m| ModuleAddOn::for_module(*m).monthly_price_cents)
        .sum();

    let addon_annual_cents: u64 = chargeable
        .iter()
        .map(|m| ModuleAddOn::for_module(*m).annual_price_cents)
        .sum();

    SubscriptionQuote {
        base_plan: plan_id,
        base_monthly_cents: plan.monthly_price_cents,
        base_annual_cents: plan.annual_price_cents,
        addon_modules: chargeable,
        addon_monthly_cents,
        addon_annual_cents,
        total_monthly_cents: plan.monthly_price_cents + addon_monthly_cents,
        total_annual_cents: plan.annual_price_cents + addon_annual_cents,
    }
}

/// Resolve the effective feature set when add-on modules are added to a base plan.
///
/// Returns a copy of the base plan's `PlanFeatures` with the `modules` field extended
/// to the union of base modules and add-on modules (deduplicated).
pub fn effective_features(plan_id: PlanId, addon_modules: &[Module]) -> PlanFeatures {
    let plan = Plan::by_id(plan_id);
    let mut features = plan.features;

    let mut union: Vec<Module> = features.modules.clone();
    for m in addon_modules.iter().copied() {
        if !union.contains(&m) {
            union.push(m);
        }
    }
    features.modules = union;
    features
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quote_free_plus_reporting_addon() {
        // Free plan ($0) + Reporting addon ($25) = $25/mo total.
        let quote = quote_subscription(PlanId::Free, &[Module::Reporting]);
        assert_eq!(quote.base_monthly_cents, 0);
        assert_eq!(quote.addon_monthly_cents, 2500);
        assert_eq!(quote.total_monthly_cents, 2500);
        assert_eq!(quote.addon_modules, vec![Module::Reporting]);

        // Effective features should contain both InvoiceCapture (bundled) and Reporting (addon).
        let features = effective_features(PlanId::Free, &[Module::Reporting]);
        assert!(features.modules.contains(&Module::InvoiceCapture));
        assert!(features.modules.contains(&Module::Reporting));
    }

    #[test]
    fn test_quote_ignores_already_bundled_addon() {
        // Professional plan already bundles Reporting -- requesting it should not charge.
        let quote = quote_subscription(PlanId::Professional, &[Module::Reporting]);
        assert_eq!(quote.addon_monthly_cents, 0);
        assert_eq!(quote.addon_annual_cents, 0);
        assert!(quote.addon_modules.is_empty());
        // Total should equal base plan price alone.
        assert_eq!(quote.total_monthly_cents, quote.base_monthly_cents);
    }

    #[test]
    fn test_quote_starter_plus_two_addons() {
        // Starter ($49) + InvoiceProcessing ($39) + Reporting ($25) = $113/mo.
        let quote = quote_subscription(
            PlanId::Starter,
            &[Module::InvoiceProcessing, Module::Reporting],
        );
        assert_eq!(quote.base_monthly_cents, 4900);
        assert_eq!(quote.addon_monthly_cents, 6400); // 3900 + 2500
        assert_eq!(quote.total_monthly_cents, 11300);
        assert_eq!(quote.addon_modules.len(), 2);
    }

    #[test]
    fn test_effective_features_unions_modules() {
        // Starter base modules: InvoiceCapture + VendorManagement.
        // Adding Reporting yields 3 modules total.
        let features = effective_features(PlanId::Starter, &[Module::Reporting]);
        assert_eq!(features.modules.len(), 3);
        assert!(features.modules.contains(&Module::InvoiceCapture));
        assert!(features.modules.contains(&Module::VendorManagement));
        assert!(features.modules.contains(&Module::Reporting));
    }

    #[test]
    fn test_effective_features_dedupe() {
        // Requesting an already-bundled module should not duplicate it.
        let features = effective_features(PlanId::Starter, &[Module::InvoiceCapture]);
        let count = features
            .modules
            .iter()
            .filter(|m| **m == Module::InvoiceCapture)
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_catalog_covers_all_modules() {
        let catalog = ModuleAddOn::catalog();
        assert_eq!(catalog.len(), 4);
        assert!(catalog.iter().any(|a| a.module == Module::InvoiceCapture));
        assert!(catalog.iter().any(|a| a.module == Module::InvoiceProcessing));
        assert!(catalog.iter().any(|a| a.module == Module::VendorManagement));
        assert!(catalog.iter().any(|a| a.module == Module::Reporting));
    }
}
