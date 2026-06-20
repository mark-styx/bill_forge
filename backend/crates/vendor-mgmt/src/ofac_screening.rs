//! OFAC/SDN sanctions screening for vendor records.
//!
//! Screens vendor names and DBA names against a bundled seed list of SDN entries
//! using exact normalized matching (fail), token Jaccard similarity >= 0.8 or
//! SDN-token-subset match (review), or clean pass. The screener now tracks the
//! list version it loaded and when, so callers can surface staleness instead of
//! silently screening against a months-old snapshot. `load_latest` reads the most
//! recent row from `ofac_list_versions`; the daily worker refresh writes new
//! rows from the configured source (or the embedded list when no URL is set).
//!
//! This module is the shared home used by both the API crate (one-time screening
//! at vendor create/update) and the worker crate (continuous VendorRiskRescan).
//! The API crate re-exports it as `crate::ofac_screening` to keep its existing
//! callsites unchanged.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// A single entry from the OFAC SDN list. Used for both the embedded seed and
/// rows loaded from `ofac_list_versions.entries_json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanctionsEntry {
    pub primary_name: String,
    pub aliases: Vec<String>,
    pub sdn_id: String,
    pub list: String,
}

/// A match hit returned when screening flags a vendor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfacMatch {
    pub sdn_id: String,
    pub matched_name: String,
    pub score: f64,
}

/// Screening outcome for a vendor name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfacScreenOutcome {
    pub status: &'static str, // "pass" | "review" | "fail"
    pub matches: Vec<OfacMatch>,
}

// ---------------------------------------------------------------------------
// Screener
// ---------------------------------------------------------------------------

/// OFAC screener that holds a shared, parsed SDN list in memory along with the
/// version identifier and load timestamp of the list it is screening against.
#[derive(Debug, Clone)]
pub struct OfacScreener {
    entries: Arc<Vec<SanctionsEntry>>,
    list_version: String,
    loaded_at: DateTime<Utc>,
}

/// Default list version used when falling back to the compiled-in seed.
pub const EMBEDDED_LIST_VERSION: &str = "seed-v1";

impl OfacScreener {
    /// Load the bundled seed JSON at compile time and parse it once.
    /// Panics if the seed file is malformed (a build-time invariant).
    pub fn load_from_embedded() -> Self {
        let entries = parse_embedded_entries();
        Self {
            entries: Arc::new(entries),
            list_version: EMBEDDED_LIST_VERSION.to_string(),
            loaded_at: Utc::now(),
        }
    }

    /// Load the most recent OFAC list version persisted in `ofac_list_versions`.
    /// Falls back to the embedded seed when the table is empty, missing, or the
    /// stored row cannot be parsed - this keeps the screener available on cold
    /// start and never blocks vendor creation on a refresh outage.
    pub async fn load_latest(pool: &sqlx::PgPool) -> sqlx::Result<Self> {
        let row: Option<(String, DateTime<Utc>, serde_json::Value)> = sqlx::query_as(
            r#"
            SELECT list_version, loaded_at, entries_json
            FROM ofac_list_versions
            ORDER BY loaded_at DESC
            LIMIT 1
            "#,
        )
        .fetch_optional(pool)
        .await
        .unwrap_or(None);

        let Some((version, loaded_at, entries_json)) = row else {
            return Ok(Self::load_from_embedded());
        };

        let entries: Vec<SanctionsEntry> =
            serde_json::from_value(entries_json).unwrap_or_default();
        if entries.is_empty() {
            return Ok(Self::load_from_embedded());
        }

        Ok(Self {
            entries: Arc::new(entries),
            list_version: version,
            loaded_at,
        })
    }

    /// Construct a screener from an explicit set of entries + version metadata.
    /// Used by the refresh job after it parses a remote list, and by tests that
    /// need to assert staleness without touching a database.
    pub fn from_entries(
        entries: Vec<SanctionsEntry>,
        list_version: impl Into<String>,
        loaded_at: DateTime<Utc>,
    ) -> Self {
        Self {
            entries: Arc::new(entries),
            list_version: list_version.into(),
            loaded_at,
        }
    }

    /// Version identifier of the currently loaded list (e.g. `"seed-v1"` or
    /// `"sdn-<hash>"`). Surfaced in screening_results so callers can audit which
    /// list version a given screen was performed against.
    pub fn list_version(&self) -> &str {
        &self.list_version
    }

    /// Timestamp the loaded list was persisted/embedded. Combined with
    /// `is_stale()` this is the per-result staleness surface.
    pub fn loaded_at(&self) -> DateTime<Utc> {
        self.loaded_at
    }

    /// Returns true when the list is older than `max_age`. Used by the refresh
    /// job to emit a staleness warning when nothing has refreshed in time.
    pub fn is_stale(&self, max_age: Duration) -> bool {
        Utc::now() - self.loaded_at > max_age
    }

    /// Re-parse the compiled-in seed JSON into a fresh `Vec<SanctionsEntry>`.
    /// Used by the worker's refresh job when no remote source URL is configured
    /// so the closed loop (refresh -> persist -> reload) still runs locally and
    /// exercises the staleness pipeline.
    pub fn embedded_entries() -> Vec<SanctionsEntry> {
        parse_embedded_entries()
    }

    /// Parse a Vec<SanctionsEntry> from a JSON body, e.g. the response of a
    /// remote SDN source fetch. Kept here so the parsing contract lives next to
    /// the embedded seed it must stay compatible with.
    pub fn parse_entries_json(json: &str) -> Result<Vec<SanctionsEntry>, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Screen a vendor name (and optional DBA) against the SDN seed list.
    ///
    /// Returns:
    /// - `fail` if the normalized vendor name exactly matches any SDN primary name or alias.
    /// - `review` if token Jaccard similarity >= 0.8 OR all SDN tokens are a subset of vendor tokens.
    /// - `pass` otherwise.
    pub fn screen(&self, vendor_name: &str, dba: Option<&str>) -> OfacScreenOutcome {
        let vendor_norm = normalize(vendor_name);
        let vendor_tokens = tokenize(&vendor_norm);

        let dba_norm = dba.map(normalize);
        let dba_tokens: Option<HashSet<&str>> = dba_norm.as_ref().map(|d| tokenize(d));

        let mut matches: Vec<OfacMatch> = Vec::new();

        for entry in self.entries.iter() {
            let primary_norm = normalize(&entry.primary_name);
            let primary_tokens = tokenize(&primary_norm);

            // Check primary name exact match
            if vendor_norm == primary_norm || dba_norm.as_ref().is_some_and(|d| *d == primary_norm)
            {
                let score = token_jaccard(&vendor_tokens, &primary_tokens).max(
                    dba_tokens
                        .as_ref()
                        .map_or(0.0, |dt| token_jaccard(dt, &primary_tokens)),
                );
                matches.push(OfacMatch {
                    sdn_id: entry.sdn_id.clone(),
                    matched_name: entry.primary_name.clone(),
                    score: score.max(1.0),
                });
                continue;
            }

            // Check alias exact matches
            for alias in &entry.aliases {
                let alias_norm = normalize(alias);
                if vendor_norm == alias_norm || dba_norm.as_ref().is_some_and(|d| *d == alias_norm)
                {
                    let alias_tokens = tokenize(&alias_norm);
                    let score = token_jaccard(&vendor_tokens, &alias_tokens).max(
                        dba_tokens
                            .as_ref()
                            .map_or(0.0, |dt| token_jaccard(dt, &alias_tokens)),
                    );
                    matches.push(OfacMatch {
                        sdn_id: entry.sdn_id.clone(),
                        matched_name: alias.clone(),
                        score: score.max(1.0),
                    });
                    break;
                }
            }

            // If exact match already found for this entry, skip fuzzy
            if matches.iter().any(|m| m.sdn_id == entry.sdn_id) {
                continue;
            }

            // Fuzzy check: token Jaccard >= 0.8 OR SDN tokens are subset of vendor tokens
            let vendor_jaccard = token_jaccard(&vendor_tokens, &primary_tokens);
            let dba_jaccard = dba_tokens
                .as_ref()
                .map_or(0.0, |dt| token_jaccard(dt, &primary_tokens));
            let best_jaccard = vendor_jaccard.max(dba_jaccard);

            let vendor_subset = primary_tokens.is_subset(&vendor_tokens);
            let dba_subset = dba_tokens
                .as_ref()
                .is_some_and(|dt| primary_tokens.is_subset(dt));
            let is_subset = vendor_subset || dba_subset;

            if best_jaccard >= 0.8 || is_subset {
                matches.push(OfacMatch {
                    sdn_id: entry.sdn_id.clone(),
                    matched_name: entry.primary_name.clone(),
                    score: best_jaccard,
                });
            }
        }

        let has_exact = matches.iter().any(|m| m.score >= 1.0);
        let status = if has_exact {
            "fail"
        } else if !matches.is_empty() {
            "review"
        } else {
            "pass"
        };

        OfacScreenOutcome { status, matches }
    }

    /// Convenience wrapper that screens a vendor by id, fetching its name + dba
    /// from the tenant pool. This is the entry point the VendorRiskRescan worker
    /// calls per vendor.
    pub async fn screen_vendor(
        &self,
        pool: &sqlx::PgPool,
        tenant_id: uuid::Uuid,
        vendor_id: uuid::Uuid,
    ) -> sqlx::Result<OfacScreenOutcome> {
        let row: Option<(String, Option<String>)> =
            sqlx::query_as("SELECT name, dba FROM vendors WHERE id = $1 AND tenant_id = $2")
                .bind(vendor_id)
                .bind(tenant_id)
                .fetch_optional(pool)
                .await?;

        match row {
            Some((name, dba)) => Ok(self.screen(&name, dba.as_deref())),
            None => Ok(OfacScreenOutcome {
                status: "pass",
                matches: Vec::new(),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse the compiled-in seed JSON into a Vec<SanctionsEntry>. Panics if the
/// seed file is malformed (a build-time invariant).
fn parse_embedded_entries() -> Vec<SanctionsEntry> {
    let json_str = include_str!("../../../data/ofac_sdn_seed.json");
    serde_json::from_str(json_str).expect("ofac_sdn_seed.json must be valid JSON")
}

/// Normalize a name: lowercase, strip punctuation, collapse whitespace.
fn normalize(s: &str) -> String {
    let lower = s.to_lowercase();
    let stripped: String = lower
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect();
    let mut result = String::with_capacity(stripped.len());
    let mut prev_space = true; // trim leading
    for c in stripped.chars() {
        if c == ' ' {
            if !prev_space {
                result.push(' ');
                prev_space = true;
            }
        } else {
            result.push(c);
            prev_space = false;
        }
    }
    if result.ends_with(' ') {
        result.pop();
    }
    result
}

/// Split a normalized string into unique token set.
fn tokenize(normalized: &str) -> HashSet<&str> {
    normalized.split_whitespace().collect()
}

/// Compute Jaccard similarity between two token sets.
fn token_jaccard<'a>(a: &HashSet<&'a str>, b: &HashSet<&'a str>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let intersection = a.intersection(b).count() as f64;
    let union = a.union(b).count() as f64;
    if union == 0.0 {
        return 0.0;
    }
    intersection / union
}
// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_punctuation_and_lowercases() {
        assert_eq!(normalize("AL-QAEDA"), "al qaeda");
        assert_eq!(normalize("  Foo   Bar, Inc.  "), "foo bar inc");
        assert_eq!(normalize("O'Brien & Co."), "o brien co");
    }

    #[test]
    fn tokenize_splits_into_unique_set() {
        let tokens = tokenize("foo bar foo baz");
        assert_eq!(tokens.len(), 3);
        assert!(tokens.contains("foo"));
        assert!(tokens.contains("bar"));
        assert!(tokens.contains("baz"));
    }

    #[test]
    fn jaccard_identical_sets_is_one() {
        let a = tokenize("alpha beta gamma");
        let b = tokenize("alpha beta gamma");
        assert!((token_jaccard(&a, &b) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn jaccard_disjoint_sets_is_zero() {
        let a = tokenize("alpha beta");
        let b = tokenize("gamma delta");
        assert!((token_jaccard(&a, &b)).abs() < f64::EPSILON);
    }

    #[test]
    fn screener_loads_embedded_seed() {
        // The seed file must parse and contain at least one entry.
        let screener = OfacScreener::load_from_embedded();
        assert!(!screener.entries.is_empty(), "OFAC seed list is empty");
    }

    #[test]
    fn embedded_load_carries_seed_version() {
        let screener = OfacScreener::load_from_embedded();
        assert_eq!(screener.list_version(), EMBEDDED_LIST_VERSION);
        // loaded_at is bounded by the test's wall clock.
        assert!(screener.loaded_at() <= Utc::now());
    }

    #[test]
    fn is_stale_returns_true_after_max_age() {
        let stale_loaded_at = Utc::now() - Duration::days(8);
        let screener =
            OfacScreener::from_entries(parse_embedded_entries(), "sdn-test", stale_loaded_at);
        assert!(
            screener.is_stale(Duration::days(7)),
            "list 8 days old must be stale vs 7-day budget"
        );
    }

    #[test]
    fn is_stale_returns_false_within_max_age() {
        let fresh_loaded_at = Utc::now() - Duration::hours(1);
        let screener =
            OfacScreener::from_entries(parse_embedded_entries(), "sdn-test", fresh_loaded_at);
        assert!(
            !screener.is_stale(Duration::days(7)),
            "list 1 hour old must not be stale vs 7-day budget"
        );
    }
}
