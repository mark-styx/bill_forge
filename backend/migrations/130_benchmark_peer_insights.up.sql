-- 130: Benchmark peer insights - opt-in columns, tenant KPI rollup table, cohort percentile function.
--
-- Adds benchmark_opt_in and cohort descriptor columns to tenants.
-- Creates a tenant_benchmark_kpis table in the metadata DB to store per-tenant AP KPI
-- rollups (populated from per-tenant databases). A SECURITY DEFINER function computes
-- cohort percentiles (p25/p50/p75) on-the-fly with a k-anonymity floor (cohort_size >= 5).
--
-- Architecture note: tenants live in the metadata DB while invoices live in per-tenant
-- databases, so cross-tenant aggregation cannot join them directly. Instead, per-tenant
-- KPIs are computed in the tenant DB and upserted into tenant_benchmark_kpis here.
-- A nightly worker will handle batch refresh in a follow-up.

-- ---------------------------------------------------------------------------
-- 1. Add benchmark columns to tenants
-- ---------------------------------------------------------------------------

ALTER TABLE tenants
    ADD COLUMN IF NOT EXISTS benchmark_opt_in BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS benchmark_industry TEXT,
    ADD COLUMN IF NOT EXISTS benchmark_headcount_band TEXT,
    ADD COLUMN IF NOT EXISTS benchmark_volume_band TEXT;

-- ---------------------------------------------------------------------------
-- 2. Per-tenant benchmark KPIs rollup table
-- ---------------------------------------------------------------------------
-- Stores pre-computed KPIs from each opted-in tenant's per-tenant database.
-- Populated by publish_tenant_kpis() in benchmark.rs and/or a nightly worker.

CREATE TABLE IF NOT EXISTS tenant_benchmark_kpis (
    tenant_id                   UUID PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    industry                    TEXT    NOT NULL,
    headcount_band              TEXT    NOT NULL,
    volume_band                 TEXT    NOT NULL,
    dpo_days                    DOUBLE PRECISION NOT NULL DEFAULT 0,
    avg_approval_cycle_hours    DOUBLE PRECISION NOT NULL DEFAULT 0,
    ocr_straight_through_rate   DOUBLE PRECISION NOT NULL DEFAULT 0,
    exception_rate              DOUBLE PRECISION NOT NULL DEFAULT 0,
    discount_capture_rate       DOUBLE PRECISION NOT NULL DEFAULT 0,
    cost_per_invoice            DOUBLE PRECISION NOT NULL DEFAULT 0,
    computed_at                 TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_tenant_benchmark_kpis_cohort
    ON tenant_benchmark_kpis(industry, headcount_band, volume_band);

-- ---------------------------------------------------------------------------
-- 3. SECURITY DEFINER function for cohort percentiles
-- ---------------------------------------------------------------------------
-- Computes p25/p50/p75 on-the-fly from tenant_benchmark_kpis.
-- Only returns results when cohort_size >= 5 (k-anonymity floor).
-- SECURITY DEFINER bypasses RLS on the metadata pool so that tenant-scoped
-- connections can read aggregated cohort data without seeing individual rows.

CREATE OR REPLACE FUNCTION get_peer_cohort_kpis(
    p_industry       TEXT,
    p_headcount_band TEXT,
    p_volume_band    TEXT
)
RETURNS TABLE (
    cohort_size                      BIGINT,
    dpo_days_p25                     FLOAT,
    dpo_days_p50                     FLOAT,
    dpo_days_p75                     FLOAT,
    avg_approval_cycle_hours_p25     FLOAT,
    avg_approval_cycle_hours_p50     FLOAT,
    avg_approval_cycle_hours_p75     FLOAT,
    ocr_straight_through_rate_p25    FLOAT,
    ocr_straight_through_rate_p50    FLOAT,
    ocr_straight_through_rate_p75    FLOAT,
    exception_rate_p25               FLOAT,
    exception_rate_p50               FLOAT,
    exception_rate_p75               FLOAT,
    discount_capture_rate_p25        FLOAT,
    discount_capture_rate_p50        FLOAT,
    discount_capture_rate_p75        FLOAT,
    cost_per_invoice_p25             FLOAT,
    cost_per_invoice_p50             FLOAT,
    cost_per_invoice_p75             FLOAT
)
LANGUAGE sql
SECURITY DEFINER
STABLE
AS $$
    SELECT
        COUNT(*) AS cohort_size,

        PERCENTILE_CONT(0.25) WITHIN GROUP (ORDER BY dpo_days)                  AS dpo_days_p25,
        PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY dpo_days)                  AS dpo_days_p50,
        PERCENTILE_CONT(0.75) WITHIN GROUP (ORDER BY dpo_days)                  AS dpo_days_p75,

        PERCENTILE_CONT(0.25) WITHIN GROUP (ORDER BY avg_approval_cycle_hours)  AS avg_approval_cycle_hours_p25,
        PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY avg_approval_cycle_hours)  AS avg_approval_cycle_hours_p50,
        PERCENTILE_CONT(0.75) WITHIN GROUP (ORDER BY avg_approval_cycle_hours)  AS avg_approval_cycle_hours_p75,

        PERCENTILE_CONT(0.25) WITHIN GROUP (ORDER BY ocr_straight_through_rate) AS ocr_straight_through_rate_p25,
        PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY ocr_straight_through_rate) AS ocr_straight_through_rate_p50,
        PERCENTILE_CONT(0.75) WITHIN GROUP (ORDER BY ocr_straight_through_rate) AS ocr_straight_through_rate_p75,

        PERCENTILE_CONT(0.25) WITHIN GROUP (ORDER BY exception_rate)            AS exception_rate_p25,
        PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY exception_rate)            AS exception_rate_p50,
        PERCENTILE_CONT(0.75) WITHIN GROUP (ORDER BY exception_rate)            AS exception_rate_p75,

        PERCENTILE_CONT(0.25) WITHIN GROUP (ORDER BY discount_capture_rate)     AS discount_capture_rate_p25,
        PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY discount_capture_rate)     AS discount_capture_rate_p50,
        PERCENTILE_CONT(0.75) WITHIN GROUP (ORDER BY discount_capture_rate)     AS discount_capture_rate_p75,

        PERCENTILE_CONT(0.25) WITHIN GROUP (ORDER BY cost_per_invoice)          AS cost_per_invoice_p25,
        PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY cost_per_invoice)          AS cost_per_invoice_p50,
        PERCENTILE_CONT(0.75) WITHIN GROUP (ORDER BY cost_per_invoice)          AS cost_per_invoice_p75

    FROM tenant_benchmark_kpis
    WHERE industry       = p_industry
      AND headcount_band = p_headcount_band
      AND volume_band    = p_volume_band
    HAVING COUNT(*) >= 5;
$$;
