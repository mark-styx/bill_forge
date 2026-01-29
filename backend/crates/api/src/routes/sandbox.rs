//! Sandbox/Development persona management routes
//!
//! These endpoints are only available in sandbox mode for switching between
//! different product configurations to test various subscription scenarios.

use crate::error::{ApiError, ApiResult};
use crate::extractors::{AuthUser, TenantCtx};
use crate::state::AppState;
use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use billforge_core::{PersonaInfo, SandboxPersona, Module};
use serde::{Deserialize, Serialize};

pub fn routes() -> Router<AppState> {
    Router::new()
        // List all available personas
        .route("/personas", get(list_personas))
        // Get current persona for tenant
        .route("/personas/current", get(get_current_persona))
        // Switch to a different persona
        .route("/personas/switch", post(switch_persona))
        // Get tenant context with full module info
        .route("/context", get(get_tenant_context))
}

/// List all available sandbox personas
async fn list_personas(
    State(_state): State<AppState>,
) -> ApiResult<Json<Vec<PersonaInfo>>> {
    let personas: Vec<PersonaInfo> = SandboxPersona::all()
        .into_iter()
        .map(|p| p.into())
        .collect();
    
    Ok(Json(personas))
}

/// Get the current persona for the authenticated tenant
async fn get_current_persona(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<Json<CurrentPersonaResponse>> {
    // Determine persona from enabled modules
    let persona = determine_persona_from_modules(&tenant.enabled_modules);
    
    Ok(Json(CurrentPersonaResponse {
        persona: persona.into(),
        tenant_id: tenant.tenant_id.to_string(),
        tenant_name: tenant.tenant_name,
    }))
}

#[derive(Debug, Serialize)]
pub struct CurrentPersonaResponse {
    pub persona: PersonaInfo,
    pub tenant_id: String,
    pub tenant_name: String,
}

/// Determine which persona best matches the enabled modules
fn determine_persona_from_modules(modules: &[Module]) -> SandboxPersona {
    let has_capture = modules.contains(&Module::InvoiceCapture);
    let has_processing = modules.contains(&Module::InvoiceProcessing);
    let has_vendor = modules.contains(&Module::VendorManagement);
    
    match (has_capture, has_processing, has_vendor) {
        (true, true, true) => SandboxPersona::FullPlatform,
        (true, false, false) => SandboxPersona::InvoiceOcrOnly,
        (false, true, false) => SandboxPersona::InvoiceProcessingOnly,
        (false, false, true) => SandboxPersona::VendorManagementOnly,
        (false, true, true) => SandboxPersona::ApLite,
        // Default to full platform if unclear
        _ => SandboxPersona::FullPlatform,
    }
}

#[derive(Debug, Deserialize)]
pub struct SwitchPersonaRequest {
    pub persona_id: String,
}

#[derive(Debug, Serialize)]
pub struct SwitchPersonaResponse {
    pub success: bool,
    pub persona: PersonaInfo,
    pub message: String,
}

/// Switch the tenant to a different persona (updates enabled modules)
async fn switch_persona(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    TenantCtx(tenant): TenantCtx,
    Json(req): Json<SwitchPersonaRequest>,
) -> ApiResult<Json<SwitchPersonaResponse>> {
    // Only admins can switch personas
    if !user.is_admin() {
        return Err(ApiError(billforge_core::Error::Forbidden(
            "Only administrators can switch personas".to_string(),
        )));
    }
    
    // Parse persona
    let persona = SandboxPersona::from_id(&req.persona_id)
        .ok_or_else(|| ApiError(billforge_core::Error::Validation(
            format!("Invalid persona ID: {}", req.persona_id),
        )))?;
    
    // Update tenant modules
    let modules = persona.enabled_modules();
    state
        .db
        .metadata()
        .update_tenant_modules(&tenant.tenant_id, &modules)
        .await
        .map_err(|e| ApiError(billforge_core::Error::Database(e.to_string())))?;
    
    tracing::info!(
        tenant_id = %tenant.tenant_id,
        persona = %persona.id(),
        user_id = %user.user_id,
        "Switched tenant persona"
    );
    
    Ok(Json(SwitchPersonaResponse {
        success: true,
        persona: persona.into(),
        message: format!("Successfully switched to {} persona", persona.display_name()),
    }))
}

#[derive(Debug, Serialize)]
pub struct TenantContextResponse {
    pub tenant_id: String,
    pub tenant_name: String,
    pub persona: PersonaInfo,
    pub enabled_modules: Vec<ModuleStatus>,
    pub available_roles: Vec<RoleStatus>,
    pub reporting_sections: Vec<String>,
    pub settings: TenantSettingsResponse,
}

#[derive(Debug, Serialize)]
pub struct ModuleStatus {
    pub id: String,
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct RoleStatus {
    pub id: String,
    pub name: String,
    pub available: bool,
    pub description: String,
}

#[derive(Debug, Serialize)]
pub struct TenantSettingsResponse {
    pub logo_url: Option<String>,
    pub primary_color: Option<String>,
    pub company_name: String,
    pub timezone: String,
    pub default_currency: String,
}

/// Get full tenant context with all module and role information
async fn get_tenant_context(
    State(_state): State<AppState>,
    AuthUser(_user): AuthUser,
    TenantCtx(tenant): TenantCtx,
) -> ApiResult<Json<TenantContextResponse>> {
    let persona = determine_persona_from_modules(&tenant.enabled_modules);
    let persona_info: PersonaInfo = persona.into();
    
    let enabled_modules = vec![
        ModuleStatus {
            id: "invoice_capture".to_string(),
            name: "Invoice Capture (OCR)".to_string(),
            enabled: tenant.has_module(Module::InvoiceCapture),
        },
        ModuleStatus {
            id: "invoice_processing".to_string(),
            name: "Invoice Processing".to_string(),
            enabled: tenant.has_module(Module::InvoiceProcessing),
        },
        ModuleStatus {
            id: "vendor_management".to_string(),
            name: "Vendor Management".to_string(),
            enabled: tenant.has_module(Module::VendorManagement),
        },
        ModuleStatus {
            id: "reporting".to_string(),
            name: "Reporting & Analytics".to_string(),
            enabled: true, // Always enabled
        },
    ];
    
    let available_roles = vec![
        RoleStatus {
            id: "tenant_admin".to_string(),
            name: "Administrator".to_string(),
            available: true,
            description: "Full system access for the tenant".to_string(),
        },
        RoleStatus {
            id: "ap_user".to_string(),
            name: "AP User".to_string(),
            available: persona.has_role(billforge_core::Role::ApUser),
            description: "Process invoices through workflows".to_string(),
        },
        RoleStatus {
            id: "approver".to_string(),
            name: "Approver".to_string(),
            available: persona.has_role(billforge_core::Role::Approver),
            description: "Approve invoices based on rules".to_string(),
        },
        RoleStatus {
            id: "vendor_manager".to_string(),
            name: "Vendor Manager".to_string(),
            available: persona.has_role(billforge_core::Role::VendorManager),
            description: "Manage vendor records and documents".to_string(),
        },
        RoleStatus {
            id: "report_viewer".to_string(),
            name: "Report Viewer".to_string(),
            available: true,
            description: "Read-only access to reports".to_string(),
        },
    ];
    
    Ok(Json(TenantContextResponse {
        tenant_id: tenant.tenant_id.to_string(),
        tenant_name: tenant.tenant_name.clone(),
        persona: persona_info,
        enabled_modules,
        available_roles,
        reporting_sections: persona.reporting_sections().iter().map(|s| s.to_string()).collect(),
        settings: TenantSettingsResponse {
            logo_url: tenant.settings.logo_url.clone(),
            primary_color: tenant.settings.primary_color.clone(),
            company_name: tenant.settings.company_name.clone(),
            timezone: tenant.settings.timezone.clone(),
            default_currency: tenant.settings.default_currency.clone(),
        },
    }))
}
