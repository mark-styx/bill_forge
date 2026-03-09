//! Database Migration Rollback Tool
//!
//! Provides safe, automated rollback of database migrations with backup and verification.
//!
//! Usage:
//!   cargo run -p billforge-db --bin rollback -- --to-version 045
//!   cargo run -p billforge-db --bin rollback -- --dry-run --to-version 045
//!   cargo run -p billforge-db --bin rollback -- --list

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::Parser;
use sqlx::PgPool;
use std::path::PathBuf;
use std::process::Command;
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(name = "rollback")]
#[command(about = "Rollback database migrations safely")]
struct Args {
    /// Target migration version to rollback to
    #[arg(short, long)]
    to_version: Option<i32>,

    /// Number of migrations to rollback
    #[arg(short = 'n', long)]
    steps: Option<u32>,

    /// Perform a dry run without executing rollback
    #[arg(short, long)]
    dry_run: bool,

    /// List available migrations and their status
    #[arg(short, long)]
    list: bool,

    /// Skip backup creation (dangerous!)
    #[arg(long)]
    skip_backup: bool,

    /// Tenant ID for tenant-specific rollback
    #[arg(short, long)]
    tenant: Option<String>,

    /// Force rollback without confirmation
    #[arg(short, long)]
    force: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct MigrationRecord {
    version: i32,
    name: String,
    applied_at: DateTime<Utc>,
}

#[derive(Debug)]
struct MigrationFile {
    version: i32,
    name: String,
    up_path: PathBuf,
    down_path: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging (simple env-filter based)
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    // Load database URL
    let database_url = std::env::var("DATABASE_URL")
        .context("DATABASE_URL must be set")?;

    // Connect to database
    info!("Connecting to database...");
    let pool = PgPool::connect(&database_url)
        .await
        .context("Failed to connect to database")?;

    if args.list {
        list_migrations(&pool).await?;
        return Ok(());
    }

    // Determine target version
    let target_version = match (args.to_version, args.steps) {
        (Some(v), None) => v,
        (None, Some(steps)) => {
            let current = get_current_version(&pool).await?;
            current - steps as i32
        }
        (None, None) => {
            anyhow::bail!("Must specify either --to-version or --steps");
        }
        (Some(_), Some(_)) => {
            anyhow::bail!("Cannot specify both --to-version and --steps");
        }
    };

    // Get current version
    let current_version = get_current_version(&pool).await?;

    if target_version >= current_version {
        anyhow::bail!(
            "Target version {} must be less than current version {}",
            target_version,
            current_version
        );
    }

    info!(
        "Planning rollback from version {} to {}",
        current_version, target_version
    );

    // Load migration files
    let migrations = load_migration_files()?;

    // Plan rollback
    let rollback_plan = plan_rollback(&pool, &migrations, target_version).await?;

    if rollback_plan.is_empty() {
        info!("No migrations to rollback");
        return Ok(());
    }

    // Display rollback plan
    println!("\nRollback Plan:");
    println!("  Current version: {}", current_version);
    println!("  Target version:  {}", target_version);
    println!("  Migrations to rollback: {}", rollback_plan.len());
    println!("\n  {}", "↓".repeat(60));

    for migration in &rollback_plan {
        println!("  - v{}: {}", migration.version, migration.name);
    }

    println!("  {}\n", "↓".repeat(60));

    if args.dry_run {
        info!("Dry run mode - no changes will be made");
        return Ok(());
    }

    // Confirmation (unless --force)
    if !args.force {
        println!("This will rollback {} migration(s).", rollback_plan.len());
        println!("A backup will be created before proceeding.");
        print!("Continue? [y/N]: ");

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        if !input.trim().to_lowercase().starts_with('y') {
            info!("Rollback cancelled");
            return Ok(());
        }
    }

    // Create backup (unless --skip-backup)
    let backup_id = if !args.skip_backup {
        info!("Creating database backup...");
        let backup_id = create_backup(&args.tenant).await?;
        info!("Backup created: {}", backup_id);
        Some(backup_id)
    } else {
        warn!("Skipping backup (--skip-backup) - this is dangerous!");
        None
    };

    // Execute rollback
    execute_rollback(&pool, &rollback_plan, args.tenant.as_deref()).await?;

    info!("Rollback completed successfully");

    if let Some(backup_id) = backup_id {
        info!("Backup ID: {}", backup_id);
        info!("To restore from backup, run: ./scripts/restore-backup.sh {}", backup_id);
    }

    Ok(())
}

async fn list_migrations(pool: &PgPool) -> Result<()> {
    // Ensure migration table exists
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS _migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create migrations table")?;

    // Get applied migrations
    let applied: Vec<MigrationRecord> = sqlx::query_as(
        "SELECT version, name, applied_at FROM _migrations ORDER BY version DESC",
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch applied migrations")?;

    // Load all migration files
    let all_migrations = load_migration_files()?;

    println!("\nMigration Status:");
    println!("  {:<10} {:<40} {:<20} {}", "Version", "Name", "Status", "Applied At");
    println!("  {}", "-".repeat(90));

    for migration in all_migrations {
        let applied_record = applied.iter().find(|m| m.version == migration.version);

        let (status, applied_at) = match applied_record {
            Some(record) => ("APPLIED", record.applied_at.format("%Y-%m-%d %H:%M:%S").to_string()),
            None => ("PENDING", "-".to_string()),
        };

        let status_display = if status == "APPLIED" {
            "\x1b[32mAPPLIED\x1b[0m"
        } else {
            "\x1b[33mPENDING\x1b[0m"
        };

        println!(
            "  {:<10} {:<40} {:<20} {}",
            migration.version,
            migration.name,
            status_display,
            applied_at
        );
    }

    println!();

    Ok(())
}

async fn get_current_version(pool: &PgPool) -> Result<i32> {
    // Ensure migration table exists
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS _migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await
    .context("Failed to create migrations table")?;

    let result: Option<(i32,)> = sqlx::query_as(
        "SELECT MAX(version) FROM _migrations",
    )
    .fetch_optional(pool)
    .await
    .context("Failed to get current version")?;

    Ok(result.map(|(v,)| v).unwrap_or(0))
}

fn load_migration_files() -> Result<Vec<MigrationFile>> {
    let migrations_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../migrations");

    if !migrations_dir.exists() {
        anyhow::bail!("Migrations directory not found: {:?}", migrations_dir);
    }

    let mut migrations = Vec::new();

    for entry in std::fs::read_dir(&migrations_dir)? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let dir_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .context("Invalid directory name")?;

        // Parse version from directory name (e.g., "001_create_tenants")
        let parts: Vec<&str> = dir_name.splitn(2, '_').collect();
        if parts.len() != 2 {
            warn!("Skipping invalid migration directory: {}", dir_name);
            continue;
        }

        let version: i32 = parts[0].parse().context("Invalid migration version")?;
        let name = parts[1].to_string();

        let up_path = path.join("up.sql");
        let down_path = path.join("down.sql");

        if !up_path.exists() {
            warn!("Missing up.sql for migration {}", dir_name);
            continue;
        }

        if !down_path.exists() {
            warn!("Missing down.sql for migration {} - cannot rollback", dir_name);
            continue;
        }

        migrations.push(MigrationFile {
            version,
            name,
            up_path,
            down_path,
        });
    }

    // Sort by version
    migrations.sort_by_key(|m| m.version);

    Ok(migrations)
}

async fn plan_rollback(
    pool: &PgPool,
    migrations: &[MigrationFile],
    target_version: i32,
) -> Result<Vec<MigrationFile>> {
    // Get applied migrations
    let applied: Vec<MigrationRecord> = sqlx::query_as(
        "SELECT version, name, applied_at FROM _migrations WHERE version > $1 ORDER BY version DESC",
    )
    .bind(target_version)
    .fetch_all(pool)
    .await
    .context("Failed to fetch applied migrations")?;

    // Match with migration files
    let mut rollback_plan: Vec<MigrationFile> = Vec::new();

    for applied_migration in applied {
        if let Some(migration_file) = migrations.iter().find(|m| m.version == applied_migration.version) {
            rollback_plan.push((*migration_file).clone());
        } else {
            anyhow::bail!(
                "Migration file not found for version {} ({})",
                applied_migration.version,
                applied_migration.name
            );
        }
    }

    Ok(rollback_plan)
}

async fn create_backup(tenant_id: &Option<String>) -> Result<String> {
    let backup_id = Uuid::new_v4().to_string();
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S");

    let backup_script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../scripts/backup.sh");

    if !backup_script.exists() {
        warn!("Backup script not found at {:?}, skipping backup", backup_script);
        return Ok(backup_id);
    }

    let mut cmd = Command::new(&backup_script);
    cmd.arg("--backup-id").arg(&backup_id);

    if let Some(tenant) = tenant_id {
        cmd.arg("--tenant").arg(tenant);
    }

    let status = cmd
        .status()
        .context("Failed to execute backup script")?;

    if !status.success() {
        anyhow::bail!("Backup script failed with status: {}", status);
    }

    Ok(backup_id)
}

async fn execute_rollback(
    pool: &PgPool,
    rollback_plan: &[MigrationFile],
    tenant_id: Option<&str>,
) -> Result<()> {
    let mut tx = pool.begin().await?;

    for migration in rollback_plan {
        info!("Rolling back migration v{}: {}", migration.version, migration.name);

        // Read down.sql
        let down_sql = std::fs::read_to_string(&migration.down_path)
            .context(format!("Failed to read {:?}", migration.down_path))?;

        // Execute down migration
        sqlx::query(&down_sql)
            .execute(&mut *tx)
            .await
            .context(format!(
                "Failed to execute rollback for migration v{}",
                migration.version
            ))?;

        // Remove migration record
        sqlx::query("DELETE FROM _migrations WHERE version = $1")
            .bind(migration.version)
            .execute(&mut *tx)
            .await
            .context("Failed to remove migration record")?;

        info!("Successfully rolled back v{}", migration.version);
    }

    // Commit transaction
    tx.commit().await?;

    Ok(())
}
