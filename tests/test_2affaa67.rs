// src/tests/utils.rs
use sqlx::postgres::{PgPool, PgPoolOptions};

pub async fn init_db() -> PgPool {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect("postgresql://user:password@localhost/db")
        .await
        .unwrap();

    // Create table if not exists
    sqlx::query!(
        r#"
            CREATE TABLE IF NOT EXISTS invoices (
                id SERIAL PRIMARY KEY,
                amount NUMERIC NOT NULL,
                description VARCHAR(255) NOT NULL
            )
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    pool
}