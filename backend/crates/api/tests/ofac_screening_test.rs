//! Unit tests for OFAC/SDN sanctions screening.
//!
//! Exercises the real bundled seed list via `OfacScreener::load_from_embedded()`.
//! No database required.

use billforge_api::ofac_screening::OfacScreener;

/// Vendor name exactly matches a seeded SDN primary name (different casing/punctuation).
#[test]
fn exact_normalized_match_returns_fail() {
    let screener = OfacScreener::load_from_embedded();

    // "AL-QAEDA" with different casing and hyphen
    let outcome = screener.screen("Al-Qaeda", None);
    assert_eq!(outcome.status, "fail", "exact match should be fail");
    assert!(
        !outcome.matches.is_empty(),
        "should have at least one match"
    );

    // Try another: "GAZPROMBANK" with different casing
    let outcome2 = screener.screen("gazprombank", None);
    assert_eq!(
        outcome2.status, "fail",
        "exact match on primary name should be fail"
    );
}

/// Vendor name exactly matches a seeded alias.
#[test]
fn alias_match_returns_fail() {
    let screener = OfacScreener::load_from_embedded();

    // "COZY BEAR" is an alias of APT29
    let outcome = screener.screen("Cozy Bear", None);
    assert_eq!(outcome.status, "fail", "alias match should be fail");
    assert!(
        outcome
            .matches
            .iter()
            .any(|m| m.matched_name.contains("COZY BEAR")),
        "should match the COZY BEAR alias"
    );

    // "ISIS" is an alias of ISLAMIC STATE OF IRAQ AND THE LEVANT
    let outcome2 = screener.screen("ISIS", None);
    assert_eq!(outcome2.status, "fail", "alias match should be fail");
}

/// Vendor name with one extra token vs. SDN entry triggers review.
#[test]
fn near_match_returns_review() {
    let screener = OfacScreener::load_from_embedded();

    // "GAZPROMBANK MOSCOW" - "gazprombank" is a token in the SDN entry but "moscow" is extra.
    // The SDN tokens {"gazprombank", "joint", "stock", "company"} are NOT a subset of
    // {"gazprombank", "moscow"}, but "gazprombank" is an alias with tokens {"gazprombank"}
    // which IS a subset. So this should be an exact alias match -> fail.
    //
    // Instead use a name where the token overlap is partial but not exact:
    // "LAZARUS GROUP ORGANIZATION" - "lazarus group" is an alias (exact match to alias tokens).
    //
    // Let's use a name that triggers review via token overlap without exact match:
    // "SBERBANK OF RUSSIA AND PARTNERS" - "sberbank of russia" is a primary name.
    // Tokens: {sberbank, of, russia, and, partners} vs SDN {sberbank, of, russia}
    // SDN tokens are a subset of vendor tokens -> review (or fail if primary matches exactly).
    // Actually the normalized primary is "sberbank of russia" and the vendor is "sberbank of russia and partners"
    // so exact match fails, but SDN tokens are a subset -> review.
    let outcome = screener.screen("Sberbank of Russia and Partners", None);
    assert_eq!(
        outcome.status, "review",
        "near match with SDN-token subset should be review, got {:?}",
        outcome
    );
}

/// Clearly unrelated vendor name returns pass with empty matches.
#[test]
fn clean_vendor_returns_pass() {
    let screener = OfacScreener::load_from_embedded();

    let outcome = screener.screen("Acme Coffee LLC", None);
    assert_eq!(outcome.status, "pass", "clean vendor should pass");
    assert!(
        outcome.matches.is_empty(),
        "clean vendor should have no matches"
    );

    let outcome2 = screener.screen("Bob's Plumbing Service", None);
    assert_eq!(outcome2.status, "pass");
    assert!(outcome2.matches.is_empty());
}

/// DBA name matching triggers the same screening as vendor name.
#[test]
fn dba_name_match_returns_fail() {
    let screener = OfacScreener::load_from_embedded();

    // Vendor name is clean but DBA matches an alias
    let outcome = screener.screen("Some Innocent LLC", Some("DarkSide"));
    assert_eq!(outcome.status, "fail", "DBA alias match should be fail");
}

/// AVS/Plaid honest not_configured status in build_screening_results.
#[test]
fn build_screening_results_returns_not_configured_for_avs_plaid() {
    let signals = billforge_api::fraud_guard::FraudSignals {
        domain_age: billforge_api::fraud_guard::DomainAgeSignal {
            risk: billforge_api::fraud_guard::RiskLevel::Low,
            domain: "test.com".to_string(),
            first_seen_at: None,
            days_since_first_seen: Some(100),
        },
        lookalike: billforge_api::fraud_guard::LookalikeSignal {
            risk: billforge_api::fraud_guard::RiskLevel::Low,
            top_match: None,
        },
        bank_change: billforge_api::fraud_guard::BankChangeSignal {
            risk: billforge_api::fraud_guard::RiskLevel::Low,
            recent_changes: 0,
        },
        country_mismatch: billforge_api::fraud_guard::CountrySignal {
            risk: billforge_api::fraud_guard::RiskLevel::Unknown,
            vendor_country: None,
            bank_country: None,
        },
        overall_risk: billforge_api::fraud_guard::RiskLevel::Low,
    };

    let screener = OfacScreener::load_from_embedded();
    let json = billforge_api::fraud_guard::build_screening_results(
        &signals,
        &screener,
        "Clean Vendor Name",
        None,
    );

    // AVS should be not_configured
    let avs = json.get("avs").expect("should have avs key");
    assert_eq!(
        avs["status"], "not_configured",
        "avs status should be not_configured"
    );

    // Plaid should be not_configured
    let plaid = json.get("plaid").expect("should have plaid key");
    assert_eq!(
        plaid["status"], "not_configured",
        "plaid status should be not_configured"
    );

    // OFAC should be pass for a clean name
    let ofac = json.get("ofac").expect("should have ofac key");
    assert_eq!(
        ofac["status"], "pass",
        "ofac status should be pass for clean name"
    );
}
