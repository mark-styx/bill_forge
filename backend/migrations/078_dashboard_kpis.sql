-- 078: Dashboard KPIs materialized view for sub-second reads.
--
-- Aggregates per-tenant invoice metrics from the invoices table.
-- Refresh strategy: caller responsibility (pg_cron, worker job, or on-demand).
-- Use `REFRESH MATERIALIZED VIEW CONCURRENTLY dashboard_kpis_mv` to refresh
-- without locking reads.

CREATE MATERIALIZED VIEW IF NOT EXISTS dashboard_kpis_mv AS
SELECT
    i.tenant_id,

    -- Status counts
    COUNT(*) FILTER (WHERE i.status IN ('received', 'in_review', 'pending_approval'))
        AS queue_count,
    COUNT(*) FILTER (WHERE i.status = 'approved')
        AS approved_count,
    COUNT(*) FILTER (WHERE i.status = 'paid')
        AS paid_count,
    COUNT(*) FILTER (WHERE i.status = 'rejected')
        AS rejected_count,

    -- Aging buckets for queued invoices (received / in_review / pending_approval)
    COUNT(*) FILTER (
        WHERE i.status IN ('received', 'in_review', 'pending_approval')
          AND i.created_at >= NOW() - INTERVAL '7 days'
    ) AS aging_0_7,
    COALESCE(SUM(i.total_amount_cents) FILTER (
        WHERE i.status IN ('received', 'in_review', 'pending_approval')
          AND i.created_at >= NOW() - INTERVAL '7 days'
    ), 0) AS aging_0_7_amount,

    COUNT(*) FILTER (
        WHERE i.status IN ('received', 'in_review', 'pending_approval')
          AND i.created_at >= NOW() - INTERVAL '14 days'
          AND i.created_at <  NOW() - INTERVAL '7 days'
    ) AS aging_8_14,
    COALESCE(SUM(i.total_amount_cents) FILTER (
        WHERE i.status IN ('received', 'in_review', 'pending_approval')
          AND i.created_at >= NOW() - INTERVAL '14 days'
          AND i.created_at <  NOW() - INTERVAL '7 days'
    ), 0) AS aging_8_14_amount,

    COUNT(*) FILTER (
        WHERE i.status IN ('received', 'in_review', 'pending_approval')
          AND i.created_at >= NOW() - INTERVAL '30 days'
          AND i.created_at <  NOW() - INTERVAL '14 days'
    ) AS aging_15_30,
    COALESCE(SUM(i.total_amount_cents) FILTER (
        WHERE i.status IN ('received', 'in_review', 'pending_approval')
          AND i.created_at >= NOW() - INTERVAL '30 days'
          AND i.created_at <  NOW() - INTERVAL '14 days'
    ), 0) AS aging_15_30_amount,

    COUNT(*) FILTER (
        WHERE i.status IN ('received', 'in_review', 'pending_approval')
          AND i.created_at < NOW() - INTERVAL '30 days'
    ) AS aging_30_plus,
    COALESCE(SUM(i.total_amount_cents) FILTER (
        WHERE i.status IN ('received', 'in_review', 'pending_approval')
          AND i.created_at < NOW() - INTERVAL '30 days'
    ), 0) AS aging_30_plus_amount,

    -- Top 10 vendors by spend (paid invoices, last 30 days) as JSONB
    (
        SELECT COALESCE(jsonb_agg(jsonb_build_object(
            'vendor_id', sub.vendor_id,
            'vendor_name', sub.vendor_name,
            'total_amount', sub.total_amount,
            'invoice_count', sub.invoice_count
        )), '[]'::jsonb)
        FROM (
            SELECT
                i2.vendor_id::text AS vendor_id,
                COALESCE(i2.vendor_name, 'Unknown') AS vendor_name,
                SUM(i2.total_amount_cents) AS total_amount,
                COUNT(*) AS invoice_count
            FROM invoices i2
            WHERE i2.tenant_id = i.tenant_id
              AND i2.status = 'paid'
              AND i2.updated_at >= NOW() - INTERVAL '30 days'
            GROUP BY i2.vendor_id, i2.vendor_name
            ORDER BY SUM(i2.total_amount_cents) DESC
            LIMIT 10
        ) sub
    ) AS spend_by_vendor,

    -- Total spend (paid invoices, last 30 days)
    COALESCE(SUM(i.total_amount_cents) FILTER (
        WHERE i.status = 'paid'
          AND i.updated_at >= NOW() - INTERVAL '30 days'
    ), 0) AS total_spend_30d,

    -- Average processing hours (paid invoices, last 30 days)
    -- Uses created_at -> updated_at as a proxy for processing time
    COALESCE(AVG(EXTRACT(EPOCH FROM (i.updated_at - i.created_at)) / 3600) FILTER (
        WHERE i.status = 'paid'
          AND i.updated_at >= NOW() - INTERVAL '30 days'
    ), 0) AS avg_processing_hours

FROM invoices i
GROUP BY i.tenant_id;

-- Unique index required for REFRESH CONCURRENTLY
CREATE UNIQUE INDEX IF NOT EXISTS idx_dashboard_kpis_mv_tenant_id
    ON dashboard_kpis_mv(tenant_id);
