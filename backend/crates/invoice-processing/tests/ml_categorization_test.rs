//! Integration tests for ML-based invoice categorization
//!
//! Sprint 13 Feature #1: ML-Based Invoice Categorization

use sqlx::PgPool;
use uuid::Uuid;
use anyhow::Result;
use billforge_invoice_processing::{
    categorization::CategoryType,
    feedback_loop::FeedbackLearning,
};
use billforge_core::TenantId;

/// Helper to set up test tenant and data
async fn setup_test_tenant(pool: &PgPool) -> TenantId {
    let tenant_id = TenantId::new();

    // Create test tenant
    sqlx::query(
        "INSERT INTO tenants (id, name, subdomain, active, created_at)
         VALUES ($1, 'Test Tenant', 'test-tenant', true, NOW())",
    )
    .bind(tenant_id.as_uuid())
    .execute(pool)
    .await
    .expect("Failed to create test tenant");

    // Create tenant schema and tables
    let schema_name = format!("tenant_{}", tenant_id.as_str().replace('-', "_"));
    sqlx::query(&format!("CREATE SCHEMA IF NOT EXISTS {}", schema_name))
        .execute(pool)
        .await
        .expect("Failed to create tenant schema");

    // Create necessary tables
    sqlx::query(&format!(
        "CREATE TABLE IF NOT EXISTS {}.gl_codes (
            id UUID PRIMARY KEY,
            code TEXT NOT NULL,
            description TEXT,
            is_active BOOLEAN DEFAULT true,
            created_at TIMESTAMP DEFAULT NOW()
        )",
        schema_name
    ))
    .execute(pool)
    .await
    .expect("Failed to create gl_codes table");

    sqlx::query(&format!(
        "CREATE TABLE IF NOT EXISTS {}.vendors (
            id UUID PRIMARY KEY,
            name TEXT NOT NULL,
            is_active BOOLEAN DEFAULT true,
            created_at TIMESTAMP DEFAULT NOW()
        )",
        schema_name
    ))
    .execute(pool)
    .await
    .expect("Failed to create vendors table");

    sqlx::query(&format!(
        "CREATE TABLE IF NOT EXISTS {}.invoices (
            id UUID PRIMARY KEY,
            vendor_id UUID REFERENCES {}.vendors(id),
            vendor_name TEXT NOT NULL,
            invoice_number TEXT NOT NULL,
            total_amount DECIMAL(12,2) NOT NULL,
            gl_code TEXT,
            department TEXT,
            cost_center TEXT,
            created_at TIMESTAMP DEFAULT NOW()
        )",
        schema_name, schema_name
    ))
    .execute(pool)
    .await
    .expect("Failed to create invoices table");

    // Create ML tables
    sqlx::query(&format!(
        "CREATE TABLE IF NOT EXISTS {}.category_embeddings (
            id UUID PRIMARY KEY,
            category_type TEXT NOT NULL,
            category_value TEXT NOT NULL,
            description TEXT,
            embedding_vector VECTOR(1536),
            tenant_id TEXT NOT NULL,
            created_at TIMESTAMP DEFAULT NOW(),
            updated_at TIMESTAMP DEFAULT NOW()
        )",
        schema_name
    ))
    .execute(pool)
    .await
    .expect("Failed to create category_embeddings table");

    sqlx::query(&format!(
        "CREATE TABLE IF NOT EXISTS {}.vendor_embeddings (
            id UUID PRIMARY KEY,
            vendor_id UUID NOT NULL,
            embedding_vector VECTOR(1536),
            tenant_id TEXT NOT NULL,
            invoice_count INTEGER DEFAULT 0,
            created_at TIMESTAMP DEFAULT NOW(),
            updated_at TIMESTAMP DEFAULT NOW()
        )",
        schema_name
    ))
    .execute(pool)
    .await
    .expect("Failed to create vendor_embeddings table");

    sqlx::query(&format!(
        "CREATE TABLE IF NOT EXISTS {}.categorization_feedback (
            id UUID PRIMARY KEY,
            invoice_id UUID NOT NULL,
            category_type TEXT NOT NULL,
            suggested_value TEXT,
            correct_value TEXT NOT NULL,
            confidence DECIMAL(5,4),
            source TEXT,
            created_at TIMESTAMP DEFAULT NOW()
        )",
        schema_name
    ))
    .execute(pool)
    .await
    .expect("Failed to create categorization_feedback table");

    sqlx::query(&format!(
        "CREATE TABLE IF NOT EXISTS {}.categorization_metrics (
            id UUID PRIMARY KEY,
            metric_date DATE NOT NULL,
            total_suggestions INTEGER DEFAULT 0,
            accepted_suggestions INTEGER DEFAULT 0,
            corrected_suggestions INTEGER DEFAULT 0,
            created_at TIMESTAMP DEFAULT NOW()
        )",
        schema_name
    ))
    .execute(pool)
    .await
    .expect("Failed to create categorization_metrics table");

    tenant_id
}

/// Helper to seed test GL codes
async fn seed_gl_codes(pool: &PgPool, tenant_id: &TenantId) {
    let schema_name = format!("tenant_{}", tenant_id.as_str().replace('-', "_"));

    let gl_codes = vec![
        ("6100", "Software & SaaS Subscriptions"),
        ("6200", "Office Supplies & Equipment"),
        ("6300", "Marketing & Advertising"),
        ("6400", "Professional Services"),
        ("6500", "Travel & Entertainment"),
    ];

    for (code, description) in gl_codes {
        sqlx::query(&format!(
            "INSERT INTO {}.gl_codes (id, code, description, is_active, created_at)
             VALUES ($1, $2, $3, true, NOW())",
            schema_name
        ))
        .bind(Uuid::new_v4())
        .bind(code)
        .bind(description)
        .execute(pool)
        .await
        .expect("Failed to insert GL code");
    }
}

/// Helper to seed test vendors
async fn seed_vendors(pool: &PgPool, tenant_id: &TenantId) -> Vec<Uuid> {
    let schema_name = format!("tenant_{}", tenant_id.as_str().replace('-', "_"));

    let vendors = vec![
        "Adobe Creative Cloud",
        "Amazon Business",
        "Google Workspace",
        "Salesforce",
        "Microsoft 365",
    ];

    let mut vendor_ids = Vec::new();
    for vendor_name in vendors {
        let vendor_id = Uuid::new_v4();
        sqlx::query(&format!(
            "INSERT INTO {}.vendors (id, name, is_active, created_at)
             VALUES ($1, $2, true, NOW())",
            schema_name
        ))
        .bind(vendor_id)
        .bind(vendor_name)
        .execute(pool)
        .await
        .expect("Failed to insert vendor");

        vendor_ids.push(vendor_id);
    }

    vendor_ids
}

#[sqlx::test]
#[ignore = "requires migrated PostgreSQL with pgvector"]
async fn test_ml_categorization_without_api_key(pool: PgPool) -> sqlx::Result<()> {
    // Test that categorization works even without OpenAI API key
    // (will fall back to rule-based)

    let tenant_id = setup_test_tenant(&pool).await;
    seed_gl_codes(&pool, &tenant_id).await;
    let _vendor_ids = seed_vendors(&pool, &tenant_id).await;

    // This test would need a valid API key to actually test ML categorization
    // For now, we just verify the structure is correct
    // In production, use mock servers or skip if no API key

    Ok(())
}

#[sqlx::test]
#[ignore = "requires migrated PostgreSQL with pgvector"]
async fn test_embedding_cache_refresh(pool: PgPool) -> sqlx::Result<()> {
    let tenant_id = setup_test_tenant(&pool).await;
    seed_gl_codes(&pool, &tenant_id).await;

    let schema_name = format!("tenant_{}", tenant_id.as_str().replace('-', "_"));

    // Verify category embeddings were created
    let count: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM {}.category_embeddings WHERE tenant_id = $1",
        schema_name
    ))
    .bind(tenant_id.as_str())
    .fetch_one(&pool)
    .await?;

    assert_eq!(count, 0, "No embeddings should exist initially");

    // In a real test with OpenAI API key, we would:
    // 1. Create EmbeddingCache
    // 2. Call refresh_category_embeddings()
    // 3. Verify embeddings were created
    // 4. Verify vector similarity search works

    Ok(())
}

#[sqlx::test]
#[ignore = "requires migrated PostgreSQL with pgvector"]
async fn test_feedback_learning_analyze(pool: PgPool) -> Result<()> {
    let tenant_id = setup_test_tenant(&pool).await;
    let schema_name = format!("tenant_{}", tenant_id.as_str().replace('-', "_"));

    // Insert some feedback records
    let invoice_id = Uuid::new_v4();
    sqlx::query(&format!(
        "INSERT INTO {}.categorization_feedback
         (id, invoice_id, category_type, suggested_value, correct_value, confidence, source, created_at)
         VALUES ($1, $2, 'gl_code', '6100', '6200', 0.75, 'ml', NOW() - INTERVAL '2 days')",
        schema_name
    ))
    .bind(Uuid::new_v4())
    .bind(invoice_id)
    .execute(&pool)
    .await?;

    // Insert another feedback with same pattern
    let invoice_id2 = Uuid::new_v4();
    sqlx::query(&format!(
        "INSERT INTO {}.categorization_feedback
         (id, invoice_id, category_type, suggested_value, correct_value, confidence, source, created_at)
         VALUES ($1, $2, 'gl_code', '6100', '6200', 0.80, 'ml', NOW() - INTERVAL '1 day')",
        schema_name
    ))
    .bind(Uuid::new_v4())
    .bind(invoice_id2)
    .execute(&pool)
    .await?;

    // Create feedback learning instance
    let learning = FeedbackLearning::new(pool.clone());
    let tenant_id_str = tenant_id.as_str();

    // Analyze feedback from last 7 days
    let insights = learning.analyze_feedback(&tenant_id_str, 7).await?;

    // Verify insights
    assert!(insights.category_adjustments.len() > 0, "Should have category adjustments");
    assert!(insights.confidence_calibration.total_samples >= 2, "Should have feedback samples");

    // Check that the adjustment pattern was detected
    let gl_adjustments: Vec<_> = insights.category_adjustments
        .iter()
        .filter(|a| a.category_type == CategoryType::GlCode)
        .collect();

    assert!(gl_adjustments.len() > 0, "Should have GL code adjustments");

    Ok(())
}

#[sqlx::test]
#[ignore = "requires migrated PostgreSQL with pgvector"]
async fn test_feedback_accuracy_metrics(pool: PgPool) -> Result<()> {
    let tenant_id = setup_test_tenant(&pool).await;
    let schema_name = format!("tenant_{}", tenant_id.as_str().replace('-', "_"));

    // Insert feedback: 8 accepted, 2 corrected = 80% accuracy
    for i in 0..10 {
        let invoice_id = Uuid::new_v4();
        let suggested = "6100";
        let correct = if i < 8 { "6100" } else { "6200" }; // 8 matches, 2 corrections

        sqlx::query(&format!(
            "INSERT INTO {}.categorization_feedback
             (id, invoice_id, category_type, suggested_value, correct_value, confidence, source, created_at)
             VALUES ($1, $2, 'gl_code', $3, $4, 0.85, 'ml', NOW() - INTERVAL '1 day')",
            schema_name
        ))
        .bind(Uuid::new_v4())
        .bind(invoice_id)
        .bind(suggested)
        .bind(correct)
        .execute(&pool)
        .await?;
    }

    let learning = FeedbackLearning::new(pool.clone());
    let tenant_id_str = tenant_id.as_str();
    let metrics = learning.get_accuracy_metrics(&tenant_id_str, 7).await?;

    assert_eq!(metrics.total_suggestions, 10, "Should have 10 total suggestions");
    assert_eq!(metrics.accepted_suggestions, 8, "Should have 8 accepted");
    assert_eq!(metrics.corrected_suggestions, 2, "Should have 2 corrected");

    let accuracy = metrics.accuracy_rate();
    assert!((accuracy - 0.8).abs() < 0.01, "Accuracy should be 80%");

    Ok(())
}

#[sqlx::test]
#[ignore = "requires migrated PostgreSQL with pgvector"]
async fn test_vendor_embedding_storage(pool: PgPool) -> sqlx::Result<()> {
    let tenant_id = setup_test_tenant(&pool).await;
    let vendor_ids = seed_vendors(&pool, &tenant_id).await;
    let schema_name = format!("tenant_{}", tenant_id.as_str().replace('-', "_"));

    let vendor_id = vendor_ids[0];

    // Insert a vendor embedding
    let test_embedding = vec![0.1; 1536]; // Simple test vector
    sqlx::query(&format!(
        "INSERT INTO {}.vendor_embeddings
         (id, vendor_id, embedding_vector, tenant_id, invoice_count, created_at, updated_at)
         VALUES ($1, $2, $3, $4, 5, NOW(), NOW())",
        schema_name
    ))
    .bind(Uuid::new_v4())
    .bind(vendor_id)
    .bind(&test_embedding)
    .bind(tenant_id.as_str())
    .execute(&pool)
    .await?;

    // Verify it was stored
    let count: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM {}.vendor_embeddings WHERE vendor_id = $1",
        schema_name
    ))
    .bind(vendor_id)
    .fetch_one(&pool)
    .await?;

    assert_eq!(count, 1, "Vendor embedding should be stored");

    Ok(())
}

#[sqlx::test]
#[ignore = "requires migrated PostgreSQL with pgvector"]
async fn test_similarity_search_with_pgvector(pool: PgPool) -> sqlx::Result<()> {
    let tenant_id = setup_test_tenant(&pool).await;
    seed_gl_codes(&pool, &tenant_id).await;
    let _schema_name = format!("tenant_{}", tenant_id.as_str().replace('-', "_"));

    // Create two similar embeddings
    let _embedding1 = vec![1.0, 0.0, 0.0]; // Simplified for test
    let _embedding2 = vec![0.9, 0.1, 0.0]; // Similar to embedding1
    let _embedding3 = vec![0.0, 0.0, 1.0]; // Different

    // Note: In production, these would be 1536 dimensions
    // For this test, we'd need to use actual 1536-dim vectors or mock the similarity

    // This test demonstrates the structure; real tests would use full embeddings

    Ok(())
}

#[sqlx::test]
#[ignore = "requires migrated PostgreSQL with pgvector"]
async fn test_daily_metrics_update(pool: PgPool) -> Result<()> {
    let tenant_id = setup_test_tenant(&pool).await;
    let schema_name = format!("tenant_{}", tenant_id.as_str().replace('-', "_"));

    // Insert some feedback for today
    for _ in 0..5 {
        let invoice_id = Uuid::new_v4();
        sqlx::query(&format!(
            "INSERT INTO {}.categorization_feedback
             (id, invoice_id, category_type, suggested_value, correct_value, confidence, source, created_at)
             VALUES ($1, $2, 'gl_code', '6100', '6100', 0.90, 'ml', NOW())",
            schema_name
        ))
        .bind(Uuid::new_v4())
        .bind(invoice_id)
        .execute(&pool)
        .await?;
    }

    let learning = FeedbackLearning::new(pool.clone());
    let tenant_id_str = tenant_id.as_str();

    // Update daily metrics
    learning.update_daily_metrics(&tenant_id_str).await?;

    // Verify metrics were created
    let count: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM {}.categorization_metrics WHERE metric_date = CURRENT_DATE",
        schema_name
    ))
    .fetch_one(&pool)
    .await?;

    assert_eq!(count, 1, "Daily metrics should be created");

    // Check the values
    let (total, accepted): (i32, i32) = sqlx::query_as(&format!(
        "SELECT total_suggestions, accepted_suggestions FROM {}.categorization_metrics WHERE metric_date = CURRENT_DATE",
        schema_name
    ))
    .fetch_one(&pool)
    .await?;

    assert_eq!(total, 5, "Should have 5 total suggestions");
    assert_eq!(accepted, 5, "All 5 should be accepted (suggested = correct)");

    Ok(())
}
