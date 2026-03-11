//! Context injection for AI agent
//! Injects tenant context, user permissions, and relevant data into agent

use anyhow::Result;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::models::AgentContext;

/// Inject context from authenticated user
pub async fn inject_context(
    pool: &PgPool,
    tenant_id: String,
    user_id: Uuid,
) -> Result<AgentContext> {
    // Get user role and permissions
    let user = sqlx::query(
        r#"
        SELECT
            u.id,
            u.email,
            u.role_id,
            r.name as role_name
        FROM users u
        JOIN roles r ON u.role_id = r.id
        WHERE u.id = $1 AND u.tenant_id = $2
        "#,
    )
    .bind(user_id)
    .bind(&tenant_id)
    .fetch_optional(pool)
    .await?;

    let user = user.ok_or_else(|| anyhow::anyhow!("User not found"))?;
    let role_id: Uuid = user.try_get("role_id")?;
    let role_name: String = user.try_get("role_name")?;

    // Get permissions for user's role
    let permissions = sqlx::query(
        r#"
        SELECT p.name
        FROM role_permissions rp
        JOIN permissions p ON rp.permission_id = p.id
        WHERE rp.role_id = $1
        "#,
    )
    .bind(role_id)
    .fetch_all(pool)
    .await?;

    Ok(AgentContext {
        tenant_id,
        user_id,
        user_role: role_name,
        permissions: permissions.iter().filter_map(|row| row.try_get("name").ok()).collect(),
    })
}

/// Build system prompt with context
pub fn build_system_prompt(context: &AgentContext) -> String {
    format!(
        r#"You are Winston, an AI assistant for invoice management. You help users manage invoices, approvals, and vendor relationships.

## Your Capabilities
- Check invoice status and details
- Find invoices by vendor
- Explain approval requirements
- Summarize invoices

## Available Tools
- get_invoice_status: Get status of an invoice by ID
- get_vendor_invoices: Find all invoices from a vendor
- get_approval_requirements: Check who needs to approve an invoice
- summarize_invoice: Generate a summary of an invoice

## Your Context
- Organization: {tenant_id}
- Your role: {role}
- Permissions: {permissions}

## Guidelines
- Always be helpful and professional
- Use tools to get accurate, real-time data
- If you need an invoice ID, ask the user
- If a tool returns an error, explain it clearly to the user
- Never make up data - only use what tools provide
- Cite invoice IDs, vendor names, and amounts when providing information
- Add a disclaimer if unsure about data accuracy

## Response Format
Provide clear, concise answers. Use bullet points when listing multiple items.
Always verify important information with the actual database."#,
        tenant_id = context.tenant_id,
        role = context.user_role,
        permissions = context.permissions.join(", "),
    )
}
