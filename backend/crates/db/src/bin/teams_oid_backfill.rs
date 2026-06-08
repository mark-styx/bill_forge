//! Backfill teams_webhooks.aad_object_id for an existing Teams registration.
//!
//! Required because /teams/actions binds the BillForge actor to the AAD oid
//! claim from a validated Microsoft JWT, and the column cannot be populated
//! from user-facing input (impersonation risk, see issue #362).
//!
//! Usage:
//!   cargo run -p billforge-db --bin teams_oid_backfill -- \
//!     --tenant <uuid> --user <uuid> --oid <uuid>
//!   cargo run -p billforge-db --bin teams_oid_backfill -- \
//!     --tenant <uuid> --user <uuid> --oid <uuid> --dry-run

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(name = "teams_oid_backfill")]
#[command(about = "Backfill teams_webhooks.aad_object_id for a registered user")]
struct Cli {
    /// BillForge tenant id (matches teams_webhooks.tenant_id).
    #[arg(long)]
    tenant: Uuid,

    /// BillForge user id (matches teams_webhooks.user_id).
    #[arg(long)]
    user: Uuid,

    /// Microsoft AAD object id (oid claim) to bind to the row.
    #[arg(long)]
    oid: Uuid,

    /// Print the target row without writing.
    #[arg(long)]
    dry_run: bool,

    /// Optional override; defaults to DATABASE_URL_MIGRATIONS / DATABASE_URL.
    #[arg(long)]
    database_url: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    let cli = Cli::parse();

    let database_url = cli
        .database_url
        .or_else(|| std::env::var("DATABASE_URL_MIGRATIONS").ok())
        .or_else(|| std::env::var("DATABASE_URL").ok())
        .ok_or_else(|| {
            anyhow!("DATABASE_URL must be provided via --database-url or env var")
        })?;

    let pool = PgPool::connect(&database_url)
        .await
        .context("Failed to connect to database")?;

    // teams_webhooks has no UNIQUE constraint on (tenant_id, user_id), so we
    // load every matching row under a transaction with FOR UPDATE, refuse to
    // touch anything if there's more than one, and update by primary key
    // before committing. This avoids the failure mode where a duplicate-row
    // history would let an UPDATE on (tenant_id, user_id) silently mutate
    // multiple rows and trip the rows_affected check only after committing.
    let mut tx = pool.begin().await.context("Failed to begin transaction")?;

    let rows = sqlx::query(
        "SELECT id, aad_object_id, is_active
           FROM teams_webhooks
          WHERE tenant_id = $1 AND user_id = $2
          FOR UPDATE",
    )
    .bind(cli.tenant)
    .bind(cli.user)
    .fetch_all(&mut *tx)
    .await
    .context("Failed to look up teams_webhooks row")?;

    if rows.is_empty() {
        tx.rollback().await.ok();
        return Err(anyhow!(
            "No teams_webhooks row for tenant={} user={}. Run configure_teams first.",
            cli.tenant,
            cli.user
        ));
    }
    if rows.len() > 1 {
        tx.rollback().await.ok();
        return Err(anyhow!(
            "Found {} teams_webhooks rows for tenant={} user={}; refusing to update. \
             Resolve the duplicates manually before retrying.",
            rows.len(),
            cli.tenant,
            cli.user
        ));
    }

    let row = &rows[0];
    let row_id: Uuid = row.try_get("id")?;
    let current_oid: Option<String> = row.try_get("aad_object_id")?;
    let is_active: bool = row.try_get("is_active")?;

    println!(
        "Target row {row_id}: is_active={is_active}, current aad_object_id={current_oid:?}"
    );

    if cli.dry_run {
        tx.rollback().await.ok();
        println!("--dry-run; no write performed.");
        pool.close().await;
        return Ok(());
    }

    let result = sqlx::query(
        "UPDATE teams_webhooks
            SET aad_object_id = $2, updated_at = NOW()
          WHERE id = $1",
    )
    .bind(row_id)
    .bind(cli.oid.to_string())
    .execute(&mut *tx)
    .await
    .context("Failed to update teams_webhooks row")?;

    if result.rows_affected() != 1 {
        tx.rollback().await.ok();
        return Err(anyhow!(
            "Expected exactly 1 row update for id={}, got {}",
            row_id,
            result.rows_affected()
        ));
    }

    tx.commit().await.context("Failed to commit update")?;

    println!(
        "Wrote aad_object_id={} for tenant={} user={}",
        cli.oid, cli.tenant, cli.user
    );

    pool.close().await;
    Ok(())
}
