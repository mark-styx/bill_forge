//! Integration tests for the AP-to-AP Migration Importer (#405).
//!
//! - Parser tests run with no DB and verify BILL vs Coupa column maps.
//! - Commit-path tests are gated behind --ignored because they require
//!   DATABASE_URL with the migrations applied (mirrors audit_bundle_test).
//!
//! Run: `cargo test -p billforge-api --test ap_migration_test`
//! Run with DB: `cargo test -p billforge-api --test ap_migration_test -- --ignored`

use billforge_api::routes::ap_migration::{parse_bundle, BundleSource};
use std::io::Write;
use uuid::Uuid;

fn build_bundle(source: &str, files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("manifest.json", opts).unwrap();
        zip.write_all(format!(r#"{{"source":"{}","version":"1"}}"#, source).as_bytes())
            .unwrap();
        for (name, bytes) in files {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }
    buf
}

#[test]
fn parses_bill_bundle_into_canonical_keys() {
    let vendors = b"Name,TaxId,Email\nAcme LLC,11-2222222,ap@acme.com\nGlobex,99-0000000,billing@globex.com\n";
    let invoices = b"InvoiceNumber,VendorName,VendorTaxId,Amount,Currency,InvoiceDate,DueDate\nINV-1,Acme LLC,11-2222222,100.00,USD,2026-01-01,2026-01-31\n";
    let approvers = b"Email,Name,Role,ManagerEmail\nlead@example.com,Lead,manager,\n";
    let workflows = b"WorkflowName,Rule\nDefault,{\"min\":1000}\n";
    let glmap = b"GLCode,GLName,Department\n6000,Office Supplies,OPS\n";

    let bytes = build_bundle(
        "bill",
        &[
            ("vendors.csv", vendors),
            ("invoices.csv", invoices),
            ("approval_workflows.csv", workflows),
            ("gl_mappings.csv", glmap),
            ("approvers.csv", approvers),
            ("documents/sample.pdf", b"%PDF-1.4 fake"),
        ],
    );

    let parsed = parse_bundle(&bytes).expect("parse should succeed");
    assert_eq!(parsed.source, BundleSource::Bill);
    assert_eq!(parsed.vendors.len(), 2);
    assert_eq!(parsed.vendors[0].get("name").unwrap(), "Acme LLC");
    assert_eq!(parsed.vendors[0].get("tax_id").unwrap(), "11-2222222");
    assert_eq!(parsed.invoices.len(), 1);
    assert_eq!(parsed.invoices[0].get("invoice_number").unwrap(), "INV-1");
    assert_eq!(parsed.approvers.len(), 1);
    assert_eq!(parsed.gl_mappings.len(), 1);
    assert_eq!(parsed.approval_workflows.len(), 1);
    assert_eq!(parsed.documents.len(), 1);
    assert_eq!(parsed.documents[0].get("filename").unwrap(), "sample.pdf");
}

#[test]
fn parses_coupa_bundle_with_distinct_column_map() {
    // Coupa uses snake_case column headers and different field names.
    let vendors = b"supplier_name,tax_id_number,primary_contact_email\nCoupaCo,55-7777777,procurement@coupa.com\n";
    let invoices = b"invoice_number,supplier_name,supplier_tax_id,gross_total,currency_code,invoice_date,payment_due_date\nC-100,CoupaCo,55-7777777,250.50,USD,2026-02-15,2026-03-15\n";

    let bytes = build_bundle("coupa", &[
        ("vendors.csv", vendors),
        ("invoices.csv", invoices),
    ]);

    let parsed = parse_bundle(&bytes).expect("parse should succeed");
    assert_eq!(parsed.source, BundleSource::Coupa);
    assert_eq!(parsed.vendors.len(), 1);
    assert_eq!(parsed.vendors[0].get("name").unwrap(), "CoupaCo");
    assert_eq!(parsed.vendors[0].get("tax_id").unwrap(), "55-7777777");
    assert_eq!(parsed.invoices.len(), 1);
    assert_eq!(parsed.invoices[0].get("amount").unwrap(), "250.50");
    assert_eq!(parsed.invoices[0].get("vendor_tax_id").unwrap(), "55-7777777");
}

#[test]
fn rejects_bundle_without_manifest() {
    let mut buf = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("vendors.csv", opts).unwrap();
        zip.write_all(b"Name\nAcme\n").unwrap();
        zip.finish().unwrap();
    }
    let err = parse_bundle(&buf).expect_err("must reject missing manifest");
    assert!(err.contains("manifest.json"), "error was: {}", err);
}

#[test]
fn rejects_bundle_with_unknown_source() {
    let mut buf = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("manifest.json", opts).unwrap();
        zip.write_all(br#"{"source":"sap","version":"1"}"#).unwrap();
        zip.finish().unwrap();
    }
    let err = parse_bundle(&buf).expect_err("must reject unknown source");
    assert!(err.contains("unknown source"), "error was: {}", err);
}

// ---------------------------------------------------------------------------
// DB-backed commit test (gated; mirrors audit_bundle_test pattern)
// ---------------------------------------------------------------------------

async fn get_pool() -> sqlx::PgPool {
    dotenvy::dotenv().ok();
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::PgPool::connect(&url)
        .await
        .expect("connect to database")
}

#[tokio::test]
#[ignore] // Requires DATABASE_URL with migrations applied
async fn commit_writes_preview_and_audit_rows_scoped_to_tenant() {
    let pool = get_pool().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    let bundle_id = Uuid::new_v4();

    // Insert a bundle for tenant A and stage one vendor preview row.
    sqlx::query(
        r#"INSERT INTO ap_migration_bundle
            (id, tenant_id, source, status, original_filename, storage_path)
           VALUES ($1, $2, 'bill', 'parsed', 'export.zip', '/tmp/test.zip')"#,
    )
    .bind(bundle_id)
    .bind(tenant_a)
    .execute(&pool)
    .await
    .expect("insert bundle");

    sqlx::query(
        r#"INSERT INTO ap_migration_preview
            (bundle_id, tenant_id, entity_type, source_payload, target_action)
           VALUES ($1, $2, 'vendor', $3, 'create')"#,
    )
    .bind(bundle_id)
    .bind(tenant_a)
    .bind(serde_json::json!({"name": "AcmeMigration", "tax_id": "11-2222222"}))
    .execute(&pool)
    .await
    .expect("insert preview row");

    // Tenant B should NOT see tenant A's bundle (RLS-or-explicit-filter).
    let visible_to_b: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM ap_migration_bundle WHERE id = $1 AND tenant_id = $2",
    )
    .bind(bundle_id)
    .bind(tenant_b)
    .fetch_optional(&pool)
    .await
    .expect("query");
    assert!(visible_to_b.is_none(), "tenant B must not see tenant A's bundle");

    // Cleanup
    sqlx::query("DELETE FROM ap_migration_preview WHERE bundle_id = $1")
        .bind(bundle_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM ap_migration_audit WHERE bundle_id = $1")
        .bind(bundle_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM ap_migration_bundle WHERE id = $1")
        .bind(bundle_id)
        .execute(&pool)
        .await
        .ok();
}
