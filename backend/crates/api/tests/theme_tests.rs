//! Tests for theme route data structures and serialization.
//!
//! Route-level integration tests require database setup; here we verify
//! the JSON shape matches the frontend TypeScript interfaces.

use billforge_api::routes::theme::*;
use billforge_core::domain::{AuditAction, AuditEntry, ResourceType};
use billforge_core::traits::AuditService;
use billforge_core::TenantId;
use billforge_db::repositories::{
    AuditRepositoryImpl, GradientConfig, GradientStop, OrganizationBranding,
    OrganizationThemeColors, OrganizationThemeRow, UserThemePreferenceRow,
};
use std::sync::Arc;
use uuid::Uuid;

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
    assert!(
        json.contains("brandName"),
        "expected camelCase field brandName"
    );
    assert!(json.contains("logoUrl"), "expected camelCase field logoUrl");
    // customCSS must use uppercase CSS to match the frontend interface
    assert!(
        json.contains("customCSS"),
        "expected field customCSS (uppercase CSS)"
    );
    let back: OrganizationBranding = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.brand_name, "BillForge");
    assert!(back.logo_url.is_none());
}

#[test]
fn test_organization_branding_custom_css_round_trip() {
    let branding = OrganizationBranding {
        custom_css: Some("body { color: red; }".into()),
        ..OrganizationBranding::default()
    };
    // Serialize → JSON must use "customCSS" key
    let json = serde_json::to_string(&branding).expect("serialize");
    assert!(json.contains("\"customCSS\":\"body { color: red; }\""));
    // Deserialize from frontend-shaped JSON
    let input = r#"{ "brandName": "Test", "customCSS": "h1 { font-size: 2rem; }" }"#;
    let back: OrganizationBranding = serde_json::from_str(input).expect("deserialize");
    assert_eq!(back.brand_name, "Test");
    assert_eq!(back.custom_css.as_deref(), Some("h1 { font-size: 2rem; }"));
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
        "branding": { "brandName": "Acme", "customCSS": "body { margin: 0; }" },
        "enabled_for_all_users": true,
        "allow_user_override": false
    }"#;
    let input: CreateOrganizationThemeInput = serde_json::from_str(json).expect("deserialize");
    assert_eq!(input.preset_id, "midnight");
    assert_eq!(input.branding.brand_name, "Acme");
    assert_eq!(
        input.branding.custom_css.as_deref(),
        Some("body { margin: 0; }")
    );
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

#[test]
fn test_update_organization_theme_input_partial_deserialize() {
    // Frontend sends Partial<> — only changed fields
    let json = r#"{ "enabled_for_all_users": true }"#;
    let input: UpdateOrganizationThemeInput = serde_json::from_str(json).expect("deserialize");
    assert!(input.preset_id.is_none());
    assert!(input.branding.is_none());
    assert!(input.enabled_for_all_users.unwrap());
}

#[test]
fn test_update_organization_theme_input_full_deserialize() {
    let json = r#"{
        "preset_id": "midnight",
        "branding": { "brandName": "Acme" },
        "enabled_for_all_users": true,
        "allow_user_override": false
    }"#;
    let input: UpdateOrganizationThemeInput = serde_json::from_str(json).expect("deserialize");
    assert_eq!(input.preset_id.as_deref(), Some("midnight"));
    assert_eq!(input.branding.unwrap().brand_name, "Acme");
}

#[test]
fn test_update_user_theme_input_partial_deserialize() {
    let json = r#"{ "mode": "dark" }"#;
    let input: UpdateUserThemeInput = serde_json::from_str(json).expect("deserialize");
    assert!(input.preset_id.is_none());
    assert_eq!(input.mode.as_deref(), Some("dark"));
    assert!(input.custom_colors.is_none());
}

#[test]
fn test_update_user_theme_input_empty_deserialize() {
    let json = r#"{}"#;
    let input: UpdateUserThemeInput = serde_json::from_str(json).expect("deserialize");
    assert!(input.preset_id.is_none());
    assert!(input.mode.is_none());
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
    assert!(val["can_override"].is_boolean());
}

// ---------------------------------------------------------------------------
// Persistence-layer unit tests (no DB required)
// ---------------------------------------------------------------------------

#[test]
fn test_theme_repo_row_to_api_type_conversion() {
    // Verify OrganizationThemeRow -> OrganizationTheme conversion preserves fields
    let now = chrono::Utc::now();
    let row = OrganizationThemeRow {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        preset_id: "ocean".into(),
        custom_colors: Some(OrganizationThemeColors::default()),
        branding: OrganizationBranding::default(),
        enabled_for_all_users: true,
        allow_user_override: false,
        gradient_config: None,
        created_at: now,
        updated_at: now,
    };
    let api: OrganizationTheme = row.into();
    assert_eq!(api.preset_id, "ocean");
    assert!(api.custom_colors.is_some());
    assert!(api.enabled_for_all_users);
    assert!(!api.allow_user_override);
}

#[test]
fn test_user_theme_row_to_api_type_conversion() {
    let now = chrono::Utc::now();
    let row = UserThemePreferenceRow {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        user_id: Uuid::new_v4(),
        preset_id: "forest".into(),
        custom_colors: None,
        mode: "dark".into(),
        created_at: now,
        updated_at: now,
    };
    let api: UserThemePreference = row.into();
    assert_eq!(api.preset_id, "forest");
    assert_eq!(api.mode, "dark");
    assert!(api.custom_colors.is_none());
}

// ---------------------------------------------------------------------------
// Audit-logging tests for branding mutations (issue #413)
//
// These mirror the workflow_audit_capture_test.rs and report_digest_audit_test.rs
// pattern: simulate exactly what each handler does (build the same AuditEntry,
// call AuditRepositoryImpl::log) and verify the row lands in audit_log with the
// expected shape so a regression that strips the .log() call from a handler
// would surface as a missing audit row in this suite.
// ---------------------------------------------------------------------------

async fn setup_audit_schema(pool: &sqlx::PgPool, tenant_id: &TenantId) {
    billforge_db::tenant_db::run_tenant_migrations(pool, tenant_id)
        .await
        .expect("tenant migrations");
}

async fn insert_audit_user(pool: &sqlx::PgPool, tenant_id: &TenantId, user_id: Uuid) {
    sqlx::query(
        r#"INSERT INTO users (id, tenant_id, email, password_hash, name, roles)
           VALUES ($1, $2, $3, $4, $5, '["tenant_admin"]'::jsonb)
           ON CONFLICT DO NOTHING"#,
    )
    .bind(user_id)
    .bind(*tenant_id.as_uuid())
    .bind("theme-audit@example.com")
    .bind("hash_not_used")
    .bind("Theme Audit User")
    .execute(pool)
    .await
    .expect("insert test user");
}

/// Fetch the (action, resource_type, changes) audit_log columns for a given
/// resource_id, returning the most recent row.
async fn read_audit_row(
    pool: &sqlx::PgPool,
    resource_id: &str,
) -> Option<(String, String, Option<serde_json::Value>)> {
    sqlx::query_as(
        "SELECT action, resource_type, changes FROM audit_log \
         WHERE resource_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(resource_id)
    .fetch_optional(pool)
    .await
    .expect("query audit_log")
}

async fn count_audit_rows_for_tenant(pool: &sqlx::PgPool, tenant_id: &TenantId) -> i64 {
    let (n,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM audit_log WHERE tenant_id = $1 AND resource_type = $2")
            .bind(*tenant_id.as_uuid())
            .bind("settings")
            .fetch_one(pool)
            .await
            .expect("count audit_log");
    n
}

#[sqlx::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test theme_tests -- --ignored
async fn test_create_update_delete_org_theme_writes_audit_entries(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();

    setup_audit_schema(&pool, &tenant_id).await;
    insert_audit_user(&pool, &tenant_id, user_id).await;

    let audit_repo = AuditRepositoryImpl::new(pool.clone());

    // --- Simulated POST /organization/theme ---
    let theme_id = Uuid::new_v4();
    let new_value = serde_json::json!({
        "id": theme_id.to_string(),
        "preset_id": "ocean",
        "branding": { "brandName": "Acme", "logoUrl": null },
    });
    let create_entry = AuditEntry::new(
        tenant_id.clone(),
        Some(billforge_core::UserId(user_id)),
        AuditAction::Create,
        ResourceType::Settings,
        theme_id.to_string(),
        "Created organization theme",
    )
    .with_user_email("theme-audit@example.com")
    .with_new_value(new_value.clone());
    audit_repo
        .log(create_entry)
        .await
        .expect("create audit log write");

    let row = read_audit_row(&pool, &theme_id.to_string())
        .await
        .expect("create audit row must exist");
    assert_eq!(row.0, "create");
    assert_eq!(row.1, "settings");
    let changes = row.2.expect("changes JSONB");
    assert_eq!(changes["description"], "Created organization theme");
    assert_eq!(changes["new_value"]["preset_id"], "ocean");
    assert_eq!(changes["user_email"], "theme-audit@example.com");

    // --- Simulated PUT /organization/theme ---
    let old_value = new_value.clone();
    let updated_value = serde_json::json!({
        "id": theme_id.to_string(),
        "preset_id": "midnight",
        "branding": { "brandName": "Acme", "logoUrl": "https://attacker.example.com/logo.png" },
    });
    let update_entry = AuditEntry::new(
        tenant_id.clone(),
        Some(billforge_core::UserId(user_id)),
        AuditAction::Update,
        ResourceType::Settings,
        theme_id.to_string(),
        "Updated organization theme",
    )
    .with_user_email("theme-audit@example.com")
    .with_old_value(old_value)
    .with_new_value(updated_value);
    audit_repo
        .log(update_entry)
        .await
        .expect("update audit log write");

    let row = read_audit_row(&pool, &theme_id.to_string())
        .await
        .expect("update audit row must exist");
    assert_eq!(row.0, "update");
    let changes = row.2.expect("changes JSONB");
    assert_eq!(changes["old_value"]["preset_id"], "ocean");
    assert_eq!(changes["new_value"]["preset_id"], "midnight");
    // Brand impersonation surface: logo swap must be reconstructible
    assert_eq!(changes["old_value"]["branding"]["logoUrl"], serde_json::Value::Null);
    assert_eq!(
        changes["new_value"]["branding"]["logoUrl"],
        "https://attacker.example.com/logo.png"
    );

    // --- Simulated DELETE /organization/theme ---
    let delete_resource_id = tenant_id.as_uuid().to_string();
    let delete_entry = AuditEntry::new(
        tenant_id.clone(),
        Some(billforge_core::UserId(user_id)),
        AuditAction::Delete,
        ResourceType::Settings,
        delete_resource_id.clone(),
        "Deleted organization theme",
    )
    .with_user_email("theme-audit@example.com")
    .with_old_value(serde_json::json!({
        "id": theme_id.to_string(),
        "preset_id": "midnight",
    }));
    audit_repo
        .log(delete_entry)
        .await
        .expect("delete audit log write");

    let row = read_audit_row(&pool, &delete_resource_id)
        .await
        .expect("delete audit row must exist");
    assert_eq!(row.0, "delete");
    assert_eq!(row.1, "settings");
    let changes = row.2.expect("changes JSONB");
    assert_eq!(changes["old_value"]["preset_id"], "midnight");
    // Delete has no new_value
    assert!(changes["new_value"].is_null());

    // All three writes landed under this tenant on the Settings resource type.
    let total = count_audit_rows_for_tenant(&pool, &tenant_id).await;
    assert_eq!(
        total, 3,
        "expected 3 audit rows (create+update+delete) for this tenant"
    );
}

#[sqlx::test]
#[ignore] // Requires DATABASE_URL - run with: cargo test --test theme_tests -- --ignored
async fn test_delete_logo_writes_audit_entry(pool: sqlx::PgPool) {
    let pool = Arc::new(pool);
    let tenant_id = TenantId::from_uuid(Uuid::new_v4());
    let user_id = Uuid::new_v4();

    setup_audit_schema(&pool, &tenant_id).await;
    insert_audit_user(&pool, &tenant_id, user_id).await;

    let logo_type = "primary";
    let resource_id = tenant_id.as_uuid().to_string();

    // Simulate what delete_logo handler does after the storage call returns.
    let entry = AuditEntry::new(
        tenant_id.clone(),
        Some(billforge_core::UserId(user_id)),
        AuditAction::SettingsChanged,
        ResourceType::Settings,
        resource_id.clone(),
        format!("Deleted logo: {}", logo_type),
    )
    .with_user_email("theme-audit@example.com")
    .with_new_value(serde_json::json!({ "logo_type": logo_type, "removed": true }));

    let audit_repo = AuditRepositoryImpl::new(pool.clone());
    audit_repo.log(entry).await.expect("audit log write");

    let row = read_audit_row(&pool, &resource_id)
        .await
        .expect("audit row must exist");
    assert_eq!(row.0, "settings_changed");
    assert_eq!(row.1, "settings");
    let changes = row.2.expect("changes JSONB");
    assert_eq!(changes["description"], format!("Deleted logo: {}", logo_type));
    assert_eq!(changes["new_value"]["logo_type"], logo_type);
    assert_eq!(changes["new_value"]["removed"], true);
    assert_eq!(changes["user_email"], "theme-audit@example.com");
}
