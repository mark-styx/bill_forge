//! Integration tests for QuickBooks vendor sync background job (#138).
//!
//! Tests the database interactions that `sync_vendors` relies on by running
//! the SQL directly against a test database. The full `sync_vendors` function
//! requires a `PgManager` with multi-tenant routing, so we test the SQL
//! queries in isolation here and verify the config/stub behavior separately.

use sqlx::PgPool;
use uuid::Uuid;

/// Insert a row into the tenants table so FK constraints on tenant_id are satisfied.
async fn seed_tenant(pool: &PgPool, tenant_id: Uuid) -> sqlx::Result<()> {
    let slug = format!("test-{}", tenant_id);
    sqlx::query(
        "INSERT INTO tenants (id, name, slug) VALUES ($1, 'Test Tenant', $2)",
    )
    .bind(tenant_id)
    .bind(&slug)
    .execute(pool)
    .await?;
    Ok(())
}

/// Verify that a quickbooks_sync_log row can be inserted and transitioned
/// from 'running' -> 'completed' with record counts, matching the pattern
/// used by the worker's sync_vendors function.
#[sqlx::test(migrations = "../../migrations")]
async fn sync_log_lifecycle(pool: PgPool) -> sqlx::Result<()> {
    let tenant_id = Uuid::new_v4();
    let sync_id = Uuid::new_v4();

    seed_tenant(&pool, tenant_id).await?;

    // Insert running
    sqlx::query(
        "INSERT INTO quickbooks_sync_log (id, tenant_id, sync_type, status, started_at) \
         VALUES ($1, $2, 'vendors', 'running', NOW())",
    )
    .bind(sync_id)
    .bind(tenant_id)
    .execute(&pool)
    .await?;

    let status: String =
        sqlx::query_scalar("SELECT status FROM quickbooks_sync_log WHERE id = $1")
            .bind(sync_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(status, "running");

    // Transition to completed
    sqlx::query(
        "UPDATE quickbooks_sync_log \
         SET status = 'completed', completed_at = NOW(), records_processed = 5, records_created = 3, records_updated = 2 \
         WHERE id = $1",
    )
    .bind(sync_id)
    .execute(&pool)
    .await?;

    let (status, processed, created, updated): (String, i32, i32, i32) = sqlx::query_as(
        "SELECT status, records_processed, records_created, records_updated FROM quickbooks_sync_log WHERE id = $1",
    )
    .bind(sync_id)
    .fetch_one(&pool)
    .await?;

    assert_eq!(status, "completed");
    assert_eq!(processed, 5);
    assert_eq!(created, 3);
    assert_eq!(updated, 2);

    Ok(())
}

/// Verify that a quickbooks_sync_log row can be transitioned to 'failed'
/// status, matching the error path in the worker's sync_vendors.
#[sqlx::test(migrations = "../../migrations")]
async fn sync_log_failed_status(pool: PgPool) -> sqlx::Result<()> {
    let tenant_id = Uuid::new_v4();
    let sync_id = Uuid::new_v4();

    seed_tenant(&pool, tenant_id).await?;

    sqlx::query(
        "INSERT INTO quickbooks_sync_log (id, tenant_id, sync_type, status, started_at) \
         VALUES ($1, $2, 'vendors', 'running', NOW())",
    )
    .bind(sync_id)
    .bind(tenant_id)
    .execute(&pool)
    .await?;

    sqlx::query(
        "UPDATE quickbooks_sync_log SET status = 'failed', completed_at = NOW() WHERE id = $1",
    )
    .bind(sync_id)
    .execute(&pool)
    .await?;

    let status: String =
        sqlx::query_scalar("SELECT status FROM quickbooks_sync_log WHERE id = $1")
            .bind(sync_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(status, "failed");

    Ok(())
}

/// Verify that quickbooks_connections lookup by tenant_id with sync_enabled=true
/// returns the expected columns (company_id, access_token, refresh_token,
/// access_token_expires_at). When no row exists, returns None.
#[sqlx::test(migrations = "../../migrations")]
async fn connection_lookup_by_tenant(pool: PgPool) -> sqlx::Result<()> {
    let tenant_id = Uuid::new_v4();

    seed_tenant(&pool, tenant_id).await?;

    // No row -> None
    let row: Option<(String, String, String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT company_id, access_token, refresh_token, access_token_expires_at \
         FROM quickbooks_connections WHERE tenant_id = $1 AND sync_enabled = true",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await?;
    assert!(row.is_none());

    // Seed a connection
    let expires_at = chrono::Utc::now() + chrono::Duration::hours(1);
    sqlx::query(
        "INSERT INTO quickbooks_connections \
         (tenant_id, company_id, access_token, refresh_token, \
          access_token_expires_at, refresh_token_expires_at, environment, sync_enabled) \
         VALUES ($1, $2, $3, $4, $5, $6, 'sandbox', true)",
    )
    .bind(tenant_id)
    .bind("test_company_123")
    .bind("access_tok")
    .bind("refresh_tok")
    .bind(expires_at)
    .bind(expires_at + chrono::Duration::days(30))
    .execute(&pool)
    .await?;

    let row: Option<(String, String, String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT company_id, access_token, refresh_token, access_token_expires_at \
         FROM quickbooks_connections WHERE tenant_id = $1 AND sync_enabled = true",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await?;
    assert!(row.is_some());
    let (company_id, access_token, refresh_token, _) = row.unwrap();
    assert_eq!(company_id, "test_company_123");
    assert_eq!(access_token, "access_tok");
    assert_eq!(refresh_token, "refresh_tok");

    Ok(())
}

/// Verify that last_sync_at is updated on quickbooks_connections after a sync.
#[sqlx::test(migrations = "../../migrations")]
async fn connection_last_sync_updated(pool: PgPool) -> sqlx::Result<()> {
    let tenant_id = Uuid::new_v4();
    let expires_at = chrono::Utc::now() + chrono::Duration::hours(1);

    seed_tenant(&pool, tenant_id).await?;

    sqlx::query(
        "INSERT INTO quickbooks_connections \
         (tenant_id, company_id, access_token, refresh_token, \
          access_token_expires_at, refresh_token_expires_at, environment, sync_enabled) \
         VALUES ($1, $2, $3, $4, $5, $6, 'sandbox', true)",
    )
    .bind(tenant_id)
    .bind("comp1")
    .bind("tok")
    .bind("rtok")
    .bind(expires_at)
    .bind(expires_at + chrono::Duration::days(30))
    .execute(&pool)
    .await?;

    // Initially NULL
    let last_sync: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT last_sync_at FROM quickbooks_connections WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await?;
    assert!(last_sync.is_none());

    // Update (mimicking end of sync_vendors)
    sqlx::query(
        "UPDATE quickbooks_connections SET last_sync_at = NOW() WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .execute(&pool)
    .await?;

    let last_sync: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT last_sync_at FROM quickbooks_connections WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await?;
    assert!(last_sync.is_some());

    Ok(())
}

/// Verify vendor insert + mapping insert (new vendor path).
/// Also tests the vendor lookup query used to detect existing mappings.
#[sqlx::test(migrations = "../../migrations")]
async fn vendor_insert_and_mapping_lookup(pool: PgPool) -> sqlx::Result<()> {
    let tenant_id = Uuid::new_v4();
    let vendor_id = Uuid::new_v4();

    seed_tenant(&pool, tenant_id).await?;

    // Insert a vendor (matches the INSERT in sync_vendors for new vendors)
    sqlx::query(
        "INSERT INTO vendors (id, tenant_id, name, vendor_type, email, phone, status, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())",
    )
    .bind(vendor_id)
    .bind(tenant_id)
    .bind("Test Vendor")
    .bind("business")
    .bind("test@example.com")
    .bind("555-1234")
    .bind("active")
    .execute(&pool)
    .await?;

    // Insert mapping
    sqlx::query(
        "INSERT INTO quickbooks_vendor_mappings \
         (tenant_id, quickbooks_vendor_id, billforge_vendor_id, quickbooks_vendor_name, \
          sync_token, last_synced_at, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, NOW(), NOW(), NOW())",
    )
    .bind(tenant_id)
    .bind("qb_vendor_99")
    .bind(vendor_id)
    .bind("Test Vendor")
    .bind("0")
    .execute(&pool)
    .await?;

    // Lookup by QB vendor ID (same query sync_vendors uses)
    let found: Option<(Uuid,)> = sqlx::query_as::<_, (Uuid,)>(
        "SELECT v.id FROM vendors v \
         INNER JOIN quickbooks_vendor_mappings m ON m.billforge_vendor_id = v.id \
         WHERE m.tenant_id = $1 AND m.quickbooks_vendor_id = $2",
    )
    .bind(tenant_id)
    .bind("qb_vendor_99")
    .fetch_optional(&pool)
    .await?;
    assert!(found.is_some());
    assert_eq!(found.unwrap().0, vendor_id);

    // Lookup for non-existent QB vendor ID returns None
    let not_found: Option<(Uuid,)> = sqlx::query_as::<_, (Uuid,)>(
        "SELECT v.id FROM vendors v \
         INNER JOIN quickbooks_vendor_mappings m ON m.billforge_vendor_id = v.id \
         WHERE m.tenant_id = $1 AND m.quickbooks_vendor_id = $2",
    )
    .bind(tenant_id)
    .bind("nonexistent")
    .fetch_optional(&pool)
    .await?;
    assert!(not_found.is_none());

    Ok(())
}

/// Verify vendor update + mapping update (existing vendor path).
#[sqlx::test(migrations = "../../migrations")]
async fn vendor_update_and_mapping_update(pool: PgPool) -> sqlx::Result<()> {
    let tenant_id = Uuid::new_v4();
    let vendor_id = Uuid::new_v4();

    seed_tenant(&pool, tenant_id).await?;

    // Seed vendor + mapping
    sqlx::query(
        "INSERT INTO vendors (id, tenant_id, name, vendor_type, email, phone, status, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())",
    )
    .bind(vendor_id)
    .bind(tenant_id)
    .bind("Old Name")
    .bind("contractor")
    .bind("")
    .bind("")
    .bind("active")
    .execute(&pool)
    .await?;

    sqlx::query(
        "INSERT INTO quickbooks_vendor_mappings \
         (tenant_id, quickbooks_vendor_id, billforge_vendor_id, quickbooks_vendor_name, \
          sync_token, last_synced_at, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, NOW(), NOW(), NOW())",
    )
    .bind(tenant_id)
    .bind("qb_v1")
    .bind(vendor_id)
    .bind("Old Name")
    .bind("0")
    .execute(&pool)
    .await?;

    // Update vendor (same UPDATE sync_vendors uses)
    sqlx::query(
        "UPDATE vendors SET name = $2, email = $3, phone = $4, updated_at = NOW() \
         WHERE id = $1",
    )
    .bind(vendor_id)
    .bind("Updated Name")
    .bind("new@example.com")
    .bind("555-9999")
    .execute(&pool)
    .await?;

    // Update mapping
    sqlx::query(
        "UPDATE quickbooks_vendor_mappings \
         SET quickbooks_vendor_name = $3, sync_token = $4, last_synced_at = NOW(), updated_at = NOW() \
         WHERE tenant_id = $1 AND quickbooks_vendor_id = $2",
    )
    .bind(tenant_id)
    .bind("qb_v1")
    .bind("Updated Name")
    .bind("1")
    .execute(&pool)
    .await?;

    // Verify
    let (name, email, phone): (String, String, String) = sqlx::query_as(
        "SELECT name, email, phone FROM vendors WHERE id = $1",
    )
    .bind(vendor_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(name, "Updated Name");
    assert_eq!(email, "new@example.com");
    assert_eq!(phone, "555-9999");

    let (qb_name, sync_token): (String, String) = sqlx::query_as(
        "SELECT quickbooks_vendor_name, sync_token FROM quickbooks_vendor_mappings \
         WHERE tenant_id = $1 AND quickbooks_vendor_id = $2",
    )
    .bind(tenant_id)
    .bind("qb_v1")
    .fetch_one(&pool)
    .await?;
    assert_eq!(qb_name, "Updated Name");
    assert_eq!(sync_token, "1");

    Ok(())
}

/// Verify that the token refresh SQL (UPDATE quickbooks_connections SET
/// access_token, refresh_token, etc.) works correctly.
#[sqlx::test(migrations = "../../migrations")]
async fn token_refresh_persistence(pool: PgPool) -> sqlx::Result<()> {
    let tenant_id = Uuid::new_v4();
    let expires_at = chrono::Utc::now() + chrono::Duration::minutes(10);
    let refresh_expires = chrono::Utc::now() + chrono::Duration::days(30);

    seed_tenant(&pool, tenant_id).await?;

    sqlx::query(
        "INSERT INTO quickbooks_connections \
         (tenant_id, company_id, access_token, refresh_token, \
          access_token_expires_at, refresh_token_expires_at, environment, sync_enabled) \
         VALUES ($1, $2, $3, $4, $5, $6, 'sandbox', true)",
    )
    .bind(tenant_id)
    .bind("comp1")
    .bind("old_access")
    .bind("old_refresh")
    .bind(expires_at)
    .bind(refresh_expires)
    .execute(&pool)
    .await?;

    let new_access_expires = chrono::Utc::now() + chrono::Duration::hours(1);
    let new_refresh_expires = chrono::Utc::now() + chrono::Duration::days(100);

    // Same UPDATE that sync_vendors runs after token refresh
    sqlx::query(
        "UPDATE quickbooks_connections \
         SET access_token = $2, refresh_token = $3, \
             access_token_expires_at = $4, refresh_token_expires_at = $5, updated_at = NOW() \
         WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .bind("new_access")
    .bind("new_refresh")
    .bind(new_access_expires)
    .bind(new_refresh_expires)
    .execute(&pool)
    .await?;

    let (access, refresh): (String, String) = sqlx::query_as(
        "SELECT access_token, refresh_token FROM quickbooks_connections WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(access, "new_access");
    assert_eq!(refresh, "new_refresh");

    Ok(())
}
