//! Database seeding binary
//!
//! Seeds pilot customer data for testing and demos

use anyhow::Result;
use billforge_db::seed::seed_pilot_customers;
use sqlx::PgPool;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://billforge:billforge_dev@localhost:5432/billforge_control".to_string());

    println!("🔌 Connecting to database...");
    let pool = PgPool::connect(&database_url).await?;

    seed_pilot_customers(&pool).await?;

    println!("\n🎉 Database seeding complete!");
    println!("   Run 'cargo run -p billforge-api' to start the server");

    Ok(())
}
