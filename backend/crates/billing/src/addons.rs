//! A-la-carte module add-on catalog, pricing quotes, and effective feature resolution.
//!
//! Each `Module` variant that is NOT already bundled in a subscriber's base plan can be
//! purchased individually. This module provides the pure pricing/entitlement core that
//! downstream integrations (API, UI, Stripe) will consume.

use crate::plans::{Plan, PlanFeatures, PlanId};
use crate::subscription::BillingCycle;
use billforge_core::Module;

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
            monthly_price_cents: 1900, // $19/mo
            annual_price_cents: 18200, // $182/yr
            stripe_monthly_price_id: Some("price_invoice_capture_monthly".to_string()),
            stripe_annual_price_id: Some("price_invoice_capture_annual".to_string()),
        }
    }

    pub fn invoice_processing() -> Self {
        Self {
            module: Module::InvoiceProcessing,
            name: "Invoice Processing".to_string(),
            description: "Automated invoice coding, approval routing, and GL posting".to_string(),
            monthly_price_cents: 3900, // $39/mo
            annual_price_cents: 37400, // $374/yr
            stripe_monthly_price_id: Some("price_invoice_processing_monthly".to_string()),
            stripe_annual_price_id: Some("price_invoice_processing_annual".to_string()),
        }
    }

    pub fn vendor_management() -> Self {
        Self {
            module: Module::VendorManagement,
            name: "Vendor Management".to_string(),
            description: "Vendor portal, 1099 tracking, and compliance management".to_string(),
            monthly_price_cents: 2900, // $29/mo
            annual_price_cents: 27800, // $278/yr
            stripe_monthly_price_id: Some("price_vendor_management_monthly".to_string()),
            stripe_annual_price_id: Some("price_vendor_management_annual".to_string()),
        }
    }

    pub fn reporting() -> Self {
        Self {
            module: Module::Reporting,
            name: "Reporting & Analytics".to_string(),
            description: "Dashboards, spend analytics, and custom report builder".to_string(),
            monthly_price_cents: 2500, // $25/mo
            annual_price_cents: 24000, // $240/yr
            stripe_monthly_price_id: Some("price_reporting_monthly".to_string()),
            stripe_annual_price_id: Some("price_reporting_annual".to_string()),
        }
    }

    pub fn ai_assistant() -> Self {
        Self {
            module: Module::AiAssistant,
            name: "Winston AI Assistant".to_string(),
            description: "Paid conversational AI assistant add-on powered by Winston".to_string(),
            monthly_price_cents: 29900, // $299/mo fixed monthly
            annual_price_cents: 358800, // $299/mo * 12 = $3,588/yr fixed
            stripe_monthly_price_id: Some("price_ai_assistant_monthly".to_string()),
            stripe_annual_price_id: Some("price_ai_assistant_annual".to_string()),
        }
    }

    // -----------------------------------------------------------------------
    // Integration add-on constructors
    // -----------------------------------------------------------------------

    pub fn quickbooks() -> Self {
        Self {
            module: Module::Quickbooks,
            name: "QuickBooks Online".to_string(),
            description: "Two-way sync with QuickBooks Online for bills, vendors, and payments"
                .to_string(),
            monthly_price_cents: 4900,
            annual_price_cents: 47000,
            stripe_monthly_price_id: Some("price_integration_quickbooks_monthly".to_string()),
            stripe_annual_price_id: Some("price_integration_quickbooks_annual".to_string()),
        }
    }

    pub fn xero() -> Self {
        Self {
            module: Module::Xero,
            name: "Xero".to_string(),
            description: "Two-way sync with Xero for invoices, contacts, and bank transactions"
                .to_string(),
            monthly_price_cents: 4900,
            annual_price_cents: 47000,
            stripe_monthly_price_id: Some("price_integration_xero_monthly".to_string()),
            stripe_annual_price_id: Some("price_integration_xero_annual".to_string()),
        }
    }

    pub fn net_suite() -> Self {
        Self {
            module: Module::NetSuite,
            name: "NetSuite".to_string(),
            description:
                "Enterprise integration with Oracle NetSuite for vendor bills and payments"
                    .to_string(),
            monthly_price_cents: 9900,
            annual_price_cents: 95000,
            stripe_monthly_price_id: Some("price_integration_net_suite_monthly".to_string()),
            stripe_annual_price_id: Some("price_integration_net_suite_annual".to_string()),
        }
    }

    pub fn sage_intacct() -> Self {
        Self {
            module: Module::SageIntacct,
            name: "Sage Intacct".to_string(),
            description: "Integration with Sage Intacct for AP automation and GL posting"
                .to_string(),
            monthly_price_cents: 7900,
            annual_price_cents: 76000,
            stripe_monthly_price_id: Some("price_integration_sage_intacct_monthly".to_string()),
            stripe_annual_price_id: Some("price_integration_sage_intacct_annual".to_string()),
        }
    }

    pub fn salesforce() -> Self {
        Self {
            module: Module::Salesforce,
            name: "Salesforce".to_string(),
            description: "Sync vendors and purchase orders with Salesforce".to_string(),
            monthly_price_cents: 5900,
            annual_price_cents: 57000,
            stripe_monthly_price_id: Some("price_integration_salesforce_monthly".to_string()),
            stripe_annual_price_id: Some("price_integration_salesforce_annual".to_string()),
        }
    }

    pub fn workday() -> Self {
        Self {
            module: Module::Workday,
            name: "Workday".to_string(),
            description: "Integration with Workday Financial Management for expenses and payments"
                .to_string(),
            monthly_price_cents: 7900,
            annual_price_cents: 76000,
            stripe_monthly_price_id: Some("price_integration_workday_monthly".to_string()),
            stripe_annual_price_id: Some("price_integration_workday_annual".to_string()),
        }
    }

    pub fn bill_com() -> Self {
        Self {
            module: Module::BillCom,
            name: "Bill.com".to_string(),
            description: "Sync invoices and payments with Bill.com for streamlined AP workflows"
                .to_string(),
            monthly_price_cents: 4900,
            annual_price_cents: 47000,
            stripe_monthly_price_id: Some("price_integration_bill_com_monthly".to_string()),
            stripe_annual_price_id: Some("price_integration_bill_com_annual".to_string()),
        }
    }

    pub fn edi_addon() -> Self {
        Self {
            module: Module::Edi,
            name: "EDI".to_string(),
            description: "EDI (ANSI X12) integration for automated vendor invoice exchange"
                .to_string(),
            monthly_price_cents: 6900,
            annual_price_cents: 66000,
            stripe_monthly_price_id: Some("price_integration_edi_monthly".to_string()),
            stripe_annual_price_id: Some("price_integration_edi_annual".to_string()),
        }
    }

    /// Returns the full add-on catalog (one entry per `Module` variant).
    pub fn catalog() -> Vec<Self> {
        vec![
            Self::invoice_capture(),
            Self::invoice_processing(),
            Self::vendor_management(),
            Self::reporting(),
            Self::ai_assistant(),
            // Integration add-ons
            Self::quickbooks(),
            Self::xero(),
            Self::net_suite(),
            Self::sage_intacct(),
            Self::salesforce(),
            Self::workday(),
            Self::bill_com(),
            Self::edi_addon(),
        ]
    }

    /// Look up a catalog add-on by its module variant.
    pub fn for_module(m: Module) -> Self {
        match m {
            Module::InvoiceCapture => Self::invoice_capture(),
            Module::InvoiceProcessing => Self::invoice_processing(),
            Module::VendorManagement => Self::vendor_management(),
            Module::Reporting => Self::reporting(),
            Module::AiAssistant => Self::ai_assistant(),
            Module::Quickbooks => Self::quickbooks(),
            Module::Xero => Self::xero(),
            Module::NetSuite => Self::net_suite(),
            Module::SageIntacct => Self::sage_intacct(),
            Module::Salesforce => Self::salesforce(),
            Module::Workday => Self::workday(),
            Module::BillCom => Self::bill_com(),
            Module::Edi => Self::edi_addon(),
        }
    }

    /// Resolve the Stripe price ID for this add-on given a billing cycle.
    pub fn stripe_price_id(&self, cycle: BillingCycle) -> Option<&str> {
        match cycle {
            BillingCycle::Monthly => self.stripe_monthly_price_id.as_deref(),
            BillingCycle::Annual => self.stripe_annual_price_id.as_deref(),
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
        assert_eq!(catalog.len(), 13);
        assert!(catalog.iter().any(|a| a.module == Module::InvoiceCapture));
        assert!(catalog
            .iter()
            .any(|a| a.module == Module::InvoiceProcessing));
        assert!(catalog.iter().any(|a| a.module == Module::VendorManagement));
        assert!(catalog.iter().any(|a| a.module == Module::Reporting));
        assert!(catalog.iter().any(|a| a.module == Module::AiAssistant));
        // Integration add-ons
        assert!(catalog.iter().any(|a| a.module == Module::Quickbooks));
        assert!(catalog.iter().any(|a| a.module == Module::Xero));
        assert!(catalog.iter().any(|a| a.module == Module::NetSuite));
        assert!(catalog.iter().any(|a| a.module == Module::SageIntacct));
        assert!(catalog.iter().any(|a| a.module == Module::Salesforce));
        assert!(catalog.iter().any(|a| a.module == Module::Workday));
        assert!(catalog.iter().any(|a| a.module == Module::BillCom));
        assert!(catalog.iter().any(|a| a.module == Module::Edi));
    }

    // ========================================================================
    // Winston AI Assistant add-on catalog metadata tests
    // ========================================================================

    #[test]
    fn test_winston_addon_catalog_metadata() {
        let addon = ModuleAddOn::ai_assistant();
        assert_eq!(addon.module, Module::AiAssistant);
        assert_eq!(addon.name, "Winston AI Assistant");
        assert_eq!(addon.monthly_price_cents, 29900);
        assert_eq!(addon.annual_price_cents, 358800); // $299 * 12, no annual discount
    }

    #[test]
    fn test_winston_addon_chargeable_on_all_mvp_plans() {
        // Winston is not bundled in any base plan, so quoting it on every MVP
        // plan should result in the full add-on price being charged.
        let expected_cents = 29900u64;
        for plan_id in [
            PlanId::Free,
            PlanId::Starter,
            PlanId::Professional,
            PlanId::Enterprise,
        ] {
            let quote = quote_subscription(plan_id, &[Module::AiAssistant]);
            assert_eq!(
                quote.addon_monthly_cents, expected_cents,
                "Winston should be chargeable on {:?}",
                plan_id,
            );
            assert!(quote.addon_modules.contains(&Module::AiAssistant));
        }
    }

    // ========================================================================
    // Default-containment: no MVP plan bundles AiAssistant
    // ========================================================================

    #[test]
    fn test_no_mvp_plan_bundles_ai_assistant() {
        for plan in [
            Plan::free(),
            Plan::starter(),
            Plan::professional(),
            Plan::enterprise(),
        ] {
            assert!(
                !plan.features.modules.contains(&Module::AiAssistant),
                "{:?} plan must not bundle Module::AiAssistant",
                plan.id,
            );
        }
    }

    // ========================================================================
    // Stripe price ID coverage
    // ========================================================================

    #[test]
    fn test_addon_catalog_has_stripe_price_ids() {
        for addon in ModuleAddOn::catalog() {
            assert!(
                addon.stripe_monthly_price_id.is_some(),
                "Add-on {:?} missing monthly Stripe price ID",
                addon.module,
            );
            assert!(
                addon.stripe_annual_price_id.is_some(),
                "Add-on {:?} missing annual Stripe price ID",
                addon.module,
            );
        }
    }

    #[test]
    fn test_stripe_price_id_resolves_by_cycle() {
        let addon = ModuleAddOn::reporting();
        assert_eq!(
            addon.stripe_price_id(BillingCycle::Monthly),
            Some("price_reporting_monthly")
        );
        assert_eq!(
            addon.stripe_price_id(BillingCycle::Annual),
            Some("price_reporting_annual")
        );
    }

    // ========================================================================
    // Integration add-on catalog & for_module tests
    // ========================================================================

    #[test]
    fn test_integration_catalog_has_non_empty_pricing() {
        let catalog = ModuleAddOn::catalog();
        let integration_modules = [
            Module::Quickbooks,
            Module::Xero,
            Module::NetSuite,
            Module::SageIntacct,
            Module::Salesforce,
            Module::Workday,
            Module::BillCom,
            Module::Edi,
        ];
        for m in &integration_modules {
            let addon = catalog
                .iter()
                .find(|a| a.module == *m)
                .unwrap_or_else(|| panic!("Integration module {:?} missing from catalog", m));
            assert!(
                addon.monthly_price_cents > 0,
                "Integration {:?} must have non-zero monthly pricing",
                m,
            );
            assert!(
                addon.annual_price_cents > 0,
                "Integration {:?} must have non-zero annual pricing",
                m,
            );
            assert!(
                !addon.name.is_empty(),
                "Integration {:?} must have a non-empty display name",
                m,
            );
            assert!(
                !addon.description.is_empty(),
                "Integration {:?} must have a non-empty description",
                m,
            );
        }
    }

    #[test]
    fn test_for_module_returns_matching_sku_for_integrations() {
        for m in [
            Module::Quickbooks,
            Module::Xero,
            Module::NetSuite,
            Module::SageIntacct,
            Module::Salesforce,
            Module::Workday,
            Module::BillCom,
            Module::Edi,
        ] {
            let addon = ModuleAddOn::for_module(m);
            assert_eq!(addon.module, m);
            assert!(addon.monthly_price_cents > 0);
            assert!(addon.stripe_monthly_price_id.is_some());
        }
    }

    #[test]
    fn test_module_from_str_round_trips_integrations() {
        let cases = [
            (Module::Quickbooks, "quickbooks"),
            (Module::Xero, "xero"),
            (Module::NetSuite, "net_suite"),
            (Module::SageIntacct, "sage_intacct"),
            (Module::Salesforce, "salesforce"),
            (Module::Workday, "workday"),
            (Module::BillCom, "bill_com"),
            (Module::Edi, "edi"),
        ];
        for (module, slug) in &cases {
            // FromStr
            let parsed: Module = slug
                .parse()
                .unwrap_or_else(|e| panic!("Failed to parse {:?} as Module: {}", slug, e));
            assert_eq!(parsed, *module, "FromStr round-trip failed for {:?}", slug);
            // as_str
            assert_eq!(module.as_str(), *slug, "as_str mismatch for {:?}", module);
        }
    }
}
