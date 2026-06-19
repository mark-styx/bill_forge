//! Feature-gating metadata tests for issue #365.
//!
//! The seven pillar/business crates (capture, processing, vendor-mgmt, reporting,
//! billing, ai-agent, analytics) must be declared as *optional* path dependencies
//! of `billforge-api`, with one Cargo feature per pillar, and all seven must be
//! included in the `default` feature set so existing deployments are unaffected.
//!
//! This is a compile-time metadata contract: it is what makes a single-pillar
//! (e.g. Capture-only) binary buildable via `--no-default-features --features capture`.
//!
//! The crate manifest is embedded at compile time with `include_str!`, so no extra
//! test dependency (e.g. `toml`/`cargo_metadata`) is required.

/// The seven pillar crates that must be optional compile dependencies.
const PILLAR_CRATES: &[&str] = &[
    "billforge-invoice-capture",
    "billforge-invoice-processing",
    "billforge-vendor-mgmt",
    "billforge-reporting",
    "billforge-billing",
    "billforge-ai-agent",
    "billforge-analytics",
];

/// The Cargo feature name backed by each pillar crate (matches the `dep:` mapping).
const PILLAR_FEATURES: &[&str] = &[
    "capture",
    "processing",
    "vendor-mgmt",
    "reporting",
    "billing",
    "ai-agent",
    "analytics",
];

const MANIFEST: &str = include_str!("../Cargo.toml");

/// Return the single manifest line that declares `crate_name` in `[dependencies]`.
///
/// Dependency declarations look like:
///     billforge-billing = { path = "../billing", optional = true }
fn dep_line(crate_name: &str) -> String {
    let needle = format!("{crate_name} = ");
    let line = MANIFEST
        .lines()
        .find(|l| l.trim_start().starts_with(&needle))
        .unwrap_or_else(|| panic!("dependency declaration for `{crate_name}` not found"));
    line.trim().to_string()
}

#[test]
fn pillar_crates_are_optional_dependencies() {
    for crate_name in PILLAR_CRATES {
        let line = dep_line(crate_name);
        assert!(
            line.contains("optional = true"),
            "pillar crate `{crate_name}` must be marked `optional = true`; got: {line}"
        );
        assert!(
            line.contains("path ="),
            "pillar crate `{crate_name}` should remain a path dependency; got: {line}"
        );
    }
}

#[test]
fn each_pillar_feature_maps_to_its_crate() {
    let features_section = section_after("[features]");
    for (crate_name, feature) in PILLAR_CRATES.iter().zip(PILLAR_FEATURES.iter()) {
        let expected = format!("{feature} = [\"dep:{crate_name}\"]");
        assert!(
            features_section.lines().any(|l| l.trim() == expected),
            "expected feature declaration `{expected}` not found in [features]"
        );
    }
}

#[test]
fn default_includes_all_pillar_features() {
    let features_section = section_after("[features]");
    let default_line = features_section
        .lines()
        .find(|l| l.trim_start().starts_with("default = "))
        .expect("no `default = [...]` line in [features]");
    for feature in PILLAR_FEATURES {
        let quoted = format!("\"{feature}\"");
        assert!(
            default_line.contains(&quoted),
            "default feature set must include `{feature}`; got: {default_line}"
        );
    }
}

#[test]
fn pillar_features_are_disabled_under_no_default_features() {
    // Sanity check: the feature names we assert on are the real, declared feature
    // names (not typos). Each must appear as a standalone feature key in [features].
    let features_section = section_after("[features]");
    for feature in PILLAR_FEATURES {
        let key = format!("{feature} = ");
        assert!(
            features_section
                .lines()
                .any(|l| l.trim_start().starts_with(&key)),
            "no feature named `{feature}` declared in [features]"
        );
    }
}

/// Return the contents of `Cargo.toml` from a section header (e.g. `[features]`)
/// up to the next top-level `[section]` header.
fn section_after(header: &str) -> String {
    let mut out = String::new();
    let mut in_section = false;
    for line in MANIFEST.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if trimmed == header {
                in_section = true;
                continue;
            } else if in_section {
                break;
            }
        } else if in_section {
            out.push_str(line);
            out.push('\n');
        }
    }
    assert!(!out.is_empty(), "section {header} not found in manifest");
    out
}
