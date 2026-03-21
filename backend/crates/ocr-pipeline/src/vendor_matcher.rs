//! Vendor matching engine
//!
//! Matches OCR-extracted vendor names to known vendors using a three-tier strategy:
//! 1. **Exact match** — case-insensitive name comparison
//! 2. **Alias match** — check learned and manual aliases
//! 3. **Fuzzy match** — Levenshtein-based similarity scoring
//!
//! Uses runtime sqlx queries (not compile-time macros).

use crate::error::PipelineError;
use crate::types::{VendorMatchMethod, VendorMatchResult};
use billforge_core::types::TenantId;
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Minimum similarity score (0.0–1.0) for a fuzzy match to be considered valid
const FUZZY_MATCH_THRESHOLD: f32 = 0.7;

/// Vendor matching engine
pub struct VendorMatcher {
    pool: PgPool,
}

impl VendorMatcher {
    /// Create a new vendor matcher
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Match an OCR-extracted vendor name to a known vendor.
    ///
    /// Strategy: exact → alias → fuzzy
    pub async fn match_vendor(
        &self,
        tenant_id: &TenantId,
        extracted_name: &str,
    ) -> Result<VendorMatchResult, PipelineError> {
        let tenant_uuid = *tenant_id.as_uuid();
        let normalized = normalize_vendor_name(extracted_name);

        // 1. Exact match (case-insensitive)
        let exact_row = sqlx::query(
            r#"
            SELECT id, name
            FROM vendors
            WHERE tenant_id = $1 AND LOWER(TRIM(name)) = LOWER($2)
            LIMIT 1
            "#,
        )
        .bind(tenant_uuid)
        .bind(&normalized)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = exact_row {
            return Ok(VendorMatchResult {
                vendor_id: Some(row.get::<Uuid, _>("id")),
                vendor_name: Some(row.get::<String, _>("name")),
                confidence: 1.0,
                match_method: VendorMatchMethod::Exact,
            });
        }

        // 2. Alias match
        let alias_row = sqlx::query(
            r#"
            SELECT va.vendor_id, v.name
            FROM vendor_aliases va
            JOIN vendors v ON v.id = va.vendor_id AND v.tenant_id = va.tenant_id
            WHERE va.tenant_id = $1 AND LOWER(TRIM(va.alias)) = LOWER($2)
            LIMIT 1
            "#,
        )
        .bind(tenant_uuid)
        .bind(&normalized)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = alias_row {
            return Ok(VendorMatchResult {
                vendor_id: Some(row.get::<Uuid, _>("vendor_id")),
                vendor_name: Some(row.get::<String, _>("name")),
                confidence: 0.95,
                match_method: VendorMatchMethod::Alias,
            });
        }

        // 3. Fuzzy match using PostgreSQL similarity (pg_trgm)
        let fuzzy_row: Option<PgRow> = sqlx::query(
            r#"
            SELECT id, name, similarity(LOWER(name), LOWER($1)) as sim
            FROM vendors
            WHERE tenant_id = $2 AND similarity(LOWER(name), LOWER($1)) > $3
            ORDER BY sim DESC
            LIMIT 1
            "#,
        )
        .bind(&normalized)
        .bind(tenant_uuid)
        .bind(FUZZY_MATCH_THRESHOLD)
        .fetch_optional(&self.pool)
        .await
        .unwrap_or(None);

        if let Some(row) = fuzzy_row {
            let similarity = row.get::<f32, _>("sim");
            return Ok(VendorMatchResult {
                vendor_id: Some(row.get::<Uuid, _>("id")),
                vendor_name: Some(row.get::<String, _>("name")),
                confidence: similarity,
                match_method: VendorMatchMethod::Fuzzy,
            });
        }

        // No match found
        Ok(VendorMatchResult {
            vendor_id: None,
            vendor_name: None,
            confidence: 0.0,
            match_method: VendorMatchMethod::None,
        })
    }

    /// Learn a new vendor alias from a user correction
    pub async fn learn_alias(
        &self,
        tenant_id: &TenantId,
        vendor_id: Uuid,
        alias: &str,
    ) -> Result<(), PipelineError> {
        let tenant_uuid = *tenant_id.as_uuid();
        let normalized = normalize_vendor_name(alias);

        // Upsert to avoid duplicate alias entries
        sqlx::query(
            r#"
            INSERT INTO vendor_aliases (id, tenant_id, vendor_id, alias, is_learned, created_at)
            VALUES ($1, $2, $3, $4, true, NOW())
            ON CONFLICT (tenant_id, LOWER(alias)) DO UPDATE
            SET vendor_id = $3, is_learned = true
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(tenant_uuid)
        .bind(vendor_id)
        .bind(&normalized)
        .execute(&self.pool)
        .await?;

        tracing::info!(
            vendor_id = %vendor_id,
            alias = %normalized,
            "Vendor alias learned from correction"
        );

        Ok(())
    }
}

/// Normalize a vendor name for matching: trim whitespace and collapse internal spaces
fn normalize_vendor_name(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ")
}
