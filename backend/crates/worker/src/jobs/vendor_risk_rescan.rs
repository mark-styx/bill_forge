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
use billforge_vendor_mgmt::federated_risk::{self, FederatedSignalType};
use billforge_vendor_mgmt::ofac_screening::{OfacScreenOutcome, OfacScreener};
use chrono::{Datelike, NaiveDate, Utc};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

/// Days before an expiry where the rescan job starts emitting a `*_expiring`
/// warning. Matches the 30-day reminder cadence in the AP playbook so AP teams
/// have a full month to chase a refreshed form.
const EXPIRY_WARNING_DAYS: i64 = 30;

/// 1099 reporting threshold in cents ($600.00). A vendor whose `ytd_paid_cents`
/// has crossed this AND who has no W-9 on file triggers a
/// `threshold_1099_no_w9` soft hit so AP can chase the W-9 before 1099 season.
const THRESHOLD_1099_CENTS: i64 = 600_00;

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
// VendorRow: the per-vendor projection consumed by the per-page loop. Adds the
// compliance columns (#441) on top of the OFAC + federated-network fields.
// ---------------------------------------------------------------------------

#[derive(Debug, sqlx::FromRow)]
struct VendorRow {
    id: Uuid,
    name: String,
    dba: Option<String>,
    tax_id: Option<String>,
    bank_account_last_four: Option<String>,
    w9_on_file: bool,
    w9_expires_on: Option<NaiveDate>,
    w8_received_date: Option<NaiveDate>,
    w8_expires_on: Option<NaiveDate>,
    coi_expires_on: Option<NaiveDate>,
    is_1099_eligible: bool,
    ytd_paid_cents: i64,
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
        sqlx::query_as("SELECT id::text FROM tenants WHERE is_active = true")
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
    let metadata_pool = pg_manager.metadata().clone();
    let salt = network_hash_salt();
    rescan_tenant_with_provider(
        &pool,
        *tenant_id.as_uuid(),
        &provider,
        Some((&metadata_pool, salt.as_deref())),
    )
    .await
}

/// Read the `NETWORK_HASH_SALT` env var. Returns `None` when the salt is
/// unset so the federated contribution path silently disables itself
/// (the rest of the rescan still runs end-to-end). The API path treats
/// the same condition as a fail-fast error since it would otherwise
/// compute a wrong hash bucket on user-visible reads.
fn network_hash_salt() -> Option<String> {
    std::env::var("NETWORK_HASH_SALT")
        .ok()
        .filter(|v| !v.trim().is_empty())
}

/// Rescan a single tenant using an explicit provider. Exposed so tests can drive
/// the loop with a real PgPool and a deterministic provider.
///
/// `federation` is `Some((metadata_pool, salt))` when the tenant should also
/// contribute hashed signals to the Federated Vendor Risk Network (#408).
/// The contribution is a no-op for tenants without an active opt-in row.
pub async fn rescan_tenant_with_provider(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    provider: &dyn RiskProvider,
    federation: Option<(&sqlx::PgPool, Option<&str>)>,
) -> Result<()> {
    let tenant_id_str = tenant_id.to_string();
    info!(tenant_id = %tenant_id_str, "Rescanning vendor risk");

    // Load the latest persisted SDN list (falls back to embedded seed when the
    // refresh table is empty) so rescans pick up entries added since the seed
    // was bundled instead of re-screening against the same stale snapshot.
    let screener = OfacScreener::load_latest(pool)
        .await
        .unwrap_or_else(|_| OfacScreener::load_from_embedded());

    let today = Utc::now().date_naive();

    // Iterate vendors in pages. Each row carries its dba + tax_id + bank
    // fingerprint so we can hash the vendor identity for the federated
    // contribution path (#408) without exposing raw values.
    let mut offset: i64 = 0;
    loop {
        let page: Vec<VendorRow> = sqlx::query_as(
            r#"
                SELECT id, name, dba, tax_id, bank_account_last_four,
                       w9_on_file, w9_expires_on,
                       w8_received_date, w8_expires_on,
                       coi_expires_on,
                       is_1099_eligible, ytd_paid_cents
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

        for row in &page {
            let VendorRow {
                id: vendor_id,
                name,
                dba,
                tax_id,
                bank_account_last_four: bank_last_four,
                ..
            } = row;
            // 1. OFAC / sanctions re-screen.
            let outcome = screener.screen(name, dba.as_deref());
            if outcome.status != "pass" {
                record_sanctions_alert(pool, tenant_id, *vendor_id, &outcome).await?;
                // Contribute an OFAC near-match signal to the federated
                // network (no-op when the tenant has not opted in).
                if let Some((metadata_pool, Some(salt))) = federation {
                    contribute_federated_signal(
                        metadata_pool,
                        tenant_id,
                        name,
                        tax_id.as_deref(),
                        bank_last_four.as_deref(),
                        FederatedSignalType::OfacNearMatch,
                        salt,
                    )
                    .await;
                }
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
                            // Map provider alert types into federated signal
                            // categories. Other variants are intentionally
                            // skipped: only the four supported network signals
                            // are contributed.
                            if let Some(signal) = map_alert_to_federated(&alert_type) {
                                if let Some((metadata_pool, Some(salt))) = federation {
                                    contribute_federated_signal(
                                        metadata_pool,
                                        tenant_id,
                                        name,
                                        tax_id.as_deref(),
                                        bank_last_four.as_deref(),
                                        signal,
                                        salt,
                                    )
                                    .await;
                                }
                            }
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

            // 3. Compliance watchlist soft hits (#441). These NEVER set
            //    payment_hold; sanctions_hit remains the only kind that does.
            //    Each helper is idempotent on payload_hash so re-runs collapse.
            check_w9_expiry(pool, tenant_id, row, today).await?;
            check_w8_expiry(pool, tenant_id, row, today).await?;
            check_coi_expiry(pool, tenant_id, row, today).await?;
            check_1099_threshold(pool, tenant_id, row).await?;

            // 4. Refresh last_risk_rescan_at for this vendor.
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

// ---------------------------------------------------------------------------
// Compliance watchlist soft-hit writers (#441)
//
// These NEVER set vendors.payment_hold. The contract per the issue is:
//   - sanctions_hit              -> critical, payment_hold=true (hard block)
//   - w9/w8/coi expiry, 1099 thr -> warning/high, payment_hold=false (soft warn)
// The shared `record_soft_alert` writer enforces the soft policy regardless of
// severity, so future callers cannot accidentally promote a soft hit to a hard
// block by setting severity=critical.
// ---------------------------------------------------------------------------

/// W-9 / W-8 / COI expiry alerts: warn when an on-file form is within 30 days
/// of expiry, escalate when already expired. W-9 expired during 1099 season
/// (Nov 1 .. Jan 31) escalates from warning to high so the inbox surfaces it
/// at the same prominence as 1099-threshold misses.
async fn check_w9_expiry(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    vendor: &VendorRow,
    today: NaiveDate,
) -> Result<()> {
    if !vendor.w9_on_file {
        return Ok(());
    }
    let Some(expires_on) = vendor.w9_expires_on else {
        return Ok(());
    };
    let days_until = (expires_on - today).num_days();

    if days_until < 0 {
        // Expired. Escalate during 1099 season; the AP team can no longer
        // file a 1099 for this vendor without a refreshed W-9.
        let severity = if in_1099_season(today) { "high" } else { "medium" };
        let payload = serde_json::json!({
            "expires_on": expires_on.to_string(),
            "days_overdue": -days_until,
            "in_1099_season": severity == "high",
        });
        record_soft_alert(pool, tenant_id, vendor.id, "w9_expired", severity, payload).await?;
    } else if days_until <= EXPIRY_WARNING_DAYS {
        let payload = serde_json::json!({
            "expires_on": expires_on.to_string(),
            "days_until_expiry": days_until,
        });
        record_soft_alert(pool, tenant_id, vendor.id, "w9_expiring", "medium", payload).await?;
    }
    Ok(())
}

async fn check_w8_expiry(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    vendor: &VendorRow,
    today: NaiveDate,
) -> Result<()> {
    // W-8s only exist when a received_date was recorded; gate on that so we
    // don't fire on every vendor that simply has no W-8 (most US vendors).
    if vendor.w8_received_date.is_none() {
        return Ok(());
    }
    let Some(expires_on) = vendor.w8_expires_on else {
        return Ok(());
    };
    let days_until = (expires_on - today).num_days();

    if days_until < 0 {
        let payload = serde_json::json!({
            "expires_on": expires_on.to_string(),
            "days_overdue": -days_until,
        });
        record_soft_alert(pool, tenant_id, vendor.id, "w8_expired", "high", payload).await?;
    } else if days_until <= EXPIRY_WARNING_DAYS {
        let payload = serde_json::json!({
            "expires_on": expires_on.to_string(),
            "days_until_expiry": days_until,
        });
        record_soft_alert(pool, tenant_id, vendor.id, "w8_expiring", "medium", payload).await?;
    }
    Ok(())
}

async fn check_coi_expiry(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    vendor: &VendorRow,
    today: NaiveDate,
) -> Result<()> {
    let Some(expires_on) = vendor.coi_expires_on else {
        return Ok(());
    };
    let days_until = (expires_on - today).num_days();

    if days_until < 0 {
        let payload = serde_json::json!({
            "expires_on": expires_on.to_string(),
            "days_overdue": -days_until,
        });
        record_soft_alert(pool, tenant_id, vendor.id, "coi_expired", "high", payload).await?;
    } else if days_until <= EXPIRY_WARNING_DAYS {
        let payload = serde_json::json!({
            "expires_on": expires_on.to_string(),
            "days_until_expiry": days_until,
        });
        record_soft_alert(pool, tenant_id, vendor.id, "coi_expiring", "medium", payload).await?;
    }
    Ok(())
}

async fn check_1099_threshold(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    vendor: &VendorRow,
) -> Result<()> {
    if !vendor.is_1099_eligible {
        return Ok(());
    }
    if vendor.ytd_paid_cents < THRESHOLD_1099_CENTS {
        return Ok(());
    }
    if vendor.w9_on_file {
        return Ok(());
    }
    let payload = serde_json::json!({
        "ytd_paid_cents": vendor.ytd_paid_cents,
        "threshold_cents": THRESHOLD_1099_CENTS,
    });
    record_soft_alert(
        pool,
        tenant_id,
        vendor.id,
        "threshold_1099_no_w9",
        "high",
        payload,
    )
    .await
}

/// `true` for dates in the IRS 1099 filing window (Nov 1 .. Jan 31), used to
/// escalate expired-W-9 alerts to `high` severity. Outside the window the
/// finding is informational, so we keep it as `warning`.
fn in_1099_season(today: NaiveDate) -> bool {
    matches!(today.month(), 11 | 12 | 1)
}

/// Soft-hit insert: idempotent on (vendor_id, alert_type, payload_hash) like
/// the hard-hit path, but never touches `vendors.payment_hold`. Re-runs of the
/// same finding only update `created_at`-equivalent freshness via the dedupe
/// short-circuit (no row update needed; the open row stands).
async fn record_soft_alert(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    vendor_id: Uuid,
    alert_type: &str,
    severity: &str,
    payload: serde_json::Value,
) -> Result<()> {
    let payload_hash = stable_payload_hash(&payload);
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
    .context("Failed to look up existing open soft alert")?;

    if existing.is_some() {
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
    .context("Failed to insert soft vendor_risk_alert")?;

    Ok(())
}

/// Map a local `vendor_risk_alerts.alert_type` to the federated network
/// signal taxonomy, if any. Variants without a network analogue return
/// `None` so the federated contribution short-circuits.
fn map_alert_to_federated(alert_type: &str) -> Option<FederatedSignalType> {
    match alert_type {
        "sanctions_hit" => Some(FederatedSignalType::OfacNearMatch),
        "banking_change" => Some(FederatedSignalType::BankAccountChange),
        _ => None,
    }
}

/// Contribute a hashed signal to the federated network. Logs and swallows
/// errors so a network outage never blocks the per-tenant rescan loop.
async fn contribute_federated_signal(
    metadata_pool: &sqlx::PgPool,
    tenant_id: Uuid,
    vendor_name: &str,
    tax_id: Option<&str>,
    bank_fingerprint: Option<&str>,
    signal_type: FederatedSignalType,
    network_salt: &str,
) {
    let normalized = federated_risk::normalize_vendor_name(vendor_name);
    let vendor_hash = federated_risk::vendor_hash(
        &normalized,
        tax_id,
        bank_fingerprint,
        network_salt,
    );
    if let Err(e) = federated_risk::contribute_signal(
        metadata_pool,
        tenant_id,
        &vendor_hash,
        signal_type,
        1.0,
        network_salt,
    )
    .await
    {
        warn!(
            tenant_id = %tenant_id,
            signal = %signal_type.as_db_str(),
            error = %e,
            "Failed to contribute federated vendor risk signal"
        );
    }
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
