//! Theme repository implementation for organization themes and user preferences

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Row types matching the JSONB column shapes expected by the API handlers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationThemeColors {
    pub primary: String,
    pub accent: String,
    pub capture: String,
    pub processing: String,
    pub vendor: String,
    pub reporting: String,
}

impl Default for OrganizationThemeColors {
    fn default() -> Self {
        Self {
            primary: "#3B82F6".into(),
            accent: "#8B5CF6".into(),
            capture: "#10B981".into(),
            processing: "#F59E0B".into(),
            vendor: "#EC4899".into(),
            reporting: "#6366F1".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationBranding {
    pub logo_url: Option<String>,
    pub logo_mark: Option<String>,
    pub favicon_url: Option<String>,
    pub brand_name: String,
    pub brand_gradient: Option<String>,
    #[serde(rename = "customCSS")]
    pub custom_css: Option<String>,
}

impl Default for OrganizationBranding {
    fn default() -> Self {
        Self {
            logo_url: None,
            logo_mark: None,
            favicon_url: None,
            brand_name: "BillForge".into(),
            brand_gradient: None,
            custom_css: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradientConfig {
    pub enabled: bool,
    #[serde(rename = "type")]
    pub gradient_type: String,
    pub angle: Option<f64>,
    pub positions: Option<Vec<GradientStop>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradientStop {
    pub color: String,
    pub position: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationThemeRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub preset_id: String,
    pub custom_colors: Option<OrganizationThemeColors>,
    pub branding: OrganizationBranding,
    pub enabled_for_all_users: bool,
    pub allow_user_override: bool,
    pub gradient_config: Option<GradientConfig>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserThemePreferenceRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub preset_id: String,
    pub custom_colors: Option<OrganizationThemeColors>,
    pub mode: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// Repository
// ---------------------------------------------------------------------------

pub struct ThemeRepository {
    pool: Arc<PgPool>,
}

impl ThemeRepository {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    // -- Organization theme ---------------------------------------------------

    pub async fn get_org_theme(&self, tenant_id: Uuid) -> Result<Option<OrganizationThemeRow>, sqlx::Error> {
        let row = sqlx::query_as::<_, OrgThemeSqlRow>(
            "SELECT id, tenant_id, preset_id, custom_colors, branding,
                    enabled_for_all_users, allow_user_override, gradient_config,
                    created_at, updated_at
             FROM organization_themes
             WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.map(|r| r.into_domain()))
    }

    pub async fn upsert_org_theme(
        &self,
        tenant_id: Uuid,
        preset_id: &str,
        custom_colors: Option<&OrganizationThemeColors>,
        branding: &OrganizationBranding,
        enabled_for_all_users: bool,
        allow_user_override: bool,
        gradient_config: Option<&GradientConfig>,
    ) -> Result<OrganizationThemeRow, sqlx::Error> {
        let colors_json = custom_colors.map(serde_json::to_value).transpose().ok().flatten();
        let branding_json = serde_json::to_value(branding).unwrap_or_default();
        let gradient_json = gradient_config.map(serde_json::to_value).transpose().ok().flatten();

        let row = sqlx::query_as::<_, OrgThemeSqlRow>(
            "INSERT INTO organization_themes
                (tenant_id, preset_id, custom_colors, branding,
                 enabled_for_all_users, allow_user_override, gradient_config)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (tenant_id) DO UPDATE SET
                preset_id = EXCLUDED.preset_id,
                custom_colors = EXCLUDED.custom_colors,
                branding = EXCLUDED.branding,
                enabled_for_all_users = EXCLUDED.enabled_for_all_users,
                allow_user_override = EXCLUDED.allow_user_override,
                gradient_config = EXCLUDED.gradient_config,
                updated_at = NOW()
             RETURNING id, tenant_id, preset_id, custom_colors, branding,
                       enabled_for_all_users, allow_user_override, gradient_config,
                       created_at, updated_at",
        )
        .bind(tenant_id)
        .bind(preset_id)
        .bind(&colors_json)
        .bind(&branding_json)
        .bind(enabled_for_all_users)
        .bind(allow_user_override)
        .bind(&gradient_json)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row.into_domain())
    }

    pub async fn delete_org_theme(&self, tenant_id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM organization_themes WHERE tenant_id = $1")
            .bind(tenant_id)
            .execute(&*self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Clear logo-related branding fields from stored org theme.
    pub async fn delete_logo(&self, tenant_id: Uuid, logo_type: &str) -> Result<bool, sqlx::Error> {
        let result = match logo_type {
            "main" => sqlx::query(
                "UPDATE organization_themes SET branding = branding - 'logoUrl', updated_at = NOW() WHERE tenant_id = $1",
            )
            .bind(tenant_id)
            .execute(&*self.pool)
            .await?,
            "mark" => sqlx::query(
                "UPDATE organization_themes SET branding = branding - 'logoMark', updated_at = NOW() WHERE tenant_id = $1",
            )
            .bind(tenant_id)
            .execute(&*self.pool)
            .await?,
            "favicon" => sqlx::query(
                "UPDATE organization_themes SET branding = branding - 'faviconUrl', updated_at = NOW() WHERE tenant_id = $1",
            )
            .bind(tenant_id)
            .execute(&*self.pool)
            .await?,
            _ => return Ok(false),
        };
        Ok(result.rows_affected() > 0)
    }

    // -- User theme preference ------------------------------------------------

    pub async fn get_user_theme(&self, tenant_id: Uuid, user_id: Uuid) -> Result<Option<UserThemePreferenceRow>, sqlx::Error> {
        let row = sqlx::query_as::<_, UserThemeSqlRow>(
            "SELECT id, tenant_id, user_id, preset_id, custom_colors, mode,
                    created_at, updated_at
             FROM user_theme_preferences
             WHERE tenant_id = $1 AND user_id = $2",
        )
        .bind(tenant_id)
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(row.map(|r| r.into_domain()))
    }

    pub async fn upsert_user_theme(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        preset_id: &str,
        custom_colors: Option<&OrganizationThemeColors>,
        mode: &str,
    ) -> Result<UserThemePreferenceRow, sqlx::Error> {
        let colors_json = custom_colors.map(serde_json::to_value).transpose().ok().flatten();

        let row = sqlx::query_as::<_, UserThemeSqlRow>(
            "INSERT INTO user_theme_preferences
                (tenant_id, user_id, preset_id, custom_colors, mode)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (tenant_id, user_id) DO UPDATE SET
                preset_id = EXCLUDED.preset_id,
                custom_colors = EXCLUDED.custom_colors,
                mode = EXCLUDED.mode,
                updated_at = NOW()
             RETURNING id, tenant_id, user_id, preset_id, custom_colors, mode,
                       created_at, updated_at",
        )
        .bind(tenant_id)
        .bind(user_id)
        .bind(preset_id)
        .bind(&colors_json)
        .bind(mode)
        .fetch_one(&*self.pool)
        .await?;

        Ok(row.into_domain())
    }

    pub async fn delete_user_theme(&self, tenant_id: Uuid, user_id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "DELETE FROM user_theme_preferences WHERE tenant_id = $1 AND user_id = $2",
        )
        .bind(tenant_id)
        .bind(user_id)
        .execute(&*self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}

// ---------------------------------------------------------------------------
// Internal SQL row types (JSONB columns read as serde_json::Value)
// ---------------------------------------------------------------------------

#[derive(Debug, sqlx::FromRow)]
struct OrgThemeSqlRow {
    id: Uuid,
    tenant_id: Uuid,
    preset_id: String,
    custom_colors: Option<serde_json::Value>,
    branding: serde_json::Value,
    enabled_for_all_users: bool,
    allow_user_override: bool,
    gradient_config: Option<serde_json::Value>,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl OrgThemeSqlRow {
    fn into_domain(self) -> OrganizationThemeRow {
        OrganizationThemeRow {
            id: self.id,
            tenant_id: self.tenant_id,
            preset_id: self.preset_id,
            custom_colors: self.custom_colors.and_then(|v| serde_json::from_value(v).ok()),
            branding: serde_json::from_value(self.branding).unwrap_or_default(),
            enabled_for_all_users: self.enabled_for_all_users,
            allow_user_override: self.allow_user_override,
            gradient_config: self.gradient_config.and_then(|v| serde_json::from_value(v).ok()),
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct UserThemeSqlRow {
    id: Uuid,
    tenant_id: Uuid,
    user_id: Uuid,
    preset_id: String,
    custom_colors: Option<serde_json::Value>,
    mode: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl UserThemeSqlRow {
    fn into_domain(self) -> UserThemePreferenceRow {
        UserThemePreferenceRow {
            id: self.id,
            tenant_id: self.tenant_id,
            user_id: self.user_id,
            preset_id: self.preset_id,
            custom_colors: self.custom_colors.and_then(|v| serde_json::from_value(v).ok()),
            mode: self.mode,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}
