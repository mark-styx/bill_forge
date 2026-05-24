//! Database migration utilities.

use billforge_core::{Error, Result};
use sqlx::PgPool;
use std::time::Instant;

const TRACKING_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS billforge_schema_migrations (
    migration_name TEXT PRIMARY KEY,
    checksum TEXT NOT NULL,
    execution_time_ms BIGINT NOT NULL DEFAULT 0,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
"#;

/// Database-backed migration version tracking for tenant databases.
#[derive(Debug, Clone, Default)]
pub struct MigrationRunner;

impl MigrationRunner {
    pub fn new() -> Self {
        Self
    }

    /// Ensure the durable migration tracking table exists.
    pub async fn ensure_tracking_table(&self, pool: &PgPool) -> Result<()> {
        sqlx::raw_sql(TRACKING_TABLE_SQL)
            .execute(pool)
            .await
            .map_err(|e| {
                Error::Migration(format!("Failed to create migration tracking table: {}", e))
            })?;

        Ok(())
    }

    /// Check if a migration has been applied in the current database.
    pub async fn is_applied(&self, pool: &PgPool, name: &str) -> Result<bool> {
        self.ensure_tracking_table(pool).await?;

        let applied = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM billforge_schema_migrations WHERE migration_name = $1)",
        )
        .bind(name)
        .fetch_one(pool)
        .await
        .map_err(|e| Error::Migration(format!("Failed to check migration {}: {}", name, e)))?;

        Ok(applied)
    }

    /// Run a migration once and persist its checksum.
    ///
    /// If a migration was already applied with different SQL, this returns an
    /// error instead of silently accepting schema drift.
    pub async fn apply(&self, pool: &PgPool, name: &str, sql: &str) -> Result<()> {
        self.ensure_tracking_table(pool).await?;

        let expected_checksum = checksum(sql);
        let recorded_checksum: Option<String> = sqlx::query_scalar(
            "SELECT checksum FROM billforge_schema_migrations WHERE migration_name = $1",
        )
        .bind(name)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::Migration(format!("Failed to read migration {}: {}", name, e)))?;

        if let Some(recorded_checksum) = recorded_checksum {
            if recorded_checksum != expected_checksum {
                return Err(Error::Migration(format!(
                    "Migration {} was already applied with checksum {}, current checksum {}",
                    name, recorded_checksum, expected_checksum
                )));
            }

            tracing::debug!("Skipping already-applied migration: {}", name);
            return Ok(());
        }

        tracing::debug!("Running migration: {}", name);
        let started_at = Instant::now();
        sqlx::raw_sql(sql)
            .execute(pool)
            .await
            .map_err(|e| Error::Migration(format!("Failed to run migration {}: {}", name, e)))?;

        let execution_time_ms = started_at.elapsed().as_millis().min(i64::MAX as u128) as i64;
        self.mark_applied(pool, name, &expected_checksum, execution_time_ms)
            .await
    }

    async fn mark_applied(
        &self,
        pool: &PgPool,
        name: &str,
        checksum: &str,
        execution_time_ms: i64,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO billforge_schema_migrations (
                migration_name,
                checksum,
                execution_time_ms
            )
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(name)
        .bind(checksum)
        .bind(execution_time_ms)
        .execute(pool)
        .await
        .map_err(|e| Error::Migration(format!("Failed to record migration {}: {}", name, e)))?;

        Ok(())
    }
}

fn checksum(input: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;

    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }

    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_is_stable_for_identical_sql() {
        assert_eq!(checksum("SELECT 1"), checksum("SELECT 1"));
    }

    #[test]
    fn checksum_changes_when_sql_changes() {
        assert_ne!(checksum("SELECT 1"), checksum("SELECT 2"));
    }
}
