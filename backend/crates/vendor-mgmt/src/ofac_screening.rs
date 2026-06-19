//! OFAC/SDN sanctions screening for vendor records.
//!
//! Screens vendor names and DBA names against a bundled seed list of SDN entries
//! using exact normalized matching (fail), token Jaccard similarity >= 0.8 or
//! SDN-token-subset match (review), or clean pass. No network IO - the seed list
//! is compiled into the binary via `include_str!`.
//!
//! This module is the shared home used by both the API crate (one-time screening
//! at vendor create/update) and the worker crate (continuous VendorRiskRescan).
//! The API crate re-exports it as `crate::ofac_screening` to keep its existing
//! callsites unchanged.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// A single entry from the bundled OFAC SDN seed list.
#[derive(Debug, Clone, Deserialize)]
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

/// OFAC screener that holds a shared, parsed SDN seed list in memory.
#[derive(Debug, Clone)]
pub struct OfacScreener {
    entries: Arc<Vec<SanctionsEntry>>,
}

impl OfacScreener {
    /// Load the bundled seed JSON at compile time and parse it once.
    /// Panics if the seed file is malformed (a build-time invariant).
    pub fn load_from_embedded() -> Self {
        let json_str = include_str!("../../../data/ofac_sdn_seed.json");
        let entries: Vec<SanctionsEntry> =
            serde_json::from_str(json_str).expect("ofac_sdn_seed.json must be valid JSON");
        Self {
            entries: Arc::new(entries),
        }
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
}
