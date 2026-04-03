//! Database migration runner
//!
//! Run with: cargo run --bin migrate

use anyhow::Result;
use clap::{Parser, Subcommand};
use sqlx::PgPool;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "migrate")]
#[command(about = "BillForge database migration runner", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Database URL (defaults to DATABASE_URL env var)
    #[arg(short, long)]
    database_url: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run all pending migrations
    Up,

    /// Rollback last migration
    Down,

    /// Show migration status
    Status,

    /// Create a new migration file
    Create {
        /// Migration name
        name: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let database_url = cli
        .database_url
        .ok_or_else(|| anyhow::anyhow!("DATABASE_URL must be provided or set as env var"))?;

    match cli.command {
        Commands::Up => {
            println!("Running migrations...");
            run_migrations(&database_url).await?;
            println!("Migrations completed successfully!");
        }
        Commands::Down => {
            println!("Rollback not yet implemented");
        }
        Commands::Status => {
            show_status(&database_url).await?;
        }
        Commands::Create { name } => {
            create_migration(&name)?;
        }
    }

    Ok(())
}

async fn run_migrations(database_url: &str) -> Result<()> {
    let pool = PgPool::connect(database_url).await?;

    // TODO: Use sqlx::migrate!() macro once migrations are set up
    // For now, manually run migration files

    println!("Running 001_create_tenants.sql...");
    let migration_001 = include_str!("../../../../migrations/001_create_tenants.sql");
    sqlx::raw_sql(migration_001).execute(&pool).await?;

    println!("Running 002_create_users.sql...");
    let migration_002 = include_str!("../../../../migrations/002_create_users.sql");
    sqlx::raw_sql(migration_002).execute(&pool).await?;

    println!("Running 003_create_vendors.sql...");
    let migration_003 = include_str!("../../../../migrations/003_create_vendors.sql");
    sqlx::raw_sql(migration_003).execute(&pool).await?;

    println!("Running 004_create_invoices.sql...");
    let migration_004 = include_str!("../../../../migrations/004_create_invoices.sql");
    sqlx::raw_sql(migration_004).execute(&pool).await?;

    println!("Running 005_create_workflow_tables.sql...");
    let migration_005 = include_str!("../../../../migrations/005_create_workflow_tables.sql");
    sqlx::raw_sql(migration_005).execute(&pool).await?;

    println!("Running 006_create_quickbooks_tables.sql...");
    let migration_006 = include_str!("../../../../migrations/006_create_quickbooks_tables.sql");
    sqlx::raw_sql(migration_006).execute(&pool).await?;

    println!("Running 007_create_vendor_documents.sql...");
    let migration_007 = include_str!("../../../../migrations/007_create_vendor_documents.sql");
    sqlx::raw_sql(migration_007).execute(&pool).await?;

    println!("Running 008_create_vendor_contacts.sql...");
    let migration_008 = include_str!("../../../../migrations/008_create_vendor_contacts.sql");
    sqlx::raw_sql(migration_008).execute(&pool).await?;

    println!("Running 009_create_email_notifications.sql...");
    let migration_009 = include_str!("../../../../migrations/009_create_email_notifications.sql");
    sqlx::raw_sql(migration_009).execute(&pool).await?;

    println!("All migrations completed successfully!");

    pool.close().await;
    Ok(())
}

async fn show_status(database_url: &str) -> Result<()> {
    let pool = PgPool::connect(database_url).await?;

    // Check if migrations table exists
    let tables: Vec<(String,)> = sqlx::query_as(
        "SELECT tablename FROM pg_tables WHERE schemaname = 'public' ORDER BY tablename",
    )
    .fetch_all(&pool)
    .await?;

    println!("Database tables:");
    for (table,) in tables {
        println!("  - {}", table);
    }

    pool.close().await;
    Ok(())
}

fn create_migration(name: &str) -> Result<()> {
    let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S");
    let filename = format!("{}_{name}.sql", timestamp);
    let path = PathBuf::from("migrations").join(&filename);

    std::fs::write(&path, format!("-- Migration: {}\n\n", name))?;

    println!("Created migration: {}", path.display());
    Ok(())
}
