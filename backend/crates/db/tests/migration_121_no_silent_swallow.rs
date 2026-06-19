//! Regression tests for migration 121 (RLS on tenant_db-created tables).
//!
//! Issue #369: every DO block in migration 121 ended with
//! `EXCEPTION WHEN OTHERS THEN NULL`, which silently swallowed any failure of
//! `ENABLE/FORCE ROW LEVEL SECURITY`, `DROP/CREATE POLICY`, or the final
//! `GRANT`. The migration would report success while tenant isolation was
//! silently downgraded on `audit_log`, `vendor_bank_accounts`,
//! `vendor_statements`, etc.
//!
//! These tests lock in the fix:
//!  1. `migration_121_does_not_silently_swallow_rls_errors` (no DB required):
//!     asserts the committed migration SQL contains zero blanket
//!     `EXCEPTION WHEN OTHERS THEN NULL` handlers, while the per-table
//!     `IF EXISTS` guards that legitimately skip lazy tenant tables remain.
//!  2. `migration_121_aborts_when_rls_precondition_broken` (`#[ignore]`):
//!     applies migration 121 against a real tenant DB whose `documents`
//!     precondition is broken and asserts the migration aborts loudly.

use billforge_core::TenantId;
use billforge_db::PgManager;
use uuid::Uuid;

/// The committed migration source. `include_str!` re-reads this file from
/// disk on every recompile, so the static check always reflects the current
/// source tree, not a stale snapshot.
const MIGRATION_121_SQL: &str =
    include_str!("../../../migrations/121_enable_rls_tenant_db_tables.sql");

/// The anti-pattern that must never return to migration 121. Case-insensitive
/// so variant capitalisations (`exception when others then null`) are caught.
const SWALLOW: &str = "exception when others then null";

// ===========================================================================
// Static-source regression check (runs without a database)
// ===========================================================================

#[test]
fn migration_121_does_not_silently_swallow_rls_errors() {
    // 1. No blanket `EXCEPTION WHEN OTHERS THEN NULL` may remain anywhere in
    //    the migration. Each DO block now propagates any RLS-application
    //    failure so the migration aborts loudly instead of reporting success.
    let count = MIGRATION_121_SQL.to_lowercase().matches(SWALLOW).count();
    assert_eq!(
        count, 0,
        "migration 121 must not swallow RLS errors with blanket \
         `EXCEPTION WHEN OTHERS THEN NULL`, but found {count} occurrence(s). \
         Reintroducing the swallow silently downgrades tenant isolation (issue #369).\n\
         --- migration source ---\n{MIGRATION_121_SQL}"
    );

    // 2. The per-table `IF EXISTS (SELECT 1 FROM pg_class WHERE relname = ...)`
    //    guards must still be present for every tenant table. They preserve the
    //    only legitimate no-op case: a lazy-created tenant table that has not
    //    yet been provisioned. There are 13 tenant tables in migration 121.
    let guard_count = MIGRATION_121_SQL
        .matches("IF EXISTS (SELECT 1 FROM pg_class WHERE relname =")
        .count();
    assert_eq!(
        guard_count, 13,
        "expected 13 per-table existence guards in migration 121 (one per \
         tenant table), found {guard_count}. Removing the swallow must not \
         also remove the lazy-table skip-if-absent guard."
    );

    // 3. The final load-bearing GRANT to the app role must remain; it must no
    //    longer be masked by a swallow now that migration 120 (which creates
    //    `billforge_app`) is its declared prerequisite.
    assert!(
        MIGRATION_121_SQL.contains(
            "GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO billforge_app"
        ),
        "migration 121 must still grant privileges to billforge_app"
    );
}

// ===========================================================================
// Integration check (requires a live Postgres tenant DB; `#[ignore]` by
// default, matching the policy used by rls_tenant_db_tables.rs).
//
// Run with:
//   cargo test -p billforge-db --test migration_121_no_silent_swallow -- --ignored
// ===========================================================================

#[tokio::test]
#[ignore = "requires a live Postgres tenant DB (TEST_DATABASE_URL); see rls_tenant_db_tables.rs"]
async fn migration_121_aborts_when_rls_precondition_broken() {
    let metadata_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/billforge_test".to_string()
    });
    let tenant_template = std::env::var("TEST_TENANT_DB_TEMPLATE")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/{database}".to_string());

    let manager = PgManager::new(&metadata_url, tenant_template)
        .await
        .expect("PgManager");

    let tenant_id: TenantId = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"m121-no-silent-swallow")
        .to_string()
        .parse()
        .unwrap();

    // Start from a clean, fully-migrated tenant DB so the `documents` table
    // exists and migration 121 has already been recorded as applied.
    manager.delete_tenant(&tenant_id).await.ok();
    manager
        .create_tenant(&tenant_id, "M121 No-Silent-Swallow Tenant")
        .await
        .expect("create tenant");

    let admin_pool = (*manager.tenant(&tenant_id).await.expect("tenant pool")).clone();
    manager
        .run_tenant_migrations(&admin_pool)
        .await
        .expect("baseline tenant migrations");

    // Break the precondition that the `documents` RLS policy depends on:
    // drop the `tenant_id` column. CASCADE removes the existing
    // `rls_tenant_documents` policy so the column drop itself succeeds. The
    // `documents` table still exists, so migration 121's `IF EXISTS` guard
    // will take the policy-creation branch on re-run.
    sqlx::query("ALTER TABLE documents DROP COLUMN tenant_id CASCADE")
        .execute(&admin_pool)
        .await
        .expect("drop documents.tenant_id to break RLS precondition");

    // Re-execute migration 121's SQL directly (bypassing MigrationRunner's
    // already-applied tracking) so the documents DO block actually runs.
    // With the swallow removed, `CREATE POLICY ... USING (tenant_id = ...)`
    // must fail loudly because the column no longer exists.
    let result = sqlx::raw_sql(MIGRATION_121_SQL).execute(&admin_pool).await;

    assert!(
        result.is_err(),
        "migration 121 must abort when an RLS precondition is broken, but it \
         reported success. This means a blanket EXCEPTION swallow was \
         reintroduced, silently downgrading tenant isolation (issue #369). \
         Underlying error if any: {:?}",
        result.as_ref().err()
    );

    manager.delete_tenant(&tenant_id).await.ok();
}
