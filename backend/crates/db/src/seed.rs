//! Database seeding for pilot customers
//!
//! Creates 5 mock pilot tenants with realistic data for testing and demos

use anyhow::Result;
use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// Pilot customer profiles
const PILOT_CUSTOMERS: &[(&str, &str)] = &[
    ("Acme Manufacturing", "acme-mfg"),
    ("TechFlow Solutions", "techflow"),
    ("GreenLeaf Healthcare", "greenleaf"),
    ("Metro Retail Group", "metro-retail"),
    ("Pacific Trading Co", "pacific-trading"),
];

/// Seed pilot customers with realistic data
pub async fn seed_pilot_customers(pool: &PgPool) -> Result<()> {
    println!("🌱 Seeding pilot customers...");

    for (name, slug) in PILOT_CUSTOMERS {
        seed_tenant(pool, name, slug).await?;
    }

    println!("✅ Successfully seeded {} pilot customers", PILOT_CUSTOMERS.len());
    Ok(())
}

async fn seed_tenant(pool: &PgPool, name: &str, slug: &str) -> Result<()> {
    println!("  Creating tenant: {} ({})", name, slug);

    // Create tenant in control plane
    let tenant_id = Uuid::new_v4();
    let db_name = format!("billforge_tenant_{}", slug.replace('-', "_"));

    sqlx::query(
        r#"
        INSERT INTO tenants (id, name, slug, settings, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (slug) DO UPDATE SET name = EXCLUDED.name
        RETURNING id
        "#,
    )
    .bind(tenant_id)
    .bind(name)
    .bind(slug)
    .bind(serde_json::json!({
        "modules": ["invoice_capture", "invoice_processing", "vendor_mgmt"],
        "privacy_mode": false,
        "ocr_provider": "auto",
        "timezone": "America/New_York"
    }))
    .bind(Utc::now())
    .bind(Utc::now())
    .fetch_one(pool)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO tenant_databases (tenant_id, database_name, status, created_at)
        VALUES ($1, $2, 'active', $3)
        ON CONFLICT (database_name) DO NOTHING
        "#,
    )
    .bind(tenant_id)
    .bind(&db_name)
    .bind(Utc::now())
    .execute(pool)
    .await?;

    // Create tenant database and seed data
    create_tenant_database(pool, &db_name, tenant_id, name).await?;

    Ok(())
}

async fn create_tenant_database(
    pool: &PgPool,
    db_name: &str,
    tenant_id: Uuid,
    tenant_name: &str,
) -> Result<()> {
    // Create database
    sqlx::query(&format!("CREATE DATABASE {}", db_name))
        .execute(pool)
        .await
        .ok(); // Ignore if exists

    // Connect to tenant database
    let tenant_url = format!(
        "postgres://billforge:billforge_dev@localhost:5432/{}",
        db_name
    );
    let tenant_pool = PgPool::connect(&tenant_url).await?;

    // Run migrations (simplified - just create tables)
    create_tenant_tables(&tenant_pool).await?;

    // Seed users
    let users = seed_users(&tenant_pool, tenant_id, tenant_name).await?;

    // Seed vendors
    let vendors = seed_vendors(&tenant_pool, tenant_id, tenant_name).await?;

    // Seed invoices
    seed_invoices(&tenant_pool, tenant_id, &users, &vendors).await?;

    // Seed default queues
    seed_default_queues(&tenant_pool, tenant_id).await?;

    // Seed default invoice status config
    seed_default_status_config(&tenant_pool, tenant_id).await?;

    Ok(())
}

async fn create_tenant_tables(pool: &PgPool) -> Result<()> {
    // Users table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL,
            email TEXT NOT NULL,
            password_hash TEXT NOT NULL,
            name TEXT NOT NULL,
            roles JSONB NOT NULL DEFAULT '[]',
            settings JSONB NOT NULL DEFAULT '{}',
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE(tenant_id, email)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Vendors table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS vendors (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL,
            name TEXT NOT NULL,
            tax_id TEXT,
            payment_terms INTEGER DEFAULT 30,
            default_gl_code TEXT,
            status TEXT DEFAULT 'active',
            metadata JSONB DEFAULT '{}',
            created_at TIMESTAMPTZ DEFAULT NOW(),
            updated_at TIMESTAMPTZ DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Work queues table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS work_queues (
            id UUID PRIMARY KEY,
            tenant_id VARCHAR(255) NOT NULL,
            name VARCHAR(255) NOT NULL,
            description TEXT,
            queue_type VARCHAR(50) NOT NULL,
            assigned_users JSONB DEFAULT '[]',
            assigned_roles JSONB DEFAULT '[]',
            is_default BOOLEAN NOT NULL DEFAULT false,
            is_active BOOLEAN NOT NULL DEFAULT true,
            settings JSONB NOT NULL DEFAULT '{}',
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Invoices table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS invoices (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL,
            vendor_id UUID REFERENCES vendors(id),
            vendor_name TEXT NOT NULL,
            invoice_number TEXT NOT NULL,
            invoice_date DATE,
            due_date DATE,
            total_amount_cents BIGINT NOT NULL,
            currency TEXT NOT NULL DEFAULT 'USD',
            line_items JSONB NOT NULL DEFAULT '[]',
            capture_status TEXT NOT NULL DEFAULT 'pending',
            processing_status TEXT NOT NULL DEFAULT 'draft',
            current_queue_id UUID,
            assigned_to UUID REFERENCES users(id),
            document_id UUID NOT NULL,
            ocr_confidence REAL,
            created_by UUID NOT NULL REFERENCES users(id),
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE(tenant_id, invoice_number)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Invoice status config table
    sqlx::query(
        r#"
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
        )
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

async fn seed_users(pool: &PgPool, tenant_id: Uuid, tenant_name: &str) -> Result<Vec<(Uuid, String)>> {
    let mut users = Vec::new();

    let user_templates = vec![
        ("AP Clerk", "ap@example.com", vec!["ap_clerk"]),
        ("AP Manager", "ap.manager@example.com", vec!["ap_manager"]),
        ("Controller", "controller@example.com", vec!["controller", "approver_l2"]),
        ("CFO", "cfo@example.com", vec!["cfo", "approver_l3"]),
    ];

    // Use a dummy hash for seed data (not for real auth)
    let dummy_hash = "$argon2id$v=19$m=4096,t=3,p=1$mock$hash";

    for (role_title, email, roles) in user_templates {
        let user_id = Uuid::new_v4();
        let full_name = format!("{} - {}", tenant_name, role_title);

        sqlx::query(
            r#"
            INSERT INTO users (id, tenant_id, email, password_hash, name, roles, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (tenant_id, email) DO UPDATE SET name = EXCLUDED.name
            "#,
        )
        .bind(user_id)
        .bind(tenant_id)
        .bind(email)
        .bind(dummy_hash)
        .bind(&full_name)
        .bind(serde_json::to_value(&roles)?)
        .bind(Utc::now())
        .bind(Utc::now())
        .execute(pool)
        .await?;

        users.push((user_id, full_name));
    }

    println!("    ✓ Created {} users", users.len());
    Ok(users)
}

async fn seed_vendors(pool: &PgPool, tenant_id: Uuid, tenant_name: &str) -> Result<Vec<(Uuid, String)>> {
    let mut vendors = Vec::new();

    // Industry-specific vendors
    let vendor_names = match tenant_name {
        "Acme Manufacturing" => vec![
            "Steel Dynamics Corp",
            "Industrial Parts Supply",
            "Precision Tools Inc",
            "Safety First Equipment",
            "Raw Materials Co",
        ],
        "TechFlow Solutions" => vec![
            "AWS Services",
            "Google Cloud Platform",
            "Microsoft Azure",
            "DigitalOcean",
            "CloudFlare Inc",
        ],
        "GreenLeaf Healthcare" => vec![
            "MedSupply Corp",
            "Pharmaceuticals Direct",
            "Medical Equipment Co",
            "Lab Services Inc",
            "Healthcare Tech Solutions",
        ],
        "Metro Retail Group" => vec![
            "Wholesale Distributors",
            "Marketing Services LLC",
            "POS Systems Inc",
            "Retail Fixtures Co",
            "Shipping Partners",
        ],
        "Pacific Trading Co" => vec![
            "Import Export Corp",
            "Shipping Lines Intl",
            "Customs Brokers Inc",
            "Warehouse Services",
            "Logistics Partners",
        ],
        _ => vec!["Generic Vendor", "Office Supplies", "Utilities Co"],
    };

    for vendor_name in vendor_names {
        let vendor_id = Uuid::new_v4();

        sqlx::query(
            r#"
            INSERT INTO vendors (id, tenant_id, name, payment_terms, status, created_at)
            VALUES ($1, $2, $3, $4, 'active', $5)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(vendor_id)
        .bind(tenant_id)
        .bind(vendor_name)
        .bind(30i32)
        .bind(Utc::now())
        .execute(pool)
        .await?;

        vendors.push((vendor_id, vendor_name.to_string()));
    }

    println!("    ✓ Created {} vendors", vendors.len());
    Ok(vendors)
}

async fn seed_invoices(
    pool: &PgPool,
    tenant_id: Uuid,
    users: &[(Uuid, String)],
    vendors: &[(Uuid, String)],
) -> Result<()> {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    let statuses = [("captured", "pending_review"),
        ("captured", "in_review"),
        ("approved", "approved"),
        ("approved", "exported"),
        ("rejected", "rejected")];

    let mut invoice_count = 0;

    // Create 50-100 invoices per tenant
    let num_invoices = rng.gen_range(50..=100);

    for i in 0..num_invoices {
        let (vendor_id, vendor_name) = &vendors[rng.gen_range(0..vendors.len())];
        let (user_id, _user_name) = &users[0]; // AP Clerk creates

        let invoice_number = format!("INV-{:06}", rng.gen_range(100000..999999));
        let invoice_date = Utc::now() - Duration::days(rng.gen_range(0..90));
        let due_date = invoice_date + Duration::days(30);
        let total_cents = rng.gen_range(10000..5000000) as i64; // $100 - $50,000
        let (capture_status, processing_status) = &statuses[rng.gen_range(0..statuses.len())];

        let line_items = serde_json::json!([
            {
                "description": format!("Line item {}", i + 1),
                "quantity": 1,
                "unit_price_cents": total_cents,
                "amount_cents": total_cents
            }
        ]);

        sqlx::query(
            r#"
            INSERT INTO invoices (
                id, tenant_id, vendor_id, vendor_name, invoice_number,
                invoice_date, due_date, total_amount_cents, currency,
                line_items, capture_status, processing_status,
                document_id, ocr_confidence, created_by, created_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16
            )
            ON CONFLICT (tenant_id, invoice_number) DO NOTHING
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id)
        .bind(vendor_id)
        .bind(vendor_name)
        .bind(&invoice_number)
        .bind(invoice_date.date_naive())
        .bind(due_date.date_naive())
        .bind(total_cents)
        .bind("USD")
        .bind(&line_items)
        .bind(capture_status)
        .bind(processing_status)
        .bind(Uuid::new_v4())
        .bind(rng.gen_range(0.75..0.98) as f32)
        .bind(user_id)
        .bind(Utc::now() - Duration::days(rng.gen_range(0..30)))
        .execute(pool)
        .await?;

        invoice_count += 1;
    }

    println!("    ✓ Created {} invoices", invoice_count);
    Ok(())
}

async fn seed_default_queues(pool: &PgPool, tenant_id: Uuid) -> Result<()> {
    let default_queues = vec![
        ("AP Processing", "review", "Main AP processing queue for incoming invoices", true, 24, 48),
        ("Review Queue", "review", "Secondary review queue for flagged invoices", false, 16, 32),
        ("Error Queue", "exception", "Queue for invoices with processing errors", false, 8, 16),
        ("Approval Queue", "approval", "Queue for invoices pending approval", false, 24, 48),
        ("Payment Queue", "payment", "Queue for approved invoices ready for payment", false, 48, 72),
    ];

    for (name, queue_type, description, is_default, sla_hours, escalation_hours) in &default_queues {
        let settings = serde_json::json!({
            "default_sort": "priority_desc",
            "sla_hours": sla_hours,
            "escalation_hours": escalation_hours,
        });

        sqlx::query(
            r#"
            INSERT INTO work_queues (id, tenant_id, name, description, queue_type, is_default, settings, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id.to_string())
        .bind(name)
        .bind(description)
        .bind(queue_type)
        .bind(is_default)
        .bind(&settings)
        .bind(Utc::now())
        .bind(Utc::now())
        .execute(pool)
        .await?;
    }

    println!("    ✓ Created {} default queues", default_queues.len());
    Ok(())
}

async fn seed_default_status_config(pool: &PgPool, tenant_id: Uuid) -> Result<()> {
    let statuses = vec![
        // Processing statuses
        ("draft", "Draft", "gray", "bg-secondary", "text-muted-foreground", 0, false, "processing"),
        ("submitted", "Submitted", "blue", "bg-primary/10", "text-primary", 1, false, "processing"),
        ("pending_approval", "Pending Approval", "yellow", "bg-warning/10", "text-warning", 2, false, "processing"),
        ("approved", "Approved", "green", "bg-success/10", "text-success", 3, false, "processing"),
        ("rejected", "Rejected", "red", "bg-error/10", "text-error", 4, true, "processing"),
        ("on_hold", "On Hold", "yellow", "bg-warning/10", "text-warning", 5, false, "processing"),
        ("ready_for_payment", "Ready for Payment", "green", "bg-success/10", "text-success", 6, false, "processing"),
        ("paid", "Paid", "green", "bg-success/10", "text-success", 7, true, "processing"),
        ("voided", "Voided", "gray", "bg-secondary", "text-muted-foreground", 8, true, "processing"),
        // Capture statuses
        ("pending", "Pending", "yellow", "bg-warning/10", "text-warning", 0, false, "capture"),
        ("processing", "Processing", "blue", "bg-primary/10", "text-primary", 1, false, "capture"),
        ("ready_for_review", "Ready for Review", "yellow", "bg-warning/10", "text-warning", 2, false, "capture"),
        ("reviewed", "Reviewed", "green", "bg-success/10", "text-success", 3, true, "capture"),
        ("failed", "Failed", "red", "bg-error/10", "text-error", 4, true, "capture"),
    ];

    for (status_key, display_label, color, bg_color, text_color, sort_order, is_terminal, category) in &statuses {
        sqlx::query(
            r#"
            INSERT INTO invoice_status_config
                (id, tenant_id, status_key, display_label, color, bg_color, text_color,
                 sort_order, is_terminal, is_active, category, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, true, $10, $11, $12)
            ON CONFLICT (tenant_id, status_key) DO NOTHING
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id)
        .bind(status_key)
        .bind(display_label)
        .bind(color)
        .bind(bg_color)
        .bind(text_color)
        .bind(sort_order)
        .bind(is_terminal)
        .bind(category)
        .bind(Utc::now())
        .bind(Utc::now())
        .execute(pool)
        .await?;
    }

    println!("    ✓ Created {} default invoice status configs", statuses.len());
    Ok(())
}
