//! Dashboard metrics repository implementation

use billforge_core::types::TenantId;
use chrono::{Datelike, Duration, Utc};
use sqlx::PgPool;
use std::sync::Arc;

pub struct MetricsRepositoryImpl {
    pool: Arc<PgPool>,
}

impl MetricsRepositoryImpl {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    /// Get invoice processing metrics for a tenant
    pub async fn get_invoice_metrics(
        &self,
        tenant_id: &TenantId,
    ) -> Result<InvoiceMetrics, sqlx::Error> {
        let now = Utc::now();
        let start_of_month = now.date_naive().with_day(1).unwrap();
        let start_of_last_month = (start_of_month - Duration::days(30)).with_day(1).unwrap();

        // Total invoices and counts by status
        let stats: InvoiceStats = sqlx::query_as(
            r#"
            SELECT
                COUNT(*) as total_invoices,
                COUNT(*) FILTER (WHERE capture_status = 'pending') as pending_ocr,
                COUNT(*) FILTER (WHERE processing_status = 'draft') as ready_for_review,
                COUNT(*) FILTER (WHERE processing_status = 'submitted') as submitted,
                COUNT(*) FILTER (WHERE processing_status = 'approved') as approved,
                COUNT(*) FILTER (WHERE processing_status = 'rejected') as rejected,
                COUNT(*) FILTER (WHERE processing_status = 'paid') as paid,
                COALESCE(SUM(total_amount_cents)::BIGINT, 0) as total_value,
                COUNT(*) FILTER (WHERE created_at >= $2) as this_month,
                COUNT(*) FILTER (WHERE created_at >= $3 AND created_at < $2) as last_month
            FROM invoices
            WHERE tenant_id = $1
        "#,
        )
        .bind(*tenant_id.as_uuid())
        .bind(start_of_month)
        .bind(start_of_last_month)
        .fetch_one(&*self.pool)
        .await?;

        // Calculate average processing time
        let avg_time: Option<f64> = sqlx::query_scalar(
            r#"
            SELECT AVG(EXTRACT(EPOCH FROM (updated_at - created_at)) / 3600)
            FROM invoices
            WHERE tenant_id = $1
            AND processing_status IN ('approved', 'paid')
            AND updated_at IS NOT NULL
        "#,
        )
        .bind(*tenant_id.as_uuid())
        .fetch_one(&*self.pool)
        .await?;

        // Calculate trend vs last month
        let trend = if stats.last_month > 0 {
            ((stats.this_month as f64 - stats.last_month as f64) / stats.last_month as f64) * 100.0
        } else {
            0.0
        };

        Ok(InvoiceMetrics {
            total_invoices: stats.total_invoices,
            pending_ocr: stats.pending_ocr,
            ready_for_review: stats.ready_for_review,
            submitted: stats.submitted,
            approved: stats.approved,
            rejected: stats.rejected,
            paid: stats.paid,
            avg_processing_time_hours: avg_time.unwrap_or(0.0),
            total_value: stats.total_value,
            this_month: stats.this_month,
            trend_vs_last_month: trend,
        })
    }

    /// Get approval workflow metrics for a tenant
    pub async fn get_approval_metrics(
        &self,
        tenant_id: &TenantId,
    ) -> Result<ApprovalMetrics, sqlx::Error> {
        let now = Utc::now();
        let start_of_today = now.date_naive();

        // Approval counts
        let stats: ApprovalStats = sqlx::query_as(r#"
            SELECT
                COUNT(*) FILTER (WHERE status = 'pending') as pending_approvals,
                COUNT(*) FILTER (WHERE status = 'approved' AND DATE(responded_at) = $2) as approved_today,
                COUNT(*) FILTER (WHERE status = 'rejected' AND DATE(responded_at) = $2) as rejected_today,
                COUNT(*) FILTER (WHERE status = 'approved') as total_approved,
                COUNT(*) FILTER (WHERE status = 'rejected') as total_rejected
            FROM approval_requests
            WHERE tenant_id = $1
        "#)
        .bind(*tenant_id.as_uuid())
        .bind(start_of_today)
        .fetch_one(&*self.pool)
        .await?;

        // Average approval time in hours
        let avg_time: Option<f64> = sqlx::query_scalar(
            r#"
            SELECT AVG(EXTRACT(EPOCH FROM (responded_at - created_at)) / 3600)
            FROM approval_requests
            WHERE tenant_id = $1
            AND status IN ('approved', 'rejected')
            AND responded_at IS NOT NULL
        "#,
        )
        .bind(*tenant_id.as_uuid())
        .fetch_one(&*self.pool)
        .await?;

        // Approval rate
        let total_decided = stats.total_approved + stats.total_rejected;
        let approval_rate = if total_decided > 0 {
            (stats.total_approved as f64 / total_decided as f64) * 100.0
        } else {
            0.0
        };

        // Escalated and overdue counts
        let escalated: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM approval_requests
            WHERE tenant_id = $1
            AND status = 'pending'
            AND created_at < NOW() - INTERVAL '48 hours'
        "#,
        )
        .bind(*tenant_id.as_uuid())
        .fetch_one(&*self.pool)
        .await?;

        let overdue: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM approval_requests
            WHERE tenant_id = $1
            AND status = 'pending'
            AND expires_at < NOW()
        "#,
        )
        .bind(*tenant_id.as_uuid())
        .fetch_one(&*self.pool)
        .await?;

        Ok(ApprovalMetrics {
            pending_approvals: stats.pending_approvals,
            approved_today: stats.approved_today,
            rejected_today: stats.rejected_today,
            avg_approval_time_hours: avg_time.unwrap_or(0.0),
            approval_rate,
            escalated,
            overdue,
        })
    }

    /// Get vendor analytics for a tenant
    pub async fn get_vendor_metrics(
        &self,
        tenant_id: &TenantId,
    ) -> Result<VendorMetrics, sqlx::Error> {
        let start_of_month = Utc::now().date_naive().with_day(1).unwrap();

        // Total vendors and new this month
        let vendor_stats: VendorStats = sqlx::query_as(
            r#"
            SELECT
                COUNT(*) as total_vendors,
                COUNT(*) FILTER (WHERE created_at >= $2) as new_this_month
            FROM vendors
            WHERE tenant_id = $1
            AND is_active = true
        "#,
        )
        .bind(*tenant_id.as_uuid())
        .bind(start_of_month)
        .fetch_one(&*self.pool)
        .await?;

        // Top vendors by invoice count
        let top_vendors: Vec<TopVendorData> = sqlx::query_as(
            r#"
            SELECT
                v.id as vendor_id,
                v.name as vendor_name,
                COUNT(i.id) as invoice_count,
                COALESCE(SUM(i.total_amount_cents)::BIGINT, 0) as total_amount
            FROM vendors v
            LEFT JOIN invoices i ON v.id = i.vendor_id AND i.tenant_id = $1
            WHERE v.tenant_id = $1
            AND v.is_active = true
            GROUP BY v.id, v.name
            ORDER BY invoice_count DESC
            LIMIT 5
        "#,
        )
        .bind(*tenant_id.as_uuid())
        .fetch_all(&*self.pool)
        .await?;

        // Vendor concentration (top 10% by spend)
        let concentration: Option<f64> = sqlx::query_scalar(
            r#"
            WITH vendor_spend AS (
                SELECT v.id, SUM(i.total_amount_cents) as total_spend
                FROM vendors v
                LEFT JOIN invoices i ON v.id = i.vendor_id AND i.tenant_id = $1
                WHERE v.tenant_id = $1
                GROUP BY v.id
                ORDER BY total_spend DESC NULLS LAST
            ),
            total AS (
                SELECT SUM(total_spend) as total FROM vendor_spend
            ),
            top_vendors AS (
                SELECT SUM(total_spend) as top_spend
                FROM (
                    SELECT total_spend
                    FROM vendor_spend
                    ORDER BY total_spend DESC
                    LIMIT CEIL((SELECT COUNT(*)::float FROM vendor_spend) * 0.1)
                ) sub
            )
            SELECT
                CASE
                    WHEN (SELECT total FROM total) = 0 OR (SELECT total FROM total) IS NULL THEN 0
                    ELSE (SELECT top_spend FROM top_vendors) / (SELECT total FROM total) * 100
                END
        "#,
        )
        .bind(*tenant_id.as_uuid())
        .fetch_one(&*self.pool)
        .await?;

        let concentration = concentration.unwrap_or(0.0);

        Ok(VendorMetrics {
            total_vendors: vendor_stats.total_vendors,
            new_this_month: vendor_stats.new_this_month,
            top_vendors: top_vendors
                .into_iter()
                .map(|v| TopVendor {
                    vendor_id: v.vendor_id.to_string(),
                    vendor_name: v.vendor_name,
                    invoice_count: v.invoice_count,
                    total_amount: v.total_amount,
                })
                .collect(),
            concentration_percentage: concentration,
        })
    }

    /// Get team performance metrics for a tenant
    pub async fn get_team_metrics(&self, tenant_id: &TenantId) -> Result<TeamMetrics, sqlx::Error> {
        let start_of_month = Utc::now().date_naive().with_day(1).unwrap();

        // Team member stats
        let member_stats: Vec<TeamMemberData> = sqlx::query_as(r#"
            SELECT
                u.id as user_id,
                u.name as user_name,
                COUNT(*) FILTER (WHERE ar.status = 'approved' AND ar.responded_at >= $2) as approvals_this_month,
                COUNT(*) FILTER (WHERE ar.status = 'rejected' AND ar.responded_at >= $2) as rejections_this_month
            FROM users u
            LEFT JOIN approval_requests ar ON ar.responded_by = u.id AND ar.tenant_id = $1
            WHERE u.tenant_id = $1
            GROUP BY u.id, u.name
            ORDER BY approvals_this_month DESC
        "#)
        .bind(*tenant_id.as_uuid())
        .bind(start_of_month)
        .fetch_all(&*self.pool)
        .await?;

        // Calculate average response time per user
        let mut members = Vec::new();
        for mut member in member_stats {
            let avg_time: Option<f64> = sqlx::query_scalar(
                r#"
                SELECT AVG(EXTRACT(EPOCH FROM (responded_at - created_at)) / 3600)
                FROM approval_requests
                WHERE tenant_id = $1
                AND responded_by = $2
                AND status IN ('approved', 'rejected')
                AND responded_at IS NOT NULL
            "#,
            )
            .bind(*tenant_id.as_uuid())
            .bind(member.user_id)
            .fetch_one(&*self.pool)
            .await?;

            member.avg_response_time_hours = avg_time.unwrap_or(0.0);
            members.push(TeamMemberStats {
                user_id: member.user_id.to_string(),
                user_name: member.user_name,
                approvals_this_month: member.approvals_this_month,
                rejections_this_month: member.rejections_this_month,
                avg_response_time_hours: member.avg_response_time_hours,
            });
        }

        // Calculate averages
        let total_approvals: i64 = members.iter().map(|m| m.approvals_this_month).sum();
        let avg_approvals = if !members.is_empty() {
            total_approvals as f64 / members.len() as f64
        } else {
            0.0
        };

        // Total pending actions
        let total_pending: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM approval_requests
            WHERE tenant_id = $1
            AND status = 'pending'
        "#,
        )
        .bind(*tenant_id.as_uuid())
        .fetch_one(&*self.pool)
        .await?;

        Ok(TeamMetrics {
            members,
            avg_approvals_per_member: avg_approvals,
            total_pending_actions: total_pending,
        })
    }
}

// Data structures for metrics

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InvoiceMetrics {
    pub total_invoices: i64,
    pub pending_ocr: i64,
    pub ready_for_review: i64,
    pub submitted: i64,
    pub approved: i64,
    pub rejected: i64,
    pub paid: i64,
    pub avg_processing_time_hours: f64,
    pub total_value: i64,
    pub this_month: i64,
    pub trend_vs_last_month: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApprovalMetrics {
    pub pending_approvals: i64,
    pub approved_today: i64,
    pub rejected_today: i64,
    pub avg_approval_time_hours: f64,
    pub approval_rate: f64,
    pub escalated: i64,
    pub overdue: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VendorMetrics {
    pub total_vendors: i64,
    pub new_this_month: i64,
    pub top_vendors: Vec<TopVendor>,
    pub concentration_percentage: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TopVendor {
    pub vendor_id: String,
    pub vendor_name: String,
    pub invoice_count: i64,
    pub total_amount: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TeamMetrics {
    pub members: Vec<TeamMemberStats>,
    pub avg_approvals_per_member: f64,
    pub total_pending_actions: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TeamMemberStats {
    pub user_id: String,
    pub user_name: String,
    pub approvals_this_month: i64,
    pub rejections_this_month: i64,
    pub avg_response_time_hours: f64,
}

// Internal query result structs

#[derive(sqlx::FromRow)]
struct InvoiceStats {
    total_invoices: i64,
    pending_ocr: i64,
    ready_for_review: i64,
    submitted: i64,
    approved: i64,
    rejected: i64,
    paid: i64,
    total_value: i64,
    this_month: i64,
    last_month: i64,
}

#[derive(sqlx::FromRow)]
struct ApprovalStats {
    pending_approvals: i64,
    approved_today: i64,
    rejected_today: i64,
    total_approved: i64,
    total_rejected: i64,
}

#[derive(sqlx::FromRow)]
struct VendorStats {
    total_vendors: i64,
    new_this_month: i64,
}

#[derive(sqlx::FromRow)]
struct TopVendorData {
    vendor_id: uuid::Uuid,
    vendor_name: String,
    invoice_count: i64,
    total_amount: i64,
}

#[derive(sqlx::FromRow)]
struct TeamMemberData {
    user_id: uuid::Uuid,
    user_name: String,
    approvals_this_month: i64,
    rejections_this_month: i64,
    avg_response_time_hours: f64,
}
