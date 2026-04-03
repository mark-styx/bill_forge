//! User repository implementation

use async_trait::async_trait;
use billforge_core::traits::UserRepository;
use billforge_core::{Error, Result, TenantId, UserId};
use sqlx::PgPool;
use std::sync::Arc;

pub struct UserRepositoryImpl {
    pool: Arc<PgPool>,
}

impl UserRepositoryImpl {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserRepository for UserRepositoryImpl {
    async fn get_email_by_id(
        &self,
        tenant_id: &TenantId,
        user_id: &UserId,
    ) -> Result<Option<String>> {
        let email: Option<String> =
            sqlx::query_scalar("SELECT email FROM users WHERE tenant_id = $1 AND id = $2")
                .bind(*tenant_id.as_uuid())
                .bind(user_id.as_uuid())
                .fetch_optional(&*self.pool)
                .await
                .map_err(|e| Error::Database(format!("Failed to fetch user email: {}", e)))?;

        Ok(email)
    }

    async fn get_name_by_id(
        &self,
        tenant_id: &TenantId,
        user_id: &UserId,
    ) -> Result<Option<String>> {
        let name: Option<String> =
            sqlx::query_scalar("SELECT name FROM users WHERE tenant_id = $1 AND id = $2")
                .bind(*tenant_id.as_uuid())
                .bind(user_id.as_uuid())
                .fetch_optional(&*self.pool)
                .await
                .map_err(|e| Error::Database(format!("Failed to fetch user name: {}", e)))?;

        Ok(name)
    }

    async fn get_emails_by_ids(
        &self,
        tenant_id: &TenantId,
        user_ids: &[UserId],
    ) -> Result<Vec<String>> {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }

        let ids: Vec<uuid::Uuid> = user_ids.iter().map(|u| *u.as_uuid()).collect();

        let emails: Vec<String> =
            sqlx::query_scalar("SELECT email FROM users WHERE tenant_id = $1 AND id = ANY($2)")
                .bind(*tenant_id.as_uuid())
                .bind(&ids)
                .fetch_all(&*self.pool)
                .await
                .map_err(|e| Error::Database(format!("Failed to fetch user emails: {}", e)))?;

        Ok(emails)
    }

    async fn get_emails_by_role(&self, tenant_id: &TenantId, role: &str) -> Result<Vec<String>> {
        let emails: Vec<String> = sqlx::query_scalar(
            r#"SELECT email FROM users
               WHERE tenant_id = $1
               AND roles::jsonb @> $2::jsonb"#,
        )
        .bind(*tenant_id.as_uuid())
        .bind(
            serde_json::to_string(&serde_json::json!([role]))
                .map_err(|e| Error::Internal(format!("Failed to serialize role: {}", e)))?,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| Error::Database(format!("Failed to fetch user emails by role: {}", e)))?;

        Ok(emails)
    }
}
