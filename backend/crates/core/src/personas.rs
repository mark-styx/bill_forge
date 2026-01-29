//! Sandbox personas for development and testing
//!
//! Each persona represents a different product configuration that a customer might subscribe to.

use crate::types::{Module, Role};
use serde::{Deserialize, Serialize};

/// Predefined sandbox personas with different module configurations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxPersona {
    /// Full platform access - all modules enabled
    FullPlatform,
    /// Invoice OCR pipeline only - capture invoices from documents
    InvoiceOcrOnly,
    /// Invoice processing only - manual entry, workflows, approvals (no OCR)
    InvoiceProcessingOnly,
    /// Vendor management only - manage vendors, contacts, tax docs
    VendorManagementOnly,
    /// AP Lite - Invoice processing + Vendor management (no OCR)
    ApLite,
}

impl SandboxPersona {
    /// Get all available personas
    pub fn all() -> Vec<Self> {
        vec![
            Self::FullPlatform,
            Self::InvoiceOcrOnly,
            Self::InvoiceProcessingOnly,
            Self::VendorManagementOnly,
            Self::ApLite,
        ]
    }

    /// Get the persona by its ID string
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "full_platform" => Some(Self::FullPlatform),
            "invoice_ocr_only" => Some(Self::InvoiceOcrOnly),
            "invoice_processing_only" => Some(Self::InvoiceProcessingOnly),
            "vendor_management_only" => Some(Self::VendorManagementOnly),
            "ap_lite" => Some(Self::ApLite),
            _ => None,
        }
    }

    /// Get the persona ID string
    pub fn id(&self) -> &'static str {
        match self {
            Self::FullPlatform => "full_platform",
            Self::InvoiceOcrOnly => "invoice_ocr_only",
            Self::InvoiceProcessingOnly => "invoice_processing_only",
            Self::VendorManagementOnly => "vendor_management_only",
            Self::ApLite => "ap_lite",
        }
    }

    /// Get the display name for the persona
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::FullPlatform => "Full Platform",
            Self::InvoiceOcrOnly => "Invoice Management (OCR)",
            Self::InvoiceProcessingOnly => "Invoice Management",
            Self::VendorManagementOnly => "Vendor Management",
            Self::ApLite => "AP Lite",
        }
    }

    /// Get a description of the persona
    pub fn description(&self) -> &'static str {
        match self {
            Self::FullPlatform => "Complete accounts payable solution with OCR capture, invoice processing, vendor management, and full reporting.",
            Self::InvoiceOcrOnly => "Automated invoice capture from PDF/images with OCR extraction. Upload documents and review extracted data.",
            Self::InvoiceProcessingOnly => "Invoice workflow management with approvals, queues, and payment tracking. Manual invoice entry only.",
            Self::VendorManagementOnly => "Complete vendor database with contacts, tax documents, and communication history.",
            Self::ApLite => "Invoice processing and vendor management without OCR. Great for teams with manual invoice entry.",
        }
    }

    /// Get the modules enabled for this persona
    /// Note: Reporting is always enabled but scoped to available modules
    pub fn enabled_modules(&self) -> Vec<Module> {
        let mut modules = match self {
            Self::FullPlatform => vec![
                Module::InvoiceCapture,
                Module::InvoiceProcessing,
                Module::VendorManagement,
            ],
            Self::InvoiceOcrOnly => vec![
                Module::InvoiceCapture,
            ],
            Self::InvoiceProcessingOnly => vec![
                Module::InvoiceProcessing,
            ],
            Self::VendorManagementOnly => vec![
                Module::VendorManagement,
            ],
            Self::ApLite => vec![
                Module::InvoiceProcessing,
                Module::VendorManagement,
            ],
        };
        // Reporting is always available
        modules.push(Module::Reporting);
        modules
    }

    /// Get the available roles for this persona
    /// Roles are scoped based on enabled modules
    pub fn available_roles(&self) -> Vec<Role> {
        let mut roles = vec![Role::TenantAdmin, Role::ReportViewer];
        
        let modules = self.enabled_modules();
        
        if modules.contains(&Module::InvoiceCapture) || modules.contains(&Module::InvoiceProcessing) {
            roles.push(Role::ApUser);
        }
        
        if modules.contains(&Module::InvoiceProcessing) {
            roles.push(Role::Approver);
        }
        
        if modules.contains(&Module::VendorManagement) {
            roles.push(Role::VendorManager);
        }
        
        roles
    }

    /// Get reporting sections available for this persona
    pub fn reporting_sections(&self) -> Vec<&'static str> {
        let modules = self.enabled_modules();
        let mut sections = vec!["dashboard_summary"];
        
        if modules.contains(&Module::InvoiceCapture) {
            sections.push("ocr_metrics");
            sections.push("capture_pipeline");
        }
        
        if modules.contains(&Module::InvoiceProcessing) {
            sections.push("invoice_aging");
            sections.push("processing_metrics");
            sections.push("approval_metrics");
            sections.push("payment_analytics");
        }
        
        if modules.contains(&Module::VendorManagement) {
            sections.push("vendor_analytics");
            sections.push("vendor_spend");
        }
        
        sections
    }

    /// Check if a module is enabled for this persona
    pub fn has_module(&self, module: Module) -> bool {
        self.enabled_modules().contains(&module)
    }

    /// Check if a role is available for this persona
    pub fn has_role(&self, role: Role) -> bool {
        self.available_roles().contains(&role)
    }
}

/// Detailed persona information for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub modules: Vec<ModuleInfo>,
    pub roles: Vec<RoleInfo>,
    pub reporting_sections: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleInfo {
    pub id: String,
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleInfo {
    pub id: String,
    pub name: String,
    pub available: bool,
}

impl From<SandboxPersona> for PersonaInfo {
    fn from(persona: SandboxPersona) -> Self {
        let enabled_modules = persona.enabled_modules();
        let available_roles = persona.available_roles();
        
        Self {
            id: persona.id().to_string(),
            name: persona.display_name().to_string(),
            description: persona.description().to_string(),
            modules: vec![
                ModuleInfo {
                    id: "invoice_capture".to_string(),
                    name: "Invoice Capture (OCR)".to_string(),
                    enabled: enabled_modules.contains(&Module::InvoiceCapture),
                },
                ModuleInfo {
                    id: "invoice_processing".to_string(),
                    name: "Invoice Processing".to_string(),
                    enabled: enabled_modules.contains(&Module::InvoiceProcessing),
                },
                ModuleInfo {
                    id: "vendor_management".to_string(),
                    name: "Vendor Management".to_string(),
                    enabled: enabled_modules.contains(&Module::VendorManagement),
                },
                ModuleInfo {
                    id: "reporting".to_string(),
                    name: "Reporting & Analytics".to_string(),
                    enabled: true, // Always enabled
                },
            ],
            roles: vec![
                RoleInfo {
                    id: "tenant_admin".to_string(),
                    name: "Administrator".to_string(),
                    available: true,
                },
                RoleInfo {
                    id: "ap_user".to_string(),
                    name: "AP User".to_string(),
                    available: available_roles.contains(&Role::ApUser),
                },
                RoleInfo {
                    id: "approver".to_string(),
                    name: "Approver".to_string(),
                    available: available_roles.contains(&Role::Approver),
                },
                RoleInfo {
                    id: "vendor_manager".to_string(),
                    name: "Vendor Manager".to_string(),
                    available: available_roles.contains(&Role::VendorManager),
                },
                RoleInfo {
                    id: "report_viewer".to_string(),
                    name: "Report Viewer".to_string(),
                    available: true,
                },
            ],
            reporting_sections: persona.reporting_sections().iter().map(|s| s.to_string()).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_platform_has_all_modules() {
        let persona = SandboxPersona::FullPlatform;
        assert!(persona.has_module(Module::InvoiceCapture));
        assert!(persona.has_module(Module::InvoiceProcessing));
        assert!(persona.has_module(Module::VendorManagement));
        assert!(persona.has_module(Module::Reporting));
    }

    #[test]
    fn test_ocr_only_persona() {
        let persona = SandboxPersona::InvoiceOcrOnly;
        assert!(persona.has_module(Module::InvoiceCapture));
        assert!(!persona.has_module(Module::InvoiceProcessing));
        assert!(!persona.has_module(Module::VendorManagement));
        assert!(persona.has_module(Module::Reporting)); // Always enabled
    }

    #[test]
    fn test_roles_scoped_to_modules() {
        let vendor_only = SandboxPersona::VendorManagementOnly;
        assert!(vendor_only.has_role(Role::VendorManager));
        assert!(!vendor_only.has_role(Role::ApUser));
        assert!(!vendor_only.has_role(Role::Approver));

        let processing_only = SandboxPersona::InvoiceProcessingOnly;
        assert!(processing_only.has_role(Role::ApUser));
        assert!(processing_only.has_role(Role::Approver));
        assert!(!processing_only.has_role(Role::VendorManager));
    }
}
