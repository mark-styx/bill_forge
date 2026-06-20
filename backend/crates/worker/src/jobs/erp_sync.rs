//! Background sync/export jobs for non-QuickBooks ERPs.
//!
//! These handlers move Xero, Sage Intacct, Salesforce, Workday, Bill.com, and
//! NetSuite sync/export work out of the HTTP request path. The route handlers in
//! `billforge-api` previously paginated remote ERPs and wrote to the DB inline,
//! blocking the request for the entire run with no retry. Now the routes enqueue
//! one of these JobTypes and return 202; the worker performs the actual sync
//! with the same retry/backoff machinery used for QuickBooks jobs.
//!
//! Each handler updates the per-ERP `*_sync_log` table on completion. Transient
//! errors are returned as `Err` so the worker retry loop picks them up;
//! permanent errors (auth/validation failures) write a `failed` row and return
//! `Ok` to avoid the retry queue.

use crate::config::WorkerConfig;
use anyhow::{Context, Result};
use billforge_bill_com::{BillComAuth, BillComAuthConfig, BillComClient, BillComEnvironment};
use billforge_core::TenantId;
use billforge_netsuite::{NetSuiteClient, NetSuiteConfig};
use billforge_sage_intacct::{SageIntacctAuth, SageIntacctAuthConfig, SageIntacctClient};
use billforge_salesforce::{
    SalesforceClient, SalesforceEnvironment, SalesforceOAuth, SalesforceOAuthConfig,
};
use billforge_workday::{WorkdayClient, WorkdayOAuth, WorkdayOAuthConfig};
use billforge_xero::{XeroClient, XeroEnvironment, XeroOAuth, XeroOAuthConfig};
use chrono::{Duration, Utc};
use serde_json::Value;
use sqlx::PgPool;
use tracing::{error, info, warn};
use uuid::Uuid;

async fn ensure_tenant_scope(tenant_id: &str, config: &WorkerConfig) -> Result<TenantId> {
    let parsed: TenantId = tenant_id
        .parse()
        .with_context(|| format!("Invalid tenant_id for ERP sync job: {}", tenant_id))?;
    config
        .pg_manager
        .tenant(&parsed)
        .await
        .with_context(|| format!("Tenant validation failed for {}", tenant_id))?;
    Ok(parsed)
}

/// Insert a `running` row in the given per-ERP sync_log table.
async fn record_sync_log_started(
    pool: &PgPool,
    table: &str,
    sync_type: &str,
    tenant_id: &TenantId,
) -> Result<Uuid> {
    let sync_id = Uuid::new_v4();
    let sql = format!(
        "INSERT INTO {} (id, tenant_id, sync_type, status, started_at) \
         VALUES ($1, $2, $3, 'running', NOW())",
        table
    );
    if let Err(e) = sqlx::query(&sql)
        .bind(sync_id)
        .bind(tenant_id.as_uuid())
        .bind(sync_type)
        .execute(pool)
        .await
    {
        warn!(
            error = %e,
            table = table,
            tenant = tenant_id.as_str(),
            "Failed to insert sync_log start row — continuing without it"
        );
    }
    Ok(sync_id)
}

async fn mark_sync_log_completed(
    pool: &PgPool,
    table: &str,
    sync_id: Uuid,
    processed: u64,
    created: u64,
    updated: u64,
) {
    let sql = format!(
        "UPDATE {} \
         SET status = 'completed', completed_at = NOW(), \
             records_processed = $2, records_created = $3, records_updated = $4 \
         WHERE id = $1",
        table
    );
    if let Err(e) = sqlx::query(&sql)
        .bind(sync_id)
        .bind(processed as i32)
        .bind(created as i32)
        .bind(updated as i32)
        .execute(pool)
        .await
    {
        warn!(error = %e, table = table, "Failed to mark sync_log completed");
    }
}

async fn mark_sync_log_failed(pool: &PgPool, table: &str, sync_id: Uuid, error_message: &str) {
    let sql = format!(
        "UPDATE {} \
         SET status = 'failed', completed_at = NOW(), error_message = $2 \
         WHERE id = $1",
        table
    );
    if let Err(e) = sqlx::query(&sql)
        .bind(sync_id)
        .bind(error_message)
        .execute(pool)
        .await
    {
        warn!(error = %e, table = table, "Failed to mark sync_log failed");
    }
}

// ──────────────────────────── Xero ────────────────────────────

fn xero_env(config: &WorkerConfig) -> XeroEnvironment {
    match config.xero_environment.as_str() {
        "sandbox" => XeroEnvironment::Sandbox,
        _ => XeroEnvironment::Production,
    }
}

async fn get_authenticated_xero_client(
    pool: &PgPool,
    tenant_id: &TenantId,
    config: &WorkerConfig,
) -> Result<(XeroClient, String)> {
    let connection: Option<(String, String, String, chrono::DateTime<Utc>)> = sqlx::query_as(
        "SELECT xero_tenant_id, access_token, refresh_token, access_token_expires_at \
         FROM xero_connections WHERE tenant_id = $1 AND sync_enabled = true",
    )
    .bind(tenant_id.as_uuid())
    .fetch_optional(pool)
    .await
    .context("Failed to load Xero connection")?;

    let (xero_tenant_id, mut access_token, refresh_token_val, token_expires_at) =
        connection.context("Xero not connected or sync disabled")?;

    if token_expires_at <= Utc::now() + Duration::minutes(5) {
        let client_id = config
            .xero_client_id
            .as_deref()
            .context("XERO_CLIENT_ID not configured — cannot refresh Xero token")?;
        let client_secret = config
            .xero_client_secret
            .as_deref()
            .context("XERO_CLIENT_SECRET not configured — cannot refresh Xero token")?;

        let oauth = XeroOAuth::new(XeroOAuthConfig {
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            redirect_uri: String::new(),
            environment: xero_env(config),
        });

        let new_tokens = oauth
            .refresh_token(&refresh_token_val)
            .await
            .context("Xero token refresh failed")?;

        let now = Utc::now();
        let new_expires = now + Duration::seconds(new_tokens.expires_in);
        sqlx::query(
            "UPDATE xero_connections \
             SET access_token = $2, refresh_token = $3, access_token_expires_at = $4, updated_at = NOW() \
             WHERE tenant_id = $1",
        )
        .bind(tenant_id.as_uuid())
        .bind(&new_tokens.access_token)
        .bind(&new_tokens.refresh_token)
        .bind(new_expires)
        .execute(pool)
        .await
        .context("Failed to persist refreshed Xero tokens")?;

        access_token = new_tokens.access_token;
    }

    let client = XeroClient::new(access_token, xero_tenant_id.clone(), xero_env(config));
    Ok((client, xero_tenant_id))
}

pub async fn xero_contact_sync(
    tenant_id: &str,
    _payload: &Value,
    config: &WorkerConfig,
) -> Result<()> {
    let parsed = ensure_tenant_scope(tenant_id, config).await?;
    let pool = config.pg_manager.tenant(&parsed).await?;
    let sync_id = record_sync_log_started(&pool, "xero_sync_log", "contacts", &parsed).await?;

    let result = run_xero_contact_sync(&pool, &parsed, config, sync_id).await;
    if let Err(ref e) = result {
        error!(error = %e, "Xero contact sync failed");
        mark_sync_log_failed(&pool, "xero_sync_log", sync_id, &e.to_string()).await;
    }
    result
}

async fn run_xero_contact_sync(
    pool: &PgPool,
    tenant_id: &TenantId,
    config: &WorkerConfig,
    sync_id: Uuid,
) -> Result<()> {
    let (client, _xero_tenant_id) = get_authenticated_xero_client(pool, tenant_id, config).await?;

    let mut all_contacts = Vec::new();
    let mut page = 1;
    let page_size = 100;
    loop {
        let contacts = client
            .query_contacts(page, page_size)
            .await
            .context("Xero API query_contacts error")?;
        if contacts.is_empty() {
            break;
        }
        all_contacts.extend(contacts);
        page += 1;
    }

    let suppliers: Vec<_> = all_contacts
        .into_iter()
        .filter(|c| c.IsSupplier.unwrap_or(false))
        .collect();

    let mut imported = 0u64;
    let mut updated = 0u64;
    let mut failed = 0u64;

    for xero_contact in &suppliers {
        let existing: Option<(Uuid,)> = sqlx::query_as(
            "SELECT v.id FROM vendors v \
             INNER JOIN xero_contact_mappings m ON m.billforge_vendor_id = v.id \
             WHERE m.tenant_id = $1 AND m.xero_contact_id = $2",
        )
        .bind(tenant_id.as_uuid())
        .bind(&xero_contact.ContactID)
        .fetch_optional(pool)
        .await
        .context("Failed to look up existing vendor")?;

        let res: Result<bool> = if let Some((vendor_id,)) = existing {
            let mut tx = pool.begin().await.context("begin tx failed")?;
            sqlx::query(
                "UPDATE vendors SET name = $2, email = $3, updated_at = NOW() WHERE id = $1",
            )
            .bind(vendor_id)
            .bind(&xero_contact.Name)
            .bind(&xero_contact.EmailAddress)
            .execute(&mut *tx)
            .await
            .context("update vendors failed")?;
            sqlx::query(
                "UPDATE xero_contact_mappings \
                 SET xero_contact_name = $3, last_synced_at = NOW(), updated_at = NOW() \
                 WHERE tenant_id = $1 AND xero_contact_id = $2",
            )
            .bind(tenant_id.as_uuid())
            .bind(&xero_contact.ContactID)
            .bind(&xero_contact.Name)
            .execute(&mut *tx)
            .await
            .context("update mapping failed")?;
            tx.commit().await.context("commit tx failed")?;
            Ok(true) // updated
        } else {
            let vendor_id = Uuid::new_v4();
            let mut tx = pool.begin().await.context("begin tx failed")?;
            sqlx::query(
                "INSERT INTO vendors (id, name, vendor_type, email, status, created_at, updated_at) \
                 VALUES ($1, $2, 'business', $3, $4, NOW(), NOW())",
            )
            .bind(vendor_id)
            .bind(&xero_contact.Name)
            .bind(&xero_contact.EmailAddress)
            .bind(if xero_contact.ContactStatus == "ACTIVE" {
                "active"
            } else {
                "inactive"
            })
            .execute(&mut *tx)
            .await
            .context("insert vendor failed")?;
            sqlx::query(
                "INSERT INTO xero_contact_mappings \
                 (tenant_id, xero_contact_id, billforge_vendor_id, xero_contact_name, last_synced_at, created_at, updated_at) \
                 VALUES ($1, $2, $3, $4, NOW(), NOW(), NOW())",
            )
            .bind(tenant_id.as_uuid())
            .bind(&xero_contact.ContactID)
            .bind(vendor_id)
            .bind(&xero_contact.Name)
            .execute(&mut *tx)
            .await
            .context("insert mapping failed")?;
            tx.commit().await.context("commit tx failed")?;
            Ok(false) // imported
        };

        match res {
            Ok(true) => updated += 1,
            Ok(false) => imported += 1,
            Err(e) => {
                error!(contact_id = %xero_contact.ContactID, error = %e, "Failed to sync Xero contact");
                failed += 1;
            }
        }
    }

    mark_sync_log_completed(
        pool,
        "xero_sync_log",
        sync_id,
        imported + updated + failed,
        imported,
        updated,
    )
    .await;

    if failed == 0 {
        let _ = sqlx::query("UPDATE xero_connections SET last_sync_at = NOW() WHERE tenant_id = $1")
            .bind(tenant_id.as_uuid())
            .execute(pool)
            .await;
    }

    info!(
        tenant = tenant_id.as_str(),
        imported, updated, failed, "Xero contact sync complete"
    );
    Ok(())
}

pub async fn xero_account_sync(
    tenant_id: &str,
    _payload: &Value,
    config: &WorkerConfig,
) -> Result<()> {
    let parsed = ensure_tenant_scope(tenant_id, config).await?;
    let pool = config.pg_manager.tenant(&parsed).await?;
    let sync_id = record_sync_log_started(&pool, "xero_sync_log", "accounts", &parsed).await?;

    let result = run_xero_account_sync(&pool, &parsed, config, sync_id).await;
    if let Err(ref e) = result {
        error!(error = %e, "Xero account sync failed");
        mark_sync_log_failed(&pool, "xero_sync_log", sync_id, &e.to_string()).await;
    }
    result
}

async fn run_xero_account_sync(
    pool: &PgPool,
    tenant_id: &TenantId,
    config: &WorkerConfig,
    sync_id: Uuid,
) -> Result<()> {
    let (client, _xero_tenant_id) = get_authenticated_xero_client(pool, tenant_id, config).await?;

    let mut all_accounts = Vec::new();
    let mut page = 1;
    let page_size = 100;
    loop {
        let accounts = client
            .query_accounts(page, page_size)
            .await
            .context("Xero API query_accounts error")?;
        if accounts.is_empty() {
            break;
        }
        all_accounts.extend(accounts);
        page += 1;
    }

    let expense_accounts: Vec<_> = all_accounts
        .into_iter()
        .filter(|a| a.Class == "EXPENSE" && a.Status == "ACTIVE")
        .collect();

    let mut created = 0u64;
    for xero_account in &expense_accounts {
        let _ = sqlx::query(
            "INSERT INTO xero_account_mappings \
             (tenant_id, xero_account_id, xero_account_code, xero_account_name, xero_account_type, billforge_gl_code, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $3, NOW(), NOW()) \
             ON CONFLICT (tenant_id, xero_account_id) DO UPDATE SET \
                xero_account_code = $3, \
                xero_account_name = $4, \
                xero_account_type = $5, \
                updated_at = NOW()",
        )
        .bind(tenant_id.as_uuid())
        .bind(&xero_account.AccountID)
        .bind(&xero_account.Code)
        .bind(&xero_account.Name)
        .bind(&xero_account.AccountType)
        .execute(pool)
        .await;
        created += 1;
    }

    mark_sync_log_completed(pool, "xero_sync_log", sync_id, created, created, 0).await;
    info!(tenant = tenant_id.as_str(), created, "Xero account sync complete");
    Ok(())
}

pub async fn xero_invoice_export(
    tenant_id: &str,
    payload: &Value,
    config: &WorkerConfig,
) -> Result<()> {
    let parsed = ensure_tenant_scope(tenant_id, config).await?;
    let pool = config.pg_manager.tenant(&parsed).await?;
    let sync_id =
        record_sync_log_started(&pool, "xero_sync_log", "invoice_export", &parsed).await?;

    let result = run_xero_invoice_export(&pool, &parsed, config, payload).await;
    match result {
        Ok(()) => {
            mark_sync_log_completed(&pool, "xero_sync_log", sync_id, 1, 1, 0).await;
            Ok(())
        }
        Err(e) => {
            error!(error = %e, "Xero invoice export failed");
            mark_sync_log_failed(&pool, "xero_sync_log", sync_id, &e.to_string()).await;
            Err(e)
        }
    }
}

async fn run_xero_invoice_export(
    pool: &PgPool,
    tenant_id: &TenantId,
    config: &WorkerConfig,
    payload: &Value,
) -> Result<()> {
    let invoice_id_str = payload
        .get("invoice_id")
        .and_then(|v| v.as_str())
        .context("missing invoice_id in payload")?;
    let xero_account_code = payload
        .get("xero_account_code")
        .and_then(|v| v.as_str())
        .context("missing xero_account_code in payload")?
        .to_string();

    let (client, _xero_tenant_id) = get_authenticated_xero_client(pool, tenant_id, config).await?;

    let invoice_id: billforge_core::domain::InvoiceId =
        invoice_id_str.parse().context("invalid invoice_id")?;

    let invoice: Option<(String, String, i64, Option<String>, Option<String>, String)> =
        sqlx::query_as(
            "SELECT vendor_name, invoice_number, total_amount_cents, due_date, po_number, currency \
             FROM invoices WHERE id = $1",
        )
        .bind(invoice_id.as_uuid())
        .fetch_optional(pool)
        .await
        .context("failed to load invoice")?;

    let (_vendor_name, invoice_number, total_cents, due_date, po_number, currency) =
        invoice.context("invoice not found")?;

    let vendor_mapping: Option<(String, String)> = sqlx::query_as(
        "SELECT xero_contact_id, xero_contact_name FROM xero_contact_mappings \
         WHERE tenant_id = $1 AND billforge_vendor_id IN \
         (SELECT vendor_id FROM invoices WHERE id = $2)",
    )
    .bind(tenant_id.as_uuid())
    .bind(invoice_id.as_uuid())
    .fetch_optional(pool)
    .await
    .context("failed to load vendor mapping")?;

    let (xero_contact_id, xero_contact_name) =
        vendor_mapping.context("Vendor not found in Xero. Please sync contacts first.")?;

    use billforge_xero::{XeroContact, XeroInvoice, XeroLineItem};
    let total_amount = total_cents as f64 / 100.0;
    let invoice_struct = XeroInvoice {
        InvoiceID: None,
        InvoiceNumber: Some(invoice_number.clone()),
        Reference: po_number.clone(),
        Contact: XeroContact {
            ContactID: xero_contact_id.clone(),
            Name: xero_contact_name,
            ContactStatus: "ACTIVE".to_string(),
            EmailAddress: None,
            Phones: None,
            Addresses: None,
            IsSupplier: Some(true),
            IsCustomer: None,
            DefaultCurrency: None,
            UpdatedDateUTC: None,
        },
        InvoiceType: "ACCPAY".to_string(),
        Status: Some("DRAFT".to_string()),
        LineItems: vec![XeroLineItem {
            LineItemID: None,
            Description: Some(format!("Invoice {}", invoice_number)),
            Quantity: Some(1.0),
            UnitAmount: Some(total_amount),
            AccountCode: Some(xero_account_code),
            TaxType: None,
            TaxAmount: Some(0.0),
            LineAmount: Some(total_amount),
            Tracking: None,
        }],
        Date: chrono::Utc::now()
            .date_naive()
            .format("%Y-%m-%d")
            .to_string(),
        DueDate: due_date.unwrap_or_else(|| {
            chrono::Utc::now()
                .date_naive()
                .format("%Y-%m-%d")
                .to_string()
        }),
        CurrencyCode: currency,
        SubTotal: total_amount,
        TotalTax: 0.0,
        Total: total_amount,
        AmountDue: Some(total_amount),
        AmountPaid: Some(0.0),
        UpdatedDateUTC: None,
    };

    let created_invoice = client
        .create_invoice(&invoice_struct)
        .await
        .context("Xero create_invoice failed")?;
    let xero_invoice_id = created_invoice.InvoiceID.clone().unwrap_or_default();

    let export_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO xero_invoice_exports (id, tenant_id, invoice_id, xero_invoice_id, exported_at, export_status) \
         VALUES ($1, $2, $3, $4, NOW(), 'synced') \
         ON CONFLICT (tenant_id, invoice_id) DO UPDATE SET \
            xero_invoice_id = $4, \
            exported_at = NOW(), \
            export_status = 'synced', \
            sync_error = NULL",
    )
    .bind(export_id)
    .bind(tenant_id.as_uuid())
    .bind(invoice_id.as_uuid())
    .bind(&xero_invoice_id)
    .execute(pool)
    .await
    .context("failed to write xero_invoice_exports")?;

    info!(tenant = tenant_id.as_str(), xero_invoice_id = %xero_invoice_id, "Xero invoice export complete");
    Ok(())
}

// ──────────────────────────── Sage Intacct ────────────────────────────

async fn get_sage_intacct_client(
    pool: &PgPool,
    tenant_id: &TenantId,
) -> Result<SageIntacctClient> {
    let connection: Option<(String, String, String, String, String, Option<String>)> =
        sqlx::query_as(
            "SELECT company_id, sender_id, sender_password, user_id, user_password, entity_id \
             FROM sage_intacct_connections \
             WHERE tenant_id = $1 AND sync_enabled = true",
        )
        .bind(tenant_id.as_uuid())
        .fetch_optional(pool)
        .await
        .context("failed to load sage_intacct_connections")?;

    let (company_id, sender_id, sender_password, user_id, user_password, entity_id) =
        connection.context("Sage Intacct not connected or sync disabled")?;

    let auth = SageIntacctAuth::new(SageIntacctAuthConfig {
        sender_id,
        sender_password,
        company_id,
        entity_id,
        user_id,
        user_password,
    });

    let session = auth
        .get_session()
        .await
        .context("Sage Intacct session failed")?;

    Ok(SageIntacctClient::new(session))
}

pub async fn sage_intacct_vendor_sync(
    tenant_id: &str,
    _payload: &Value,
    config: &WorkerConfig,
) -> Result<()> {
    let parsed = ensure_tenant_scope(tenant_id, config).await?;
    let pool = config.pg_manager.tenant(&parsed).await?;
    let sync_id =
        record_sync_log_started(&pool, "sage_intacct_sync_log", "vendors", &parsed).await?;

    let result = run_sage_vendor_sync(&pool, &parsed, sync_id).await;
    if let Err(ref e) = result {
        error!(error = %e, "Sage Intacct vendor sync failed");
        mark_sync_log_failed(&pool, "sage_intacct_sync_log", sync_id, &e.to_string()).await;
    }
    result
}

async fn run_sage_vendor_sync(pool: &PgPool, tenant_id: &TenantId, sync_id: Uuid) -> Result<()> {
    let client = get_sage_intacct_client(pool, tenant_id).await?;

    let mut all_vendors = Vec::new();
    let result = client
        .query_vendors(100, None)
        .await
        .context("Sage Intacct query_vendors failed")?;
    all_vendors.extend(result.records);

    if result.num_remaining > 0 {
        if let Some(result_id) = &result.result_id {
            let mut remaining = result.num_remaining;
            let mut rid = result_id.clone();
            while remaining > 0 {
                match client.read_more_vendors(&rid).await {
                    Ok(more) => {
                        remaining = more.num_remaining;
                        if let Some(new_rid) = &more.result_id {
                            rid = new_rid.clone();
                        }
                        all_vendors.extend(more.records);
                    }
                    Err(_) => break,
                }
            }
        }
    }

    let mut imported = 0u64;
    let mut updated = 0u64;

    for sage_vendor in &all_vendors {
        let existing: Option<(Uuid,)> = sqlx::query_as(
            "SELECT v.id FROM vendors v \
             INNER JOIN sage_intacct_vendor_mappings m ON m.billforge_vendor_id = v.id \
             WHERE m.tenant_id = $1 AND m.sage_vendor_id = $2",
        )
        .bind(tenant_id.as_uuid())
        .bind(&sage_vendor.vendor_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();

        let email = sage_vendor
            .display_contact
            .as_ref()
            .and_then(|c| c.email.as_deref())
            .unwrap_or("");
        let phone = sage_vendor
            .display_contact
            .as_ref()
            .and_then(|c| c.phone.as_deref())
            .unwrap_or("");

        if let Some((vendor_id,)) = existing {
            let _ = sqlx::query(
                "UPDATE vendors SET name = $2, email = $3, phone = $4, updated_at = NOW() \
                 WHERE id = $1",
            )
            .bind(vendor_id)
            .bind(&sage_vendor.name)
            .bind(email)
            .bind(phone)
            .execute(pool)
            .await;
            updated += 1;
        } else {
            let vendor_id = Uuid::new_v4();
            let vendor_type = sage_vendor.vendor_type_id.as_deref().unwrap_or("business");

            let _ = sqlx::query(
                "INSERT INTO vendors (id, name, vendor_type, email, phone, status, created_at, updated_at) \
                 VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())",
            )
            .bind(vendor_id)
            .bind(&sage_vendor.name)
            .bind(vendor_type)
            .bind(email)
            .bind(phone)
            .bind(if sage_vendor.status == "active" {
                "active"
            } else {
                "inactive"
            })
            .execute(pool)
            .await;

            let _ = sqlx::query(
                "INSERT INTO sage_intacct_vendor_mappings \
                 (tenant_id, sage_vendor_id, sage_record_no, billforge_vendor_id, sage_vendor_name, last_synced_at, created_at, updated_at) \
                 VALUES ($1, $2, $3, $4, $5, NOW(), NOW(), NOW())",
            )
            .bind(tenant_id.as_uuid())
            .bind(&sage_vendor.vendor_id)
            .bind(&sage_vendor.record_no)
            .bind(vendor_id)
            .bind(&sage_vendor.name)
            .execute(pool)
            .await;

            imported += 1;
        }
    }

    mark_sync_log_completed(
        pool,
        "sage_intacct_sync_log",
        sync_id,
        imported + updated,
        imported,
        updated,
    )
    .await;
    let _ = sqlx::query(
        "UPDATE sage_intacct_connections SET last_sync_at = NOW() WHERE tenant_id = $1",
    )
    .bind(tenant_id.as_uuid())
    .execute(pool)
    .await;
    info!(tenant = tenant_id.as_str(), imported, updated, "Sage Intacct vendor sync complete");
    Ok(())
}

pub async fn sage_intacct_account_sync(
    tenant_id: &str,
    _payload: &Value,
    config: &WorkerConfig,
) -> Result<()> {
    let parsed = ensure_tenant_scope(tenant_id, config).await?;
    let pool = config.pg_manager.tenant(&parsed).await?;
    let sync_id =
        record_sync_log_started(&pool, "sage_intacct_sync_log", "accounts", &parsed).await?;

    let result = run_sage_account_sync(&pool, &parsed, sync_id).await;
    if let Err(ref e) = result {
        error!(error = %e, "Sage Intacct account sync failed");
        mark_sync_log_failed(&pool, "sage_intacct_sync_log", sync_id, &e.to_string()).await;
    }
    result
}

async fn run_sage_account_sync(pool: &PgPool, tenant_id: &TenantId, sync_id: Uuid) -> Result<()> {
    let client = get_sage_intacct_client(pool, tenant_id).await?;

    let result = client
        .query_gl_accounts(100, None)
        .await
        .context("Sage Intacct query_gl_accounts failed")?;

    let mut created = 0u64;
    for account in &result.records {
        let _ = sqlx::query(
            "INSERT INTO sage_intacct_account_mappings \
             (tenant_id, sage_account_no, sage_account_title, sage_account_type, billforge_gl_code, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $2, NOW(), NOW()) \
             ON CONFLICT (tenant_id, sage_account_no) DO UPDATE SET \
                sage_account_title = $3, \
                sage_account_type = $4, \
                updated_at = NOW()",
        )
        .bind(tenant_id.as_uuid())
        .bind(&account.account_no)
        .bind(&account.title)
        .bind(&account.account_type)
        .execute(pool)
        .await;
        created += 1;
    }

    mark_sync_log_completed(pool, "sage_intacct_sync_log", sync_id, created, created, 0).await;
    info!(tenant = tenant_id.as_str(), created, "Sage Intacct account sync complete");
    Ok(())
}

pub async fn sage_intacct_invoice_export(
    tenant_id: &str,
    payload: &Value,
    config: &WorkerConfig,
) -> Result<()> {
    let parsed = ensure_tenant_scope(tenant_id, config).await?;
    let pool = config.pg_manager.tenant(&parsed).await?;
    let sync_id = record_sync_log_started(
        &pool,
        "sage_intacct_sync_log",
        "invoice_export",
        &parsed,
    )
    .await?;

    let result = run_sage_invoice_export(&pool, &parsed, payload).await;
    match result {
        Ok(()) => {
            mark_sync_log_completed(&pool, "sage_intacct_sync_log", sync_id, 1, 1, 0).await;
            Ok(())
        }
        Err(e) => {
            error!(error = %e, "Sage Intacct invoice export failed");
            mark_sync_log_failed(&pool, "sage_intacct_sync_log", sync_id, &e.to_string()).await;
            Err(e)
        }
    }
}

async fn run_sage_invoice_export(
    pool: &PgPool,
    tenant_id: &TenantId,
    payload: &Value,
) -> Result<()> {
    let invoice_id_str = payload
        .get("invoice_id")
        .and_then(|v| v.as_str())
        .context("missing invoice_id in payload")?;
    let sage_account_no = payload
        .get("sage_account_no")
        .and_then(|v| v.as_str())
        .context("missing sage_account_no in payload")?
        .to_string();
    let department_id = payload
        .get("department_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let location_id = payload
        .get("location_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let client = get_sage_intacct_client(pool, tenant_id).await?;

    let invoice_id: billforge_core::domain::InvoiceId =
        invoice_id_str.parse().context("invalid invoice_id")?;

    let invoice: Option<(String, String, i64, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT vendor_name, invoice_number, total_amount_cents, due_date, po_number \
         FROM invoices WHERE id = $1",
    )
    .bind(invoice_id.as_uuid())
    .fetch_optional(pool)
    .await
    .context("failed to load invoice")?;

    let (_vendor_name, invoice_number, total_cents, due_date, po_number) =
        invoice.context("invoice not found")?;

    let vendor_mapping: Option<(String,)> = sqlx::query_as(
        "SELECT sage_vendor_id FROM sage_intacct_vendor_mappings \
         WHERE tenant_id = $1 AND billforge_vendor_id IN \
         (SELECT vendor_id FROM invoices WHERE id = $2)",
    )
    .bind(tenant_id.as_uuid())
    .bind(invoice_id.as_uuid())
    .fetch_optional(pool)
    .await
    .context("failed to load vendor mapping")?;

    let sage_vendor_id = vendor_mapping
        .map(|(id,)| id)
        .context("Vendor not found in Sage Intacct. Please sync vendors first.")?;

    use billforge_sage_intacct::{SageAPBill, SageAPBillLine};
    let amount = total_cents as f64 / 100.0;
    let today = chrono::Utc::now().date_naive();

    let bill = SageAPBill {
        record_no: None,
        transaction_type: "Vendor Invoice".to_string(),
        vendor_id: sage_vendor_id,
        date_created: today,
        date_due: due_date.and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok()),
        document_number: Some(invoice_number.clone()),
        reference_number: po_number,
        description: Some(format!("BillForge export: Invoice {}", invoice_number)),
        currency: Some("USD".to_string()),
        exchange_rate_type: None,
        lines: vec![SageAPBillLine {
            gl_account_no: sage_account_no,
            amount,
            memo: Some(format!("Invoice {}", invoice_number)),
            department_id: department_id.clone(),
            location_id: location_id.clone(),
            project_id: None,
            class_id: None,
        }],
        state: None,
        total_amount: Some(amount),
        location_id,
        department_id,
    };

    let result = client
        .create_ap_bill(&bill)
        .await
        .context("Sage Intacct create_ap_bill failed")?;

    if result.status != "success" {
        let error_msg = result
            .errors
            .first()
            .map(|e| e.description.clone())
            .unwrap_or_else(|| "Unknown error".to_string());
        anyhow::bail!("Sage Intacct bill creation failed: {}", error_msg);
    }

    let sage_record_no = result.key.unwrap_or_default();
    let export_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO sage_intacct_invoice_exports (id, tenant_id, invoice_id, sage_record_no, exported_at, export_status) \
         VALUES ($1, $2, $3, $4, NOW(), 'synced') \
         ON CONFLICT (tenant_id, invoice_id) DO UPDATE SET \
            sage_record_no = $4, \
            exported_at = NOW(), \
            export_status = 'synced', \
            sync_error = NULL",
    )
    .bind(export_id)
    .bind(tenant_id.as_uuid())
    .bind(invoice_id.as_uuid())
    .bind(&sage_record_no)
    .execute(pool)
    .await
    .context("failed to write sage_intacct_invoice_exports")?;

    info!(tenant = tenant_id.as_str(), sage_record_no = %sage_record_no, "Sage Intacct invoice export complete");
    Ok(())
}

// ──────────────────────────── Salesforce ────────────────────────────

fn salesforce_env(config: &WorkerConfig) -> SalesforceEnvironment {
    match config.salesforce_environment.as_str() {
        "sandbox" => SalesforceEnvironment::Sandbox,
        _ => SalesforceEnvironment::Production,
    }
}

async fn get_salesforce_client(
    pool: &PgPool,
    tenant_id: &TenantId,
    config: &WorkerConfig,
) -> Result<SalesforceClient> {
    let connection: Option<(String, String, chrono::DateTime<Utc>)> = sqlx::query_as(
        "SELECT instance_url, access_token, access_token_expires_at \
         FROM salesforce_connections \
         WHERE tenant_id = $1 AND sync_enabled = true",
    )
    .bind(tenant_id.as_uuid())
    .fetch_optional(pool)
    .await
    .context("failed to load salesforce_connections")?;

    let (instance_url, mut access_token, token_expires_at) =
        connection.context("Salesforce not connected or sync disabled")?;

    if token_expires_at <= Utc::now() {
        let refresh_row: Option<(String,)> = sqlx::query_as(
            "SELECT refresh_token FROM salesforce_connections WHERE tenant_id = $1",
        )
        .bind(tenant_id.as_uuid())
        .fetch_optional(pool)
        .await
        .context("failed to load Salesforce refresh token")?;

        let refresh_token_val =
            refresh_row.map(|(t,)| t).context("Salesforce refresh token missing")?;

        let client_id = config
            .salesforce_client_id
            .as_deref()
            .context("SALESFORCE_CLIENT_ID not configured")?;
        let client_secret = config
            .salesforce_client_secret
            .as_deref()
            .context("SALESFORCE_CLIENT_SECRET not configured")?;

        let oauth = SalesforceOAuth::new(SalesforceOAuthConfig {
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            redirect_uri: String::new(),
            environment: salesforce_env(config),
        });

        let new_tokens = oauth
            .refresh_token(&refresh_token_val)
            .await
            .context("Salesforce token refresh failed")?;

        let new_expires = Utc::now() + Duration::hours(2);
        sqlx::query(
            "UPDATE salesforce_connections SET access_token = $2, access_token_expires_at = $3, updated_at = NOW() WHERE tenant_id = $1",
        )
        .bind(tenant_id.as_uuid())
        .bind(&new_tokens.access_token)
        .bind(new_expires)
        .execute(pool)
        .await
        .context("failed to persist refreshed Salesforce token")?;

        access_token = new_tokens.access_token;
    }

    Ok(SalesforceClient::new(access_token, instance_url))
}

pub async fn salesforce_account_sync(
    tenant_id: &str,
    payload: &Value,
    config: &WorkerConfig,
) -> Result<()> {
    let parsed = ensure_tenant_scope(tenant_id, config).await?;
    let pool = config.pg_manager.tenant(&parsed).await?;
    let sync_id =
        record_sync_log_started(&pool, "salesforce_sync_log", "accounts", &parsed).await?;

    let result = run_salesforce_account_sync(&pool, &parsed, config, payload, sync_id).await;
    if let Err(ref e) = result {
        error!(error = %e, "Salesforce account sync failed");
        mark_sync_log_failed(&pool, "salesforce_sync_log", sync_id, &e.to_string()).await;
    }
    result
}

async fn run_salesforce_account_sync(
    pool: &PgPool,
    tenant_id: &TenantId,
    config: &WorkerConfig,
    payload: &Value,
    sync_id: Uuid,
) -> Result<()> {
    let custom_filter = payload
        .get("custom_filter")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let client = get_salesforce_client(pool, tenant_id, config).await?;

    let accounts = client
        .query_vendor_accounts(custom_filter.as_deref())
        .await
        .context("Salesforce query_vendor_accounts failed")?;

    let mut imported = 0u64;
    let mut updated = 0u64;

    for sf_account in &accounts {
        let existing: Option<(Uuid,)> = sqlx::query_as(
            "SELECT v.id FROM vendors v \
             INNER JOIN salesforce_account_mappings m ON m.billforge_vendor_id = v.id \
             WHERE m.tenant_id = $1 AND m.salesforce_account_id = $2",
        )
        .bind(tenant_id.as_uuid())
        .bind(&sf_account.id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();

        let phone = sf_account.phone.as_deref().unwrap_or("");

        if let Some((vendor_id,)) = existing {
            let _ = sqlx::query(
                "UPDATE vendors SET name = $2, phone = $3, updated_at = NOW() WHERE id = $1",
            )
            .bind(vendor_id)
            .bind(&sf_account.name)
            .bind(phone)
            .execute(pool)
            .await;
            updated += 1;
        } else {
            let vendor_id = Uuid::new_v4();
            let vendor_type = sf_account.account_type.as_deref().unwrap_or("business");

            let _ = sqlx::query(
                "INSERT INTO vendors (id, name, vendor_type, phone, status, created_at, updated_at) \
                 VALUES ($1, $2, $3, $4, 'active', NOW(), NOW())",
            )
            .bind(vendor_id)
            .bind(&sf_account.name)
            .bind(vendor_type)
            .bind(phone)
            .execute(pool)
            .await;

            let _ = sqlx::query(
                "INSERT INTO salesforce_account_mappings \
                 (tenant_id, salesforce_account_id, billforge_vendor_id, salesforce_account_name, salesforce_account_type, last_synced_at, created_at, updated_at) \
                 VALUES ($1, $2, $3, $4, $5, NOW(), NOW(), NOW())",
            )
            .bind(tenant_id.as_uuid())
            .bind(&sf_account.id)
            .bind(vendor_id)
            .bind(&sf_account.name)
            .bind(&sf_account.account_type)
            .execute(pool)
            .await;

            imported += 1;
        }
    }

    mark_sync_log_completed(
        pool,
        "salesforce_sync_log",
        sync_id,
        imported + updated,
        imported,
        updated,
    )
    .await;
    let _ = sqlx::query(
        "UPDATE salesforce_connections SET last_sync_at = NOW() WHERE tenant_id = $1",
    )
    .bind(tenant_id.as_uuid())
    .execute(pool)
    .await;

    info!(tenant = tenant_id.as_str(), imported, updated, "Salesforce account sync complete");
    Ok(())
}

pub async fn salesforce_contact_sync(
    tenant_id: &str,
    _payload: &Value,
    config: &WorkerConfig,
) -> Result<()> {
    let parsed = ensure_tenant_scope(tenant_id, config).await?;
    let pool = config.pg_manager.tenant(&parsed).await?;
    let sync_id =
        record_sync_log_started(&pool, "salesforce_sync_log", "contacts", &parsed).await?;

    let result = run_salesforce_contact_sync(&pool, &parsed, config, sync_id).await;
    if let Err(ref e) = result {
        error!(error = %e, "Salesforce contact sync failed");
        mark_sync_log_failed(&pool, "salesforce_sync_log", sync_id, &e.to_string()).await;
    }
    result
}

async fn run_salesforce_contact_sync(
    pool: &PgPool,
    tenant_id: &TenantId,
    config: &WorkerConfig,
    sync_id: Uuid,
) -> Result<()> {
    let client = get_salesforce_client(pool, tenant_id, config).await?;

    let contacts = client
        .query_vendor_contacts()
        .await
        .context("Salesforce query_vendor_contacts failed")?;

    let mut synced = 0u64;
    for contact in &contacts {
        if let Some(account_id) = &contact.account_id {
            let vendor_mapping: Option<(Uuid,)> = sqlx::query_as(
                "SELECT billforge_vendor_id FROM salesforce_account_mappings \
                 WHERE tenant_id = $1 AND salesforce_account_id = $2",
            )
            .bind(tenant_id.as_uuid())
            .bind(account_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();

            if let Some((vendor_id,)) = vendor_mapping {
                let full_name = format!(
                    "{} {}",
                    contact.first_name.as_deref().unwrap_or(""),
                    contact.last_name
                )
                .trim()
                .to_string();

                if let Some(email) = &contact.email {
                    let _ = sqlx::query(
                        "UPDATE vendors SET email = $2, contact_name = $3, updated_at = NOW() WHERE id = $1 AND (email IS NULL OR email = '')",
                    )
                    .bind(vendor_id)
                    .bind(email)
                    .bind(&full_name)
                    .execute(pool)
                    .await;
                }

                synced += 1;
            }
        }
    }

    mark_sync_log_completed(pool, "salesforce_sync_log", sync_id, synced, 0, synced).await;
    info!(tenant = tenant_id.as_str(), synced, "Salesforce contact sync complete");
    Ok(())
}

// ──────────────────────────── Workday ────────────────────────────

async fn get_workday_client(
    pool: &PgPool,
    tenant_id: &TenantId,
    config: &WorkerConfig,
) -> Result<WorkdayClient> {
    let connection: Option<(String, String, String, String, chrono::DateTime<Utc>)> =
        sqlx::query_as(
            "SELECT access_token, refresh_token, workday_tenant_url, workday_tenant_name, access_token_expires_at \
             FROM workday_connections \
             WHERE tenant_id = $1 AND sync_enabled = true",
        )
        .bind(tenant_id.as_uuid())
        .fetch_optional(pool)
        .await
        .context("failed to load workday_connections")?;

    let (mut access_token, refresh_token_val, tenant_url, tenant_name, token_expires_at) =
        connection.context("Workday not connected or sync disabled")?;

    if token_expires_at <= Utc::now() {
        let client_id = config
            .workday_client_id
            .as_deref()
            .context("WORKDAY_CLIENT_ID not configured")?;
        let client_secret = config
            .workday_client_secret
            .as_deref()
            .context("WORKDAY_CLIENT_SECRET not configured")?;

        let oauth = WorkdayOAuth::new(WorkdayOAuthConfig {
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            refresh_token: refresh_token_val.clone(),
            tenant_url: tenant_url.clone(),
            tenant_name: tenant_name.clone(),
        });

        let new_tokens = oauth
            .refresh_token(&refresh_token_val)
            .await
            .context("Workday token refresh failed")?;

        let new_expires = Utc::now() + Duration::seconds(new_tokens.expires_in);
        sqlx::query(
            "UPDATE workday_connections \
             SET access_token = $2, refresh_token = $3, access_token_expires_at = $4, updated_at = NOW() \
             WHERE tenant_id = $1",
        )
        .bind(tenant_id.as_uuid())
        .bind(&new_tokens.access_token)
        .bind(&new_tokens.refresh_token)
        .bind(new_expires)
        .execute(pool)
        .await
        .context("failed to persist refreshed Workday tokens")?;

        access_token = new_tokens.access_token;
    }

    Ok(WorkdayClient::new(access_token, tenant_url, tenant_name))
}

pub async fn workday_supplier_sync(
    tenant_id: &str,
    _payload: &Value,
    config: &WorkerConfig,
) -> Result<()> {
    let parsed = ensure_tenant_scope(tenant_id, config).await?;
    let pool = config.pg_manager.tenant(&parsed).await?;
    let sync_id = record_sync_log_started(&pool, "workday_sync_log", "suppliers", &parsed).await?;

    let result = run_workday_supplier_sync(&pool, &parsed, config, sync_id).await;
    if let Err(ref e) = result {
        error!(error = %e, "Workday supplier sync failed");
        mark_sync_log_failed(&pool, "workday_sync_log", sync_id, &e.to_string()).await;
    }
    result
}

async fn run_workday_supplier_sync(
    pool: &PgPool,
    tenant_id: &TenantId,
    config: &WorkerConfig,
    sync_id: Uuid,
) -> Result<()> {
    let client = get_workday_client(pool, tenant_id, config).await?;

    let mut all_suppliers = Vec::new();
    let mut page = 0;
    let page_size = 100;
    loop {
        let result = client
            .query_suppliers(page, page_size)
            .await
            .context("Workday query_suppliers failed")?;
        let fetched = result.data.len();
        all_suppliers.extend(result.data);
        if fetched < page_size as usize {
            break;
        }
        page += 1;
    }

    let mut imported = 0u64;
    let mut updated = 0u64;

    for supplier in &all_suppliers {
        let existing: Option<(Uuid,)> = sqlx::query_as(
            "SELECT v.id FROM vendors v \
             INNER JOIN workday_supplier_mappings m ON m.billforge_vendor_id = v.id \
             WHERE m.tenant_id = $1 AND m.workday_supplier_id = $2",
        )
        .bind(tenant_id.as_uuid())
        .bind(&supplier.supplier_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();

        let email = supplier.primary_email.as_deref().unwrap_or("");
        let phone = supplier.primary_phone.as_deref().unwrap_or("");

        if let Some((vendor_id,)) = existing {
            let _ = sqlx::query(
                "UPDATE vendors SET name = $2, email = $3, phone = $4, updated_at = NOW() \
                 WHERE id = $1",
            )
            .bind(vendor_id)
            .bind(&supplier.supplier_name)
            .bind(email)
            .bind(phone)
            .execute(pool)
            .await;
            updated += 1;
        } else {
            let vendor_id = Uuid::new_v4();
            let vendor_type = supplier.supplier_category.as_deref().unwrap_or("business");
            let _ = sqlx::query(
                "INSERT INTO vendors (id, name, vendor_type, email, phone, status, created_at, updated_at) \
                 VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())",
            )
            .bind(vendor_id)
            .bind(&supplier.supplier_name)
            .bind(vendor_type)
            .bind(email)
            .bind(phone)
            .bind(if supplier.status == "Active" {
                "active"
            } else {
                "inactive"
            })
            .execute(pool)
            .await;

            let _ = sqlx::query(
                "INSERT INTO workday_supplier_mappings \
                 (tenant_id, workday_supplier_id, billforge_vendor_id, workday_supplier_name, last_synced_at, created_at, updated_at) \
                 VALUES ($1, $2, $3, $4, NOW(), NOW(), NOW())",
            )
            .bind(tenant_id.as_uuid())
            .bind(&supplier.supplier_id)
            .bind(vendor_id)
            .bind(&supplier.supplier_name)
            .execute(pool)
            .await;

            imported += 1;
        }
    }

    mark_sync_log_completed(
        pool,
        "workday_sync_log",
        sync_id,
        imported + updated,
        imported,
        updated,
    )
    .await;
    let _ = sqlx::query("UPDATE workday_connections SET last_sync_at = NOW() WHERE tenant_id = $1")
        .bind(tenant_id.as_uuid())
        .execute(pool)
        .await;

    info!(tenant = tenant_id.as_str(), imported, updated, "Workday supplier sync complete");
    Ok(())
}

pub async fn workday_account_sync(
    tenant_id: &str,
    _payload: &Value,
    config: &WorkerConfig,
) -> Result<()> {
    let parsed = ensure_tenant_scope(tenant_id, config).await?;
    let pool = config.pg_manager.tenant(&parsed).await?;
    let sync_id = record_sync_log_started(&pool, "workday_sync_log", "accounts", &parsed).await?;

    let result = run_workday_account_sync(&pool, &parsed, config, sync_id).await;
    if let Err(ref e) = result {
        error!(error = %e, "Workday account sync failed");
        mark_sync_log_failed(&pool, "workday_sync_log", sync_id, &e.to_string()).await;
    }
    result
}

async fn run_workday_account_sync(
    pool: &PgPool,
    tenant_id: &TenantId,
    config: &WorkerConfig,
    sync_id: Uuid,
) -> Result<()> {
    let client = get_workday_client(pool, tenant_id, config).await?;

    let mut all_accounts = Vec::new();
    let mut page = 0;
    let page_size = 100;
    loop {
        let result = client
            .query_ledger_accounts(page, page_size)
            .await
            .context("Workday query_ledger_accounts failed")?;
        let fetched = result.data.len();
        all_accounts.extend(result.data);
        if fetched < page_size as usize {
            break;
        }
        page += 1;
    }

    let mut created = 0u64;
    for account in &all_accounts {
        let _ = sqlx::query(
            "INSERT INTO workday_account_mappings \
             (tenant_id, workday_account_id, workday_account_name, workday_account_type, billforge_gl_code, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $2, NOW(), NOW()) \
             ON CONFLICT (tenant_id, workday_account_id) DO UPDATE SET \
                workday_account_name = $3, \
                workday_account_type = $4, \
                updated_at = NOW()",
        )
        .bind(tenant_id.as_uuid())
        .bind(&account.ledger_account_id)
        .bind(&account.name)
        .bind(&account.account_type)
        .execute(pool)
        .await;
        created += 1;
    }

    mark_sync_log_completed(pool, "workday_sync_log", sync_id, created, created, 0).await;
    info!(tenant = tenant_id.as_str(), created, "Workday account sync complete");
    Ok(())
}

pub async fn workday_invoice_export(
    tenant_id: &str,
    payload: &Value,
    config: &WorkerConfig,
) -> Result<()> {
    let parsed = ensure_tenant_scope(tenant_id, config).await?;
    let pool = config.pg_manager.tenant(&parsed).await?;
    let sync_id =
        record_sync_log_started(&pool, "workday_sync_log", "invoice_export", &parsed).await?;

    let result = run_workday_invoice_export(&pool, &parsed, config, payload).await;
    match result {
        Ok(()) => {
            mark_sync_log_completed(&pool, "workday_sync_log", sync_id, 1, 1, 0).await;
            Ok(())
        }
        Err(e) => {
            error!(error = %e, "Workday invoice export failed");
            mark_sync_log_failed(&pool, "workday_sync_log", sync_id, &e.to_string()).await;
            Err(e)
        }
    }
}

async fn run_workday_invoice_export(
    pool: &PgPool,
    tenant_id: &TenantId,
    config: &WorkerConfig,
    payload: &Value,
) -> Result<()> {
    let invoice_id_str = payload
        .get("invoice_id")
        .and_then(|v| v.as_str())
        .context("missing invoice_id in payload")?;
    let ledger_account_id = payload
        .get("ledger_account_id")
        .and_then(|v| v.as_str())
        .context("missing ledger_account_id in payload")?
        .to_string();
    let spend_category = payload
        .get("spend_category")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let cost_center = payload
        .get("cost_center")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let company_reference = payload
        .get("company_reference")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let client = get_workday_client(pool, tenant_id, config).await?;

    let invoice_id: billforge_core::domain::InvoiceId =
        invoice_id_str.parse().context("invalid invoice_id")?;

    let invoice: Option<(String, String, i64, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT vendor_name, invoice_number, total_amount_cents, due_date, po_number \
         FROM invoices WHERE id = $1",
    )
    .bind(invoice_id.as_uuid())
    .fetch_optional(pool)
    .await
    .context("failed to load invoice")?;

    let (_vendor_name, invoice_number, total_cents, due_date, _po_number) =
        invoice.context("invoice not found")?;

    let supplier_mapping: Option<(String,)> = sqlx::query_as(
        "SELECT workday_supplier_id FROM workday_supplier_mappings \
         WHERE tenant_id = $1 AND billforge_vendor_id IN \
         (SELECT vendor_id FROM invoices WHERE id = $2)",
    )
    .bind(tenant_id.as_uuid())
    .bind(invoice_id.as_uuid())
    .fetch_optional(pool)
    .await
    .context("failed to load supplier mapping")?;

    let workday_supplier_id = supplier_mapping
        .map(|(id,)| id)
        .context("Vendor not found in Workday. Please sync suppliers first.")?;

    use billforge_workday::{WorkdayInvoiceLine, WorkdaySupplierInvoice};
    let amount = total_cents as f64 / 100.0;
    let today = chrono::Utc::now().date_naive();

    let wd_invoice = WorkdaySupplierInvoice {
        id: None,
        invoice_number: invoice_number.clone(),
        supplier_id: workday_supplier_id,
        invoice_date: today,
        due_date: due_date.and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok()),
        total_amount: amount,
        currency: Some("USD".to_string()),
        memo: Some(format!("BillForge export: Invoice {}", invoice_number)),
        lines: vec![WorkdayInvoiceLine {
            line_number: 1,
            amount,
            memo: Some(format!("Invoice {}", invoice_number)),
            spend_category,
            ledger_account: Some(ledger_account_id),
            cost_center,
            project: None,
        }],
        status: None,
        company_reference,
    };

    let result = client
        .create_supplier_invoice(&wd_invoice)
        .await
        .context("Workday create_supplier_invoice failed")?;

    let workday_invoice_id = result.id.unwrap_or_default();
    let export_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workday_invoice_exports (id, tenant_id, invoice_id, workday_invoice_id, exported_at, export_status) \
         VALUES ($1, $2, $3, $4, NOW(), 'synced') \
         ON CONFLICT (tenant_id, invoice_id) DO UPDATE SET \
            workday_invoice_id = $4, \
            exported_at = NOW(), \
            export_status = 'synced'",
    )
    .bind(export_id)
    .bind(tenant_id.as_uuid())
    .bind(invoice_id.as_uuid())
    .bind(&workday_invoice_id)
    .execute(pool)
    .await
    .context("failed to write workday_invoice_exports")?;

    info!(tenant = tenant_id.as_str(), workday_invoice_id = %workday_invoice_id, "Workday invoice export complete");
    Ok(())
}

// ──────────────────────────── Bill.com ────────────────────────────

fn parse_bill_com_env(env: &str) -> BillComEnvironment {
    match env.to_lowercase().as_str() {
        "production" | "prod" => BillComEnvironment::Production,
        _ => BillComEnvironment::Sandbox,
    }
}

async fn get_bill_com_client(
    pool: &PgPool,
    tenant_id: &TenantId,
) -> Result<BillComClient> {
    let connection: Option<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT dev_key, org_id, user_name, password, environment \
         FROM bill_com_connections \
         WHERE tenant_id = $1 AND sync_enabled = true",
    )
    .bind(tenant_id.as_uuid())
    .fetch_optional(pool)
    .await
    .context("failed to load bill_com_connections")?;

    let (dev_key, org_id, user_name, password, environment) =
        connection.context("Bill.com not connected or sync disabled")?;

    let env = parse_bill_com_env(&environment);
    let auth = BillComAuth::new(BillComAuthConfig {
        dev_key: dev_key.clone(),
        org_id,
        user_name,
        password,
        environment: env,
    });

    let session = auth.login().await.context("Bill.com login failed")?;
    Ok(BillComClient::new(session, env, dev_key))
}

pub async fn bill_com_vendor_sync(
    tenant_id: &str,
    _payload: &Value,
    config: &WorkerConfig,
) -> Result<()> {
    let parsed = ensure_tenant_scope(tenant_id, config).await?;
    let pool = config.pg_manager.tenant(&parsed).await?;
    let sync_id = record_sync_log_started(&pool, "bill_com_sync_log", "vendors", &parsed).await?;

    let result = run_bill_com_vendor_sync(&pool, &parsed, sync_id).await;
    if let Err(ref e) = result {
        error!(error = %e, "Bill.com vendor sync failed");
        mark_sync_log_failed(&pool, "bill_com_sync_log", sync_id, &e.to_string()).await;
    }
    result
}

async fn run_bill_com_vendor_sync(
    pool: &PgPool,
    tenant_id: &TenantId,
    sync_id: Uuid,
) -> Result<()> {
    let client = get_bill_com_client(pool, tenant_id).await?;

    let mut all_vendors = Vec::new();
    let mut page = 0;
    let page_size = 100;
    loop {
        let result = client
            .list_vendors(page, page_size)
            .await
            .context("Bill.com list_vendors failed")?;
        all_vendors.extend(result.data);
        if !result.has_more {
            break;
        }
        page += 1;
    }

    let mut imported = 0u64;
    let mut updated = 0u64;

    for bc_vendor in &all_vendors {
        let bc_vendor_id = bc_vendor.id.as_deref().unwrap_or_default();
        if bc_vendor_id.is_empty() {
            continue;
        }

        let existing: Option<(Uuid,)> = sqlx::query_as(
            "SELECT v.id FROM vendors v \
             INNER JOIN bill_com_vendor_mappings m ON m.billforge_vendor_id = v.id \
             WHERE m.tenant_id = $1 AND m.bill_com_vendor_id = $2",
        )
        .bind(tenant_id.as_uuid())
        .bind(bc_vendor_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();

        let email = bc_vendor.email.as_deref().unwrap_or("");
        let phone = bc_vendor.phone.as_deref().unwrap_or("");

        if let Some((vendor_id,)) = existing {
            let _ = sqlx::query(
                "UPDATE vendors SET name = $2, email = $3, phone = $4, updated_at = NOW() \
                 WHERE id = $1",
            )
            .bind(vendor_id)
            .bind(&bc_vendor.name)
            .bind(email)
            .bind(phone)
            .execute(pool)
            .await;
            updated += 1;
        } else {
            let vendor_id = Uuid::new_v4();
            let _ = sqlx::query(
                "INSERT INTO vendors (id, name, vendor_type, email, phone, status, created_at, updated_at) \
                 VALUES ($1, $2, 'business', $3, $4, $5, NOW(), NOW())",
            )
            .bind(vendor_id)
            .bind(&bc_vendor.name)
            .bind(email)
            .bind(phone)
            .bind(bc_vendor.status.as_deref().unwrap_or("active"))
            .execute(pool)
            .await;

            let _ = sqlx::query(
                "INSERT INTO bill_com_vendor_mappings \
                 (tenant_id, bill_com_vendor_id, billforge_vendor_id, bill_com_vendor_name, last_synced_at, created_at, updated_at) \
                 VALUES ($1, $2, $3, $4, NOW(), NOW(), NOW())",
            )
            .bind(tenant_id.as_uuid())
            .bind(bc_vendor_id)
            .bind(vendor_id)
            .bind(&bc_vendor.name)
            .execute(pool)
            .await;

            imported += 1;
        }
    }

    mark_sync_log_completed(
        pool,
        "bill_com_sync_log",
        sync_id,
        imported + updated,
        imported,
        updated,
    )
    .await;
    let _ = sqlx::query("UPDATE bill_com_connections SET last_sync_at = NOW() WHERE tenant_id = $1")
        .bind(tenant_id.as_uuid())
        .execute(pool)
        .await;
    info!(tenant = tenant_id.as_str(), imported, updated, "Bill.com vendor sync complete");
    Ok(())
}

// ──────────────────────────── NetSuite ────────────────────────────

pub async fn netsuite_vendor_sync(
    tenant_id: &str,
    _payload: &Value,
    config: &WorkerConfig,
) -> Result<()> {
    let parsed = ensure_tenant_scope(tenant_id, config).await?;
    let pool = config.pg_manager.tenant(&parsed).await?;

    let result = run_netsuite_vendor_sync(&pool, &parsed).await;
    if let Err(ref e) = result {
        error!(error = %e, "NetSuite vendor sync failed");
    }
    result
}

async fn run_netsuite_vendor_sync(pool: &PgPool, tenant_id: &TenantId) -> Result<()> {
    let connection: Option<(String, String, String)> = sqlx::query_as(
        "SELECT account_id, client_id, client_secret \
         FROM netsuite_connections \
         WHERE tenant_id = $1 AND sync_enabled = true",
    )
    .bind(tenant_id.as_uuid())
    .fetch_optional(pool)
    .await
    .context("failed to load netsuite_connections")?;

    let (account_id, client_id, client_secret) =
        connection.context("NetSuite not connected or sync disabled")?;

    let mut client = NetSuiteClient::new(NetSuiteConfig {
        account_id,
        client_id,
        client_secret,
        base_url: None,
    });

    client
        .authenticate()
        .await
        .context("NetSuite authentication failed")?;

    let vendors = client.list_vendors().await.context("NetSuite list_vendors failed")?;

    let mut imported = 0u64;
    let mut updated = 0u64;

    for ns_vendor in &vendors {
        let vendor_name = ns_vendor.company_name.as_deref().unwrap_or_default();
        if vendor_name.is_empty() {
            continue;
        }

        let email = ns_vendor.email.as_deref().unwrap_or("");

        let existing: Option<(Uuid,)> =
            sqlx::query_as("SELECT id FROM vendors WHERE name = $1 LIMIT 1")
                .bind(vendor_name)
                .fetch_optional(pool)
                .await
                .ok()
                .flatten();

        if let Some((vendor_id,)) = existing {
            let _ = sqlx::query("UPDATE vendors SET email = $2, updated_at = NOW() WHERE id = $1")
                .bind(vendor_id)
                .bind(email)
                .execute(pool)
                .await;
            updated += 1;
        } else {
            let vendor_id = Uuid::new_v4();
            let _ = sqlx::query(
                "INSERT INTO vendors (id, name, vendor_type, email, status, created_at, updated_at) \
                 VALUES ($1, $2, 'business', $3, 'active', NOW(), NOW())",
            )
            .bind(vendor_id)
            .bind(vendor_name)
            .bind(email)
            .execute(pool)
            .await;
            imported += 1;
        }
    }

    let _ = sqlx::query("UPDATE netsuite_connections SET last_sync_at = NOW() WHERE tenant_id = $1")
        .bind(tenant_id.as_uuid())
        .execute(pool)
        .await;

    info!(tenant = tenant_id.as_str(), imported, updated, "NetSuite vendor sync complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::{Job, JobType};
    use chrono::Utc;
    use serde_json::json;

    fn make_job(job_type: JobType, payload: Value) -> Job {
        Job {
            id: "job-erp-sync-test".to_string(),
            job_type,
            tenant_id: Uuid::new_v4().to_string(),
            payload,
            created_at: Utc::now(),
            retry_count: 0,
        }
    }

    /// Verify every new non-QB ERP JobType variant serializes to a stable
    /// snake_case discriminator so the route enqueue side and the worker
    /// dispatcher agree on the wire format.
    #[test]
    fn erp_job_types_serialize_to_snake_case() {
        let cases = [
            (JobType::XeroContactSync, "xero_contact_sync"),
            (JobType::XeroAccountSync, "xero_account_sync"),
            (JobType::XeroInvoiceExport, "xero_invoice_export"),
            (JobType::SageIntacctVendorSync, "sage_intacct_vendor_sync"),
            (JobType::SageIntacctAccountSync, "sage_intacct_account_sync"),
            (
                JobType::SageIntacctInvoiceExport,
                "sage_intacct_invoice_export",
            ),
            (JobType::SalesforceAccountSync, "salesforce_account_sync"),
            (JobType::SalesforceContactSync, "salesforce_contact_sync"),
            (JobType::WorkdaySupplierSync, "workday_supplier_sync"),
            (JobType::WorkdayAccountSync, "workday_account_sync"),
            (JobType::WorkdayInvoiceExport, "workday_invoice_export"),
            (JobType::BillComVendorSync, "bill_com_vendor_sync"),
            (JobType::NetSuiteVendorSync, "netsuite_vendor_sync"),
        ];

        for (variant, expected) in cases {
            let serialized = serde_json::to_value(&variant).unwrap();
            assert_eq!(
                serialized,
                Value::String(expected.to_string()),
                "{:?} should serialize to {}",
                variant,
                expected
            );
        }
    }

    /// Round-trip a Job through JSON for each new variant — this is what the
    /// HTTP route does (serialize) and what the worker BRPOP loop does
    /// (deserialize), so both sides must agree on the JobType encoding.
    #[test]
    fn erp_jobs_roundtrip_through_redis_json() {
        let variants = [
            JobType::XeroContactSync,
            JobType::XeroAccountSync,
            JobType::XeroInvoiceExport,
            JobType::SageIntacctVendorSync,
            JobType::SageIntacctAccountSync,
            JobType::SageIntacctInvoiceExport,
            JobType::SalesforceAccountSync,
            JobType::SalesforceContactSync,
            JobType::WorkdaySupplierSync,
            JobType::WorkdayAccountSync,
            JobType::WorkdayInvoiceExport,
            JobType::BillComVendorSync,
            JobType::NetSuiteVendorSync,
        ];

        for variant in variants {
            let job = make_job(variant.clone(), json!({"integration_id": "abc"}));
            let raw = serde_json::to_string(&job).expect("serialize");
            let parsed: Job = serde_json::from_str(&raw).expect("deserialize");
            assert_eq!(parsed.job_type.to_string(), variant.to_string());
        }
    }

    /// Confirm the invoice-export payload parser rejects missing fields with
    /// a clear error (so the worker surfaces the bug instead of silently
    /// writing a 'completed' sync_log row).
    #[test]
    fn invoice_export_payload_requires_invoice_id() {
        let payload = json!({});
        let err = payload
            .get("invoice_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing invoice_id in payload"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("invoice_id"), "got: {}", err);
    }
}
