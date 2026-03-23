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

    // OCR pipeline tables (batch processing, corrections, vendor aliases)
    run_ocr_pipeline_migrations(pool).await?;

    // Approval chain tables (policies, chains, steps, activity log)
    run_approval_chain_migrations(pool).await?;

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

/// Run OCR pipeline migrations for a tenant database.
/// Schema matches the columns used by the `ocr-pipeline` crate's runtime SQL queries.
pub async fn run_ocr_pipeline_migrations(pool: &PgPool) -> Result<()> {
    sqlx::raw_sql(
        r#"
        -- OCR processing jobs (async pipeline)
        -- Column names match the ocr-pipeline crate's runtime queries exactly.
        CREATE TABLE IF NOT EXISTS ocr_jobs (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL,
            document_id UUID NOT NULL,
            file_name TEXT NOT NULL DEFAULT '',
            mime_type TEXT NOT NULL DEFAULT 'application/octet-stream',

            -- Job configuration
            provider TEXT NOT NULL DEFAULT 'tesseract',
            priority INTEGER NOT NULL DEFAULT 100,

            -- Status tracking
            status TEXT NOT NULL DEFAULT 'pending',
            attempt_count INTEGER NOT NULL DEFAULT 0,
            max_attempts INTEGER NOT NULL DEFAULT 3,

            -- Results (JSONB column named 'result' as pipeline code expects)
            result JSONB,

            -- Vendor matching
            matched_vendor_id UUID REFERENCES vendors(id),
            vendor_match_confidence REAL,

            -- Processing metadata
            error_message TEXT,
            processing_time_ms BIGINT,

            -- Timestamps
            started_at TIMESTAMPTZ,
            completed_at TIMESTAMPTZ,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

        CREATE INDEX IF NOT EXISTS idx_ocr_jobs_tenant ON ocr_jobs(tenant_id);
        CREATE INDEX IF NOT EXISTS idx_ocr_jobs_status ON ocr_jobs(tenant_id, status);
        CREATE INDEX IF NOT EXISTS idx_ocr_jobs_document ON ocr_jobs(document_id);
        CREATE INDEX IF NOT EXISTS idx_ocr_jobs_queued ON ocr_jobs(priority ASC, created_at ASC) WHERE status = 'pending';

        -- OCR corrections (table name matches pipeline code: ocr_corrections)
        CREATE TABLE IF NOT EXISTS ocr_corrections (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL,
            job_id UUID NOT NULL REFERENCES ocr_jobs(id) ON DELETE CASCADE,
            field_name TEXT NOT NULL,
            original_value TEXT,
            corrected_value TEXT NOT NULL,
            corrected_by UUID NOT NULL REFERENCES users(id),
            corrected_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

        CREATE INDEX IF NOT EXISTS idx_ocr_corrections_tenant ON ocr_corrections(tenant_id);
        CREATE INDEX IF NOT EXISTS idx_ocr_corrections_job ON ocr_corrections(job_id);

        -- Vendor aliases (learned from corrections, used for fuzzy matching)
        CREATE TABLE IF NOT EXISTS vendor_aliases (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL,
            vendor_id UUID NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
            alias TEXT NOT NULL,
            is_learned BOOLEAN NOT NULL DEFAULT false,
            source TEXT NOT NULL DEFAULT 'manual',
            match_count INTEGER NOT NULL DEFAULT 1,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

            UNIQUE(tenant_id, vendor_id, alias)
        );

        CREATE INDEX IF NOT EXISTS idx_vendor_aliases_tenant ON vendor_aliases(tenant_id);
        CREATE INDEX IF NOT EXISTS idx_vendor_aliases_lookup ON vendor_aliases(tenant_id, alias);
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Migration(format!("Failed to run OCR pipeline migrations: {}", e)))?;

    Ok(())
}

/// Run approval chain migrations for a tenant database
/// Creates tables for multi-level approval policies, chains, steps, and activity log.
/// Adapted from migration 065 — removes FK references to tenants table.
pub async fn run_approval_chain_migrations(pool: &PgPool) -> Result<()> {
    sqlx::raw_sql(
        r#"
        -- Approval policies: define how approvals work per tenant
        CREATE TABLE IF NOT EXISTS approval_policies (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL,
            name VARCHAR(255) NOT NULL,
            description TEXT,
            is_active BOOLEAN NOT NULL DEFAULT true,
            is_default BOOLEAN NOT NULL DEFAULT false,

            match_criteria JSONB NOT NULL DEFAULT '{}',
            priority INTEGER NOT NULL DEFAULT 0,

            require_sequential BOOLEAN NOT NULL DEFAULT true,
            require_all_levels BOOLEAN NOT NULL DEFAULT true,
            allow_self_approval BOOLEAN NOT NULL DEFAULT false,
            auto_approve_below_cents BIGINT,

            escalation_enabled BOOLEAN NOT NULL DEFAULT true,
            escalation_timeout_hours INTEGER DEFAULT 48,
            final_escalation_user_id UUID REFERENCES users(id),

            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

        CREATE INDEX IF NOT EXISTS idx_approval_policies_tenant ON approval_policies(tenant_id);
        CREATE INDEX IF NOT EXISTS idx_approval_policies_active ON approval_policies(tenant_id, is_active, priority DESC);

        -- Approval chain levels: ordered steps within a policy
        CREATE TABLE IF NOT EXISTS approval_chain_levels (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            policy_id UUID NOT NULL REFERENCES approval_policies(id) ON DELETE CASCADE,
            tenant_id UUID NOT NULL,

            level_order INTEGER NOT NULL,
            name VARCHAR(255) NOT NULL,

            approver_type TEXT NOT NULL,
            approver_user_ids JSONB DEFAULT '[]',
            approver_role TEXT,

            min_amount_cents BIGINT DEFAULT 0,
            max_amount_cents BIGINT,

            required_approver_count INTEGER NOT NULL DEFAULT 1,

            timeout_hours INTEGER,

            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

            UNIQUE(policy_id, level_order)
        );

        CREATE INDEX IF NOT EXISTS idx_chain_levels_policy ON approval_chain_levels(policy_id);
        CREATE INDEX IF NOT EXISTS idx_chain_levels_tenant ON approval_chain_levels(tenant_id);

        -- Active approval chains: tracks a running approval for a specific invoice
        CREATE TABLE IF NOT EXISTS active_approval_chains (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL,
            invoice_id UUID NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
            policy_id UUID NOT NULL REFERENCES approval_policies(id),

            status TEXT NOT NULL DEFAULT 'in_progress',
            current_level INTEGER NOT NULL DEFAULT 1,
            total_levels INTEGER NOT NULL,

            final_decision TEXT,
            final_decided_by UUID REFERENCES users(id),
            final_decided_at TIMESTAMPTZ,

            escalation_count INTEGER NOT NULL DEFAULT 0,
            last_escalated_at TIMESTAMPTZ,

            initiated_by UUID NOT NULL REFERENCES users(id),
            initiated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            completed_at TIMESTAMPTZ,

            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

        CREATE INDEX IF NOT EXISTS idx_active_chains_tenant ON active_approval_chains(tenant_id);
        CREATE INDEX IF NOT EXISTS idx_active_chains_invoice ON active_approval_chains(invoice_id);
        CREATE INDEX IF NOT EXISTS idx_active_chains_status ON active_approval_chains(tenant_id, status);

        -- Individual approval steps within an active chain
        CREATE TABLE IF NOT EXISTS approval_chain_steps (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            chain_id UUID NOT NULL REFERENCES active_approval_chains(id) ON DELETE CASCADE,
            tenant_id UUID NOT NULL,
            level_id UUID NOT NULL REFERENCES approval_chain_levels(id),

            level_order INTEGER NOT NULL,

            assigned_to UUID NOT NULL REFERENCES users(id),

            status TEXT NOT NULL DEFAULT 'pending',
            decision TEXT,
            comments TEXT,

            delegated_to UUID REFERENCES users(id),
            delegated_at TIMESTAMPTZ,
            delegation_reason TEXT,

            assigned_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            due_at TIMESTAMPTZ,
            responded_at TIMESTAMPTZ,

            escalated_at TIMESTAMPTZ,
            escalated_to UUID REFERENCES users(id),
            escalation_reason TEXT,

            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

        CREATE INDEX IF NOT EXISTS idx_chain_steps_chain ON approval_chain_steps(chain_id);
        CREATE INDEX IF NOT EXISTS idx_chain_steps_assigned ON approval_chain_steps(assigned_to, status);
        CREATE INDEX IF NOT EXISTS idx_chain_steps_pending ON approval_chain_steps(tenant_id, status, due_at) WHERE status = 'pending';

        -- Approval activity log (immutable audit trail)
        CREATE TABLE IF NOT EXISTS approval_activity_log (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tenant_id UUID NOT NULL,
            chain_id UUID NOT NULL REFERENCES active_approval_chains(id) ON DELETE CASCADE,
            step_id UUID REFERENCES approval_chain_steps(id),
            invoice_id UUID NOT NULL REFERENCES invoices(id),

            action TEXT NOT NULL,
            actor_id UUID NOT NULL REFERENCES users(id),
            actor_role TEXT,

            comments TEXT,
            metadata JSONB DEFAULT '{}',
            ip_address INET,

            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

        CREATE INDEX IF NOT EXISTS idx_approval_activity_chain ON approval_activity_log(chain_id);
        CREATE INDEX IF NOT EXISTS idx_approval_activity_invoice ON approval_activity_log(invoice_id);
        CREATE INDEX IF NOT EXISTS idx_approval_activity_actor ON approval_activity_log(actor_id);
        CREATE INDEX IF NOT EXISTS idx_approval_activity_tenant ON approval_activity_log(tenant_id, created_at DESC);
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| Error::Migration(format!("Failed to run approval chain migrations: {}", e)))?;

    Ok(())
}
