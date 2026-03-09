# Performance Optimization Guide

## Overview

This guide provides production-tested optimizations for BillForge based on real-world usage patterns from pilot customers.

## Database Optimization

### PostgreSQL Configuration

**Production Settings** (`postgresql.conf`):
```ini
# Memory (adjust based on available RAM)
shared_buffers = 2GB                    # 25% of RAM
effective_cache_size = 6GB              # 75% of RAM
work_mem = 64MB                         # Per-operation memory
maintenance_work_mem = 512MB

# Connection Pooling
max_connections = 200
superuser_reserved_connections = 3

# WAL (Write-Ahead Logging)
wal_buffers = 64MB
checkpoint_completion_target = 0.9
max_wal_size = 2GB
min_wal_size = 1GB

# Query Planner
random_page_cost = 1.1                  # SSD optimization
effective_io_concurrency = 200

# Parallel Queries
max_parallel_workers_per_gather = 4
max_parallel_workers = 8
max_parallel_maintenance_workers = 4

# Logging
log_min_duration_statement = 500        # Log queries > 500ms
log_checkpoints = on
log_connections = on
log_disconnections = on

# Autovacuum
autovacuum = on
autovacuum_max_workers = 3
autovacuum_naptime = 30s
```

### Index Optimization

**Critical Indexes:**
```sql
-- Invoices by tenant and date
CREATE INDEX CONCURRENTLY idx_invoices_tenant_date
ON invoices(tenant_id, invoice_date DESC);

-- Invoices by vendor
CREATE INDEX CONCURRENTLY idx_invoices_vendor
ON invoices(vendor_id) WHERE deleted_at IS NULL;

-- Active workflows
CREATE INDEX CONCURRENTLY idx_workflows_active
ON approval_workflows(tenant_id, is_active) WHERE is_active = true;

-- Pending approvals
CREATE INDEX CONCURRENTLY idx_approvals_pending
ON approval_steps(approver_id, status, created_at)
WHERE status = 'pending';

-- Audit log queries
CREATE INDEX CONCURRENTLY idx_audit_entity
ON audit_log(entity_type, entity_id, created_at DESC);
```

### Query Optimization

**Use `EXPLAIN ANALYZE`:**
```sql
EXPLAIN (ANALYZE, BUFFERS) SELECT
  i.id, i.invoice_number, i.total_amount_cents
FROM invoices i
WHERE i.tenant_id = 'uuid-here'
  AND i.invoice_date >= '2026-01-01'
ORDER BY i.created_at DESC
LIMIT 50;
```

**Common Optimizations:**
- Add `LIMIT` clauses to all queries
- Use `SELECT` only required columns (avoid `SELECT *`)
- Use `IN` instead of `OR` for multiple values
- Use `EXISTS` instead of `IN` for subqueries
- Add `WHERE` clauses to reduce scan size

### Connection Pooling

**PgBouncer Configuration:**
```ini
[databases]
billforge_control = host=localhost port=5432 dbname=billforge_control

[pgbouncer]
listen_port = 6432
listen_addr = *
auth_type = scram-sha-256
auth_file = /etc/pgbouncer/userlist.txt

# Pool sizing
pool_mode = transaction
max_client_conn = 1000
default_pool_size = 25
reserve_pool_size = 5
reserve_pool_timeout = 3

# Timeouts
server_idle_timeout = 300
client_idle_timeout = 0
client_login_timeout = 60

# Logging
log_connections = 1
log_disconnections = 1
log_pooler_errors = 1
```

## Redis Optimization

### Configuration

**Production Settings** (`redis.conf`):
```conf
# Memory
maxmemory 2gb
maxmemory-policy allkeys-lru

# Persistence
save 900 1
save 300 10
save 60 10000
appendonly yes
appendfsync everysec

# Replication (if using replicas)
replica-serve-stale-data yes
replica-read-only yes

# Performance
tcp-keepalive 300
timeout 0
tcp-backlog 511

# Slow Log
slowlog-log-slower-than 10000
slowlog-max-len 128
```

### Usage Patterns

**Job Queue Optimization:**
```rust
// Batch job processing
pub async fn process_job_batch(conn: &mut RedisConnection, batch_size: usize) -> Result<Vec<Job>> {
    let mut pipe = redis::pipe();

    // Atomic batch pop
    for _ in 0..batch_size {
        pipe.rpop("billforge:jobs:queue");
    }

    let results: Vec<Option<String>> = pipe.query_async(conn).await?;

    Ok(results
        .into_iter()
        .filter_map(|r| r)
        .filter_map(|json| serde_json::from_str(&json).ok())
        .collect())
}
```

**Session Optimization:**
```rust
// Use Redis hashes for session data
// HSET session:{token} user_id {id} tenant_id {id} created_at {ts}
// EXPIRE session:{token} 86400

// Atomic session operations
pub async fn create_session(
    conn: &mut RedisConnection,
    token: &str,
    user_id: Uuid,
    tenant_id: Uuid,
) -> Result<()> {
    let key = format!("session:{}", token);

    redis::cmd("HSET")
        .arg(&key)
        .arg("user_id")
        .arg(user_id.to_string())
        .arg("tenant_id")
        .arg(tenant_id.to_string())
        .arg("created_at")
        .arg(Utc::now().timestamp())
        .query_async(conn)
        .await?;

    redis::cmd("EXPIRE")
        .arg(&key)
        .arg(86400) // 24 hours
        .query_async(conn)
        .await?;

    Ok(())
}
```

## API Optimization

### Response Caching

```rust
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;

// Cache static assets
let app = Router::new()
    .route("/api/v1/:tenant/invoices", get(list_invoices))
    .layer(SetResponseHeaderLayer::overriding(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=300"), // 5 minutes
    ));
```

### Pagination

**Cursor-based pagination (recommended):**
```rust
pub async fn list_invoices_cursor(
    pool: &PgPool,
    tenant_id: Uuid,
    cursor: Option<DateTime<Utc>>,
    limit: usize,
) -> Result<Vec<Invoice>> {
    let limit = limit.min(100); // Enforce max

    sqlx::query_as!(
        Invoice,
        r#"
        SELECT * FROM invoices
        WHERE tenant_id = $1
          AND ($2::timestamptz IS NULL OR created_at < $2)
        ORDER BY created_at DESC
        LIMIT $3
        "#,
        tenant_id,
        cursor,
        limit as i64 + 1, // Fetch one extra for next cursor
    )
    .fetch_all(pool)
    .await
}
```

### Request Batching

```rust
// Batch API for bulk operations
#[derive(Deserialize)]
pub struct BulkApproveRequest {
    pub invoice_ids: Vec<Uuid>,
}

pub async fn bulk_approve_invoices(
    pool: &PgPool,
    tenant_id: Uuid,
    req: BulkApproveRequest,
) -> Result<BulkApproveResponse> {
    if req.invoice_ids.len() > 100 {
        return Err(Error::Validation("Maximum 100 invoices per batch".into()));
    }

    let mut tx = pool.begin().await?;

    // Update all in single transaction
    let updated = sqlx::query!(
        r#"
        UPDATE invoices
        SET processing_status = 'approved',
            updated_at = NOW()
        WHERE tenant_id = $1
          AND id = ANY($2)
          AND processing_status = 'pending_review'
        "#,
        tenant_id,
        &req.invoice_ids,
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(BulkApproveResponse {
        updated_count: updated.rows_affected(),
    })
}
```

## OCR Pipeline Optimization

### Provider Caching

```rust
use lru::LruCache;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct OCRProviderCache {
    tesseract_results: Arc<Mutex<LruCache<String, OCRResult>>>,
}

impl OCRProviderCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            tesseract_results: Arc::new(Mutex::new(LruCache::new(capacity))),
        }
    }

    pub async fn get_or_compute(&self, document_hash: &str) -> Result<OCRResult> {
        let mut cache = self.tesseract_results.lock().await;

        if let Some(result) = cache.get(document_hash) {
            return Ok(result.clone());
        }

        // Compute OCR
        let result = self.run_tesseract(document_hash).await?;
        cache.put(document_hash.to_string(), result.clone());

        Ok(result)
    }
}
```

### Parallel Processing

```rust
use rayon::prelude::*;

pub async fn extract_fields_parallel(regions: Vec<Region>) -> Vec<ExtractedField> {
    tokio::task::spawn_blocking(move || {
        regions
            .par_iter()
            .map(|region| extract_single_field(region))
            .collect()
    })
    .await
    .unwrap()
}
```

## Frontend Optimization

### Next.js Configuration

```javascript
// next.config.js
module.exports = {
  experimental: {
    optimizeCss: true,
  },

  images: {
    formats: ['image/avif', 'image/webp'],
    deviceSizes: [640, 750, 828, 1080, 1200, 1920],
  },

  compiler: {
    removeConsole: process.env.NODE_ENV === 'production',
  },

  async headers() {
    return [
      {
        source: '/api/:path*',
        headers: [
          { key: 'Cache-Control', value: 's-maxage=60, stale-while-revalidate=300' },
        ],
      },
    ];
  },
};
```

### React Query Optimization

```typescript
// useInfiniteQuery for pagination
const {
  data,
  fetchNextPage,
  hasNextPage,
} = useInfiniteQuery({
  queryKey: ['invoices', tenantId],
  queryFn: ({ pageParam }) => fetchInvoices(tenantId, pageParam),
  getNextPageParam: (lastPage) => lastPage.nextCursor,
  staleTime: 5 * 60 * 1000, // 5 minutes
  cacheTime: 10 * 60 * 1000, // 10 minutes
});

// Prefetching
const queryClient = useQueryClient();

const prefetchNextPage = () => {
  if (hasNextPage) {
    queryClient.prefetchInfiniteQuery({
      queryKey: ['invoices', tenantId],
      queryFn: ({ pageParam }) => fetchInvoices(tenantId, pageParam),
    });
  }
};
```

## Load Testing

### k6 Configuration

```javascript
// load-test.js
import http from 'k6/http';
import { check, sleep } from 'k6';

export let options = {
  stages: [
    { duration: '2m', target: 100 },  // Ramp up to 100 users
    { duration: '5m', target: 100 },  // Stay at 100 users
    { duration: '2m', target: 200 },  // Ramp up to 200 users
    { duration: '5m', target: 200 },  // Stay at 200 users
    { duration: '2m', target: 0 },    // Ramp down
  ],
  thresholds: {
    http_req_duration: ['p(95)<500', 'p(99)<1000'], // 95% < 500ms
    http_req_failed: ['rate<0.05'],                 // Error rate < 5%
  },
};

const BASE_URL = 'https://api.billforge.io';

export default function () {
  // Health check
  let res = http.get(`${BASE_URL}/health`);
  check(res, { 'health check passed': (r) => r.status === 200 });

  sleep(1);

  // List invoices
  res = http.get(`${BASE_URL}/api/v1/acme-mfg/invoices`, {
    headers: { Authorization: `Bearer ${__ENV.API_TOKEN}` },
  });
  check(res, { 'invoices list 200': (r) => r.status === 200 });

  sleep(2);

  // Upload invoice
  const payload = JSON.stringify({
    vendor_name: 'Test Vendor',
    invoice_number: `TEST-${__VU}-${__ITER}`,
    total_amount_cents: 10000,
  });

  res = http.post(`${BASE_URL}/api/v1/acme-mfg/invoices`, payload, {
    headers: {
      Authorization: `Bearer ${__ENV.API_TOKEN}`,
      'Content-Type': 'application/json',
    },
  });
  check(res, { 'invoice created': (r) => r.status === 201 });

  sleep(3);
}
```

**Run:**
```bash
k6 run --vus 200 --duration 10m load-test.js
```

## Monitoring Performance

### Key Metrics

**API:**
- Request rate (requests/second)
- Response time (P50, P95, P99)
- Error rate (4xx, 5xx)
- Active connections

**Database:**
- Query latency
- Connection pool usage
- Transaction duration
- Lock wait time

**Worker:**
- Job processing rate
- Job queue depth
- Job failure rate
- Processing time per job

### Dashboards

**Grafana Dashboard JSON:**
```json
{
  "dashboard": {
    "title": "BillForge Performance",
    "panels": [
      {
        "title": "API Response Time",
        "type": "graph",
        "targets": [
          {
            "expr": "histogram_quantile(0.95, rate(billforge_http_request_duration_seconds_bucket[5m]))",
            "legendFormat": "P95"
          },
          {
            "expr": "histogram_quantile(0.99, rate(billforge_http_request_duration_seconds_bucket[5m]))",
            "legendFormat": "P99"
          }
        ]
      },
      {
        "title": "Database Connections",
        "type": "graph",
        "targets": [
          {
            "expr": "pg_stat_activity_count",
            "legendFormat": "Active"
          }
        ]
      }
    ]
  }
}
```

## Optimization Checklist

- [ ] PostgreSQL configured with optimal settings
- [ ] Critical indexes created
- [ ] Connection pooling enabled
- [ ] Redis configured with maxmemory
- [ ] API response caching enabled
- [ ] Cursor-based pagination implemented
- [ ] Batch APIs for bulk operations
- [ ] OCR result caching enabled
- [ ] Parallel OCR processing
- [ ] Frontend images optimized
- [ ] React Query caching configured
- [ ] Load testing passed
- [ ] Performance dashboards created
- [ ] Alerts configured for performance degradation
