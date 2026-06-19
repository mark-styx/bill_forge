//! Tenant-specific database utilities
//!
//! Provides migration functions for tenant databases

use billforge_core::{Result, TenantId};
use sqlx::PgPool;

async fn apply_migration(pool: &PgPool, name: &str, sql: &str) -> Result<()> {
    crate::migrations::MigrationRunner::new()
        .apply(pool, name, sql)
        .await
}

/// Run all migrations for a tenant database
pub async fn run_tenant_migrations(pool: &PgPool, _tenant_id: &TenantId) -> Result<()> {
    // Migration 002: Users table
    let migration_002 = include_str!("../../../migrations/002_create_users.sql");
    apply_migration(pool, "002_create_users.sql", migration_002).await?;

    // Migration 003: Vendors table
    let migration_003 = include_str!("../../../migrations/003_create_vendors.sql");
    apply_migration(pool, "003_create_vendors.sql", migration_003).await?;

    // Migration 004: Invoices table
    let migration_004 = include_str!("../../../migrations/004_create_invoices.sql");
    apply_migration(pool, "004_create_invoices.sql", migration_004).await?;

    // Additional tenant-specific tables (work queues, workflow rules, etc.)
    run_workflow_migrations(pool).await?;
    run_vendor_statement_migrations(pool).await?;
    run_reconciliation_migrations(pool).await?;
    run_purchase_order_migrations(pool).await?;
    run_edi_outbound_migrations(pool).await?;
    run_payment_request_migrations(pool).await?;
    run_categorization_migrations(pool).await?;
    run_invoice_state_machine_migrations(pool).await?;
    run_dashboard_kpis_migrations(pool).await?;
    run_rls_migrations(pool).await?;
    run_ai_conversation_migrations(pool).await?;
    run_ai_rls_migrations(pool).await?;
    run_theme_migrations(pool).await?;
    run_implementation_migrations(pool).await?;
    run_banking_verification_migrations(pool).await?;

    Ok(())
}

/// Run workflow-related migrations (public so it can be re-run on existing tenants)
pub async fn run_workflow_migrations(pool: &PgPool) -> Result<()> {
    // Core workflow tables from canonical migration file
    let migration_005 = include_str!("../../../migrations/005_create_workflow_tables.sql");
    apply_migration(pool, "005_create_workflow_tables.sql", migration_005).await?;

    // Workflow templates from canonical migration file
    let migration_057 = include_str!("../../../migrations/057_create_workflow_templates.sql");
    apply_migration(pool, "057_create_workflow_templates.sql", migration_057).await?;

    let migration_091 = include_str!("../../../migrations/091_approval_sla_tracking.sql");
    apply_migration(pool, "091_approval_sla_tracking.sql", migration_091).await?;

    apply_migration(
        pool,
        "093_harden_workflow_tenant_id_uuid_types.sql",
        include_str!("../../../migrations/093_harden_workflow_tenant_id_uuid_types.sql"),
    )
    .await?;

    // OCR field calibration table for confidence scoring
    apply_migration(
        pool,
        "102_create_ocr_field_calibration.sql",
        include_str!("../../../migrations/102_create_ocr_field_calibration.sql"),
    )
    .await?;

    // Non-workflow tables that were historically bundled here
    apply_migration(
        pool,
        "inline_workflow_support_tables.sql",
        r#"
        -- Documents table
        CREATE TABLE IF NOT EXISTS documents (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL,
            filename TEXT NOT NULL,
            mime_type TEXT NOT NULL,
            size_bytes BIGINT NOT NULL,
            storage_key TEXT NOT NULL,
            invoice_id UUID REFERENCES invoices(id) ON DELETE SET NULL,
            doc_type TEXT NOT NULL DEFAULT 'invoice_original',
            uploaded_by UUID NOT NULL REFERENCES users(id),
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

        -- Generic audit log
        CREATE TABLE IF NOT EXISTS audit_log (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL,
            user_id UUID NOT NULL REFERENCES users(id),
            action TEXT NOT NULL,
            resource_type TEXT NOT NULL,
            resource_id TEXT,
            changes JSONB,
            ip_address TEXT,
            user_agent TEXT,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

        -- Ensure vendor_type column exists on vendors table
        ALTER TABLE vendors ADD COLUMN IF NOT EXISTS vendor_type TEXT NOT NULL DEFAULT 'business';
        ALTER TABLE vendors ADD COLUMN IF NOT EXISTS email TEXT;
        ALTER TABLE vendors ADD COLUMN IF NOT EXISTS phone TEXT;
        ALTER TABLE vendors ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'active';
        ALTER TABLE vendors ADD COLUMN IF NOT EXISTS address_line1 TEXT;

        -- Vendor routing rules and payment_terms_days (migration 077)
        ALTER TABLE vendors ADD COLUMN IF NOT EXISTS payment_terms_days INT NOT NULL DEFAULT 30;
        ALTER TABLE vendors ADD COLUMN IF NOT EXISTS routing_rules JSONB NOT NULL DEFAULT '{}';
        CREATE UNIQUE INDEX IF NOT EXISTS idx_vendors_tenant_tax_id ON vendors(tenant_id, tax_id) WHERE tax_id IS NOT NULL;
        CREATE INDEX IF NOT EXISTS idx_vendors_tenant_status ON vendors(tenant_id, status);

        -- Invoice status config
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

        -- Approval limits
        CREATE TABLE IF NOT EXISTS approval_limits (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL,
            user_id UUID NOT NULL REFERENCES users(id),
            max_amount_cents BIGINT NOT NULL,
            currency VARCHAR(3) NOT NULL DEFAULT 'USD',
            vendor_restrictions JSONB,
            department_restrictions JSONB,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

        -- Indexes for non-workflow tables
        CREATE INDEX IF NOT EXISTS idx_documents_tenant ON documents(tenant_id);
        CREATE INDEX IF NOT EXISTS idx_audit_log_tenant ON audit_log(tenant_id);
        CREATE INDEX IF NOT EXISTS idx_audit_log_resource ON audit_log(resource_type, resource_id);

        -- EDI connections
        CREATE TABLE IF NOT EXISTS edi_connections (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL UNIQUE,
            provider VARCHAR(50) NOT NULL,
            api_key_encrypted TEXT NOT NULL,
            webhook_secret TEXT NOT NULL,
            api_base_url TEXT,
            our_isa_qualifier VARCHAR(10),
            our_isa_id VARCHAR(50),
            is_active BOOLEAN NOT NULL DEFAULT true,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

        -- EDI documents (inbound and outbound)
        CREATE TABLE IF NOT EXISTS edi_documents (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL,
            document_type VARCHAR(20) NOT NULL,
            direction VARCHAR(10) NOT NULL,
            interchange_control VARCHAR(50),
            sender_id VARCHAR(50),
            receiver_id VARCHAR(50),
            status VARCHAR(20) NOT NULL DEFAULT 'received',
            invoice_id UUID REFERENCES invoices(id),
            raw_payload JSONB NOT NULL,
            mapped_data JSONB,
            error_message TEXT,
            ack_status VARCHAR(20),
            ack_received_at TIMESTAMPTZ,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            processed_at TIMESTAMPTZ
        );

        -- EDI trading partners
        CREATE TABLE IF NOT EXISTS edi_trading_partners (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL,
            name VARCHAR(255) NOT NULL,
            edi_qualifier VARCHAR(10),
            edi_id VARCHAR(50) NOT NULL,
            vendor_id UUID REFERENCES vendors(id),
            is_active BOOLEAN NOT NULL DEFAULT true,
            settings JSONB NOT NULL DEFAULT '{}',
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE(tenant_id, edi_id)
        );

        CREATE INDEX IF NOT EXISTS idx_edi_documents_tenant ON edi_documents(tenant_id);
        CREATE INDEX IF NOT EXISTS idx_edi_documents_status ON edi_documents(tenant_id, status);
        CREATE INDEX IF NOT EXISTS idx_edi_partners_tenant ON edi_trading_partners(tenant_id);

        -- EDI webhook nonce deduplication (replay protection)
        CREATE TABLE IF NOT EXISTS edi_webhook_nonces (
            tenant_id UUID NOT NULL,
            nonce VARCHAR(128) NOT NULL,
            received_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            PRIMARY KEY (tenant_id, nonce)
        );
        CREATE INDEX IF NOT EXISTS idx_edi_nonces_received_at ON edi_webhook_nonces(received_at);
        "#,
    )
    .await?;

    Ok(())
}

/// Run vendor statement migrations
async fn run_vendor_statement_migrations(pool: &PgPool) -> Result<()> {
    apply_migration(
        pool,
        "inline_vendor_statement_support_tables.sql",
        r#"
        -- Invoice line items
        CREATE TABLE IF NOT EXISTS invoice_line_items (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            invoice_id UUID NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
            line_number INTEGER NOT NULL,
            description TEXT NOT NULL,
            quantity REAL,
            unit_price_amount BIGINT,
            unit_price_currency TEXT,
            total_amount BIGINT NOT NULL,
            total_currency TEXT NOT NULL DEFAULT 'USD',
            gl_code TEXT,
            department TEXT,
            project TEXT
        );

        -- Vendor contacts
        CREATE TABLE IF NOT EXISTS vendor_contacts (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            vendor_id UUID NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            title TEXT,
            email TEXT,
            phone TEXT,
            is_primary BOOLEAN NOT NULL DEFAULT false
        );

        -- Vendor bank accounts
        CREATE TABLE IF NOT EXISTS vendor_bank_accounts (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            vendor_id UUID NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
            bank_name TEXT NOT NULL,
            account_type TEXT NOT NULL,
            account_last_four TEXT NOT NULL,
            account_number_encrypted TEXT NOT NULL,
            routing_number_encrypted TEXT NOT NULL,
            is_primary BOOLEAN NOT NULL DEFAULT false,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

        -- Vendor statement settings
        CREATE TABLE IF NOT EXISTS vendor_statement_settings (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            vendor_id UUID NOT NULL UNIQUE REFERENCES vendors(id) ON DELETE CASCADE,
            is_enabled BOOLEAN NOT NULL DEFAULT true,
            request_frequency TEXT NOT NULL DEFAULT 'monthly',
            day_of_month INTEGER DEFAULT 1,
            day_of_week TEXT,
            contact_email TEXT,
            cc_emails JSONB,
            custom_message TEXT,
            statement_period_type TEXT NOT NULL DEFAULT 'previous_month',
            auto_send_reminders BOOLEAN NOT NULL DEFAULT true,
            reminder_days_after INTEGER DEFAULT 7,
            max_reminders INTEGER DEFAULT 3,
            next_request_date DATE,
            last_request_date DATE,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

        -- Vendor statements
        CREATE TABLE IF NOT EXISTS vendor_statements (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL,
            vendor_id UUID NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
            settings_id UUID REFERENCES vendor_statement_settings(id),
            statement_period_start DATE NOT NULL,
            statement_period_end DATE NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            request_sent_at TIMESTAMPTZ,
            request_sent_to TEXT,
            reminder_count INTEGER NOT NULL DEFAULT 0,
            last_reminder_at TIMESTAMPTZ,
            received_at TIMESTAMPTZ,
            received_from TEXT,
            document_id UUID REFERENCES documents(id),
            review_status TEXT DEFAULT 'pending',
            reviewer_id UUID REFERENCES users(id),
            reviewed_at TIMESTAMPTZ,
            review_notes TEXT,
            discrepancies_found BOOLEAN DEFAULT false,
            discrepancy_amount BIGINT DEFAULT 0,
            discrepancy_currency TEXT DEFAULT 'USD',
            discrepancy_notes TEXT,
            resolution_status TEXT,
            resolution_notes TEXT,
            resolved_at TIMESTAMPTZ,
            resolved_by UUID REFERENCES users(id),
            upload_token TEXT,
            upload_token_expires_at TIMESTAMPTZ,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

        -- Indexes
        CREATE INDEX IF NOT EXISTS idx_line_items_invoice ON invoice_line_items(invoice_id);
        CREATE INDEX IF NOT EXISTS idx_vendor_contacts_vendor ON vendor_contacts(vendor_id);
        CREATE INDEX IF NOT EXISTS idx_vendor_bank_accounts_vendor ON vendor_bank_accounts(vendor_id);
        CREATE INDEX IF NOT EXISTS idx_vendor_statements_tenant ON vendor_statements(tenant_id);
        CREATE INDEX IF NOT EXISTS idx_vendor_statements_vendor ON vendor_statements(vendor_id);
        CREATE INDEX IF NOT EXISTS idx_vendor_statements_status ON vendor_statements(tenant_id, status);
        "#,
    )
    .await?;

    Ok(())
}

/// Run purchase order and 3-way matching migrations
pub async fn run_purchase_order_migrations(pool: &PgPool) -> Result<()> {
    apply_migration(
        pool,
        "065_create_purchase_orders.sql",
        include_str!("../../../migrations/065_create_purchase_orders.sql"),
    )
    .await?;

    Ok(())
}

/// Run EDI outbound and ack tracking migrations
pub async fn run_edi_outbound_migrations(pool: &PgPool) -> Result<()> {
    apply_migration(
        pool,
        "066_edi_outbound_ack_tracking.sql",
        include_str!("../../../migrations/066_edi_outbound_ack_tracking.sql"),
    )
    .await?;

    Ok(())
}

/// Run payment request migrations.
pub async fn run_payment_request_migrations(pool: &PgPool) -> Result<()> {
    apply_migration(
        pool,
        "069_create_payment_requests.sql",
        include_str!("../../../migrations/069_create_payment_requests.sql"),
    )
    .await?;

    apply_migration(
        pool,
        "094_add_queue_items_updated_at.sql",
        include_str!("../../../migrations/094_add_queue_items_updated_at.sql"),
    )
    .await?;

    Ok(())
}

/// Run implementation wizard state migrations.
pub async fn run_implementation_migrations(pool: &PgPool) -> Result<()> {
    apply_migration(
        pool,
        "095_create_implementation_wizard_state.sql",
        include_str!("../../../migrations/095_create_implementation_wizard_state.sql"),
    )
    .await?;

    // Invoice capture table (idempotent IF NOT EXISTS)
    apply_migration(
        pool,
        "075_create_invoice_capture.sql",
        include_str!("../../../migrations/075_create_invoice_capture.sql"),
    )
    .await?;

    // source_email_id column on invoices — traces OCR jobs back to inbound email
    apply_migration(
        pool,
        "099_add_source_email_to_invoices.sql",
        include_str!("../../../migrations/099_add_source_email_to_invoices.sql"),
    )
    .await?;

    // Early-payment discount optimizer columns + tenant settings
    apply_migration(
        pool,
        "100_early_payment_discounts.sql",
        include_str!("../../../migrations/100_early_payment_discounts.sql"),
    )
    .await?;

    // Single-use approval token store (persists across server restarts)
    apply_migration(
        pool,
        "101_create_used_approval_tokens.sql",
        include_str!("../../../migrations/101_create_used_approval_tokens.sql"),
    )
    .await?;

    Ok(())
}

/// Run categorization ML migrations (vendor embeddings, feedback, metrics)
async fn run_categorization_migrations(pool: &PgPool) -> Result<()> {
    // Enable pgvector extension
    apply_migration(
        pool,
        "inline_enable_pgvector.sql",
        "CREATE EXTENSION IF NOT EXISTS vector",
    )
    .await?;

    let migration_048 = include_str!("../../../migrations/048_add_categorization_ml.sql");
    apply_migration(pool, "048_add_categorization_ml.sql", migration_048).await?;

    let migration_049 = include_str!("../../../migrations/049_add_categorization_confidence.sql");
    apply_migration(pool, "049_add_categorization_confidence.sql", migration_049).await?;

    Ok(())
}

/// Run vendor statement reconciliation migrations (vendor_statement_lines table)
async fn run_reconciliation_migrations(pool: &PgPool) -> Result<()> {
    apply_migration(
        pool,
        "068_create_vendor_statements.sql",
        include_str!("../../../migrations/068_create_vendor_statements.sql"),
    )
    .await?;

    Ok(())
}

/// Run invoice status state machine and audit log migrations
pub async fn run_invoice_state_machine_migrations(pool: &PgPool) -> Result<()> {
    apply_migration(
        pool,
        "076_invoice_status_and_audit.sql",
        include_str!("../../../migrations/076_invoice_status_and_audit.sql"),
    )
    .await?;

    Ok(())
}

/// Run dashboard KPIs materialized view migration
pub async fn run_dashboard_kpis_migrations(pool: &PgPool) -> Result<()> {
    apply_migration(
        pool,
        "078_dashboard_kpis.sql",
        include_str!("../../../migrations/078_dashboard_kpis.sql"),
    )
    .await?;

    Ok(())
}

/// Enable Row Level Security on core tenant tables (invoices, users, vendors).
///
/// Defense-in-depth: even if application code omits `WHERE tenant_id = $1`,
/// Postgres will filter rows to only those matching the session variable
/// `app.current_tenant_id`.
pub async fn run_rls_migrations(pool: &PgPool) -> Result<()> {
    apply_migration(
        pool,
        "080_enable_rls_core_tables.sql",
        include_str!("../../../migrations/080_enable_rls_core_tables.sql"),
    )
    .await?;

    apply_migration(
        pool,
        "092_harden_core_rls_current_tenant_setting.sql",
        include_str!("../../../migrations/092_harden_core_rls_current_tenant_setting.sql"),
    )
    .await?;

    Ok(())
}

/// Run AI conversations and messages migrations
pub async fn run_ai_conversation_migrations(pool: &PgPool) -> Result<()> {
    apply_migration(
        pool,
        "082_create_ai_conversations.sql",
        include_str!("../../../migrations/082_create_ai_conversations.sql"),
    )
    .await?;

    apply_migration(
        pool,
        "083_create_ai_tool_call_persistence.sql",
        include_str!("../../../migrations/083_create_ai_tool_call_persistence.sql"),
    )
    .await?;

    apply_migration(
        pool,
        "084_create_ai_usage_events.sql",
        include_str!("../../../migrations/084_create_ai_usage_events.sql"),
    )
    .await?;

    apply_migration(
        pool,
        "085_create_ai_message_feedback.sql",
        include_str!("../../../migrations/085_create_ai_message_feedback.sql"),
    )
    .await?;

    apply_migration(
        pool,
        "086_create_ai_action_proposals.sql",
        include_str!("../../../migrations/086_create_ai_action_proposals.sql"),
    )
    .await?;

    apply_migration(
        pool,
        "087_ai_action_proposal_status_failed_errors.sql",
        include_str!("../../../migrations/087_ai_action_proposal_status_failed_errors.sql"),
    )
    .await?;

    Ok(())
}

/// Re-apply RLS policies for AI tables after all AI migrations have run.
pub async fn run_ai_rls_migrations(pool: &PgPool) -> Result<()> {
    apply_migration(
        pool,
        "089_enable_rls_ai_tables.sql",
        include_str!("../../../migrations/089_enable_rls_ai_tables.sql"),
    )
    .await?;

    // Force RLS on all tenant-scoped tables (core + AI) and create the
    // dedicated app role.  Must run after 089 so AI tables have ENABLE ROW
    // LEVEL SECURITY before FORCE is applied.
    apply_migration(
        pool,
        "120_force_rls_and_app_role.sql",
        include_str!("../../../migrations/120_force_rls_and_app_role.sql"),
    )
    .await?;

    // RLS on tenant_db-created tables (documents, audit_log, edi_*, etc.).
    // Must run after 120 so billforge_app role exists for the GRANT at the
    // end of the migration.
    apply_migration(
        pool,
        "121_enable_rls_tenant_db_tables.sql",
        include_str!("../../../migrations/121_enable_rls_tenant_db_tables.sql"),
    )
    .await?;

    // RLS on migration-005 workflow tables (approval_requests, queue_items,
    // approval_delegations) that #368 identified as uncovered.  Must run after
    // 120 so the billforge_app GRANT at the end of the migration succeeds.
    apply_migration(
        pool,
        "133_enable_rls_workflow_tables.sql",
        include_str!("../../../migrations/133_enable_rls_workflow_tables.sql"),
    )
    .await?;

    Ok(())
}

/// Run theme storage migrations (organization themes and user theme preferences).
pub async fn run_theme_migrations(pool: &PgPool) -> Result<()> {
    apply_migration(
        pool,
        "inline_theme_tables.sql",
        r#"
        -- Organization theme (one row per tenant)
        CREATE TABLE IF NOT EXISTS organization_themes (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL UNIQUE,
            preset_id TEXT NOT NULL DEFAULT 'default',
            custom_colors JSONB,
            branding JSONB NOT NULL DEFAULT '{}',
            enabled_for_all_users BOOLEAN NOT NULL DEFAULT false,
            allow_user_override BOOLEAN NOT NULL DEFAULT true,
            gradient_config JSONB,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

        -- User theme preference (one row per user)
        CREATE TABLE IF NOT EXISTS user_theme_preferences (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL,
            user_id UUID NOT NULL,
            preset_id TEXT NOT NULL DEFAULT 'default',
            custom_colors JSONB,
            mode TEXT NOT NULL DEFAULT 'system',
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE(tenant_id, user_id)
        );

        CREATE INDEX IF NOT EXISTS idx_user_theme_prefs_tenant
            ON user_theme_preferences(tenant_id);
        CREATE INDEX IF NOT EXISTS idx_user_theme_prefs_user
            ON user_theme_preferences(user_id);
        "#,
    )
    .await?;

    Ok(())
}

/// Run banking verification, dual-approval, and fraud-guard domain migrations.
pub async fn run_banking_verification_migrations(pool: &PgPool) -> Result<()> {
    apply_migration(
        pool,
        "097_vendor_banking_verification.sql",
        include_str!("../../../migrations/097_vendor_banking_verification.sql"),
    )
    .await?;

    apply_migration(
        pool,
        "103_vendor_banking_dual_approval.sql",
        include_str!("../../../migrations/103_vendor_banking_dual_approval.sql"),
    )
    .await?;

    apply_migration(
        pool,
        "118_vendor_domain_first_seen.sql",
        include_str!("../../../migrations/118_vendor_domain_first_seen.sql"),
    )
    .await?;

    Ok(())
}
