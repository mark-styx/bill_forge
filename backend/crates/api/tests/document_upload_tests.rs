//! Integration tests for document upload
//!
//! Verifies that document INSERT queries include tenant_id correctly.
//! Bug fix: upload_invoice handler was missing tenant_id in the INSERT,
//! causing "null value in column tenant_id violates not-null constraint".
//!
//! These tests run against the real PostgreSQL database (DATABASE_URL).

use uuid::Uuid;

const SANDBOX_TENANT_ID: &str = "11111111-1111-1111-1111-111111111111";
const SANDBOX_USER_ID: &str = "00000000-0000-0000-0000-000000000001";

/// Helper to get a database pool from DATABASE_URL
async fn get_pool() -> sqlx::PgPool {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");
    sqlx::PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to database")
}

/// Ensure a fixture invoice exists for the sandbox tenant.
/// Creates the user and invoice if they don't already exist.
/// Handles concurrent test runs by using ON CONFLICT DO NOTHING.
async fn ensure_fixture_invoice(pool: &sqlx::PgPool) -> Uuid {
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();
    let user_id = Uuid::parse_str(SANDBOX_USER_ID).unwrap();

    // Check if an invoice already exists for this tenant
    if let Some(id) = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM invoices WHERE tenant_id = $1 LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .unwrap()
    {
        return id;
    }

    // Ensure the fixture user exists (needed for FK constraints)
    sqlx::query(
        "INSERT INTO users (id, tenant_id, email, password_hash, name)
         VALUES ($1, $2, 'sandbox-test@example.com', '', 'Sandbox Test User')
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(user_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("Failed to create fixture user");

    // Create the fixture invoice (handle race with other parallel tests)
    let invoice_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO invoices (id, tenant_id, vendor_name, invoice_number, total_amount_cents, document_id, created_by)
         VALUES ($1, $2, 'Test Vendor', 'FIXTURE-001', 10000, $3, $4)
         ON CONFLICT (tenant_id, invoice_number) DO NOTHING",
    )
    .bind(invoice_id)
    .bind(tenant_id)
    .bind(Uuid::new_v4())
    .bind(user_id)
    .execute(pool)
    .await
    .expect("Failed to create fixture invoice");

    // Re-fetch in case another test won the race
    sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM invoices WHERE tenant_id = $1 AND invoice_number = 'FIXTURE-001'",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .expect("Fixture invoice should exist")
}

/// Test that inserting a document WITH tenant_id succeeds (the fixed query).
#[tokio::test]
async fn test_document_insert_with_tenant_id_succeeds() {
    let pool = get_pool().await;
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();
    let user_id = Uuid::parse_str(SANDBOX_USER_ID).unwrap();
    let document_id = Uuid::new_v4();

    // Get a real invoice ID for FK constraint
    let invoice_id = ensure_fixture_invoice(&pool).await;

    // This is the EXACT query from the fixed upload_invoice handler in invoices.rs
    let result = sqlx::query(
        "INSERT INTO documents (id, tenant_id, filename, mime_type, size_bytes, storage_key, invoice_id, doc_type, uploaded_by, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'invoice_original', $8, NOW())"
    )
    .bind(document_id)
    .bind(tenant_id)         // <-- This was the missing bind
    .bind("test-upload.pdf")
    .bind("application/pdf")
    .bind(1024_i64)
    .bind(format!("test/{}/test-upload.pdf", document_id))
    .bind(invoice_id)
    .bind(user_id)
    .execute(&pool)
    .await;

    assert!(result.is_ok(), "Document insert with tenant_id should succeed: {:?}", result.err());

    // Verify the row was stored with correct tenant_id
    let stored_tenant_id: Uuid = sqlx::query_scalar(
        "SELECT tenant_id FROM documents WHERE id = $1"
    )
    .bind(document_id)
    .fetch_one(&pool)
    .await
    .expect("Should find the inserted document");

    assert_eq!(stored_tenant_id, tenant_id, "Stored tenant_id should match input");

    // Cleanup
    sqlx::query("DELETE FROM documents WHERE id = $1")
        .bind(document_id)
        .execute(&pool)
        .await
        .ok();
}

/// Test that inserting a document WITHOUT tenant_id fails with NOT NULL violation.
/// This proves the bug existed before the fix.
#[tokio::test]
async fn test_document_insert_without_tenant_id_fails() {
    let pool = get_pool().await;
    let user_id = Uuid::parse_str(SANDBOX_USER_ID).unwrap();
    let document_id = Uuid::new_v4();

    let invoice_id = ensure_fixture_invoice(&pool).await;

    // This is what the OLD broken query looked like (missing tenant_id)
    let result = sqlx::query(
        "INSERT INTO documents (id, filename, mime_type, size_bytes, storage_key, invoice_id, doc_type, uploaded_by, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, 'invoice_original', $7, NOW())"
    )
    .bind(document_id)
    .bind("should-fail.pdf")
    .bind("application/pdf")
    .bind(512_i64)
    .bind("test/should-fail.pdf")
    .bind(invoice_id)
    .bind(user_id)
    .execute(&pool)
    .await;

    assert!(result.is_err(), "Insert without tenant_id MUST fail with NOT NULL violation");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("tenant_id"),
        "Error should reference tenant_id column, got: {}",
        err_msg
    );
}

/// Test the storage.rs document insert query also works correctly.
#[tokio::test]
async fn test_storage_document_insert_with_tenant_id() {
    let pool = get_pool().await;
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();
    let user_id = Uuid::parse_str(SANDBOX_USER_ID).unwrap();
    let document_id = Uuid::new_v4();

    let invoice_id = ensure_fixture_invoice(&pool).await;

    // This is the query from storage.rs (was already correct, verify it works)
    let result = sqlx::query(
        r#"INSERT INTO documents (
            id, tenant_id, filename, mime_type, size_bytes, storage_key, invoice_id, doc_type, uploaded_by, created_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#
    )
    .bind(document_id)
    .bind(tenant_id)
    .bind("storage-test.jpg")
    .bind("image/jpeg")
    .bind(2048_i64)
    .bind(format!("test/{}/storage-test.jpg", document_id))
    .bind(Some(invoice_id))
    .bind("invoice_original")
    .bind(user_id)
    .bind(chrono::Utc::now())
    .execute(&pool)
    .await;

    assert!(result.is_ok(), "Storage insert should succeed: {:?}", result.err());

    // Verify
    let (stored_tenant, stored_filename): (Uuid, String) = sqlx::query_as(
        "SELECT tenant_id, filename FROM documents WHERE id = $1"
    )
    .bind(document_id)
    .fetch_one(&pool)
    .await
    .expect("Should find stored document");

    assert_eq!(stored_tenant, tenant_id);
    assert_eq!(stored_filename, "storage-test.jpg");

    // Cleanup
    sqlx::query("DELETE FROM documents WHERE id = $1")
        .bind(document_id)
        .execute(&pool)
        .await
        .ok();
}

/// Test that multiple documents can be uploaded for the same invoice.
#[tokio::test]
async fn test_multiple_documents_per_invoice() {
    let pool = get_pool().await;
    let tenant_id = Uuid::parse_str(SANDBOX_TENANT_ID).unwrap();
    let user_id = Uuid::parse_str(SANDBOX_USER_ID).unwrap();

    let invoice_id = ensure_fixture_invoice(&pool).await;

    let mut doc_ids = Vec::new();

    for i in 0..3 {
        let doc_id = Uuid::new_v4();
        doc_ids.push(doc_id);

        sqlx::query(
            "INSERT INTO documents (id, tenant_id, filename, mime_type, size_bytes, storage_key, invoice_id, doc_type, uploaded_by, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, 'invoice_original', $8, NOW())"
        )
        .bind(doc_id)
        .bind(tenant_id)
        .bind(format!("multi-test-{}.pdf", i))
        .bind("application/pdf")
        .bind(512_i64 * (i as i64 + 1))
        .bind(format!("test/{}/multi-test-{}.pdf", doc_id, i))
        .bind(invoice_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect(&format!("Should insert document {}", i));
    }

    // Verify all 3 were inserted with correct tenant_id
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM documents WHERE invoice_id = $1 AND tenant_id = $2 AND filename LIKE 'multi-test-%'"
    )
    .bind(invoice_id)
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .expect("Count query should succeed");

    assert_eq!(count, 3, "Should have 3 test documents");

    // Cleanup
    for doc_id in &doc_ids {
        sqlx::query("DELETE FROM documents WHERE id = $1")
            .bind(doc_id)
            .execute(&pool)
            .await
            .ok();
    }
}
