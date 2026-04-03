//! Application state shared across handlers

use crate::Config;
use anyhow::Result;
use billforge_auth::AuthService;
use billforge_core::{
    traits::{AuditService, StorageService},
    Module, Role, TenantId,
};
use billforge_db::metadata_db::CreateUserInput;
use billforge_db::repositories::AuditRepositoryImpl;
use billforge_db::{create_storage_service, DatabaseManager, LocalStorageService, StorageConfig};
use billforge_email::{EmailService, EmailServiceImpl, MockEmailService};
use std::sync::Arc;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<DatabaseManager>,
    pub auth: Arc<AuthService>,
    pub storage: Arc<dyn StorageService>,
    pub audit: Arc<AuditRepositoryImpl>,
    pub email: Arc<dyn EmailService>,
    pub config: Arc<Config>,
}

impl AppState {
    pub async fn new(config: &Config) -> Result<Self> {
        // Initialize database manager
        let db = DatabaseManager::new(&config.database_url, &config.tenant_db_path).await?;
        let db = Arc::new(db);

        // Initialize auth service
        let database_url = std::env::var("DATABASE_URL")
            .map_err(|e| anyhow::anyhow!("DATABASE_URL not set: {}", e))?;
        let metadata_db = Arc::new(
            billforge_db::MetadataDatabase::new(&database_url)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create metadata database: {}", e))?,
        );
        let auth = AuthService::new(config.jwt.clone(), metadata_db);
        let auth = Arc::new(auth);

        // Initialize storage service based on STORAGE_PROVIDER env var
        let storage: Arc<dyn StorageService> = match std::env::var("STORAGE_PROVIDER").as_deref() {
            Ok("s3") => {
                let bucket =
                    std::env::var("S3_BUCKET").unwrap_or_else(|_| "billforge-files".to_string());
                let region = std::env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".to_string());
                let endpoint = std::env::var("S3_ENDPOINT").ok();
                tracing::info!(bucket = %bucket, region = %region, endpoint = ?endpoint, "Using S3 storage");
                let storage_config = StorageConfig::S3 {
                    bucket,
                    region,
                    endpoint,
                    key_prefix: None,
                };
                Arc::from(
                    create_storage_service(storage_config).await.map_err(|e| {
                        anyhow::anyhow!("Failed to create S3 storage service: {}", e)
                    })?,
                )
            }
            _ => {
                let storage_path = std::env::var("LOCAL_STORAGE_PATH").unwrap_or_else(|_| {
                    std::path::Path::new(&config.tenant_db_path)
                        .parent()
                        .unwrap_or_else(|| std::path::Path::new("./data"))
                        .to_string_lossy()
                        .to_string()
                });
                tracing::info!(path = %storage_path, "Using local storage");
                Arc::new(LocalStorageService::new(&storage_path).await.map_err(|e| {
                    anyhow::anyhow!("Failed to create local storage service: {}", e)
                })?)
            }
        };

        // Initialize audit service
        let audit_pool = db.metadata();
        let audit = Arc::new(AuditRepositoryImpl::new(audit_pool));

        // Initialize email service (real provider or mock)
        let email: Arc<dyn EmailService> = if let Some(ref email_config) = config.email {
            match EmailServiceImpl::new(email_config.clone()) {
                Ok(service) => {
                    tracing::info!(
                        "Email service initialized with provider: {:?}",
                        email_config.provider
                    );
                    Arc::new(service)
                }
                Err(e) => {
                    tracing::warn!("Failed to initialize email service, using mock: {}", e);
                    Arc::new(MockEmailService::new())
                }
            }
        } else {
            tracing::info!("Email service disabled (no provider configured)");
            Arc::new(MockEmailService::new())
        };

        // Initialize sandbox data if needed
        Self::init_sandbox(&db, &auth, &audit).await?;

        Ok(Self {
            db,
            auth,
            storage,
            audit,
            email,
            config: Arc::new(config.clone()),
        })
    }

    /// Get the audit service
    pub fn audit_service(&self) -> &dyn AuditService {
        self.audit.as_ref()
    }

    /// Initialize sandbox tenant and user for development
    async fn init_sandbox(
        db: &Arc<DatabaseManager>,
        _auth: &Arc<AuthService>,
        audit: &Arc<AuditRepositoryImpl>,
    ) -> Result<()> {
        let sandbox_tenant_id: TenantId = "11111111-1111-1111-1111-111111111111"
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid sandbox tenant ID: {}", e))?;

        // Get metadata database URL from environment
        let database_url = std::env::var("DATABASE_URL")
            .map_err(|e| anyhow::anyhow!("DATABASE_URL not set: {}", e))?;

        // Create metadata database wrapper
        let metadata_db = billforge_db::MetadataDatabase::new(&database_url)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create metadata database: {}", e))?;

        // Check if sandbox tenant already exists
        if metadata_db.tenant_exists(&sandbox_tenant_id).await? {
            tracing::info!("Sandbox tenant already exists, ensuring migrations are up to date...");
            // Re-run migrations to pick up new tables
            // (uses CREATE TABLE IF NOT EXISTS / ALTER TABLE ADD COLUMN IF NOT EXISTS)
            let tenant_pool = db
                .tenant(&sandbox_tenant_id)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to get tenant db: {}", e))?;
            billforge_db::tenant_db::run_workflow_migrations(&tenant_pool)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to re-run workflow migrations: {}", e))?;
            billforge_db::tenant_db::run_ocr_pipeline_migrations(&tenant_pool)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to run OCR pipeline migrations: {}", e))?;
            billforge_db::tenant_db::run_approval_chain_migrations(&tenant_pool)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to run approval chain migrations: {}", e))?;
            tracing::info!("Sandbox tenant migrations updated successfully");
            return Ok(());
        }

        tracing::info!("Creating sandbox tenant and admin user...");

        // Create sandbox tenant with all modules enabled
        metadata_db
            .create_tenant(&sandbox_tenant_id, "Meridian Industries")
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create sandbox tenant: {}", e))?;

        // Enable all modules for sandbox
        metadata_db
            .update_tenant_modules(
                &sandbox_tenant_id,
                &[
                    Module::InvoiceCapture,
                    Module::InvoiceProcessing,
                    Module::VendorManagement,
                    Module::Reporting,
                ],
            )
            .await
            .map_err(|e| anyhow::anyhow!("Failed to enable modules: {}", e))?;

        // Create tenant database
        let tenant_pool = db
            .tenant(&sandbox_tenant_id)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create tenant db: {}", e))?;

        // Run tenant migrations (inline for now)
        sqlx::raw_sql(include_str!("../../../migrations/002_create_users.sql"))
            .execute(&*tenant_pool)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to run tenant migrations: {}", e))?;

        sqlx::raw_sql(include_str!("../../../migrations/003_create_vendors.sql"))
            .execute(&*tenant_pool)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to run vendor migrations: {}", e))?;

        sqlx::raw_sql(include_str!("../../../migrations/004_create_invoices.sql"))
            .execute(&*tenant_pool)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to run invoice migrations: {}", e))?;

        sqlx::raw_sql(include_str!(
            "../../../migrations/005_create_workflow_tables.sql"
        ))
        .execute(&*tenant_pool)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to run workflow migrations: {}", e))?;

        sqlx::raw_sql(include_str!(
            "../../../migrations/006_create_quickbooks_tables.sql"
        ))
        .execute(&*tenant_pool)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to run QuickBooks migrations: {}", e))?;

        // Run workflow + vendor-statement migrations (adds vendor columns, line items, etc.)
        billforge_db::tenant_db::run_workflow_migrations(&tenant_pool)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to run workflow migrations: {}", e))?;

        // Run OCR pipeline migrations (batch processing, corrections, vendor aliases)
        billforge_db::tenant_db::run_ocr_pipeline_migrations(&tenant_pool)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to run OCR pipeline migrations: {}", e))?;

        // Run approval chain migrations (policies, chains, steps, activity log)
        billforge_db::tenant_db::run_approval_chain_migrations(&tenant_pool)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to run approval chain migrations: {}", e))?;

        // Create invoice_line_items and invoice_status_config for seed data
        sqlx::raw_sql(
            r#"
            CREATE TABLE IF NOT EXISTS invoice_line_items (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                invoice_id UUID NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
                line_number INTEGER NOT NULL DEFAULT 0,
                description TEXT NOT NULL,
                quantity REAL,
                unit_price_cents BIGINT,
                total_amount_cents BIGINT NOT NULL,
                notes TEXT,
                gl_code TEXT,
                department TEXT
            );
            CREATE TABLE IF NOT EXISTS invoice_status_config (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                tenant_id UUID NOT NULL,
                status_key VARCHAR(50) NOT NULL,
                display_label VARCHAR(100) NOT NULL,
                color VARCHAR(50) NOT NULL DEFAULT 'gray',
                bg_color VARCHAR(50) NOT NULL DEFAULT 'bg-secondary',
                text_color VARCHAR(50) NOT NULL DEFAULT 'text-muted-foreground',
                sort_order INTEGER NOT NULL DEFAULT 0,
                is_terminal BOOLEAN NOT NULL DEFAULT false,
                is_active BOOLEAN NOT NULL DEFAULT true,
                category VARCHAR(20) NOT NULL DEFAULT 'processing',
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                UNIQUE(tenant_id, status_key)
            );
            CREATE INDEX IF NOT EXISTS idx_line_items_invoice ON invoice_line_items(invoice_id);
        "#,
        )
        .execute(&*tenant_pool)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create line items tables: {}", e))?;

        // Run audit migrations
        audit
            .run_migrations(&sandbox_tenant_id)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to run audit migrations: {}", e))?;

        // Create sandbox users with hashed password
        let password_service = billforge_auth::PasswordService::new();
        let password_hash = password_service
            .hash("sandbox123")
            .map_err(|e| anyhow::anyhow!("Failed to hash password: {}", e))?;

        // Admin user
        metadata_db
            .create_user(&CreateUserInput {
                tenant_id: sandbox_tenant_id.clone(),
                email: "admin@sandbox.local".to_string(),
                password_hash: password_hash.clone(),
                name: "Sarah Chen".to_string(),
                roles: vec![Role::TenantAdmin],
            })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create sandbox admin: {}", e))?;

        // AP Clerk user
        let _ = metadata_db
            .create_user(&CreateUserInput {
                tenant_id: sandbox_tenant_id.clone(),
                email: "ap.clerk@sandbox.local".to_string(),
                password_hash: password_hash.clone(),
                name: "Mike Johnson".to_string(),
                roles: vec![Role::ApUser],
            })
            .await;

        // Approver user
        let _ = metadata_db
            .create_user(&CreateUserInput {
                tenant_id: sandbox_tenant_id.clone(),
                email: "approver@sandbox.local".to_string(),
                password_hash: password_hash.clone(),
                name: "Lisa Wang".to_string(),
                roles: vec![Role::Approver],
            })
            .await;

        // Report Viewer user
        let _ = metadata_db
            .create_user(&CreateUserInput {
                tenant_id: sandbox_tenant_id.clone(),
                email: "viewer@sandbox.local".to_string(),
                password_hash,
                name: "Tom Garcia".to_string(),
                roles: vec![Role::ReportViewer],
            })
            .await;

        // Seed demo data
        Self::seed_sandbox_data(&tenant_pool, &sandbox_tenant_id).await?;

        tracing::info!("Demo environment initialized. Login with:");
        tracing::info!("  Tenant: Meridian Industries ({})", sandbox_tenant_id);
        tracing::info!("  Email: admin@sandbox.local");
        tracing::info!("  Password: sandbox123");

        Ok(())
    }

    /// Seed demo vendors, invoices, and default queues for the sandbox
    async fn seed_sandbox_data(pool: &sqlx::PgPool, tenant_id: &TenantId) -> Result<()> {
        let tenant_uuid = *tenant_id.as_uuid();

        // ==========================================================
        // Seed users in the tenant database (needed for FK references)
        // ==========================================================
        let users = vec![
            (
                "17b66d9b-6da5-4cfb-93ad-f8d2f1aefe8f",
                "admin@sandbox.local",
                "Sarah Chen",
                r#"["tenant_admin"]"#,
            ),
            (
                "22222222-2222-2222-2222-222222220001",
                "ap.clerk@sandbox.local",
                "Mike Johnson",
                r#"["ap_user"]"#,
            ),
            (
                "22222222-2222-2222-2222-222222220002",
                "approver@sandbox.local",
                "Lisa Wang",
                r#"["approver"]"#,
            ),
            (
                "22222222-2222-2222-2222-222222220003",
                "viewer@sandbox.local",
                "Tom Garcia",
                r#"["report_viewer"]"#,
            ),
        ];

        for (id, email, name, roles) in &users {
            let id_uuid = uuid::Uuid::parse_str(id)?;
            // Use a dummy password hash — auth is checked via metadata DB, not tenant DB
            sqlx::query(
                "INSERT INTO users (id, tenant_id, email, password_hash, name, roles, is_active, created_at, updated_at)
                 VALUES ($1, $2, $3, 'not-used-for-auth', $4, $5::jsonb, true, NOW(), NOW())
                 ON CONFLICT (tenant_id, email) DO NOTHING"
            )
            .bind(id_uuid)
            .bind(tenant_uuid)
            .bind(email)
            .bind(name)
            .bind(roles)
            .execute(pool)
            .await
            .ok();
        }

        // ==========================================================
        // Seed default work queues (the standard AP workflow pipeline)
        // ==========================================================
        let queues = vec![
            (
                "11111111-4444-5555-6666-777777770001",
                "OCR Error Queue",
                "Invoices that couldn't be processed by OCR",
                "exception",
                0,
            ),
            (
                "11111111-4444-5555-6666-777777770002",
                "Accounts Payable Queue",
                "Initial review queue for AP staff",
                "review",
                1,
            ),
            (
                "11111111-4444-5555-6666-777777770003",
                "Pending Approval",
                "Invoices waiting for manager approval",
                "approval",
                0,
            ),
            (
                "11111111-4444-5555-6666-777777770004",
                "Ready for Payment",
                "Approved invoices ready to be paid",
                "payment",
                0,
            ),
            (
                "11111111-4444-5555-6666-777777770005",
                "On Hold",
                "Invoices temporarily on hold",
                "hold",
                0,
            ),
        ];

        for (id, name, description, queue_type, is_default) in &queues {
            let id_uuid = uuid::Uuid::parse_str(id)?;
            sqlx::query(
                "INSERT INTO work_queues (id, name, description, queue_type, is_default, is_active, settings, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, true, '{}'::jsonb, NOW(), NOW())
                 ON CONFLICT (id) DO NOTHING"
            )
            .bind(id_uuid)
            .bind(name)
            .bind(description)
            .bind(queue_type)
            .bind(*is_default)
            .execute(pool)
            .await
            .ok();
        }

        // ==========================================================
        // Seed sample assignment rules
        // ==========================================================
        let assignment_rules = vec![
            (
                "11111111-5555-6666-7777-888888880001",
                "11111111-4444-5555-6666-777777770002",
                "High Value to Manager",
                "Invoices over $10,000 go to manager review",
                100,
                r#"[{"field":"amount","operator":"greater_than","value":1000000}]"#,
                r#"{"Role":"tenant_admin"}"#,
            ),
            (
                "11111111-5555-6666-7777-888888880002",
                "11111111-4444-5555-6666-777777770003",
                "IT Department Approval",
                "IT invoices need IT manager approval",
                80,
                r#"[{"field":"department","operator":"equals","value":"IT"}]"#,
                r#"{"Role":"approver"}"#,
            ),
            (
                "11111111-5555-6666-7777-888888880003",
                "11111111-4444-5555-6666-777777770002",
                "Contractor Invoices",
                "All contractor invoices assigned to AP lead",
                60,
                r#"[{"field":"vendor_type","operator":"equals","value":"contractor"}]"#,
                r#"{"User":"17b66d9b-6da5-4cfb-93ad-f8d2f1aefe8f"}"#,
            ),
        ];

        for (id, queue_id, name, description, priority, conditions, assign_to) in &assignment_rules
        {
            let id_uuid = uuid::Uuid::parse_str(id)?;
            let queue_uuid = uuid::Uuid::parse_str(queue_id)?;
            sqlx::query(
                "INSERT INTO assignment_rules (id, queue_id, name, description, priority, conditions, assign_to, is_active, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6::jsonb, $7::jsonb, true, NOW(), NOW())
                 ON CONFLICT (id) DO NOTHING"
            )
            .bind(id_uuid)
            .bind(queue_uuid)
            .bind(name)
            .bind(description)
            .bind(*priority)
            .bind(conditions)
            .bind(assign_to)
            .execute(pool)
            .await
            .ok();
        }

        // ==========================================================
        // Seed vendors with realistic data
        // ==========================================================
        let vendors = vec![
            // Business vendors
            (
                "11111111-2222-3333-4444-555555550001",
                "Acme Corporation",
                "business",
                "billing@acme.com",
                "+1-555-0100",
                "123 Industrial Way, Chicago, IL 60601",
                "active",
            ),
            (
                "11111111-2222-3333-4444-555555550002",
                "TechSupplies Inc",
                "business",
                "ap@techsupplies.com",
                "+1-555-0101",
                "456 Tech Park Dr, San Jose, CA 95110",
                "active",
            ),
            (
                "11111111-2222-3333-4444-555555550003",
                "Office Depot",
                "business",
                "invoices@officedepot.com",
                "+1-800-463-3768",
                "6600 N Military Trail, Boca Raton, FL 33496",
                "active",
            ),
            (
                "11111111-2222-3333-4444-555555550004",
                "Amazon Web Services",
                "business",
                "aws-billing@amazon.com",
                "+1-206-266-4064",
                "410 Terry Ave N, Seattle, WA 98109",
                "active",
            ),
            (
                "11111111-2222-3333-4444-555555550005",
                "Microsoft Azure",
                "business",
                "azure-billing@microsoft.com",
                "+1-800-642-7676",
                "One Microsoft Way, Redmond, WA 98052",
                "active",
            ),
            (
                "11111111-2222-3333-4444-555555550006",
                "Google Cloud",
                "business",
                "cloud-billing@google.com",
                "+1-855-492-5685",
                "1600 Amphitheatre Pkwy, Mountain View, CA 94043",
                "active",
            ),
            (
                "11111111-2222-3333-4444-555555550007",
                "Utilities Co",
                "business",
                "billing@utilities.com",
                "+1-555-0107",
                "789 Power St, Houston, TX 77001",
                "active",
            ),
            (
                "11111111-2222-3333-4444-555555550008",
                "Premium Insurance Group",
                "business",
                "premiums@insurance.com",
                "+1-555-0108",
                "321 Policy Blvd, Hartford, CT 06103",
                "active",
            ),
            (
                "11111111-2222-3333-4444-555555550009",
                "Global Shipping Co",
                "business",
                "ar@globalshipping.com",
                "+1-555-0109",
                "999 Harbor Dr, Long Beach, CA 90802",
                "active",
            ),
            (
                "11111111-2222-3333-4444-555555550010",
                "Clean Janitorial Services",
                "business",
                "billing@cleanjanitorial.com",
                "+1-555-0110",
                "555 Clean St, Austin, TX 78701",
                "active",
            ),
            // Contractors
            (
                "11111111-2222-3333-4444-555555550011",
                "John Smith Consulting",
                "contractor",
                "john@jsconsulting.com",
                "+1-555-0111",
                "123 Freelance Ave, Portland, OR 97201",
                "active",
            ),
            (
                "11111111-2222-3333-4444-555555550012",
                "Jane Doe Design",
                "contractor",
                "jane@janedoedesign.com",
                "+1-555-0112",
                "456 Creative Blvd, Brooklyn, NY 11201",
                "active",
            ),
            (
                "11111111-2222-3333-4444-555555550013",
                "DevOps Solutions LLC",
                "contractor",
                "billing@devopssolutions.io",
                "+1-555-0113",
                "789 Code Lane, Denver, CO 80202",
                "active",
            ),
            (
                "11111111-2222-3333-4444-555555550014",
                "Marketing Maven Agency",
                "contractor",
                "invoices@marketingmaven.co",
                "+1-555-0114",
                "321 Brand St, Los Angeles, CA 90001",
                "active",
            ),
            (
                "11111111-2222-3333-4444-555555550015",
                "Legal Eagles LLP",
                "contractor",
                "accounts@legaleagles.law",
                "+1-555-0115",
                "100 Justice Way, Boston, MA 02101",
                "active",
            ),
            // Inactive vendor
            (
                "11111111-2222-3333-4444-555555550016",
                "Old Supplier Inc",
                "business",
                "ap@oldsupplier.com",
                "+1-555-0116",
                "999 Legacy Rd, Detroit, MI 48201",
                "inactive",
            ),
        ];

        for (id, name, vtype, email, phone, address, status) in &vendors {
            let id_uuid = uuid::Uuid::parse_str(id)?;
            sqlx::query(
                "INSERT INTO vendors (id, name, vendor_type, email, phone, address_line1, status, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
                 ON CONFLICT (id) DO NOTHING"
            )
            .bind(id_uuid)
            .bind(name)
            .bind(vtype)
            .bind(email)
            .bind(phone)
            .bind(address)
            .bind(status)
            .execute(pool)
            .await
            .ok();
        }

        // ==========================================================
        // Seed comprehensive invoices with realistic data
        // ==========================================================
        let admin_id = "17b66d9b-6da5-4cfb-93ad-f8d2f1aefe8f";
        let ap_queue = "11111111-4444-5555-6666-777777770002";
        let approval_queue = "11111111-4444-5555-6666-777777770003";
        let payment_queue = "11111111-4444-5555-6666-777777770004";
        let error_queue = "11111111-4444-5555-6666-777777770001";
        let hold_queue = "11111111-4444-5555-6666-777777770005";

        // Invoice data: (id, vendor_id, vendor_name, invoice_number, amount_cents, invoice_date, due_date, capture_status, processing_status, queue_id, department, gl_code, po_number, notes)
        let invoices = vec![
            // === PENDING REVIEW (AP Queue) ===
            (
                "aaaaaaaa-0001-0001-0001-000000000001",
                "11111111-2222-3333-4444-555555550001",
                "Acme Corporation",
                "ACME-2024-0156",
                245000,
                "2024-01-15",
                "2024-02-14",
                "ready_for_review",
                "submitted",
                ap_queue,
                "Operations",
                "5100",
                "PO-2024-001",
                "",
            ),
            (
                "aaaaaaaa-0001-0001-0001-000000000002",
                "11111111-2222-3333-4444-555555550002",
                "TechSupplies Inc",
                "TSI-78234",
                89900,
                "2024-01-16",
                "2024-02-15",
                "ready_for_review",
                "submitted",
                ap_queue,
                "IT",
                "6200",
                "PO-2024-012",
                "Network equipment order",
            ),
            (
                "aaaaaaaa-0001-0001-0001-000000000003",
                "11111111-2222-3333-4444-555555550003",
                "Office Depot",
                "OD-5567234",
                34575,
                "2024-01-17",
                "2024-02-16",
                "ready_for_review",
                "submitted",
                ap_queue,
                "Admin",
                "6100",
                "",
                "Monthly supplies",
            ),
            (
                "aaaaaaaa-0001-0001-0001-000000000004",
                "11111111-2222-3333-4444-555555550011",
                "John Smith Consulting",
                "JSC-2024-015",
                750000,
                "2024-01-18",
                "2024-02-17",
                "ready_for_review",
                "submitted",
                ap_queue,
                "HR",
                "7100",
                "",
                "Q1 HR consulting",
            ),
            (
                "aaaaaaaa-0001-0001-0001-000000000005",
                "11111111-2222-3333-4444-555555550012",
                "Jane Doe Design",
                "JDD-INV-0089",
                450000,
                "2024-01-19",
                "2024-02-18",
                "ready_for_review",
                "submitted",
                ap_queue,
                "Marketing",
                "7200",
                "PO-2024-023",
                "Brand refresh project",
            ),
            (
                "aaaaaaaa-0001-0001-0001-000000000006",
                "11111111-2222-3333-4444-555555550010",
                "Clean Janitorial Services",
                "CJS-JAN-2024",
                125000,
                "2024-01-20",
                "2024-02-19",
                "ready_for_review",
                "submitted",
                ap_queue,
                "Facilities",
                "6500",
                "",
                "January cleaning service",
            ),
            // === PENDING APPROVAL (Approval Queue) ===
            (
                "aaaaaaaa-0002-0002-0002-000000000001",
                "11111111-2222-3333-4444-555555550004",
                "Amazon Web Services",
                "AWS-2024-JAN",
                1523467,
                "2024-01-05",
                "2024-02-04",
                "reviewed",
                "pending_approval",
                approval_queue,
                "IT",
                "6300",
                "",
                "January cloud infrastructure",
            ),
            (
                "aaaaaaaa-0002-0002-0002-000000000002",
                "11111111-2222-3333-4444-555555550005",
                "Microsoft Azure",
                "MSAZ-987654",
                897500,
                "2024-01-06",
                "2024-02-05",
                "reviewed",
                "pending_approval",
                approval_queue,
                "IT",
                "6300",
                "",
                "Azure services - Jan",
            ),
            (
                "aaaaaaaa-0002-0002-0002-000000000003",
                "11111111-2222-3333-4444-555555550001",
                "Acme Corporation",
                "ACME-2024-0145",
                567800,
                "2024-01-07",
                "2024-02-06",
                "reviewed",
                "pending_approval",
                approval_queue,
                "Operations",
                "5100",
                "PO-2024-008",
                "Equipment maintenance",
            ),
            (
                "aaaaaaaa-0002-0002-0002-000000000004",
                "11111111-2222-3333-4444-555555550014",
                "Marketing Maven Agency",
                "MMA-Q1-2024",
                1250000,
                "2024-01-08",
                "2024-02-07",
                "reviewed",
                "pending_approval",
                approval_queue,
                "Marketing",
                "7300",
                "PO-2024-005",
                "Q1 marketing campaign",
            ),
            (
                "aaaaaaaa-0002-0002-0002-000000000005",
                "11111111-2222-3333-4444-555555550008",
                "Premium Insurance Group",
                "PIG-POL-2024",
                2400000,
                "2024-01-01",
                "2024-02-01",
                "reviewed",
                "pending_approval",
                approval_queue,
                "Admin",
                "6700",
                "",
                "Annual liability insurance",
            ),
            // === READY FOR PAYMENT (Payment Queue) ===
            (
                "aaaaaaaa-0003-0003-0003-000000000001",
                "11111111-2222-3333-4444-555555550007",
                "Utilities Co",
                "UC-JAN-2024",
                234500,
                "2024-01-10",
                "2024-02-09",
                "reviewed",
                "approved",
                payment_queue,
                "Facilities",
                "6400",
                "",
                "January utilities",
            ),
            (
                "aaaaaaaa-0003-0003-0003-000000000002",
                "11111111-2222-3333-4444-555555550002",
                "TechSupplies Inc",
                "TSI-78190",
                45600,
                "2024-01-11",
                "2024-02-10",
                "reviewed",
                "approved",
                payment_queue,
                "IT",
                "6200",
                "PO-2024-009",
                "Laptop accessories",
            ),
            (
                "aaaaaaaa-0003-0003-0003-000000000003",
                "11111111-2222-3333-4444-555555550009",
                "Global Shipping Co",
                "GSC-2024-0034",
                78900,
                "2024-01-12",
                "2024-02-11",
                "reviewed",
                "approved",
                payment_queue,
                "Operations",
                "5200",
                "",
                "Shipping charges - Jan",
            ),
            (
                "aaaaaaaa-0003-0003-0003-000000000004",
                "11111111-2222-3333-4444-555555550013",
                "DevOps Solutions LLC",
                "DOS-2024-JAN",
                350000,
                "2024-01-13",
                "2024-02-12",
                "reviewed",
                "approved",
                payment_queue,
                "IT",
                "7100",
                "",
                "Infrastructure consulting",
            ),
            (
                "aaaaaaaa-0003-0003-0003-000000000005",
                "11111111-2222-3333-4444-555555550003",
                "Office Depot",
                "OD-5567100",
                12345,
                "2024-01-14",
                "2024-02-13",
                "reviewed",
                "approved",
                payment_queue,
                "Admin",
                "6100",
                "",
                "Office supplies",
            ),
            // === PAID (No queue - completed) ===
            (
                "aaaaaaaa-0004-0004-0004-000000000001",
                "11111111-2222-3333-4444-555555550006",
                "Google Cloud",
                "GCP-DEC-2023",
                987600,
                "2023-12-01",
                "2024-01-01",
                "reviewed",
                "paid",
                "",
                "IT",
                "6300",
                "",
                "December cloud services",
            ),
            (
                "aaaaaaaa-0004-0004-0004-000000000002",
                "11111111-2222-3333-4444-555555550004",
                "Amazon Web Services",
                "AWS-2023-DEC",
                1245000,
                "2023-12-05",
                "2024-01-04",
                "reviewed",
                "paid",
                "",
                "IT",
                "6300",
                "",
                "December AWS",
            ),
            (
                "aaaaaaaa-0004-0004-0004-000000000003",
                "11111111-2222-3333-4444-555555550001",
                "Acme Corporation",
                "ACME-2023-0987",
                345000,
                "2023-12-10",
                "2024-01-09",
                "reviewed",
                "paid",
                "",
                "Operations",
                "5100",
                "PO-2023-456",
                "Year-end equipment",
            ),
            (
                "aaaaaaaa-0004-0004-0004-000000000004",
                "11111111-2222-3333-4444-555555550007",
                "Utilities Co",
                "UC-DEC-2023",
                215000,
                "2023-12-15",
                "2024-01-14",
                "reviewed",
                "paid",
                "",
                "Facilities",
                "6400",
                "",
                "December utilities",
            ),
            (
                "aaaaaaaa-0004-0004-0004-000000000005",
                "11111111-2222-3333-4444-555555550015",
                "Legal Eagles LLP",
                "LE-2023-Q4",
                890000,
                "2023-12-20",
                "2024-01-19",
                "reviewed",
                "paid",
                "",
                "Legal",
                "7400",
                "",
                "Q4 legal services",
            ),
            // === ON HOLD ===
            (
                "aaaaaaaa-0005-0005-0005-000000000001",
                "11111111-2222-3333-4444-555555550002",
                "TechSupplies Inc",
                "TSI-DISPUTE",
                156700,
                "2024-01-03",
                "2024-02-02",
                "reviewed",
                "on_hold",
                hold_queue,
                "IT",
                "6200",
                "PO-2024-003",
                "Disputed - wrong items received",
            ),
            (
                "aaaaaaaa-0005-0005-0005-000000000002",
                "11111111-2222-3333-4444-555555550011",
                "John Smith Consulting",
                "JSC-HOLD-001",
                500000,
                "2024-01-04",
                "2024-02-03",
                "reviewed",
                "on_hold",
                hold_queue,
                "HR",
                "7100",
                "",
                "Pending contract review",
            ),
            // === OCR ERRORS (Error Queue) ===
            (
                "aaaaaaaa-0006-0006-0006-000000000001",
                "",
                "Unknown Vendor",
                "UNREADABLE-001",
                0,
                "",
                "",
                "failed",
                "draft",
                error_queue,
                "",
                "",
                "",
                "OCR could not extract vendor info",
            ),
            (
                "aaaaaaaa-0006-0006-0006-000000000002",
                "",
                "Partially Readable",
                "???-12345",
                50000,
                "2024-01-18",
                "",
                "failed",
                "draft",
                error_queue,
                "",
                "",
                "",
                "Missing due date and some fields",
            ),
            // === REJECTED ===
            (
                "aaaaaaaa-0007-0007-0007-000000000001",
                "11111111-2222-3333-4444-555555550014",
                "Marketing Maven Agency",
                "MMA-REJECTED",
                2500000,
                "2024-01-02",
                "2024-02-01",
                "reviewed",
                "rejected",
                "",
                "Marketing",
                "7300",
                "",
                "Budget not approved for this campaign",
            ),
            // === OLDER INVOICES FOR AGING REPORT ===
            (
                "aaaaaaaa-0008-0008-0008-000000000001",
                "11111111-2222-3333-4444-555555550001",
                "Acme Corporation",
                "ACME-2023-OLD",
                125000,
                "2023-10-15",
                "2023-11-14",
                "reviewed",
                "pending_approval",
                approval_queue,
                "Operations",
                "5100",
                "",
                "Overdue - 60+ days",
            ),
            (
                "aaaaaaaa-0008-0008-0008-000000000002",
                "11111111-2222-3333-4444-555555550009",
                "Global Shipping Co",
                "GSC-2023-OLD",
                45000,
                "2023-11-01",
                "2023-12-01",
                "reviewed",
                "pending_approval",
                approval_queue,
                "Operations",
                "5200",
                "",
                "Overdue - 30-60 days",
            ),
            (
                "aaaaaaaa-0008-0008-0008-000000000003",
                "11111111-2222-3333-4444-555555550003",
                "Office Depot",
                "OD-2023-LATE",
                8900,
                "2023-09-15",
                "2023-10-15",
                "reviewed",
                "approved",
                payment_queue,
                "Admin",
                "6100",
                "",
                "Very overdue - 90+ days",
            ),
        ];

        for (
            id,
            vendor_id,
            vendor_name,
            invoice_number,
            amount,
            invoice_date,
            due_date,
            capture_status,
            processing_status,
            queue_id,
            department,
            gl_code,
            po_number,
            notes,
        ) in &invoices
        {
            let id_uuid = uuid::Uuid::parse_str(id)?;
            let vendor_id_val = if vendor_id.is_empty() {
                None
            } else {
                Some(uuid::Uuid::parse_str(vendor_id)?)
            };
            let invoice_date_val = if invoice_date.is_empty() {
                None
            } else {
                Some(chrono::NaiveDate::parse_from_str(invoice_date, "%Y-%m-%d")?)
            };
            let due_date_val = if due_date.is_empty() {
                None
            } else {
                Some(chrono::NaiveDate::parse_from_str(due_date, "%Y-%m-%d")?)
            };
            let queue_id_val = if queue_id.is_empty() {
                None
            } else {
                Some(uuid::Uuid::parse_str(queue_id)?)
            };
            let dept_val = if department.is_empty() {
                None
            } else {
                Some(*department)
            };
            let gl_val = if gl_code.is_empty() {
                None
            } else {
                Some(*gl_code)
            };
            let po_val = if po_number.is_empty() {
                None
            } else {
                Some(*po_number)
            };
            let notes_val = if notes.is_empty() { None } else { Some(*notes) };
            let admin_uuid = uuid::Uuid::parse_str(admin_id)?;
            let doc_uuid = uuid::Uuid::new_v4();

            sqlx::query(
                "INSERT INTO invoices (id, vendor_id, vendor_name, invoice_number, total_amount_cents, currency, invoice_date, due_date, capture_status, processing_status, current_queue_id, department, gl_code, po_number, notes, document_id, created_by, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, 'USD', $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, NOW(), NOW())
                 ON CONFLICT (id) DO NOTHING"
            )
            .bind(id_uuid)
            .bind(vendor_id_val)
            .bind(vendor_name)
            .bind(invoice_number)
            .bind(*amount as i64)
            .bind(invoice_date_val)
            .bind(due_date_val)
            .bind(capture_status)
            .bind(processing_status)
            .bind(queue_id_val)
            .bind(dept_val)
            .bind(gl_val)
            .bind(po_val)
            .bind(notes_val)
            .bind(doc_uuid)
            .bind(admin_uuid)
            .execute(pool)
            .await
            .ok();

            // Create queue_items for invoices in a queue
            if !queue_id.is_empty() {
                let item_id = uuid::Uuid::new_v4();
                let queue_uuid = uuid::Uuid::parse_str(queue_id)?;
                let priority = if *amount > 1000000 {
                    2
                } else if *amount > 500000 {
                    1
                } else {
                    0
                };
                sqlx::query(
                    "INSERT INTO queue_items (id, queue_id, invoice_id, priority, entered_at)
                     VALUES ($1, $2, $3, $4, NOW())
                     ON CONFLICT (id) DO NOTHING",
                )
                .bind(item_id)
                .bind(queue_uuid)
                .bind(id_uuid)
                .bind(priority)
                .execute(pool)
                .await
                .ok();
            }
        }

        // ==========================================================
        // Seed line items for some invoices
        // ==========================================================
        let line_items = vec![
            // AWS invoice line items
            (
                "aaaaaaaa-0002-0002-0002-000000000001",
                "EC2 Compute - On-Demand",
                2,
                456700,
                "EC2 instances",
            ),
            (
                "aaaaaaaa-0002-0002-0002-000000000001",
                "S3 Storage",
                1,
                234500,
                "Data storage",
            ),
            (
                "aaaaaaaa-0002-0002-0002-000000000001",
                "RDS Database",
                1,
                567890,
                "PostgreSQL RDS",
            ),
            (
                "aaaaaaaa-0002-0002-0002-000000000001",
                "Data Transfer",
                1,
                264377,
                "Outbound data",
            ),
            // Acme equipment invoice
            (
                "aaaaaaaa-0001-0001-0001-000000000001",
                "Industrial Widget A",
                50,
                2500,
                "Part# IW-001",
            ),
            (
                "aaaaaaaa-0001-0001-0001-000000000001",
                "Industrial Widget B",
                25,
                4800,
                "Part# IW-002",
            ),
            (
                "aaaaaaaa-0001-0001-0001-000000000001",
                "Mounting Hardware Kit",
                10,
                7000,
                "Part# MHK-100",
            ),
            // Office supplies
            (
                "aaaaaaaa-0001-0001-0001-000000000003",
                "Copy Paper - Case",
                10,
                2500,
                "",
            ),
            (
                "aaaaaaaa-0001-0001-0001-000000000003",
                "Pens - Box of 100",
                5,
                1200,
                "",
            ),
            (
                "aaaaaaaa-0001-0001-0001-000000000003",
                "Notebooks",
                20,
                575,
                "",
            ),
            // Marketing campaign
            (
                "aaaaaaaa-0002-0002-0002-000000000004",
                "Social Media Campaign",
                1,
                450000,
                "Instagram & Facebook",
            ),
            (
                "aaaaaaaa-0002-0002-0002-000000000004",
                "Google Ads Management",
                1,
                350000,
                "SEM services",
            ),
            (
                "aaaaaaaa-0002-0002-0002-000000000004",
                "Content Creation",
                1,
                250000,
                "Blog & video",
            ),
            (
                "aaaaaaaa-0002-0002-0002-000000000004",
                "Analytics & Reporting",
                1,
                200000,
                "Monthly reports",
            ),
        ];

        for (invoice_id, description, quantity, unit_price, notes) in &line_items {
            let line_id = uuid::Uuid::new_v4();
            let invoice_uuid = uuid::Uuid::parse_str(invoice_id)?;
            let total = quantity * unit_price;
            sqlx::query(
                "INSERT INTO invoice_line_items (id, invoice_id, description, quantity, unit_price_cents, total_amount_cents, notes)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT (id) DO NOTHING"
            )
            .bind(line_id)
            .bind(invoice_uuid)
            .bind(description)
            .bind(*quantity)
            .bind(*unit_price as i64)
            .bind(total as i64)
            .bind(notes)
            .execute(pool)
            .await
            .ok();
        }

        // ==========================================================
        // Seed approval requests for pending approval invoices
        // ==========================================================
        let approval_requests = vec![
            (
                "aaaaaaaa-0002-0002-0002-000000000001",
                admin_id,
                "Pending approval for AWS monthly invoice",
            ),
            (
                "aaaaaaaa-0002-0002-0002-000000000002",
                admin_id,
                "Azure services - requires manager sign-off",
            ),
            (
                "aaaaaaaa-0002-0002-0002-000000000003",
                admin_id,
                "Equipment maintenance agreement",
            ),
            (
                "aaaaaaaa-0002-0002-0002-000000000004",
                admin_id,
                "Marketing campaign - large spend",
            ),
            (
                "aaaaaaaa-0002-0002-0002-000000000005",
                admin_id,
                "Annual insurance premium - executive approval needed",
            ),
            (
                "aaaaaaaa-0008-0008-0008-000000000001",
                admin_id,
                "Overdue invoice - urgent",
            ),
            (
                "aaaaaaaa-0008-0008-0008-000000000002",
                admin_id,
                "Past due - needs immediate attention",
            ),
        ];

        for (invoice_id, requested_from, notes) in &approval_requests {
            let request_id = uuid::Uuid::new_v4();
            let invoice_uuid = uuid::Uuid::parse_str(invoice_id)?;
            let requested_uuid = uuid::Uuid::parse_str(requested_from)?;
            sqlx::query(
                "INSERT INTO approval_requests (id, invoice_id, requested_from, status, comments, created_at)
                 VALUES ($1, $2, $3, 'pending', $4, NOW())
                 ON CONFLICT (id) DO NOTHING"
            )
            .bind(request_id)
            .bind(invoice_uuid)
            .bind(requested_uuid)
            .bind(notes)
            .execute(pool)
            .await
            .ok();
        }

        // ==========================================================
        // Seed approval policies (multi-level approval engine)
        // ==========================================================
        let admin_uuid = uuid::Uuid::parse_str(admin_id)?;
        let approver_id = "22222222-2222-2222-2222-222222220002";

        // Policy 1: Standard Approval (default) — for invoices $500–$10,000
        let policy1_id = uuid::Uuid::parse_str("bbbbbbbb-0001-0001-0001-000000000001")?;
        sqlx::query(
            "INSERT INTO approval_policies (id, tenant_id, name, description, is_active, is_default,
             match_criteria, priority, require_sequential, require_all_levels, allow_self_approval,
             auto_approve_below_cents, escalation_enabled, escalation_timeout_hours, final_escalation_user_id,
             created_at, updated_at)
             VALUES ($1, $2, 'Standard Approval', 'Default approval policy for regular invoices',
             true, true, '{\"min_amount_cents\": 50000, \"max_amount_cents\": 1000000}'::jsonb,
             10, true, true, false, 50000, true, 48, $3, NOW(), NOW())
             ON CONFLICT (id) DO NOTHING"
        )
        .bind(policy1_id)
        .bind(tenant_uuid)
        .bind(admin_uuid)
        .execute(pool)
        .await
        .ok();

        // Level 1 for Standard: Manager approval (Lisa Wang)
        let level1_1_id = uuid::Uuid::parse_str("cccccccc-0001-0001-0001-000000000001")?;
        sqlx::query(
            "INSERT INTO approval_chain_levels (id, policy_id, tenant_id, level_order, name,
             approver_type, approver_user_ids, min_amount_cents, max_amount_cents,
             required_approver_count, timeout_hours, created_at, updated_at)
             VALUES ($1, $2, $3, 1, 'Manager Approval', 'user', $4::jsonb, 50000, 1000000, 1, 48, NOW(), NOW())
             ON CONFLICT (policy_id, level_order) DO NOTHING"
        )
        .bind(level1_1_id)
        .bind(policy1_id)
        .bind(tenant_uuid)
        .bind(serde_json::json!([approver_id]))
        .execute(pool)
        .await
        .ok();

        // Policy 2: High-Value Approval — for invoices > $10,000 (two-level)
        let policy2_id = uuid::Uuid::parse_str("bbbbbbbb-0002-0002-0002-000000000002")?;
        sqlx::query(
            "INSERT INTO approval_policies (id, tenant_id, name, description, is_active, is_default,
             match_criteria, priority, require_sequential, require_all_levels, allow_self_approval,
             auto_approve_below_cents, escalation_enabled, escalation_timeout_hours, final_escalation_user_id,
             created_at, updated_at)
             VALUES ($1, $2, 'High-Value Approval', 'Two-level approval for invoices over $10,000',
             true, false, '{\"min_amount_cents\": 1000000}'::jsonb,
             20, true, true, false, NULL, true, 24, $3, NOW(), NOW())
             ON CONFLICT (id) DO NOTHING"
        )
        .bind(policy2_id)
        .bind(tenant_uuid)
        .bind(admin_uuid)
        .execute(pool)
        .await
        .ok();

        // Level 1 for High-Value: Manager (Lisa Wang)
        let level2_1_id = uuid::Uuid::parse_str("cccccccc-0002-0001-0001-000000000001")?;
        sqlx::query(
            "INSERT INTO approval_chain_levels (id, policy_id, tenant_id, level_order, name,
             approver_type, approver_user_ids, min_amount_cents, max_amount_cents,
             required_approver_count, timeout_hours, created_at, updated_at)
             VALUES ($1, $2, $3, 1, 'Manager Approval', 'user', $4::jsonb, 1000000, NULL, 1, 24, NOW(), NOW())
             ON CONFLICT (policy_id, level_order) DO NOTHING"
        )
        .bind(level2_1_id)
        .bind(policy2_id)
        .bind(tenant_uuid)
        .bind(serde_json::json!([approver_id]))
        .execute(pool)
        .await
        .ok();

        // Level 2 for High-Value: Executive (Sarah Chen - admin)
        let level2_2_id = uuid::Uuid::parse_str("cccccccc-0002-0002-0002-000000000002")?;
        sqlx::query(
            "INSERT INTO approval_chain_levels (id, policy_id, tenant_id, level_order, name,
             approver_type, approver_user_ids, min_amount_cents, max_amount_cents,
             required_approver_count, timeout_hours, created_at, updated_at)
             VALUES ($1, $2, $3, 2, 'Executive Approval', 'user', $4::jsonb, 1000000, NULL, 1, 48, NOW(), NOW())
             ON CONFLICT (policy_id, level_order) DO NOTHING"
        )
        .bind(level2_2_id)
        .bind(policy2_id)
        .bind(tenant_uuid)
        .bind(serde_json::json!([admin_id]))
        .execute(pool)
        .await
        .ok();

        tracing::info!("Seeded {} queues, {} assignment rules, {} vendors, {} invoices, {} line items, {} approval requests, and 2 approval policies",
            queues.len(), assignment_rules.len(), vendors.len(), invoices.len(), line_items.len(), approval_requests.len());
        Ok(())
    }
}
