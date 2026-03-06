//! Integration test for multi-tenant database isolation
//!
//! Run with: cargo test --test multi_tenant_integration

use billforge_db::PgManager;
use billforge_core::TenantId;
use sqlx::PgPool;

/// Test multi-tenant isolation
#[tokio::test]
#[ignore] // Run with: cargo test --test multi_tenant_integration -- --ignored
async fn test_tenant_isolation() {
    // Setup test database URLs
    let metadata_url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/billforge_test".to_string());
    let tenant_template = std::env::var("TEST_TENANT_DB_TEMPLATE")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/{database}".to_string());

    // Initialize manager
    let manager = PgManager::new(&metadata_url, tenant_template)
        .await
        .expect("Failed to create PgManager");

    // Create two test tenants
    let tenant1_id: TenantId = "11111111-aaaa-1111-1111-111111111111".parse().unwrap();
    let tenant2_id: TenantId = "22222222-bbbb-2222-2222-222222222222".parse().unwrap();

    // Clean up any previous test runs
    cleanup_tenant(&manager, &tenant1_id).await.ok();
    cleanup_tenant(&manager, &tenant2_id).await.ok();

    // Create tenants
    manager.create_tenant(&tenant1_id, "Test Tenant 1")
        .await
        .expect("Failed to create tenant 1");

    manager.create_tenant(&tenant2_id, "Test Tenant 2")
        .await
        .expect("Failed to create tenant 2");

    // Get tenant connection pools
    let pool1 = manager.tenant(&tenant1_id)
        .await
        .expect("Failed to get tenant 1 pool");

    let pool2 = manager.tenant(&tenant2_id)
        .await
        .expect("Failed to get tenant 2 pool");

    // Insert a user into tenant 1
    let user1_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, tenant_id, email, password_hash, name) VALUES ($1, $2, $3, $4, $5)"
    )
    .bind(user1_id)
    .bind(tenant1_id.as_str())
    .bind("user1@tenant1.com")
    .bind("hash1")
    .bind("User 1")
    .execute(&*pool1)
    .await
    .expect("Failed to insert user 1");

    // Insert a user into tenant 2
    let user2_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, tenant_id, email, password_hash, name) VALUES ($1, $2, $3, $4, $5)"
    )
    .bind(user2_id)
    .bind(tenant2_id.as_str())
    .bind("user2@tenant2.com")
    .bind("hash2")
    .bind("User 2")
    .execute(&*pool2)
    .await
    .expect("Failed to insert user 2");

    // Verify tenant 1 can see its user but not tenant 2's user
    let count1: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE email = $1")
        .bind("user1@tenant1.com")
        .fetch_one(&*pool1)
        .await
        .expect("Failed to count users in tenant 1");

    assert_eq!(count1, 1, "Tenant 1 should have 1 user");

    let count1_cross: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE email = $1")
        .bind("user2@tenant2.com")
        .fetch_one(&*pool1)
        .await
        .expect("Failed to cross-check users in tenant 1");

    assert_eq!(count1_cross, 0, "Tenant 1 should not see tenant 2's user");

    // Verify tenant 2 can see its user but not tenant 1's user
    let count2: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE email = $1")
        .bind("user2@tenant2.com")
        .fetch_one(&*pool2)
        .await
        .expect("Failed to count users in tenant 2");

    assert_eq!(count2, 1, "Tenant 2 should have 1 user");

    let count2_cross: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE email = $1")
        .bind("user1@tenant1.com")
        .fetch_one(&*pool2)
        .await
        .expect("Failed to cross-check users in tenant 2");

    assert_eq!(count2_cross, 0, "Tenant 2 should not see tenant 1's user");

    // Clean up
    cleanup_tenant(&manager, &tenant1_id).await.expect("Failed to cleanup tenant 1");
    cleanup_tenant(&manager, &tenant2_id).await.expect("Failed to cleanup tenant 2");

    println!("✓ Multi-tenant isolation test passed!");
}

async fn cleanup_tenant(manager: &PgManager, tenant_id: &TenantId) -> Result<(), Box<dyn std::error::Error>> {
    manager.delete_tenant(tenant_id).await?;
    Ok(())
}
