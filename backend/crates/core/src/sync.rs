//! ERP sync orchestration primitives
//!
//! Shared types for change detection, delta tracking, conflict resolution,
//! and sync state management that any ERP connector can adopt.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

type SyncStateRow = (chrono::DateTime<Utc>, Option<String>, Option<String>, i32);

// ---------------------------------------------------------------------------
// Sync direction
// ---------------------------------------------------------------------------

/// Direction of a sync operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncDirection {
    PullOnly,
    PushOnly,
    Bidirectional,
}

// ---------------------------------------------------------------------------
// Sync cursor & state
// ---------------------------------------------------------------------------

/// Opaque cursor that is persisted between sync runs so the connector can
/// resume where it left off and avoid full pulls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncCursor {
    pub updated_after: DateTime<Utc>,
    pub page_token: Option<String>,
}

/// Per-(tenant, connector, entity) sync state stored in `erp_sync_state`.
#[derive(Debug, Clone)]
pub struct SyncState {
    pub tenant_id: Uuid,
    pub connector: String,
    pub entity_type: String,
    pub cursor: SyncCursor,
    pub last_sync_at: DateTime<Utc>,
    pub last_remote_version: Option<String>,
    pub conflict_count: i32,
}

impl SyncState {
    /// Load sync state from the database. Returns a default (epoch-based)
    /// state when no row exists yet.
    pub async fn load(
        pool: &PgPool,
        tenant_id: &Uuid,
        connector: &str,
        entity_type: &str,
    ) -> sqlx::Result<Self> {
        let row: Option<SyncStateRow> = sqlx::query_as(
            "SELECT last_sync_at, last_remote_version, cursor->>'page_token', conflict_count \
             FROM erp_sync_state \
             WHERE tenant_id = $1 AND connector = $2 AND entity_type = $3",
        )
        .bind(tenant_id)
        .bind(connector)
        .bind(entity_type)
        .fetch_optional(pool)
        .await?;

        Ok(match row {
            Some((last_sync_at, last_remote_version, page_token, conflict_count)) => Self {
                tenant_id: *tenant_id,
                connector: connector.to_string(),
                entity_type: entity_type.to_string(),
                cursor: SyncCursor {
                    updated_after: last_sync_at,
                    page_token,
                },
                last_sync_at,
                last_remote_version,
                conflict_count,
            },
            None => Self {
                tenant_id: *tenant_id,
                connector: connector.to_string(),
                entity_type: entity_type.to_string(),
                cursor: SyncCursor {
                    updated_after: DateTime::UNIX_EPOCH,
                    page_token: None,
                },
                last_sync_at: DateTime::UNIX_EPOCH,
                last_remote_version: None,
                conflict_count: 0,
            },
        })
    }

    /// Persist (upsert) the current sync state.
    pub async fn save(&self, pool: &PgPool) -> sqlx::Result<()> {
        let cursor_json = serde_json::json!({
            "updated_after": self.cursor.updated_after,
            "page_token": self.cursor.page_token,
        });

        sqlx::query(
            "INSERT INTO erp_sync_state (tenant_id, connector, entity_type, cursor, last_sync_at, last_remote_version, conflict_count) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             ON CONFLICT (tenant_id, connector, entity_type) \
             DO UPDATE SET cursor = $4, last_sync_at = $5, last_remote_version = $6, conflict_count = $7",
        )
        .bind(self.tenant_id)
        .bind(&self.connector)
        .bind(&self.entity_type)
        .bind(&cursor_json)
        .bind(self.last_sync_at)
        .bind(&self.last_remote_version)
        .bind(self.conflict_count)
        .execute(pool)
        .await?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Conflict resolution
// ---------------------------------------------------------------------------

/// Strategy for resolving sync conflicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    LastWriteWins,
    LocalWins,
    RemoteWins,
    ManualReview,
}

/// Reason a conflict was detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictSide {
    LocalOnly,
    RemoteOnly,
    BothModified,
}

/// Result of conflict resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedSide {
    Local,
    Remote,
    Unresolved,
}

/// Trait for resolving sync conflicts. Implementors choose a winner based on
/// the timestamps of the local and remote versions.
pub trait ConflictResolver: Send + Sync {
    fn resolve(
        &self,
        local_updated_at: DateTime<Utc>,
        remote_updated_at: DateTime<Utc>,
    ) -> ResolvedSide;
}

/// Default resolver: whichever side was updated more recently wins.
pub struct LastWriteWinsResolver;

impl ConflictResolver for LastWriteWinsResolver {
    fn resolve(
        &self,
        local_updated_at: DateTime<Utc>,
        remote_updated_at: DateTime<Utc>,
    ) -> ResolvedSide {
        if remote_updated_at > local_updated_at {
            ResolvedSide::Remote
        } else {
            ResolvedSide::Local
        }
    }
}

// ---------------------------------------------------------------------------
// Change detection
// ---------------------------------------------------------------------------

/// Compare two hashable/equatable values and report whether they diverged.
pub fn detect_change<T: std::hash::Hash + PartialEq>(local: &T, remote: &T) -> bool {
    local != remote
}

// ---------------------------------------------------------------------------
// Conflict logging
// ---------------------------------------------------------------------------

/// Write a conflict row to `erp_sync_conflicts`.
pub async fn log_conflict(
    pool: &PgPool,
    tenant_id: &Uuid,
    connector: &str,
    entity_type: &str,
    local_id: &str,
    remote_id: &str,
    reason: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO erp_sync_conflicts (id, tenant_id, connector, entity_type, local_id, remote_id, reason) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind(connector)
    .bind(entity_type)
    .bind(local_id)
    .bind(remote_id)
    .bind(reason)
    .execute(pool)
    .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests (unit, no DB)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc as ChronoUtc};

    #[test]
    fn last_write_wins_picks_newer_timestamp() {
        let resolver = LastWriteWinsResolver;

        let older = ChronoUtc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
        let newer = ChronoUtc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap();

        // Remote is newer
        assert_eq!(resolver.resolve(older, newer), ResolvedSide::Remote);
        // Local is newer
        assert_eq!(resolver.resolve(newer, older), ResolvedSide::Local);
        // Equal timestamps -> local wins (>=)
        assert_eq!(resolver.resolve(older, older), ResolvedSide::Local);
    }

    #[test]
    fn detect_change_returns_false_for_equal_values() {
        let a = ("vendor-name", "email@test.com");
        let b = ("vendor-name", "email@test.com");
        assert!(!detect_change(&a, &b));
    }

    #[test]
    fn detect_change_returns_true_for_diverged() {
        let a = ("vendor-name", "old@test.com");
        let b = ("vendor-name", "new@test.com");
        assert!(detect_change(&a, &b));
    }
}
