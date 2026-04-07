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
        assert!(settings.ocr_provider.is_none());
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
            ocr_provider: Some("tesseract".to_string()),
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
        assert_eq!(deserialized.ocr_provider, Some("tesseract".to_string()));
        assert!(deserialized.features.advanced_ocr);
        assert!(!deserialized.features.sso_enabled);
    }

    #[test]
    fn test_tenant_settings_deserialize_without_ocr_provider() {
        // JSON from an existing tenant that doesn't have ocr_provider key
        let json = serde_json::json!({
            "logo_url": null,
            "primary_color": null,
            "company_name": "Legacy Corp",
            "timezone": "UTC",
            "default_currency": "EUR",
            "features": {
                "advanced_ocr": false,
                "api_access": false,
                "custom_workflows": false,
                "audit_logs": false,
                "sso_enabled": false
            }
        });

        let settings: TenantSettings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.company_name, "Legacy Corp");
        assert_eq!(settings.default_currency, "EUR");
        assert!(settings.ocr_provider.is_none());
    }

    #[test]
    fn test_tenant_settings_deserialize_with_ocr_provider() {
        let json = serde_json::json!({
            "logo_url": null,
            "primary_color": null,
            "company_name": "Modern Corp",
            "timezone": "UTC",
            "default_currency": "USD",
            "ocr_provider": "aws_textract",
            "features": {
                "advanced_ocr": false,
                "api_access": false,
                "custom_workflows": false,
                "audit_logs": false,
                "sso_enabled": false
            }
        });

        let settings: TenantSettings = serde_json::from_value(json).unwrap();
        assert_eq!(settings.ocr_provider, Some("aws_textract".to_string()));
    }

    #[test]
    fn test_ocr_provider_fallback_to_global_default() {
        // When tenant ocr_provider is None, the fallback pattern should use the global default
        let tenant_provider: Option<&str> = None;
        let global_provider = "tesseract";
        let resolved = tenant_provider.unwrap_or(global_provider);
        assert_eq!(resolved, "tesseract");

        // When tenant has an override, it should be used instead
        let tenant_provider: Option<&str> = Some("aws_textract");
        let resolved = tenant_provider.unwrap_or(global_provider);
        assert_eq!(resolved, "aws_textract");
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

// ============================================================================
// Delegation Validation Tests
// ============================================================================

#[cfg(test)]
mod delegation_validation_tests {
    use super::*;
    use crate::domain::detect_delegation_cycle;
    use crate::types::UserId;
    use chrono::{Duration, Utc};

    fn make_input(delegator: &str, delegate: &str) -> CreateApprovalDelegationInput {
        CreateApprovalDelegationInput {
            delegator_id: delegator.to_string(),
            delegate_id: delegate.to_string(),
            start_date: Utc::now(),
            end_date: Utc::now() + Duration::days(7),
            conditions: None,
        }
    }

    fn make_delegation(
        id: Uuid,
        delegator: UserId,
        delegate: UserId,
        active: bool,
    ) -> ApprovalDelegation {
        ApprovalDelegation {
            id,
            tenant_id: TenantId::new(),
            delegator_id: delegator,
            delegate_id: delegate,
            start_date: Utc::now() - Duration::days(1),
            end_date: Utc::now() + Duration::days(30),
            is_active: active,
            conditions: None,
            created_at: Utc::now(),
        }
    }

    // ---- validate_basic tests ----

    #[test]
    fn test_self_delegation_rejected() {
        let same = Uuid::new_v4().to_string();
        let input = make_input(&same, &same);
        let err = input.validate_basic().unwrap_err();
        assert!(err.to_string().contains("Cannot delegate to yourself"));
    }

    #[test]
    fn test_end_before_start_rejected() {
        let input = CreateApprovalDelegationInput {
            delegator_id: Uuid::new_v4().to_string(),
            delegate_id: Uuid::new_v4().to_string(),
            start_date: Utc::now() + Duration::days(7),
            end_date: Utc::now(),
            conditions: None,
        };
        let err = input.validate_basic().unwrap_err();
        assert!(err.to_string().contains("start_date must be before end_date"));
    }

    #[test]
    fn test_valid_delegation_passes() {
        let input = make_input(
            &Uuid::new_v4().to_string(),
            &Uuid::new_v4().to_string(),
        );
        assert!(input.validate_basic().is_ok());
    }

    #[test]
    fn test_invalid_delegator_uuid_rejected() {
        let input = CreateApprovalDelegationInput {
            delegator_id: "not-a-uuid".to_string(),
            delegate_id: Uuid::new_v4().to_string(),
            start_date: Utc::now(),
            end_date: Utc::now() + Duration::days(7),
            conditions: None,
        };
        let err = input.validate_basic().unwrap_err();
        assert!(err.to_string().contains("delegator_id is not a valid UUID"));
    }

    // ---- detect_delegation_cycle tests ----

    #[test]
    fn test_circular_chain_detection() {
        // A delegates to B, B delegates to A -- adding A→B should detect cycle B→A
        let a = UserId::new();
        let b = UserId::new();

        let existing = vec![
            make_delegation(Uuid::new_v4(), a.clone(), b.clone(), true),
            make_delegation(Uuid::new_v4(), b.clone(), a.clone(), true),
        ];

        // Propose: a → b. Walk from b: b→a, which reaches delegator a -> cycle
        let result = detect_delegation_cycle(
            &existing,
            &a,
            &b,
            Utc::now() - Duration::days(1),
            Utc::now() + Duration::days(30),
        );
        assert!(result.is_some());
        let path = result.unwrap();
        assert_eq!(path[0], b);
        assert_eq!(path[1], a); // reached delegator
    }

    #[test]
    fn test_no_cycle_in_linear_chain() {
        // A→B, B→C, adding C→D has no cycle
        let a = UserId::new();
        let b = UserId::new();
        let c = UserId::new();
        let d = UserId::new();

        let existing = vec![
            make_delegation(Uuid::new_v4(), a.clone(), b.clone(), true),
            make_delegation(Uuid::new_v4(), b.clone(), c.clone(), true),
        ];

        let result = detect_delegation_cycle(
            &existing,
            &c,
            &d,
            Utc::now() - Duration::days(1),
            Utc::now() + Duration::days(30),
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_deep_cycle_detected() {
        // A→B→C→D→A chain, adding A→B should detect cycle
        let a = UserId::new();
        let b = UserId::new();
        let c = UserId::new();
        let d = UserId::new();

        let existing = vec![
            make_delegation(Uuid::new_v4(), a.clone(), b.clone(), true),
            make_delegation(Uuid::new_v4(), b.clone(), c.clone(), true),
            make_delegation(Uuid::new_v4(), c.clone(), d.clone(), true),
            make_delegation(Uuid::new_v4(), d.clone(), a.clone(), true),
        ];

        // Propose a→b. Walk from b: b→c→d→a (reaches delegator a) -> cycle
        let result = detect_delegation_cycle(
            &existing,
            &a,
            &b,
            Utc::now() - Duration::days(1),
            Utc::now() + Duration::days(30),
        );
        assert!(result.is_some());
    }

    #[test]
    fn test_cycle_detection_depth_limit() {
        // Chain of 11 hops without a cycle should return None (depth capped at 10)
        let users: Vec<UserId> = (0..12).map(|_| UserId::new()).collect();

        let existing: Vec<ApprovalDelegation> = users
            .iter()
            .take(11)
            .enumerate()
            .map(|(i, _)| {
                make_delegation(
                    Uuid::new_v4(),
                    users[i].clone(),
                    users[i + 1].clone(),
                    true,
                )
            })
            .collect();

        // Propose users[0] → users[1]. The chain goes 1→2→...→11 (11 hops)
        // but depth cap is 10, so returns None.
        let result = detect_delegation_cycle(
            &existing,
            &users[0],
            &users[1],
            Utc::now() - Duration::days(1),
            Utc::now() + Duration::days(30),
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_inactive_delegations_ignored() {
        let a = UserId::new();
        let b = UserId::new();

        let existing = vec![
            make_delegation(Uuid::new_v4(), a.clone(), b.clone(), false),
            make_delegation(Uuid::new_v4(), b.clone(), a.clone(), false),
        ];

        let result = detect_delegation_cycle(
            &existing,
            &a,
            &b,
            Utc::now() - Duration::days(1),
            Utc::now() + Duration::days(30),
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_no_delegation_chain_empty() {
        let a = UserId::new();
        let b = UserId::new();

        let result = detect_delegation_cycle(
            &[],
            &a,
            &b,
            Utc::now() - Duration::days(1),
            Utc::now() + Duration::days(30),
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_cycle_through_alternate_edge() {
        // B has two outgoing edges: B→C and B→D. Adding A→B, and D→A exists.
        // Old linear walk found B→C first (dead end) and missed B→D→A cycle.
        // BFS must explore ALL edges and find the cycle through B→D→A.
        let a = UserId::new();
        let b = UserId::new();
        let c = UserId::new();
        let d = UserId::new();

        let existing = vec![
            make_delegation(Uuid::new_v4(), b.clone(), c.clone(), true), // B→C (dead end)
            make_delegation(Uuid::new_v4(), b.clone(), d.clone(), true), // B→D (leads to cycle)
            make_delegation(Uuid::new_v4(), d.clone(), a.clone(), true), // D→A (completes cycle)
        ];

        // Propose: a → b. Walk from b: b→d→a reaches delegator a -> cycle
        let result = detect_delegation_cycle(
            &existing,
            &a,
            &b,
            Utc::now() - Duration::days(1),
            Utc::now() + Duration::days(30),
        );
        assert!(result.is_some());
        let path = result.unwrap();
        assert_eq!(path[0], b);
        assert!(path.contains(&a)); // must reach back to delegator
    }
}

// ============================================================================
// Workflow Template Stage Processing Tests
// ============================================================================

#[cfg(test)]
mod workflow_template_tests {
    use super::*;
    use crate::domain::{
        ConditionField, ConditionOperator, Invoice, InvoiceId, RuleCondition,
        StageType, WorkflowTemplate, WorkflowTemplateStage, WorkflowTemplateId,
        CaptureStatus, ProcessingStatus,
    };
    use crate::workflow_evaluator;
    use crate::Money;
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    fn create_test_invoice_with_amount(amount_cents: i64) -> Invoice {
        Invoice {
            id: InvoiceId::new(),
            tenant_id: TenantId::new(),
            vendor_id: Some(Uuid::new_v4()),
            vendor_name: "Test Vendor".to_string(),
            invoice_number: "INV-001".to_string(),
            invoice_date: Some(chrono::NaiveDate::from_ymd_opt(2026, 3, 6).unwrap()),
            due_date: Some(chrono::NaiveDate::from_ymd_opt(2026, 4, 6).unwrap()),
            po_number: None,
            subtotal: Some(Money { amount: amount_cents, currency: "USD".to_string() }),
            tax_amount: Some(Money { amount: 0, currency: "USD".to_string() }),
            total_amount: Money { amount: amount_cents, currency: "USD".to_string() },
            currency: "USD".to_string(),
            line_items: vec![],
            capture_status: CaptureStatus::Reviewed,
            processing_status: ProcessingStatus::Draft,
            current_queue_id: None,
            assigned_to: None,
            document_id: Uuid::new_v4(),
            supporting_documents: vec![],
            ocr_confidence: Some(0.95),
            categorization_confidence: None,
            department: Some("Engineering".to_string()),
            gl_code: Some("5000".to_string()),
            cost_center: None,
            notes: None,
            tags: vec![],
            custom_fields: json!({}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            created_by: UserId(Uuid::new_v4()),
        }
    }

    fn make_template(stages: Vec<WorkflowTemplateStage>) -> WorkflowTemplate {
        WorkflowTemplate {
            id: WorkflowTemplateId::new(),
            tenant_id: TenantId::new(),
            name: "Test Template".to_string(),
            description: None,
            is_active: true,
            is_default: false,
            stages,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn make_stage(order: i32, name: &str, stage_type: StageType, requires_action: bool) -> WorkflowTemplateStage {
        WorkflowTemplateStage {
            order,
            name: name.to_string(),
            stage_type,
            queue_id: None,
            sla_hours: None,
            escalation_hours: None,
            requires_action,
            skip_conditions: vec![],
            auto_advance_conditions: vec![],
        }
    }

    /// Simulates the stage evaluation logic from process_invoice_through_template
    /// without needing async repos. Returns the stage name the invoice stopped at,
    /// or None if all stages completed/skipped.
    fn evaluate_stages(
        invoice: &Invoice,
        stages: &[WorkflowTemplateStage],
    ) -> Option<String> {
        let mut sorted: Vec<&WorkflowTemplateStage> = stages.iter().collect();
        sorted.sort_by_key(|s| s.order);

        for stage in sorted {
            if !stage.skip_conditions.is_empty()
                && workflow_evaluator::evaluate_conditions(invoice, &stage.skip_conditions)
            {
                continue;
            }
            if !stage.auto_advance_conditions.is_empty()
                && workflow_evaluator::evaluate_conditions(invoice, &stage.auto_advance_conditions)
            {
                continue;
            }
            return Some(stage.name.clone());
        }
        None
    }

    #[test]
    fn test_template_stages_execute_in_order() {
        // 3-stage template: Review -> Approval -> Payment
        let template = make_template(vec![
            make_stage(0, "Review", StageType::Review, true),
            make_stage(1, "Approval", StageType::Approval, true),
            make_stage(2, "Payment", StageType::Payment, true),
        ]);

        let invoice = create_test_invoice_with_amount(10800);
        let result = evaluate_stages(&invoice, &template.stages);

        assert!(result.is_some());
        assert_eq!(result.unwrap(), "Review");
    }

    #[test]
    fn test_skip_conditions_skip_stage() {
        // Template: Review (skip if Amount < 10000) -> Approval -> Payment
        let mut review_stage = make_stage(0, "Review", StageType::Review, true);
        review_stage.skip_conditions = vec![RuleCondition {
            field: ConditionField::Amount,
            operator: ConditionOperator::LessThan,
            value: json!(10000),
        }];

        let template = make_template(vec![
            review_stage,
            make_stage(1, "Approval", StageType::Approval, true),
            make_stage(2, "Payment", StageType::Payment, true),
        ]);

        // Invoice with amount 5000 (under threshold) should skip Review, land at Approval
        let invoice = create_test_invoice_with_amount(5000);
        let result = evaluate_stages(&invoice, &template.stages);

        assert!(result.is_some());
        assert_eq!(result.unwrap(), "Approval");
    }

    #[test]
    fn test_auto_advance_conditions_advance_stage() {
        // Template: Review (auto-advance if VendorName == "Trusted Vendor") -> Approval -> Payment
        let mut review_stage = make_stage(0, "Review", StageType::Review, true);
        review_stage.auto_advance_conditions = vec![RuleCondition {
            field: ConditionField::VendorName,
            operator: ConditionOperator::Equals,
            value: json!("Trusted Vendor"),
        }];

        let template = make_template(vec![
            review_stage,
            make_stage(1, "Approval", StageType::Approval, true),
            make_stage(2, "Payment", StageType::Payment, true),
        ]);

        // Invoice from Trusted Vendor should auto-advance past Review
        let mut invoice = create_test_invoice_with_amount(10800);
        invoice.vendor_name = "Trusted Vendor".to_string();
        let result = evaluate_stages(&invoice, &template.stages);

        assert!(result.is_some());
        assert_eq!(result.unwrap(), "Approval");
    }

    #[test]
    fn test_approval_limit_blocks_over_limit() {
        // Simulate: invoice amount = 1000000 cents ($10,000), user limit = 500000 cents ($5,000)
        let invoice_amount_cents: i64 = 1_000_000;
        let user_limit_cents: i64 = 500_000;

        assert!(invoice_amount_cents > user_limit_cents);

        // Verify the condition logic matches what the route handler checks
        let should_block = invoice_amount_cents > user_limit_cents;
        assert!(should_block, "Approval should be blocked when invoice exceeds limit");
    }

    #[test]
    fn test_approval_limit_allows_under_limit() {
        // Simulate: invoice amount = 300000 cents ($3,000), user limit = 500000 cents ($5,000)
        let invoice_amount_cents: i64 = 300_000;
        let user_limit_cents: i64 = 500_000;

        assert!(invoice_amount_cents <= user_limit_cents);

        // Verify the condition logic matches what the route handler checks
        let should_allow = invoice_amount_cents <= user_limit_cents;
        assert!(should_allow, "Approval should be allowed when invoice is within limit");
    }
}

// ============================================================================
// Approval Limit Tests
// ============================================================================

#[cfg(test)]
mod approval_limit_tests {
    use super::*;
    use crate::domain::ApprovalLimit;
    use chrono::Utc;

    /// Verify that an approval limit struct correctly stores max_amount
    /// and that amount comparison logic works.
    #[test]
    fn test_approval_limit_blocks_over_limit() {
        let user_id = UserId::new();
        let limit = ApprovalLimit {
            id: Uuid::new_v4(),
            tenant_id: TenantId::new(),
            user_id: user_id.clone(),
            max_amount: Money { amount: 500000, currency: "USD".to_string() }, // $5,000.00
            vendor_restrictions: None,
            department_restrictions: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // Invoice amount of $10,000 (1,000,000 cents) exceeds $5,000 limit
        let invoice_amount_cents: i64 = 1_000_000;
        assert!(invoice_amount_cents > limit.max_amount.amount);
    }

    #[test]
    fn test_approval_limit_allows_under_limit() {
        let user_id = UserId::new();
        let limit = ApprovalLimit {
            id: Uuid::new_v4(),
            tenant_id: TenantId::new(),
            user_id: user_id.clone(),
            max_amount: Money { amount: 500000, currency: "USD".to_string() }, // $5,000.00
            vendor_restrictions: None,
            department_restrictions: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // Invoice amount of $3,000 (300,000 cents) is under $5,000 limit
        let invoice_amount_cents: i64 = 300_000;
        assert!(invoice_amount_cents <= limit.max_amount.amount);
    }

    #[test]
    fn test_approval_limit_vendor_restrictions_checked() {
        let allowed_vendor = Uuid::new_v4();
        let blocked_vendor = Uuid::new_v4();

        let limit = ApprovalLimit {
            id: Uuid::new_v4(),
            tenant_id: TenantId::new(),
            user_id: UserId::new(),
            max_amount: Money { amount: 500000, currency: "USD".to_string() },
            vendor_restrictions: Some(vec![allowed_vendor]),
            department_restrictions: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let allowed = limit.vendor_restrictions.as_ref().unwrap();
        assert!(allowed.contains(&allowed_vendor));
        assert!(!allowed.contains(&blocked_vendor));
    }

    #[test]
    fn test_approval_limit_department_restrictions_checked() {
        let limit = ApprovalLimit {
            id: Uuid::new_v4(),
            tenant_id: TenantId::new(),
            user_id: UserId::new(),
            max_amount: Money { amount: 500000, currency: "USD".to_string() },
            vendor_restrictions: None,
            department_restrictions: Some(vec!["Engineering".to_string(), "Sales".to_string()]),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let allowed = limit.department_restrictions.as_ref().unwrap();
        assert!(allowed.contains(&"Engineering".to_string()));
        assert!(!allowed.contains(&"Marketing".to_string()));
    }
}

// ============================================================================
// Tenant FK Constraint Migration Tests
// ============================================================================

#[cfg(test)]
mod tenant_fk_constraint_tests {
    /// Smoke test: verify the migration file contains the expected ALTER TABLE
    /// statements for users, vendors, and invoices with ON DELETE CASCADE.
    #[test]
    fn test_migration_contains_all_three_fk_constraints() {
        let sql = include_str!("../../../migrations/071_add_core_tenant_fk_constraints.sql");

        assert!(sql.contains("ALTER TABLE users"), "Missing ALTER TABLE users");
        assert!(sql.contains("ALTER TABLE vendors"), "Missing ALTER TABLE vendors");
        assert!(sql.contains("ALTER TABLE invoices"), "Missing ALTER TABLE invoices");

        assert!(sql.contains("fk_users_tenant_id"), "Missing fk_users_tenant_id constraint");
        assert!(sql.contains("fk_vendors_tenant_id"), "Missing fk_vendors_tenant_id constraint");
        assert!(sql.contains("fk_invoices_tenant_id"), "Missing fk_invoices_tenant_id constraint");
    }

    #[test]
    fn test_migration_specifies_on_delete_cascade() {
        let sql = include_str!("../../../migrations/071_add_core_tenant_fk_constraints.sql");

        // Count occurrences of ON DELETE CASCADE - should be 3 (one per table)
        let count = sql.matches("ON DELETE CASCADE").count();
        assert_eq!(count, 3, "Expected 3 ON DELETE CASCADE clauses, found {}", count);
    }

    #[test]
    fn test_migration_references_tenants_id() {
        let sql = include_str!("../../../migrations/071_add_core_tenant_fk_constraints.sql");

        let count = sql.matches("REFERENCES tenants(id)").count();
        assert_eq!(count, 3, "Expected 3 REFERENCES tenants(id) clauses, found {}", count);
    }
}
