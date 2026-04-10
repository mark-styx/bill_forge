//! Database migration runner
//!
//! Run with: cargo run --bin migrate

use anyhow::Result;
use clap::{Parser, Subcommand};
use sqlx::migrate::Migrator;
use sqlx::PgPool;
use std::path::{Path, PathBuf};

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

    /// Mark all migrations as applied without running them (for existing databases)
    Baseline,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let database_url = cli.database_url
        .or_else(|| std::env::var("DATABASE_URL").ok())
        .ok_or_else(|| anyhow::anyhow!("DATABASE_URL must be provided via --database-url or env var"))?;

    match cli.command {
        Commands::Up => {
            println!("Running migrations...");
            let pool = PgPool::connect(&database_url).await?;
            sqlx::migrate!("../../migrations").run(&pool).await?;
            pool.close().await;
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
        Commands::Baseline => {
            run_baseline(&database_url).await?;
        }
    }

    Ok(())
}

async fn show_status(database_url: &str) -> Result<()> {
    let pool = PgPool::connect(database_url).await?;

    let result: Result<Vec<(i64, String, bool, chrono::DateTime<chrono::Utc>)>, sqlx::Error> =
        sqlx::query_as(
            "SELECT version, description, success, installed_on FROM _sqlx_migrations ORDER BY version",
        )
        .fetch_all(&pool)
        .await;

    match result {
        Ok(rows) => {
            if rows.is_empty() {
                println!("No migrations applied yet.");
            } else {
                println!("{:<10} {:<45} {:<10} {}", "Version", "Description", "Success", "Installed On");
                for (version, description, success, installed_on) in rows {
                    println!("{:<10} {:<45} {:<10} {}", version, description, success, installed_on);
                }
            }
        }
        Err(e) => {
            if e.to_string().contains("42P01") || e.to_string().contains("does not exist") {
                println!("No migrations applied yet (_sqlx_migrations table does not exist).");
            } else {
                return Err(e.into());
            }
        }
    }

    pool.close().await;
    Ok(())
}

async fn run_baseline(database_url: &str) -> Result<()> {
    let pool = PgPool::connect(database_url).await?;

    // Create the _sqlx_migrations table matching sqlx's expected schema
    sqlx::raw_sql(
        "CREATE TABLE IF NOT EXISTS _sqlx_migrations (
            version BIGINT PRIMARY KEY,
            description TEXT NOT NULL,
            installed_on TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            success BOOLEAN NOT NULL,
            checksum BYTEA NOT NULL,
            execution_time BIGINT NOT NULL
        )",
    )
    .execute(&pool)
    .await?;

    // Load migrations from the migrations directory
    let migrator = Migrator::new(Path::new("./backend/migrations")).await?;

    for migration in migrator.iter() {
        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, installed_on, success, checksum, execution_time)
             VALUES ($1, $2, NOW(), true, $3, 0)
             ON CONFLICT (version) DO NOTHING",
        )
        .bind(migration.version)
        .bind(migration.description.as_ref())
        .bind(migration.checksum.as_ref())
        .execute(&pool)
        .await?;

        println!("Baselined migration {}: {}", migration.version, migration.description);
    }

    pool.close().await;
    println!("Baseline complete. All migrations marked as applied.");
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
