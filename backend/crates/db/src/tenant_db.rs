//! Tenant-specific SQLite database for tenant data

use billforge_core::{Error, Result, TenantId};
use rusqlite::Connection;
use std::sync::Arc;
use tokio::sync::Mutex;

/// SQLite database for a single tenant's data
pub struct TenantDatabase {
    conn: Arc<Mutex<Connection>>,
    tenant_id: TenantId,
}

impl TenantDatabase {
    pub async fn new(db_path: &str, tenant_id: TenantId) -> Result<Self> {
        let conn = Connection::open(db_path)
            .map_err(|e| Error::Database(format!("Failed to open tenant db: {}", e)))?;
        
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            tenant_id,
        })
    }

    /// Get the tenant ID this database belongs to
    pub fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    /// Get a connection for executing queries
    pub async fn connection(&self) -> Arc<Mutex<Connection>> {
        Arc::clone(&self.conn)
    }

    /// Run all migrations for tenant data
    pub async fn run_migrations(&self) -> Result<()> {
        let conn = self.conn.lock().await;
        
        conn.execute_batch(
            r#"
            -- Invoices table (enhanced with queue tracking and coding)
            CREATE TABLE IF NOT EXISTS invoices (
                id TEXT PRIMARY KEY,
                vendor_id TEXT,
                vendor_name TEXT NOT NULL,
                invoice_number TEXT NOT NULL,
                invoice_date TEXT,
                due_date TEXT,
                po_number TEXT,
                subtotal_amount INTEGER,
                subtotal_currency TEXT,
                tax_amount INTEGER,
                tax_currency TEXT,
                total_amount INTEGER NOT NULL,
                total_currency TEXT NOT NULL DEFAULT 'USD',
                capture_status TEXT NOT NULL DEFAULT 'pending',
                processing_status TEXT NOT NULL DEFAULT 'draft',
                -- Queue tracking
                current_queue_id TEXT REFERENCES work_queues(id),
                assigned_to TEXT,
                -- Document references
                document_id TEXT NOT NULL,
                supporting_documents TEXT, -- JSON array of document IDs
                ocr_confidence REAL,
                -- Coding
                department TEXT,
                gl_code TEXT,
                cost_center TEXT,
                -- Metadata
                notes TEXT,
                tags TEXT,
                custom_fields TEXT,
                created_by TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            -- Invoice line items
            CREATE TABLE IF NOT EXISTS invoice_line_items (
                id TEXT PRIMARY KEY,
                invoice_id TEXT NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
                line_number INTEGER NOT NULL,
                description TEXT NOT NULL,
                quantity REAL,
                unit_price_amount INTEGER,
                unit_price_currency TEXT,
                amount INTEGER NOT NULL,
                amount_currency TEXT NOT NULL DEFAULT 'USD',
                gl_code TEXT,
                department TEXT,
                project TEXT
            );

            -- Vendors table
            CREATE TABLE IF NOT EXISTS vendors (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                legal_name TEXT,
                vendor_type TEXT NOT NULL DEFAULT 'business',
                status TEXT NOT NULL DEFAULT 'active',
                email TEXT,
                phone TEXT,
                website TEXT,
                address_line1 TEXT,
                address_line2 TEXT,
                address_city TEXT,
                address_state TEXT,
                address_postal_code TEXT,
                address_country TEXT,
                tax_id TEXT,
                tax_id_type TEXT,
                w9_on_file INTEGER NOT NULL DEFAULT 0,
                w9_received_date TEXT,
                payment_terms TEXT,
                default_payment_method TEXT,
                vendor_code TEXT,
                default_gl_code TEXT,
                default_department TEXT,
                notes TEXT,
                tags TEXT,
                custom_fields TEXT,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            -- Vendor contacts
            CREATE TABLE IF NOT EXISTS vendor_contacts (
                id TEXT PRIMARY KEY,
                vendor_id TEXT NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                title TEXT,
                email TEXT,
                phone TEXT,
                is_primary INTEGER NOT NULL DEFAULT 0
            );

            -- Vendor bank accounts (encrypted fields stored as text)
            CREATE TABLE IF NOT EXISTS vendor_bank_accounts (
                id TEXT PRIMARY KEY,
                vendor_id TEXT NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
                bank_name TEXT NOT NULL,
                account_type TEXT NOT NULL,
                account_last_four TEXT NOT NULL,
                account_number_encrypted TEXT NOT NULL,
                routing_number_encrypted TEXT NOT NULL,
                is_primary INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            -- Tax documents
            CREATE TABLE IF NOT EXISTS tax_documents (
                id TEXT PRIMARY KEY,
                vendor_id TEXT NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
                document_type TEXT NOT NULL,
                tax_year INTEGER NOT NULL,
                file_id TEXT NOT NULL,
                file_name TEXT NOT NULL,
                received_date TEXT NOT NULL,
                expires_date TEXT,
                notes TEXT,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            -- Workflow rules
            CREATE TABLE IF NOT EXISTS workflow_rules (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                priority INTEGER NOT NULL DEFAULT 0,
                is_active INTEGER NOT NULL DEFAULT 1,
                rule_type TEXT NOT NULL,
                conditions TEXT NOT NULL,
                actions TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            -- Work queues
            CREATE TABLE IF NOT EXISTS work_queues (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                queue_type TEXT NOT NULL,
                assigned_users TEXT,
                assigned_roles TEXT,
                is_default INTEGER NOT NULL DEFAULT 0,
                is_active INTEGER NOT NULL DEFAULT 1,
                settings TEXT,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            -- Queue items (enhanced with assignment tracking)
            CREATE TABLE IF NOT EXISTS queue_items (
                id TEXT PRIMARY KEY,
                queue_id TEXT NOT NULL REFERENCES work_queues(id) ON DELETE CASCADE,
                invoice_id TEXT NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
                assigned_to TEXT,
                assigned_by_rule TEXT REFERENCES assignment_rules(id),
                priority INTEGER NOT NULL DEFAULT 0,
                entered_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                due_at TEXT,
                claimed_at TEXT,
                completed_at TEXT,
                completion_action TEXT,
                notes TEXT
            );

            -- Assignment rules for auto-assigning invoices within queues
            CREATE TABLE IF NOT EXISTS assignment_rules (
                id TEXT PRIMARY KEY,
                queue_id TEXT NOT NULL REFERENCES work_queues(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                description TEXT,
                priority INTEGER NOT NULL DEFAULT 0,
                is_active INTEGER NOT NULL DEFAULT 1,
                conditions TEXT NOT NULL, -- JSON array of conditions
                assign_to TEXT NOT NULL, -- JSON assignment target
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            -- Vendor approver registrations
            CREATE TABLE IF NOT EXISTS vendor_approvers (
                id TEXT PRIMARY KEY,
                vendor_id TEXT NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
                user_id TEXT NOT NULL,
                max_amount INTEGER,
                max_amount_currency TEXT DEFAULT 'USD',
                is_active INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(vendor_id, user_id)
            );

            -- Department approver registrations
            CREATE TABLE IF NOT EXISTS department_approvers (
                id TEXT PRIMARY KEY,
                department TEXT NOT NULL,
                user_id TEXT NOT NULL,
                max_amount INTEGER,
                max_amount_currency TEXT DEFAULT 'USD',
                is_active INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(department, user_id)
            );

            -- Queue flow configurations
            CREATE TABLE IF NOT EXISTS queue_flow_configs (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                stages TEXT NOT NULL, -- JSON array of queue stages
                is_default INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            -- Approval chain configurations (multi-level approvals)
            CREATE TABLE IF NOT EXISTS approval_chains (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                conditions TEXT NOT NULL, -- JSON array of conditions
                steps TEXT NOT NULL, -- JSON array of approval steps
                is_active INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            -- Approval requests
            CREATE TABLE IF NOT EXISTS approval_requests (
                id TEXT PRIMARY KEY,
                invoice_id TEXT NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
                rule_id TEXT REFERENCES workflow_rules(id),
                requested_from TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                comments TEXT,
                responded_by TEXT,
                responded_at TEXT,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                expires_at TEXT
            );

            -- Approval delegations
            CREATE TABLE IF NOT EXISTS approval_delegations (
                id TEXT PRIMARY KEY,
                delegator_id TEXT NOT NULL,
                delegate_id TEXT NOT NULL,
                start_date TEXT NOT NULL,
                end_date TEXT NOT NULL,
                is_active INTEGER NOT NULL DEFAULT 1,
                conditions TEXT,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            -- Approval limits
            CREATE TABLE IF NOT EXISTS approval_limits (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                max_amount INTEGER NOT NULL,
                max_amount_currency TEXT NOT NULL DEFAULT 'USD',
                vendor_restrictions TEXT,
                department_restrictions TEXT,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            -- Documents (file references) - enhanced
            CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                filename TEXT NOT NULL,
                mime_type TEXT NOT NULL,
                size_bytes INTEGER NOT NULL,
                storage_key TEXT NOT NULL,
                invoice_id TEXT REFERENCES invoices(id) ON DELETE SET NULL,
                doc_type TEXT NOT NULL DEFAULT 'invoice_original',
                uploaded_by TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            -- Vendor messages
            CREATE TABLE IF NOT EXISTS vendor_messages (
                id TEXT PRIMARY KEY,
                vendor_id TEXT NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
                subject TEXT NOT NULL,
                body TEXT NOT NULL,
                sender_type TEXT NOT NULL,
                sender_id TEXT,
                sender_name TEXT NOT NULL,
                attachments TEXT,
                read_at TEXT,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            -- Audit log
            CREATE TABLE IF NOT EXISTS audit_log (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                action TEXT NOT NULL,
                resource_type TEXT NOT NULL,
                resource_id TEXT,
                changes TEXT,
                ip_address TEXT,
                user_agent TEXT,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            -- Indexes for common queries
            CREATE INDEX IF NOT EXISTS idx_invoices_vendor ON invoices(vendor_id);
            CREATE INDEX IF NOT EXISTS idx_invoices_status ON invoices(capture_status, processing_status);
            CREATE INDEX IF NOT EXISTS idx_invoices_date ON invoices(invoice_date);
            CREATE INDEX IF NOT EXISTS idx_invoices_due_date ON invoices(due_date);
            CREATE INDEX IF NOT EXISTS idx_invoices_queue ON invoices(current_queue_id);
            CREATE INDEX IF NOT EXISTS idx_invoices_assigned ON invoices(assigned_to);
            CREATE INDEX IF NOT EXISTS idx_invoices_department ON invoices(department);
            CREATE INDEX IF NOT EXISTS idx_line_items_invoice ON invoice_line_items(invoice_id);
            CREATE INDEX IF NOT EXISTS idx_vendors_status ON vendors(status);
            CREATE INDEX IF NOT EXISTS idx_queue_items_queue ON queue_items(queue_id);
            CREATE INDEX IF NOT EXISTS idx_queue_items_invoice ON queue_items(invoice_id);
            CREATE INDEX IF NOT EXISTS idx_queue_items_assigned ON queue_items(assigned_to);
            CREATE INDEX IF NOT EXISTS idx_queue_items_status ON queue_items(completed_at);
            CREATE INDEX IF NOT EXISTS idx_approval_requests_invoice ON approval_requests(invoice_id);
            CREATE INDEX IF NOT EXISTS idx_approval_requests_status ON approval_requests(status);
            CREATE INDEX IF NOT EXISTS idx_audit_log_resource ON audit_log(resource_type, resource_id);
            CREATE INDEX IF NOT EXISTS idx_assignment_rules_queue ON assignment_rules(queue_id);
            CREATE INDEX IF NOT EXISTS idx_vendor_approvers_vendor ON vendor_approvers(vendor_id);
            CREATE INDEX IF NOT EXISTS idx_department_approvers_dept ON department_approvers(department);

            -- Vendor statement settings (auto-request configuration)
            CREATE TABLE IF NOT EXISTS vendor_statement_settings (
                id TEXT PRIMARY KEY,
                vendor_id TEXT NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
                is_enabled INTEGER NOT NULL DEFAULT 1,
                request_frequency TEXT NOT NULL DEFAULT 'monthly',
                day_of_month INTEGER DEFAULT 1,
                day_of_week TEXT,
                contact_email TEXT,
                cc_emails TEXT,
                custom_message TEXT,
                statement_period_type TEXT NOT NULL DEFAULT 'previous_month',
                auto_send_reminders INTEGER NOT NULL DEFAULT 1,
                reminder_days_after INTEGER DEFAULT 7,
                max_reminders INTEGER DEFAULT 3,
                next_request_date TEXT,
                last_request_date TEXT,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(vendor_id)
            );

            -- Vendor statements (individual statement records)
            CREATE TABLE IF NOT EXISTS vendor_statements (
                id TEXT PRIMARY KEY,
                vendor_id TEXT NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
                settings_id TEXT REFERENCES vendor_statement_settings(id),
                statement_period_start TEXT NOT NULL,
                statement_period_end TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                request_sent_at TEXT,
                request_sent_to TEXT,
                reminder_count INTEGER NOT NULL DEFAULT 0,
                last_reminder_at TEXT,
                received_at TEXT,
                received_from TEXT,
                document_id TEXT REFERENCES documents(id),
                review_status TEXT DEFAULT 'pending',
                reviewer_id TEXT,
                reviewed_at TEXT,
                review_notes TEXT,
                discrepancies_found INTEGER DEFAULT 0,
                discrepancy_amount INTEGER DEFAULT 0,
                discrepancy_currency TEXT DEFAULT 'USD',
                discrepancy_notes TEXT,
                resolution_status TEXT,
                resolution_notes TEXT,
                resolved_at TEXT,
                resolved_by TEXT,
                upload_token TEXT,
                upload_token_expires_at TEXT,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            -- Statement request history (audit trail for sent emails)
            CREATE TABLE IF NOT EXISTS statement_request_log (
                id TEXT PRIMARY KEY,
                statement_id TEXT NOT NULL REFERENCES vendor_statements(id) ON DELETE CASCADE,
                request_type TEXT NOT NULL,
                sent_to TEXT NOT NULL,
                sent_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                email_subject TEXT,
                email_status TEXT DEFAULT 'sent',
                error_message TEXT
            );

            -- Statement line items for reconciliation
            CREATE TABLE IF NOT EXISTS statement_line_items (
                id TEXT PRIMARY KEY,
                statement_id TEXT NOT NULL REFERENCES vendor_statements(id) ON DELETE CASCADE,
                invoice_number TEXT,
                invoice_date TEXT,
                amount INTEGER NOT NULL,
                amount_currency TEXT NOT NULL DEFAULT 'USD',
                description TEXT,
                matched_invoice_id TEXT REFERENCES invoices(id),
                match_status TEXT DEFAULT 'unmatched',
                notes TEXT,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            -- Indexes for vendor statement tables
            CREATE INDEX IF NOT EXISTS idx_statement_settings_vendor ON vendor_statement_settings(vendor_id);
            CREATE INDEX IF NOT EXISTS idx_statement_settings_next_date ON vendor_statement_settings(next_request_date);
            CREATE INDEX IF NOT EXISTS idx_statements_vendor ON vendor_statements(vendor_id);
            CREATE INDEX IF NOT EXISTS idx_statements_status ON vendor_statements(status);
            CREATE INDEX IF NOT EXISTS idx_statements_review_status ON vendor_statements(review_status);
            CREATE INDEX IF NOT EXISTS idx_statements_period ON vendor_statements(statement_period_start, statement_period_end);
            CREATE INDEX IF NOT EXISTS idx_statement_request_log_statement ON statement_request_log(statement_id);
            CREATE INDEX IF NOT EXISTS idx_statement_line_items_statement ON statement_line_items(statement_id);
            CREATE INDEX IF NOT EXISTS idx_statement_line_items_match ON statement_line_items(match_status);
            "#,
        )
        .map_err(|e| Error::Migration(format!("Failed to run tenant migrations: {}", e)))?;

        Ok(())
    }

    /// Execute a parameterized query and return results
    pub async fn query<T, F>(&self, sql: &str, params: &[&dyn rusqlite::ToSql], map_fn: F) -> Result<Vec<T>>
    where
        F: Fn(&rusqlite::Row) -> std::result::Result<T, rusqlite::Error>,
    {
        let conn = self.conn.lock().await;
        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| Error::Database(format!("Failed to prepare query: {}", e)))?;
        
        let rows = stmt
            .query_map(params, map_fn)
            .map_err(|e| Error::Database(format!("Failed to execute query: {}", e)))?;
        
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| Error::Database(e.to_string()))?);
        }
        
        Ok(results)
    }

    /// Execute an insert/update/delete and return rows affected
    pub async fn execute(&self, sql: &str, params: &[&dyn rusqlite::ToSql]) -> Result<usize> {
        let conn = self.conn.lock().await;
        let rows = conn
            .execute(sql, params)
            .map_err(|e| Error::Database(format!("Failed to execute: {}", e)))?;
        Ok(rows)
    }

    /// Execute multiple statements in a batch
    pub async fn execute_batch(&self, sql: &str) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute_batch(sql)
            .map_err(|e| Error::Database(format!("Failed to execute batch: {}", e)))?;
        Ok(())
    }

    /// Export data to CSV (basic implementation)
    pub async fn export_to_csv(&self, query: &str, output_path: &str) -> Result<()> {
        use std::io::Write;
        
        let conn = self.conn.lock().await;
        let mut stmt = conn
            .prepare(query)
            .map_err(|e| Error::Database(format!("Failed to prepare export query: {}", e)))?;
        
        let column_count = stmt.column_count();
        let column_names: Vec<_> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        
        let mut file = std::fs::File::create(output_path)
            .map_err(|e| Error::Database(format!("Failed to create CSV file: {}", e)))?;
        
        // Write header
        writeln!(file, "{}", column_names.join(","))
            .map_err(|e| Error::Database(format!("Failed to write CSV header: {}", e)))?;
        
        let rows = stmt
            .query_map([], |row| {
                let values: Vec<String> = (0..column_count)
                    .map(|i| {
                        row.get::<_, rusqlite::types::Value>(i)
                            .map(|v| match v {
                                rusqlite::types::Value::Null => "".to_string(),
                                rusqlite::types::Value::Integer(i) => i.to_string(),
                                rusqlite::types::Value::Real(f) => f.to_string(),
                                rusqlite::types::Value::Text(s) => format!("\"{}\"", s.replace('"', "\"\"")),
                                rusqlite::types::Value::Blob(_) => "[BLOB]".to_string(),
                            })
                            .unwrap_or_default()
                    })
                    .collect();
                Ok(values.join(","))
            })
            .map_err(|e| Error::Database(format!("Failed to execute export query: {}", e)))?;
        
        for row in rows {
            let line = row.map_err(|e| Error::Database(e.to_string()))?;
            writeln!(file, "{}", line)
                .map_err(|e| Error::Database(format!("Failed to write CSV row: {}", e)))?;
        }
        
        Ok(())
    }
}
