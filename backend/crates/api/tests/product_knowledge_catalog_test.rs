//! Filesystem-only integrity test for the product knowledge source catalog.
//!
//! Validates that the catalog document exists, contains every required
//! knowledge-area label, and that key referenced source files are present.
//! No database or network access required.

use std::fs;
use std::path::Path;

/// Resolve the repository root relative to the test binary.
///
/// The test binary lives under `backend/target/debug/` (or `release/`),
/// so the repo root is three directories up.
fn repo_root() -> &'static Path {
    Path::new("../../..")
}

#[test]
fn catalog_contains_all_required_knowledge_areas() {
    let catalog_path = repo_root().join("docs/product_knowledge_source_catalog.md");
    let contents = fs::read_to_string(&catalog_path)
        .unwrap_or_else(|e| panic!("catalog missing at {:?}: {e}", catalog_path));

    let required_areas = [
        "Product docs",
        "Implementation plans",
        "Sprint summaries",
        "OpenAPI metadata",
        "Route metadata",
        "Module definitions",
        "Known issues",
        "Release notes",
    ];

    for area in &required_areas {
        assert!(
            contents.contains(area),
            "catalog is missing required knowledge-area label: {area}",
        );
    }
}

#[test]
fn key_referenced_source_files_exist() {
    let root = repo_root();

    let key_files = [
        "docs/known_issues.md",
        "CHANGELOG.md",
        "backend/crates/api/src/openapi.rs",
        "backend/crates/api/src/routes/mod.rs",
        "backend/Cargo.toml",
        "apps/web/package.json",
    ];

    for file in &key_files {
        let path = root.join(file);
        assert!(
            path.exists(),
            "key source file referenced by catalog is missing: {file}",
        );
    }
}
