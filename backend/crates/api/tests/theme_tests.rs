//! Tests for theme route data structures and serialization.
//!
//! Route-level integration tests require database setup; here we verify
//! the JSON shape matches the frontend TypeScript interfaces.

use billforge_api::routes::theme::*;

// ---------------------------------------------------------------------------
// Organization theme serde round-trips
// ---------------------------------------------------------------------------

#[test]
fn test_organization_theme_colors_default_round_trip() {
    let colors = OrganizationThemeColors::default();
    let json = serde_json::to_string(&colors).expect("serialize");
    let back: OrganizationThemeColors = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.primary, colors.primary);
    assert_eq!(back.accent, colors.accent);
}

#[test]
fn test_organization_theme_colors_json_keys() {
    let colors = OrganizationThemeColors::default();
    let val: serde_json::Value = serde_json::to_value(&colors).expect("to value");
    assert!(val.get("primary").is_some());
    assert!(val.get("accent").is_some());
    assert!(val.get("capture").is_some());
    assert!(val.get("processing").is_some());
    assert!(val.get("vendor").is_some());
    assert!(val.get("reporting").is_some());
    // 6 fields total
    assert_eq!(val.as_object().unwrap().len(), 6);
}

#[test]
fn test_organization_branding_round_trip() {
    let branding = OrganizationBranding::default();
    let json = serde_json::to_string(&branding).expect("serialize");
    // Verify camelCase field names match the frontend TypeScript interface
    assert!(json.contains("brandName"), "expected camelCase field brandName");
    assert!(json.contains("logoUrl"), "expected camelCase field logoUrl");
    let back: OrganizationBranding = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.brand_name, "BillForge");
    assert!(back.logo_url.is_none());
}

#[test]
fn test_organization_theme_full_round_trip() {
    let now = chrono::Utc::now().to_rfc3339();
    let theme = OrganizationTheme {
        id: uuid::Uuid::new_v4().to_string(),
        tenant_id: "t1".into(),
        preset_id: "ocean".into(),
        custom_colors: Some(OrganizationThemeColors::default()),
        branding: OrganizationBranding::default(),
        enabled_for_all_users: true,
        allow_user_override: false,
        gradient_config: Some(GradientConfig {
            enabled: true,
            gradient_type: "linear".into(),
            angle: Some(90.0),
            positions: Some(vec![GradientStop {
                color: "#000".into(),
                position: 0.0,
            }]),
        }),
        created_at: now.clone(),
        updated_at: now,
    };

    let json = serde_json::to_string(&theme).expect("serialize");
    let val: serde_json::Value = serde_json::from_str(&json).expect("parse");
    // Verify key fields present (frontend interface contract)
    assert!(val.get("id").is_some());
    assert!(val.get("tenant_id").is_some());
    assert!(val.get("preset_id").is_some());
    assert!(val.get("custom_colors").is_some());
    assert!(val.get("branding").is_some());
    assert!(val.get("enabled_for_all_users").is_some());
    assert!(val.get("allow_user_override").is_some());
    assert!(val.get("gradient_config").is_some());
    assert!(val.get("created_at").is_some());
    assert!(val.get("updated_at").is_some());

    let back: OrganizationTheme = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.preset_id, "ocean");
    assert!(back.gradient_config.unwrap().enabled);
}

#[test]
fn test_create_organization_theme_input_deserialize() {
    let json = r#"{
        "preset_id": "midnight",
        "branding": { "brand_name": "Acme" },
        "enabled_for_all_users": true,
        "allow_user_override": false
    }"#;
    let input: CreateOrganizationThemeInput = serde_json::from_str(json).expect("deserialize");
    assert_eq!(input.preset_id, "midnight");
    assert_eq!(input.branding.brand_name, "Acme");
    assert!(input.enabled_for_all_users.unwrap());
    assert!(input.custom_colors.is_none());
}

// ---------------------------------------------------------------------------
// User theme preference serde round-trips
// ---------------------------------------------------------------------------

#[test]
fn test_user_theme_preference_round_trip() {
    let now = chrono::Utc::now().to_rfc3339();
    let pref = UserThemePreference {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: "u1".into(),
        preset_id: "forest".into(),
        custom_colors: None,
        mode: "dark".into(),
        created_at: now.clone(),
        updated_at: now,
    };
    let json = serde_json::to_string(&pref).expect("serialize");
    let val: serde_json::Value = serde_json::from_str(&json).expect("parse");
    assert!(val.get("id").is_some());
    assert!(val.get("user_id").is_some());
    assert!(val.get("preset_id").is_some());
    assert!(val.get("custom_colors").is_some());
    assert!(val.get("mode").is_some());
    assert!(val.get("created_at").is_some());
    assert!(val.get("updated_at").is_some());

    let back: UserThemePreference = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.mode, "dark");
}

#[test]
fn test_create_user_theme_input_deserialize() {
    let json = r#"{ "preset_id": "sunset", "mode": "system" }"#;
    let input: CreateUserThemeInput = serde_json::from_str(json).expect("deserialize");
    assert_eq!(input.preset_id, "sunset");
    assert_eq!(input.mode, "system");
    assert!(input.custom_colors.is_none());
}

// ---------------------------------------------------------------------------
// Effective theme shape
// ---------------------------------------------------------------------------

#[test]
fn test_effective_theme_serialization() {
    let effective = EffectiveTheme {
        theme: None,
        user_preference: None,
        effective_colors: OrganizationThemeColors::default(),
        effective_mode: "system".into(),
        can_override: true,
    };
    let val: serde_json::Value = serde_json::to_value(&effective).expect("to value");
    assert!(val["theme"].is_null());
    assert!(val["user_preference"].is_null());
    assert!(val["effective_colors"].is_object());
    assert_eq!(val["effective_mode"], "system");
    assert!(val["can_override"].is_bool());
}
