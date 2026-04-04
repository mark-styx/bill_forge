//! Tenant-specific database utilities
//!
//! Provides migration functions for tenant databases

use billforge_core::{Error, Result, TenantId};
use sqlx::PgPool;

/// Run all migrations for a tenant database
pub async fn run_tenant_migrations(pool: &PgPool, _tenant_id: &TenantId) -> Result<()> {
    // Migration 002: Users table
    let migration_002 = include_str!("../../../migrations/002_create_users.sql");
    sqlx::raw_sql(migration_002)
        .execute(pool)
        .await
        .map_err(|e| Error::Migration(format!("Failed to run migration 002: {}", e)))?;

    // Migration 003: Vendors table
    let migration_003 = include_str!("../../../migrations/003_create_vendors.sql");
    sqlx::raw_sql(migration_003)
        .execute(pool)
        .await
        .map_err(|e| Error::Migration(format!("Failed to run migration 003: {}", e)))?;

    // Migration 004: Invoices table
    let migration_004 = include_str!("../../../migrations/004_create_invoices.sql");
    sqlx::raw_sql(migration_004)
        .execute(pool)
        .await
        .map_err(|e| Error::Migration(format!("Failed to run migration 004: {}", e)))?;

    // Additional tenant-specific tables (work queues, workflow rules, etc.)
    run_workflow_migrations(pool).await?;
    run_vendor_statement_migrations(pool).await?;
    run_purchase_order_migrations(pool).await?;

    Ok(())
}

/// Run workflow-related migrations (public so it can be re-run on existing tenants)
pub async fn run_workflow_migrations(pool: &PgPool) -> Result<()> {
    sqlx::raw_sql(
        r#"
        -- Workflow rules
        CREATE TABLE IF NOT EXISTS workflow_rules (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL,
            name TEXT NOT NULL,
            description TEXT,
            priority INTEGER NOT NULL DEFAULT 0,
            is_active BOOLEAN NOT NULL DEFAULT true,
            rule_type TEXT NOT NULL,
            conditions JSONB NOT NULL DEFAULT '[]',
            actions JSONB NOT NULL DEFAULT '[]',
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

        -- Work queues
        CREATE TABLE IF NOT EXISTS work_queues (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL,
            name TEXT NOT NULL,
            description TEXT,
            queue_type TEXT NOT NULL,
            assigned_users JSONB,
            assigned_roles JSONB,
            is_default BOOLEAN NOT NULL DEFAULT false,
            is_active BOOLEAN NOT NULL DEFAULT true,
            settings JSONB NOT NULL DEFAULT '{}',
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

        -- Queue items
        CREATE TABLE IF NOT EXISTS queue_items (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            queue_id UUID NOT NULL REFERENCES work_queues(id) ON DELETE CASCADE,
            invoice_id UUID NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
            assigned_to UUID REFERENCES users(id),
            assigned_by_rule UUID,
            priority INTEGER NOT NULL DEFAULT 0,
            entered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            due_at TIMESTAMPTZ,
            claimed_at TIMESTAMPTZ,
            completed_at TIMESTAMPTZ,
            completion_action TEXT,
            notes TEXT
        );

        -- Assignment rules for auto-assigning invoices
        CREATE TABLE IF NOT EXISTS assignment_rules (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL,
            queue_id UUID NOT NULL REFERENCES work_queues(id) ON DELETE CASCADE,
            name TEXT NOT NULL,
            description TEXT,
            priority INTEGER NOT NULL DEFAULT 0,
            is_active BOOLEAN NOT NULL DEFAULT true,
            conditions JSONB NOT NULL DEFAULT '[]',
            assign_to JSONB NOT NULL DEFAULT '{}',
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

        -- Approval requests
        CREATE TABLE IF NOT EXISTS approval_requests (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            invoice_id UUID NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
            rule_id UUID REFERENCES workflow_rules(id),
            requested_from UUID NOT NULL REFERENCES users(id),
            status TEXT NOT NULL DEFAULT 'pending',
            comments TEXT,
            responded_by UUID REFERENCES users(id),
            responded_at TIMESTAMPTZ,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            expires_at TIMESTAMPTZ
        );

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

        -- Audit log
        CREATE TABLE IF NOT EXISTS audit_log (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL,
            user_id UUID NOT NULL REFERENCES users(id),
            action TEXT NOT NULL,
            resource_type TEXT NOT NULL,
            resource_id UUID,
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

        -- Approval delegations
        CREATE TABLE IF NOT EXISTS approval_delegations (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL,
            delegator_id UUID NOT NULL REFERENCES users(id),
            delegate_id UUID NOT NULL REFERENCES users(id),
            start_date TIMESTAMPTZ NOT NULL,
            end_date TIMESTAMPTZ NOT NULL,
            is_active BOOLEAN NOT NULL DEFAULT true,
            conditions JSONB,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
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

        -- Indexes
        CREATE INDEX IF NOT EXISTS idx_workflow_rules_tenant ON workflow_rules(tenant_id);
        CREATE INDEX IF NOT EXISTS idx_work_queues_tenant ON work_queues(tenant_id);
        CREATE INDEX IF NOT EXISTS idx_queue_items_queue ON queue_items(queue_id);
        CREATE INDEX IF NOT EXISTS idx_queue_items_invoice ON queue_items(invoice_id);
        CREATE INDEX IF NOT EXISTS idx_assignment_rules_queue ON assignment_rules(queue_id);
        CREATE INDEX IF NOT EXISTS idx_approval_requests_invoice ON approval_requests(invoice_id);
        CREATE INDEX IF NOT EXISTS idx_approval_requests_status ON approval_requests(status);
        CREATE INDEX IF NOT EXISTS idx_documents_tenant ON documents(tenant_id);
        CREATE INDEX IF NOT EXISTS idx_audit_log_tenant ON audit_log(tenant_id);
        CREATE INDEX IF NOT EXISTS idx_audit_log_resource ON audit_log(resource_type, resource_id);
        CREATE INDEX IF NOT EXISTS idx_line_items_invoice ON invoice_line_items(invoice_id);

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
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Migration(format!("Failed to run workflow migrations: {}", e)))?;

    Ok(())
}

/// Run vendor statement migrations
async fn run_vendor_statement_migrations(pool: &PgPool) -> Result<()> {
    sqlx::raw_sql(
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
        CREATE INDEX IF NOT EXISTS idx_vendor_statements_vendor ON vendor_statements(vendor_id);
        CREATE INDEX IF NOT EXISTS idx_vendor_statements_status ON vendor_statements(status);
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Migration(format!("Failed to run vendor statement migrations: {}", e)))?;

    Ok(())
}

/// Run purchase order and 3-way matching migrations
pub async fn run_purchase_order_migrations(pool: &PgPool) -> Result<()> {
    sqlx::raw_sql(include_str!("../../../migrations/065_create_purchase_orders.sql"))
        .execute(pool)
        .await
        .map_err(|e| Error::Migration(format!("Failed to run purchase order migrations: {}", e)))?;

    Ok(())
}
