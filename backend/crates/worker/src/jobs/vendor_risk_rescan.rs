//! Background job: Continuous Vendor Risk Rescan (refs #381)
//!
//! Moves vendor sanctions screening from a one-time onboarding check to a
//! scheduled, continuous monitor. Per-tenant the job:
//!   1. Paginates active vendors.
//!   2. Re-runs OFAC/SDN screening via `billforge_vendor_mgmt::ofac_screening`.
//!   3. For each new sanctions hit that does not already have an open
//!      `vendor_risk_alerts` row, inserts a `critical` alert + sets
//!      `vendors.payment_hold = true`.
//!   4. Updates `vendors.last_risk_rescan_at`.
//!
//! PEP / beneficial-ownership / address-drift / tax-ID re-verification are
//! stubbed behind the `RiskProvider` trait with a default `NullProvider` that
//! returns `Ok(no_change)`. The job still invokes the trait so the wiring is in
//! place when real providers land (deferred per #381 scope).

use anyhow::{Context, Result};
use billforge_core::TenantId;
use billforge_db::PgManager;
use billforge_vendor_mgmt::ofac_screening::{OfacScreenOutcome, OfacScreener};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

/// Page size for vendor iteration. Kept small so each tenant scan bounds peak
/// memory regardless of vendor count.
const VENDOR_PAGE_SIZE: i64 = 500;

// ---------------------------------------------------------------------------
// RiskProvider trait (PEP / beneficial-ownership / address / tax-ID)
// ---------------------------------------------------------------------------

/// Finding produced by an external risk provider for a single vendor.
#[derive(Debug, Clone, Default)]
pub struct RiskFinding {
    /// `true` when the provider has nothing new to report for this vendor.
    pub no_change: bool,
    /// Alert type discriminator stored on `vendor_risk_alerts.alert_type`.
    pub alert_type: Option<String>,
    /// Severity for the alert row.
    pub severity: Option<String>,
    /// Stable, sorted JSON string of the meaningful payload fields, hashed to
    /// make repeated scans of the same finding idempotent.
    pub payload: Option<serde_json::Value>,
}

/// External risk provider surface. Real integrations (PEP list, UBO registry,
/// address validation, IRS TIN matcher) implement this; the worker always
/// invokes it so wiring is in place even when no provider is configured.
///
/// Uses an explicit boxed-future return type (no `async_trait` dep) so the
/// worker crate stays dependency-light.
pub trait RiskProvider: Send + Sync {
    fn screen_vendor<'a>(
        &'a self,
        pool: &'a sqlx::PgPool,
        tenant_id: Uuid,
        vendor_id: Uuid,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = std::result::Result<RiskFinding, anyhow::Error>>
                + Send
                + 'a,
        >,
    >;
}

/// Default no-op provider. Returns `no_change=true` for every vendor so the job
/// compiles and runs end-to-end before real providers are wired.
pub struct NullProvider;

impl RiskProvider for NullProvider {
    fn screen_vendor<'a>(
        &'a self,
        _pool: &'a sqlx::PgPool,
        _tenant_id: Uuid,
        _vendor_id: Uuid,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = std::result::Result<RiskFinding, anyhow::Error>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            Ok(RiskFinding {
                no_change: true,
                ..Default::default()
            })
        })
    }
}

// ---------------------------------------------------------------------------
// Public entry points (matches the forecast_refresh / anomaly_detection shape)
// ---------------------------------------------------------------------------

/// Run a continuous rescan across all active tenants. Used by an admin/manual
/// trigger path; the scheduler enqueues per-tenant jobs instead.
pub async fn rescan_all_vendors(pg_manager: Arc<PgManager>) -> Result<()> {
    info!("Starting vendor risk rescan job");
    let metadata_pool = pg_manager.metadata();
    let tenants: Vec<(String,)> =
        sqlx::query_as("SELECT id::text FROM tenants WHERE active = true")
            .fetch_all(metadata_pool)
            .await
            .context("Failed to fetch active tenants")?;

    info!("Processing {} active tenants", tenants.len());

    for (tenant_id_str,) in tenants {
        let tenant_id = match tenant_id_str.parse::<TenantId>() {
            Ok(t) => t,
            Err(e) => {
                warn!(tenant_id = %tenant_id_str, error = %e, "Skipping invalid tenant id");
                continue;
            }
        };
        if let Err(e) = rescan_tenant(pg_manager.clone(), &tenant_id).await {
            warn!(tenant_id = %tenant_id_str, error = %e, "Failed to rescan tenant vendors");
        }
    }

    info!("Vendor risk rescan job completed");
    Ok(())
}

/// Run a continuous rescan for one validated tenant. This is the entry point
/// the worker dispatch table calls for `JobType::VendorRiskRescan`.
pub async fn rescan_tenant(pg_manager: Arc<PgManager>, tenant_id: &TenantId) -> Result<()> {
    let pool = pg_manager.tenant(tenant_id).await?;
    let provider = NullProvider;
    rescan_tenant_with_provider(&pool, *tenant_id.as_uuid(), &provider).await
}

/// Rescan a single tenant using an explicit provider. Exposed so tests can drive
/// the loop with a real PgPool and a deterministic provider.
pub async fn rescan_tenant_with_provider(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    provider: &dyn RiskProvider,
) -> Result<()> {
    let tenant_id_str = tenant_id.to_string();
    info!(tenant_id = %tenant_id_str, "Rescanning vendor risk");

    let screener = OfacScreener::load_from_embedded();

    // Iterate vendors in pages. Each row carries its dba (nullable).
    let mut offset: i64 = 0;
    loop {
        let page: Vec<(Uuid, String, Option<String>)> = sqlx::query_as(
            r#"
            SELECT id, name, dba
            FROM vendors
            WHERE tenant_id = $1
              AND status = 'active'
            ORDER BY id
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(tenant_id)
        .bind(VENDOR_PAGE_SIZE)
        .bind(offset)
        .fetch_all(pool)
        .await
        .context("Failed to page active vendors")?;

        if page.is_empty() {
            break;
        }

        for (vendor_id, name, dba) in &page {
            // 1. OFAC / sanctions re-screen.
            let outcome = screener.screen(name, dba.as_deref());
            if outcome.status != "pass" {
                record_sanctions_alert(pool, tenant_id, *vendor_id, &outcome).await?;
            }

            // 2. External risk providers (PEP / UBO / address / tax-ID).
            //    The trait is always invoked so wiring stays in place; the
            //    NullProvider short-circuits with no_change=true.
            match provider.screen_vendor(pool, tenant_id, *vendor_id).await {
                Ok(finding) => {
                    if !finding.no_change {
                        if let (Some(alert_type), Some(severity), Some(payload)) =
                            (finding.alert_type, finding.severity, finding.payload)
                        {
                            record_provider_alert(
                                pool,
                                tenant_id,
                                *vendor_id,
                                &alert_type,
                                &severity,
                                &payload,
                            )
                            .await?;
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        tenant_id = %tenant_id_str,
                        vendor_id = %vendor_id,
                        error = %e,
                        "RiskProvider error - skipping"
                    );
                }
            }

            // 3. Refresh last_risk_rescan_at for this vendor.
            sqlx::query(
                "UPDATE vendors SET last_risk_rescan_at = NOW() WHERE id = $1 AND tenant_id = $2",
            )
            .bind(vendor_id)
            .bind(tenant_id)
            .execute(pool)
            .await
            .context("Failed to update last_risk_rescan_at")?;
        }

        offset += VENDOR_PAGE_SIZE;
        if (page.len() as i64) < VENDOR_PAGE_SIZE {
            break;
        }
    }

    info!(tenant_id = %tenant_id_str, "Vendor risk rescan complete");
    Ok(())
}

// ---------------------------------------------------------------------------
// Alert writers (idempotent on payload hash)
// ---------------------------------------------------------------------------

/// Insert a sanctions_hit alert if no open one with the same payload hash
/// already exists. Sets payment_hold=true on the vendor when a new alert lands.
async fn record_sanctions_alert(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    vendor_id: Uuid,
    outcome: &OfacScreenOutcome,
) -> Result<()> {
    let payload = serde_json::json!({
        "status": outcome.status,
        "matches": outcome.matches,
    });
    let payload_hash = stable_payload_hash(&payload);
    record_alert_row(
        pool,
        tenant_id,
        vendor_id,
        "sanctions_hit",
        "critical",
        payload,
        payload_hash,
    )
    .await
}

/// Insert a provider-sourced alert (PEP / UBO / address / tax-ID). Idempotent
/// on payload hash; sets payment_hold=true when a new critical alert lands.
async fn record_provider_alert(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    vendor_id: Uuid,
    alert_type: &str,
    severity: &str,
    payload: &serde_json::Value,
) -> Result<()> {
    let payload_hash = stable_payload_hash(payload);
    record_alert_row(
        pool,
        tenant_id,
        vendor_id,
        alert_type,
        severity,
        payload.clone(),
        payload_hash,
    )
    .await
}

/// Common insert path shared by sanctions + provider alerts.
///
/// Idempotency: a producer only inserts when no OPEN row already exists with
/// the same `(vendor_id, alert_type, payload_hash)`. The
/// `idx_vendor_risk_alerts_open_dedupe` partial index makes this lookup cheap.
async fn record_alert_row(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    vendor_id: Uuid,
    alert_type: &str,
    severity: &str,
    payload: serde_json::Value,
    payload_hash: String,
) -> Result<()> {
    let existing: Option<(Uuid,)> = sqlx::query_as(
        r#"
        SELECT id FROM vendor_risk_alerts
        WHERE vendor_id = $1
          AND alert_type = $2
          AND payload_hash = $3
          AND status = 'open'
        LIMIT 1
        "#,
    )
    .bind(vendor_id)
    .bind(alert_type)
    .bind(&payload_hash)
    .fetch_optional(pool)
    .await
    .context("Failed to look up existing open alert")?;

    if existing.is_some() {
        // Already alerted; do not duplicate.
        return Ok(());
    }

    sqlx::query(
        r#"
        INSERT INTO vendor_risk_alerts
            (tenant_id, vendor_id, alert_type, severity, payload, payload_hash)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(tenant_id)
    .bind(vendor_id)
    .bind(alert_type)
    .bind(severity)
    .bind(payload)
    .bind(&payload_hash)
    .execute(pool)
    .await
    .context("Failed to insert vendor_risk_alert")?;

    // Critical sanctions/banking hits block payment release until acknowledged.
    if severity == "critical" {
        sqlx::query(
            "UPDATE vendors SET payment_hold = true, updated_at = NOW() \
             WHERE id = $1 AND tenant_id = $2 AND payment_hold = false",
        )
        .bind(vendor_id)
        .bind(tenant_id)
        .execute(pool)
        .await
        .context("Failed to set payment_hold")?;
    }

    Ok(())
}

/// Deterministic hash of the canonical JSON payload so repeated scans of the
/// same finding collapse to one open alert. SHA-256 over the canonical bytes.
pub fn stable_payload_hash(payload: &serde_json::Value) -> String {
    let canonical = canonical_json(payload);
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    hex::encode(hasher.finalize())
}

/// Recursively produce a stable, sorted-keys JSON string for hashing. We avoid
/// serde_json canonical serialization to stay independent of feature flags.
fn canonical_json(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(map) => {
            // Sort keys for stable hashing regardless of insertion order.
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys
                .into_iter()
                .map(|k| format!("{}:{}", k, canonical_json(&map[k])))
                .collect();
            format!("{{{}}}", parts.join(","))
        }
        serde_json::Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(canonical_json).collect();
            format!("[{}]", parts.join(","))
        }
        serde_json::Value::String(s) => format!("\"{}\"", s),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_payload_hash_is_deterministic_across_key_order() {
        let a = serde_json::json!({"b": 1, "a": [1, 2]});
        let b = serde_json::json!({"a": [1, 2], "b": 1});
        assert_eq!(stable_payload_hash(&a), stable_payload_hash(&b));
    }

    #[test]
    fn stable_payload_hash_distinguishes_different_payloads() {
        let a = serde_json::json!({"a": 1});
        let b = serde_json::json!({"a": 2});
        assert_ne!(stable_payload_hash(&a), stable_payload_hash(&b));
    }

    #[tokio::test]
    async fn null_provider_returns_no_change() {
        let provider = NullProvider;
        let pool = sqlx::PgPool::connect_lazy("postgres://nobody").unwrap();
        let finding = provider
            .screen_vendor(&pool, Uuid::nil(), Uuid::nil())
            .await
            .expect("null provider never errors");
        assert!(finding.no_change);
    }
}
