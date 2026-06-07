-- 130 down: drop benchmark peer insights objects

DROP FUNCTION IF EXISTS get_peer_cohort_kpis(TEXT, TEXT, TEXT);
DROP TABLE IF EXISTS tenant_benchmark_kpis;

ALTER TABLE tenants
    DROP COLUMN IF EXISTS benchmark_volume_band,
    DROP COLUMN IF EXISTS benchmark_headcount_band,
    DROP COLUMN IF EXISTS benchmark_industry,
    DROP COLUMN IF EXISTS benchmark_opt_in;
