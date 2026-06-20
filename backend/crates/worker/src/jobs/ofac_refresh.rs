//! Background job: scheduled OFAC/SDN list refresh (refs #395)
//!
//! Closes the gap that screening previously ran against a hardcoded embedded
//! seed (`list_version = "seed-v1"`) with no refresh job, no `last_refreshed`
//! tracking, and no staleness signal, so a vendor sanctioned after the seed
//! was bundled would pass screening indefinitely.
//!
//! The scheduler calls `run_ofac_refresh` directly against a tenant pool every
//! 24h (no `JobType` enum variant because there is no per-tenant payload to
//! enqueue: the SDN list is a single global resource, just persisted into each
//! tenant DB so the existing per-tenant screener reads continue to work without
//! a cross-pool fetch).
//!
//! Each refresh:
//!   1. Reads `OFAC_SDN_SOURCE_URL` (optional). When unset the embedded seed is
//!      re-parsed so the closed loop (refresh -> persist -> reload) still runs
//!      locally and the staleness pipeline can be exercised end-to-end.
//!   2. Computes a content hash over sorted `sdn_id`s; uses the first 12 chars
//!      as `list_version` (e.g. `"sdn-3a1f9c0e7b22"`).
//!   3. Skips the insert when the latest stored row already has the same hash;
//!      this keeps `loaded_at` from drifting on no-op refreshes.
//!   4. Emits a `tracing::warn!` when the most-recent loaded list is older than
//!      `OFAC_LIST_MAX_AGE_DAYS` (default 7). This surfaces stalled refreshes
//!      without yet plumbing into `vendor_risk_alerts` (out of scope).

use anyhow::{Context, Result};
use billforge_vendor_mgmt::ofac_screening::{OfacScreener, SanctionsEntry};
use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tracing::{info, warn};

/// Default staleness budget when `OFAC_LIST_MAX_AGE_DAYS` is unset.
const DEFAULT_MAX_AGE_DAYS: i64 = 7;

#[derive(Debug, Clone)]
pub struct RefreshOutcome {
    pub version: String,
    pub entry_count: usize,
    /// `true` when a new row was inserted; `false` when the latest row already
    /// had the same content hash (no-op refresh).
    pub inserted: bool,
}

/// Run a single OFAC refresh cycle against `pool`. Returns the resulting
/// version + whether a new row was persisted. Always emits a stale-list
/// warning when the latest row is older than the configured budget so the
/// signal exists even when the worker hits the no-op path.
pub async fn run_ofac_refresh(pool: &PgPool) -> Result<RefreshOutcome> {
    let source_url = std::env::var("OFAC_SDN_SOURCE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty());

    let entries = fetch_entries(source_url.as_deref()).await?;
    if entries.is_empty() {
        anyhow::bail!("OFAC refresh produced zero entries");
    }

    let version = list_version_from_entries(&entries);
    let source_label = source_url
        .as_deref()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "embedded".to_string());

    let latest: Option<(String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT list_version, loaded_at FROM ofac_list_versions ORDER BY loaded_at DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .context("Failed to read latest ofac_list_versions row")?;

    let inserted = match latest.as_ref() {
        Some((existing_version, _)) if existing_version == &version => false,
        _ => {
            sqlx::query(
                r#"
                INSERT INTO ofac_list_versions
                    (list_version, entry_count, entries_json, source)
                VALUES ($1, $2, $3, $4)
                "#,
            )
            .bind(&version)
            .bind(entries.len() as i32)
            .bind(serde_json::to_value(&entries).context("Failed to serialize entries")?)
            .bind(&source_label)
            .execute(pool)
            .await
            .context("Failed to insert ofac_list_versions row")?;
            true
        }
    };

    let max_age = max_age_from_env();
    if let Some((_, loaded_at)) = latest {
        if !inserted && Utc::now() - loaded_at > max_age {
            warn!(
                latest_version = %version,
                loaded_at = %loaded_at,
                "OFAC list is stale: refresh produced no new content and the persisted list is older than the staleness budget"
            );
        }
    }

    info!(
        version = %version,
        entry_count = entries.len(),
        inserted,
        "OFAC refresh complete"
    );

    Ok(RefreshOutcome {
        version,
        entry_count: entries.len(),
        inserted,
    })
}

/// Fetch entries from a remote JSON URL or fall back to the embedded seed.
/// HTTP errors and parse failures propagate so the scheduler can log them.
async fn fetch_entries(source_url: Option<&str>) -> Result<Vec<SanctionsEntry>> {
    let Some(url) = source_url else {
        return Ok(OfacScreener::embedded_entries());
    };

    let body = reqwest::get(url)
        .await
        .with_context(|| format!("HTTP GET failed for OFAC source {url}"))?
        .error_for_status()
        .with_context(|| format!("OFAC source returned error status: {url}"))?
        .text()
        .await
        .context("Failed to read OFAC source response body")?;

    let entries = OfacScreener::parse_entries_json(&body)
        .with_context(|| format!("Failed to parse OFAC source JSON from {url}"))?;
    Ok(entries)
}

/// Deterministic content hash over sorted SDN ids. Truncated to 12 hex chars
/// for a readable `list_version` while still distinguishing real content
/// changes. Equivalent inputs always produce the same version.
fn list_version_from_entries(entries: &[SanctionsEntry]) -> String {
    let mut ids: Vec<&str> = entries.iter().map(|e| e.sdn_id.as_str()).collect();
    ids.sort_unstable();
    let mut hasher = Sha256::new();
    for id in &ids {
        hasher.update(id.as_bytes());
        hasher.update([0u8]);
    }
    let digest = hasher.finalize();
    let hex = hex::encode(digest);
    format!("sdn-{}", &hex[..12])
}

fn max_age_from_env() -> Duration {
    std::env::var("OFAC_LIST_MAX_AGE_DAYS")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .map(Duration::days)
        .unwrap_or_else(|| Duration::days(DEFAULT_MAX_AGE_DAYS))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(sdn_id: &str) -> SanctionsEntry {
        SanctionsEntry {
            primary_name: format!("Entity {sdn_id}"),
            aliases: vec![],
            sdn_id: sdn_id.to_string(),
            list: "SDN".to_string(),
        }
    }

    #[test]
    fn list_version_is_stable_across_ordering() {
        let a = vec![entry("SDN-001"), entry("SDN-002"), entry("SDN-003")];
        let b = vec![entry("SDN-003"), entry("SDN-001"), entry("SDN-002")];
        assert_eq!(list_version_from_entries(&a), list_version_from_entries(&b));
    }

    #[test]
    fn list_version_changes_when_entries_change() {
        let a = vec![entry("SDN-001"), entry("SDN-002")];
        let b = vec![entry("SDN-001"), entry("SDN-002"), entry("SDN-003")];
        assert_ne!(list_version_from_entries(&a), list_version_from_entries(&b));
    }

    #[test]
    fn list_version_uses_sdn_prefix_and_short_hex() {
        let a = vec![entry("SDN-001")];
        let v = list_version_from_entries(&a);
        assert!(v.starts_with("sdn-"));
        // 4-char prefix + 12-char hex.
        assert_eq!(v.len(), 16);
    }

    #[tokio::test]
    async fn fetch_entries_returns_embedded_when_url_missing() {
        let entries = fetch_entries(None).await.expect("embedded fetch succeeds");
        assert!(
            !entries.is_empty(),
            "embedded seed must yield at least one entry"
        );
    }
}
