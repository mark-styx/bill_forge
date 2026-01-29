//! Comprehensive tests for billforge-core types and domain logic

use crate::*;
use uuid::Uuid;

// ============================================================================
// TenantId Tests
// ============================================================================

#[cfg(test)]
mod tenant_id_tests {
    use super::*;

    #[test]
    fn test_tenant_id_new_creates_unique_ids() {
        let id1 = TenantId::new();
        let id2 = TenantId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_tenant_id_from_uuid() {
        let uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let tenant_id = TenantId::from_uuid(uuid);
        assert_eq!(tenant_id.0, uuid);
    }

    #[test]
    fn test_tenant_id_as_str() {
        let uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let tenant_id = TenantId::from_uuid(uuid);
        assert_eq!(tenant_id.as_str(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_tenant_id_display() {
        let uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let tenant_id = TenantId::from_uuid(uuid);
        assert_eq!(format!("{}", tenant_id), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_tenant_id_from_str() {
        let tenant_id: TenantId = "550e8400-e29b-41d4-a716-446655440000".parse().unwrap();
        assert_eq!(tenant_id.as_str(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_tenant_id_from_str_invalid() {
        let result: std::result::Result<TenantId, _> = "not-a-uuid".parse();
        assert!(result.is_err());
    }

    #[test]
    fn test_tenant_id_equality() {
        let uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let id1 = TenantId::from_uuid(uuid);
        let id2 = TenantId::from_uuid(uuid);
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_tenant_id_clone() {
        let id1 = TenantId::new();
        let id2 = id1.clone();
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_tenant_id_serde_roundtrip() {
        let tenant_id = TenantId::new();
        let json = serde_json::to_string(&tenant_id).unwrap();
        let deserialized: TenantId = serde_json::from_str(&json).unwrap();
        assert_eq!(tenant_id, deserialized);
    }
}

// ============================================================================
// UserId Tests
// ============================================================================

#[cfg(test)]
mod user_id_tests {
    use super::*;

    #[test]
    fn test_user_id_new_creates_unique_ids() {
        let id1 = UserId::new();
        let id2 = UserId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_user_id_from_uuid() {
        let uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();
        let user_id = UserId::from_uuid(uuid);
        assert_eq!(user_id.0, uuid);
    }

    #[test]
    fn test_user_id_display() {
        let uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();
        let user_id = UserId::from_uuid(uuid);
        assert_eq!(format!("{}", user_id), "550e8400-e29b-41d4-a716-446655440001");
    }

    #[test]
    fn test_user_id_serde_roundtrip() {
        let user_id = UserId::new();
        let json = serde_json::to_string(&user_id).unwrap();
        let deserialized: UserId = serde_json::from_str(&json).unwrap();
        assert_eq!(user_id, deserialized);
    }
}

// ============================================================================
// Module Tests
// ============================================================================

#[cfg(test)]
mod module_tests {
    use super::*;

    #[test]
    fn test_module_as_str() {
        assert_eq!(Module::InvoiceCapture.as_str(), "invoice_capture");
        assert_eq!(Module::InvoiceProcessing.as_str(), "invoice_processing");
        assert_eq!(Module::VendorManagement.as_str(), "vendor_management");
        assert_eq!(Module::Reporting.as_str(), "reporting");
    }

    #[test]
    fn test_module_display_name() {
        assert_eq!(Module::InvoiceCapture.display_name(), "Invoice Capture");
        assert_eq!(Module::InvoiceProcessing.display_name(), "Invoice Processing");
        assert_eq!(Module::VendorManagement.display_name(), "Vendor Management");
        assert_eq!(Module::Reporting.display_name(), "Reporting & Analytics");
    }

    #[test]
    fn test_module_serde() {
        let module = Module::InvoiceCapture;
        let json = serde_json::to_string(&module).unwrap();
        assert_eq!(json, "\"invoice_capture\"");
        let deserialized: Module = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, module);
    }

    #[test]
    fn test_all_modules_deserialize() {
        let modules = ["invoice_capture", "invoice_processing", "vendor_management", "reporting"];
        for m in modules {
            let json = format!("\"{}\"", m);
            let _: Module = serde_json::from_str(&json).unwrap();
        }
    }
}

// ============================================================================
// Role Tests
// ============================================================================

#[cfg(test)]
mod role_tests {
    use super::*;

    #[test]
    fn test_role_as_str() {
        assert_eq!(Role::TenantAdmin.as_str(), "tenant_admin");
        assert_eq!(Role::ApUser.as_str(), "ap_user");
        assert_eq!(Role::Approver.as_str(), "approver");
        assert_eq!(Role::VendorManager.as_str(), "vendor_manager");
        assert_eq!(Role::ReportViewer.as_str(), "report_viewer");
        assert_eq!(Role::Custom(42).as_str(), "custom");
    }

    #[test]
    fn test_role_serde() {
        let role = Role::TenantAdmin;
        let json = serde_json::to_string(&role).unwrap();
        assert_eq!(json, "\"tenant_admin\"");
        let deserialized: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, role);
    }

    #[test]
    fn test_custom_role_equality() {
        assert_eq!(Role::Custom(1), Role::Custom(1));
        assert_ne!(Role::Custom(1), Role::Custom(2));
    }
}

// ============================================================================
// TenantContext Tests
// ============================================================================

#[cfg(test)]
mod tenant_context_tests {
    use super::*;

    fn create_test_tenant_context(modules: Vec<Module>) -> TenantContext {
        TenantContext {
            tenant_id: TenantId::new(),
            tenant_name: "Test Tenant".to_string(),
            enabled_modules: modules,
            settings: TenantSettings::default(),
        }
    }

    #[test]
    fn test_has_module_returns_true_when_enabled() {
        let context = create_test_tenant_context(vec![Module::InvoiceCapture, Module::Reporting]);
        assert!(context.has_module(Module::InvoiceCapture));
        assert!(context.has_module(Module::Reporting));
    }

    #[test]
    fn test_has_module_returns_false_when_disabled() {
        let context = create_test_tenant_context(vec![Module::InvoiceCapture]);
        assert!(!context.has_module(Module::VendorManagement));
        assert!(!context.has_module(Module::InvoiceProcessing));
    }

    #[test]
    fn test_empty_modules() {
        let context = create_test_tenant_context(vec![]);
        assert!(!context.has_module(Module::InvoiceCapture));
        assert!(!context.has_module(Module::VendorManagement));
    }
}

// ============================================================================
// UserContext Tests
// ============================================================================

#[cfg(test)]
mod user_context_tests {
    use super::*;

    fn create_test_user_context(roles: Vec<Role>) -> UserContext {
        UserContext {
            user_id: UserId::new(),
            tenant_id: TenantId::new(),
            email: "test@example.com".to_string(),
            name: "Test User".to_string(),
            roles,
        }
    }

    #[test]
    fn test_has_role_returns_true_for_assigned_role() {
        let user = create_test_user_context(vec![Role::ApUser, Role::Approver]);
        assert!(user.has_role(Role::ApUser));
        assert!(user.has_role(Role::Approver));
    }

    #[test]
    fn test_has_role_returns_false_for_unassigned_role() {
        let user = create_test_user_context(vec![Role::ApUser]);
        assert!(!user.has_role(Role::VendorManager));
    }

    #[test]
    fn test_tenant_admin_has_all_roles() {
        let user = create_test_user_context(vec![Role::TenantAdmin]);
        assert!(user.has_role(Role::TenantAdmin));
        assert!(user.has_role(Role::ApUser)); // Admin has all roles
        assert!(user.has_role(Role::Approver));
        assert!(user.has_role(Role::VendorManager));
    }

    #[test]
    fn test_is_admin() {
        let admin = create_test_user_context(vec![Role::TenantAdmin]);
        let regular = create_test_user_context(vec![Role::ApUser]);

        assert!(admin.is_admin());
        assert!(!regular.is_admin());
    }

    #[test]
    fn test_empty_roles() {
        let user = create_test_user_context(vec![]);
        assert!(!user.has_role(Role::ApUser));
        assert!(!user.is_admin());
    }
}

// ============================================================================
// Pagination Tests
// ============================================================================

#[cfg(test)]
mod pagination_tests {
    use super::*;

    #[test]
    fn test_pagination_default() {
        let pagination = Pagination::default();
        assert_eq!(pagination.page, 1);
        assert_eq!(pagination.per_page, 25);
    }

    #[test]
    fn test_pagination_offset_page_1() {
        let pagination = Pagination { page: 1, per_page: 25 };
        assert_eq!(pagination.offset(), 0);
    }

    #[test]
    fn test_pagination_offset_page_2() {
        let pagination = Pagination { page: 2, per_page: 25 };
        assert_eq!(pagination.offset(), 25);
    }

    #[test]
    fn test_pagination_offset_page_5() {
        let pagination = Pagination { page: 5, per_page: 10 };
        assert_eq!(pagination.offset(), 40);
    }

    #[test]
    fn test_pagination_offset_page_0_treated_as_page_1() {
        let pagination = Pagination { page: 0, per_page: 25 };
        assert_eq!(pagination.offset(), 0); // saturating_sub prevents underflow
    }

    #[test]
    fn test_pagination_serde() {
        let pagination = Pagination { page: 3, per_page: 50 };
        let json = serde_json::to_string(&pagination).unwrap();
        let deserialized: Pagination = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.page, 3);
        assert_eq!(deserialized.per_page, 50);
    }
}

// ============================================================================
// Money Tests
// ============================================================================

#[cfg(test)]
mod money_tests {
    use super::*;

    #[test]
    fn test_money_new() {
        let money = Money::new(12345, "USD");
        assert_eq!(money.amount, 12345);
        assert_eq!(money.currency, "USD");
    }

    #[test]
    fn test_money_usd_from_dollars() {
        let money = Money::usd(123.45);
        assert_eq!(money.amount, 12345);
        assert_eq!(money.currency, "USD");
    }

    #[test]
    fn test_money_usd_rounding() {
        let money = Money::usd(123.456);
        assert_eq!(money.amount, 12346); // Rounded up

        let money = Money::usd(123.454);
        assert_eq!(money.amount, 12345); // Rounded down
    }

    #[test]
    fn test_money_as_decimal() {
        let money = Money::new(12345, "USD");
        assert!((money.as_decimal() - 123.45).abs() < 0.001);
    }

    #[test]
    fn test_money_negative_amount() {
        let money = Money::new(-5000, "USD");
        assert_eq!(money.amount, -5000);
        assert!((money.as_decimal() - (-50.0)).abs() < 0.001);
    }

    #[test]
    fn test_money_zero() {
        let money = Money::usd(0.0);
        assert_eq!(money.amount, 0);
        assert!((money.as_decimal() - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_money_different_currencies() {
        let eur = Money::new(1000, "EUR");
        let gbp = Money::new(1000, "GBP");
        assert_eq!(eur.currency, "EUR");
        assert_eq!(gbp.currency, "GBP");
    }

    #[test]
    fn test_money_serde_roundtrip() {
        let money = Money::new(12345, "USD");
        let json = serde_json::to_string(&money).unwrap();
        let deserialized: Money = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.amount, money.amount);
        assert_eq!(deserialized.currency, money.currency);
    }
}

// ============================================================================
// TenantSettings Tests
// ============================================================================

#[cfg(test)]
mod tenant_settings_tests {
    use super::*;

    #[test]
    fn test_tenant_settings_default() {
        let settings = TenantSettings::default();
        assert!(settings.logo_url.is_none());
        assert!(settings.primary_color.is_none());
        assert_eq!(settings.timezone, "UTC");
        assert_eq!(settings.default_currency, "USD");
    }

    #[test]
    fn test_tenant_features_default() {
        let features = TenantFeatures::default();
        assert!(!features.advanced_ocr);
        assert!(!features.api_access);
        assert!(!features.custom_workflows);
        assert!(!features.audit_logs);
        assert!(!features.sso_enabled);
    }

    #[test]
    fn test_tenant_settings_serde() {
        let settings = TenantSettings {
            logo_url: Some("https://example.com/logo.png".to_string()),
            primary_color: Some("#3B82F6".to_string()),
            company_name: "Test Corp".to_string(),
            timezone: "America/New_York".to_string(),
            default_currency: "USD".to_string(),
            features: TenantFeatures {
                advanced_ocr: true,
                api_access: true,
                custom_workflows: false,
                audit_logs: true,
                sso_enabled: false,
            },
        };

        let json = serde_json::to_string(&settings).unwrap();
        let deserialized: TenantSettings = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.logo_url, settings.logo_url);
        assert_eq!(deserialized.primary_color, settings.primary_color);
        assert_eq!(deserialized.company_name, settings.company_name);
        assert_eq!(deserialized.features.advanced_ocr, true);
        assert_eq!(deserialized.features.sso_enabled, false);
    }
}

// ============================================================================
// PaginatedResponse Tests
// ============================================================================

#[cfg(test)]
mod paginated_response_tests {
    use super::*;

    #[test]
    fn test_paginated_response_structure() {
        let response: PaginatedResponse<i32> = PaginatedResponse {
            data: vec![1, 2, 3],
            pagination: PaginationMeta {
                page: 1,
                per_page: 25,
                total_items: 100,
                total_pages: 4,
            },
        };

        assert_eq!(response.data.len(), 3);
        assert_eq!(response.pagination.total_items, 100);
        assert_eq!(response.pagination.total_pages, 4);
    }

    #[test]
    fn test_paginated_response_serde() {
        let response: PaginatedResponse<String> = PaginatedResponse {
            data: vec!["item1".to_string(), "item2".to_string()],
            pagination: PaginationMeta {
                page: 2,
                per_page: 10,
                total_items: 50,
                total_pages: 5,
            },
        };

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: PaginatedResponse<String> = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.data, response.data);
        assert_eq!(deserialized.pagination.page, 2);
        assert_eq!(deserialized.pagination.total_pages, 5);
    }
}

// ============================================================================
// Error Tests
// ============================================================================

#[cfg(test)]
mod error_tests {
    use crate::error::Error;

    #[test]
    fn test_error_not_found() {
        let error = Error::NotFound {
            resource_type: "Invoice".to_string(),
            id: "123".to_string(),
        };
        let msg = format!("{}", error);
        assert!(msg.contains("Invoice"));
        assert!(msg.contains("123"));
    }

    #[test]
    fn test_error_status_codes() {
        assert_eq!(Error::Unauthenticated.status_code(), 401);
        assert_eq!(Error::InvalidCredentials.status_code(), 401);
        assert_eq!(Error::TokenExpired.status_code(), 401);
        assert_eq!(Error::Forbidden("test".to_string()).status_code(), 403);
        assert_eq!(Error::CrossTenantAccess.status_code(), 403);
        assert_eq!(Error::NotFound { resource_type: "Test".to_string(), id: "1".to_string() }.status_code(), 404);
        assert_eq!(Error::TenantNotFound("test".to_string()).status_code(), 404);
        assert_eq!(Error::FileNotFound("test".to_string()).status_code(), 404);
        assert_eq!(Error::AlreadyExists { resource_type: "Test".to_string() }.status_code(), 409);
        assert_eq!(Error::Conflict("test".to_string()).status_code(), 409);
        assert_eq!(Error::Validation("test".to_string()).status_code(), 400);
        assert_eq!(Error::InvalidInput { field: "email".to_string(), message: "invalid".to_string() }.status_code(), 400);
        assert_eq!(Error::ModuleNotAvailable("test".to_string()).status_code(), 402);
        assert_eq!(Error::RateLimited { retry_after: 60 }.status_code(), 429);
        assert_eq!(Error::Internal("test".to_string()).status_code(), 500);
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(Error::Unauthenticated.error_code(), "UNAUTHENTICATED");
        assert_eq!(Error::Forbidden("test".to_string()).error_code(), "FORBIDDEN");
        assert_eq!(Error::InvalidCredentials.error_code(), "INVALID_CREDENTIALS");
        assert_eq!(Error::NotFound { resource_type: "T".to_string(), id: "1".to_string() }.error_code(), "NOT_FOUND");
        assert_eq!(Error::Validation("test".to_string()).error_code(), "VALIDATION_ERROR");
        assert_eq!(Error::RateLimited { retry_after: 60 }.error_code(), "RATE_LIMITED");
    }

    #[test]
    fn test_error_display_messages() {
        let error = Error::InvalidInput {
            field: "email".to_string(),
            message: "Invalid email format".to_string(),
        };
        let msg = format!("{}", error);
        assert!(msg.contains("email"));
        assert!(msg.contains("Invalid email format"));

        let error = Error::InvalidStateTransition {
            from: "pending".to_string(),
            to: "approved".to_string(),
        };
        let msg = format!("{}", error);
        assert!(msg.contains("pending"));
        assert!(msg.contains("approved"));
    }

    #[test]
    fn test_rate_limited_error() {
        let error = Error::RateLimited { retry_after: 120 };
        let msg = format!("{}", error);
        assert!(msg.contains("120"));
    }

    #[test]
    fn test_external_service_error() {
        let error = Error::ExternalService {
            service: "OCR Provider".to_string(),
            message: "Connection timeout".to_string(),
        };
        let msg = format!("{}", error);
        assert!(msg.contains("OCR Provider"));
        assert!(msg.contains("Connection timeout"));
    }
}
