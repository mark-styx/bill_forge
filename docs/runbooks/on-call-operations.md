# BillForge On-Call Operations Runbook

This document provides diagnosis and remediation procedures for every Prometheus alert
configured in `config/prometheus/alerts.yml`. Each section follows the pattern:
**Symptom -> Diagnosis commands -> Fix steps -> Escalation criteria.**

## Escalation Contacts

| Role | Contact | Availability |
|------|---------|-------------|
| On-Call Engineer | PagerDuty: `billforge-oncall` | 24/7 rotation |
| Engineering Lead | Slack: `#billforge-eng` | Business hours |
| SRE Team | Slack: `#billforge-sre` | Business hours |
| CTO / Escalation | PagerDuty: `billforge-management` | 24/7, last resort |

## Environment Access

```bash
# Kubernetes context
kubectl config use-context billforge-prod

# Port-forward for direct DB access
kubectl port-forward svc/billforge-postgres 5432:5432 -n billforge

# Port-forward Redis
kubectl port-forward svc/billforge-redis 6379:6379 -n billforge
```

---

## API Alerts

### APIHighErrorRate {#apihigherrorrate}

**Severity:** warning (>1%) / critical (>5%)
**Alert group:** `billforge_critical`

**Symptom:** The rate of HTTP 5xx responses exceeds the threshold for 5 minutes.

**Diagnosis:**

```bash
# Check recent API pod logs for errors
kubectl logs -l app=billforge-api --tail=200 -n billforge | grep -i "error\|panic\|500"

# Check error rate in Grafana (dashboard: BillForge Overview)
# Panel: API Error Rate

# Verify DB connectivity from API pods
kubectl exec -it deploy/billforge-api -n billforge -- \
  pg_isready -h billforge-postgres -p 5432

# Check for recent deployments that may have introduced a regression
kubectl rollout history deploy/billforge-api -n billforge
```

**Fix steps:**

1. If a recent deploy correlates with the error spike, **rollback**:
   ```bash
   kubectl rollout undo deploy/billforge-api -n billforge
   ```
2. If DB connectivity is failing, check the PostgreSQL alerts and follow [PostgreSQLTooManyConnections](#postgresqltoomanyconnections).
3. If errors are from a specific endpoint, check for input validation issues or upstream dependency failures.
4. If no obvious cause, increase log level:
   ```bash
   kubectl set env deploy/billforge-api RUST_LOG=debug -n billforge
   ```

**Escalate if:** Error rate remains >5% after 15 minutes or rollback fails.

---

### APIHighLatency {#apihighlatency}

**Severity:** warning (P95 >1s) / critical (P95 >2s)
**Alert group:** `billforge_critical`

**Symptom:** API P95 response time exceeds threshold for 5 minutes.

**Diagnosis:**

```bash
# Check current response time distribution
kubectl logs -l app=billforge-api --tail=100 -n billforge | grep "request_duration"

# Check DB query performance
kubectl exec -it deploy/billforge-postgres -n billforge -- \
  psql -U billforge -c "SELECT * FROM pg_stat_statements ORDER BY mean_exec_time DESC LIMIT 10;"

# Check if the API pods are resource-constrained
kubectl top pods -l app=billforge-api -n billforge

# Check for slow OCR blocking async responses
kubectl logs -l app=billforge-worker --tail=100 -n billforge | grep "ocr.*timeout\|ocr.*slow"
```

**Fix steps:**

1. If DB queries are slow, check for long-running transactions:
   ```bash
   kubectl exec -it deploy/billforge-postgres -n billforge -- \
     psql -U billforge -c "SELECT pid, now()-pg_stat_activity.query_start AS duration, query FROM pg_stat_activity WHERE state='active' ORDER BY duration DESC;"
   ```
2. If pods are CPU/memory constrained, scale horizontally:
   ```bash
   kubectl scale deploy/billforge-api --replicas=4 -n billforge
   ```
3. If OCR processing is backing up, see [OCRProcessingLatency](#ocrprocessinglatency).

**Escalate if:** P95 remains >2s after 15 minutes or scaling does not help.

---

### APIDown {#apidown}

**Severity:** critical
**Alert group:** `billforge_critical`

**Symptom:** API pod is unreachable for 2+ minutes.

**Diagnosis:**

```bash
# Check pod status
kubectl get pods -l app=billforge-api -n billforge

# Check pod events
kubectl describe deploy/billforge-api -n billforge | tail -30

# Check if OOMKilled
kubectl get pods -l app=billforge-api -n billforge -o jsonpath='{.items[*].status.containerStatuses[0].lastState}'
```

**Fix steps:**

1. If pods are CrashLoopBackOff, check logs:
   ```bash
   kubectl logs -l app=billforge-api --previous -n billforge
   ```
2. If OOMKilled, increase memory limit:
   ```bash
   kubectl set resources deploy/billforge-api -c billforge-api \
     --limits=memory=1Gi -n billforge
   ```
3. If ImagePullBackOff, verify image tag and registry access.
4. Force restart as last resort:
   ```bash
   kubectl rollout restart deploy/billforge-api -n billforge
   ```

**Escalate if:** Pods do not become ready within 10 minutes of remediation.

---

## Worker Alerts

### WorkerQueueBacklog {#workerqueuebacklog}

**Severity:** warning
**Alert group:** `billforge_critical`
**Threshold:** >50 pending jobs for 10 minutes

**Symptom:** The job queue has accumulated more than 50 pending jobs.

**Diagnosis:**

```bash
# Check current queue depth
kubectl exec -it deploy/billforge-redis -n billforge -- \
  redis-cli LLEN billforge:jobs:queue

# Check failed jobs
kubectl exec -it deploy/billforge-redis -n billforge -- \
  redis-cli LLEN billforge:jobs:failed

# Check worker pod health
kubectl get pods -l app=billforge-worker -n billforge

# Check worker logs for processing errors
kubectl logs -l app=billforge-worker --tail=200 -n billforge | grep -i "error\|failed"
```

**Fix steps:**

1. If workers are healthy but slow, scale up:
   ```bash
   kubectl scale deploy/billforge-worker --replicas=6 -n billforge
   ```
2. If workers are crash-looping, check logs and fix root cause (see [WorkerDown](#workerdown)).
3. If there are failed jobs, inspect and replay:
   ```bash
   # Peek at failed jobs
   kubectl exec -it deploy/billforge-redis -n billforge -- \
     redis-cli LRANGE billforge:jobs:failed 0 5
   ```
4. To replay failed jobs (after fixing root cause):
   ```bash
   kubectl exec -it deploy/billforge-redis -n billforge -- \
     redis-cli RPOPLPUSH billforge:jobs:failed billforge:jobs:queue
   ```

**Escalate if:** Queue continues growing after scaling to 10 worker replicas.

---

### WorkerHighFailureRate {#workerhighfailurerate}

**Severity:** warning
**Alert group:** `billforge_critical`
**Threshold:** >10% failure rate for 10 minutes

**Symptom:** More than 10% of processed jobs are failing.

**Diagnosis:**

```bash
# Check recent failures
kubectl logs -l app=billforge-worker --tail=500 -n billforge | grep -i "job.*failed"

# Check if failures are tied to a specific job type
kubectl logs -l app=billforge-worker --tail=500 -n billforge | grep "job_type" | sort | uniq -c | sort -rn

# Verify external dependencies (OCR provider, email service)
kubectl exec -it deploy/billforge-worker -n billforge -- \
  curl -s -o /dev/null -w "%{http_code}" https://api.textract.us-east-1.amazonaws.com/
```

**Fix steps:**

1. If failures are from a specific job type, inspect that handler for bugs.
2. If OCR provider is down, see [OCRProviderErrors](#ocrprovidererrors) for fallback.
3. If DB errors, check [PostgreSQLTooManyConnections](#postgresqltoomanyconnections).
4. Temporarily pause the failing job type if it blocks other processing:
   ```bash
   kubectl set env deploy/billforge-worker DISABLED_JOB_TYPES=ocr_process -n billforge
   ```

**Escalate if:** Failure rate exceeds 25% or affects invoice processing SLA.

---

### WorkerDown {#workerdown}

**Severity:** critical
**Alert group:** `billforge_critical`

**Symptom:** Worker pod is unreachable for 5+ minutes.

**Diagnosis:**

```bash
kubectl get pods -l app=billforge-worker -n billforge
kubectl logs -l app=billforge-worker --previous -n billforge
```

**Fix steps:**

1. Follow same playbook as [APIDown](#apidown) for pod restart troubleshooting.
2. After workers are back, verify they pick up queued jobs:
   ```bash
   kubectl logs -l app=billforge-worker --tail=20 -n billforge | grep "processing job"
   ```

**Escalate if:** Workers do not recover within 10 minutes and queue backlog grows.

---

## OCR Pipeline Alerts

### OCRProcessingLatency {#ocrprocessinglatency}

**Severity:** warning
**Alert group:** `billforge_critical`
**Threshold:** P95 >30s for 5 minutes

**Symptom:** OCR processing is taking longer than expected.

**Diagnosis:**

```bash
# Check OCR processing times in worker logs
kubectl logs -l app=billforge-worker --tail=200 -n billforge | grep "ocr.*duration"

# Check if Tesseract/Textract is healthy
kubectl logs -l app=billforge-worker --tail=200 -n billforge | grep -i "tesseract\|textract"

# Check for large or complex documents in the queue
kubectl exec -it deploy/billforge-redis -n billforge -- \
  redis-cli LRANGE billforge:jobs:queue 0 2
```

**Fix steps:**

1. If Textract API is slow (regional degradation), switch to Tesseract fallback:
   ```bash
   kubectl set env deploy/billforge-worker OCR_PROVIDER=tesseract -n billforge
   ```
2. If documents are unusually large, consider increasing per-document timeout:
   ```bash
   kubectl set env deploy/billforge-worker OCR_TIMEOUT_SECS=60 -n billforge
   ```
3. Scale workers if backlog is contributing to latency:
   ```bash
   kubectl scale deploy/billforge-worker --replicas=8 -n billforge
   ```

**Escalate if:** OCR P95 exceeds 60s or invoices are stuck in `processing` state for >10 minutes.

---

### OCRProviderErrors {#ocrprovidererrors}

**Severity:** warning
**Alert group:** `billforge_critical`
**Threshold:** >5% OCR provider failure rate over 10 minutes

**Symptom:** External OCR provider (Textract/Tesseract) is returning errors.

**Diagnosis:**

```bash
# Check specific OCR errors
kubectl logs -l app=billforge-worker --tail=500 -n billforge | grep -i "ocr.*error\|textract.*error\|tesseract.*error"

# Check AWS service health
curl -s https://health.aws.amazon.com/health/status | grep textract

# Check if Tesseract binary is present and functional
kubectl exec -it deploy/billforge-worker -n billforge -- \
  tesseract --version 2>/dev/null || echo "Tesseract not available"
```

**Fix steps:**

1. If Textract is down, switch to Tesseract:
   ```bash
   kubectl set env deploy/billforge-worker OCR_PROVIDER=tesseract -n billforge
   ```
2. If Tesseract is failing (missing deps in container), switch to Textract:
   ```bash
   kubectl set env deploy/billforge-worker OCR_PROVIDER=textract -n billforge
   ```
3. If credentials are expired (AWS), rotate and update the secret:
   ```bash
   kubectl update secret billforge-aws-creds --from-literal=AWS_ACCESS_KEY_ID=... -n billforge
   kubectl rollout restart deploy/billforge-worker -n billforge
   ```
4. After fixing, trigger manual reprocessing of failed OCR jobs:
   ```bash
   # Reprocess invoices stuck in 'ocr_failed' state via the API
   curl -X POST $API_URL/api/v1/admin/invoices/reprocess-failed \
     -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"status": "ocr_failed"}'
   ```

**Escalate if:** Both OCR providers are down simultaneously.

---

### LowOCRConfidence {#lowocrconfidence}

**Severity:** warning
**Alert group:** `billforge_business`
**Threshold:** Average confidence <75% for 30 minutes

**Symptom:** OCR results have low confidence scores, indicating poor extraction quality.

**Diagnosis:**

```bash
# Check recent confidence scores
kubectl logs -l app=billforge-worker --tail=200 -n billforge | grep "confidence"

# Query the DB for recent low-confidence invoices
kubectl exec -it deploy/billforge-postgres -n billforge -- \
  psql -U billforge -c "SELECT id, ocr_confidence, created_at FROM invoices WHERE ocr_confidence < 0.75 ORDER BY created_at DESC LIMIT 20;"
```

**Fix steps:**

1. If a new vendor template is causing issues, review and update the template.
2. If image quality is the issue, consider adding image pre-processing.
3. Low confidence invoices should be routed for manual review - verify the workflow routing is working:
   ```bash
   kubectl logs -l app=billforge-worker --tail=100 -n billforge | grep "manual_review"
   ```

**Escalate if:** Confidence drops below 50% or manual review queue grows unbounded.

---

## Database Alerts

### PostgreSQLTooManyConnections {#postgresqltoomanyconnections}

**Severity:** warning
**Alert group:** `billforge_database`
**Threshold:** >80% of max connections for 5 minutes

**Diagnosis:**

```bash
# Check active connections
kubectl exec -it deploy/billforge-postgres -n billforge -- \
  psql -U billforge -c "SELECT count(*), state FROM pg_stat_activity GROUP BY state;"

# Find long-running idle connections
kubectl exec -it deploy/billforge-postgres -n billforge -- \
  psql -U billforge -c "SELECT pid, usename, application_name, client_addr, state, now()-query_start AS duration FROM pg_stat_activity WHERE state='idle' ORDER BY duration DESC LIMIT 20;"

# Check connection pooler (PgBouncer) if deployed
kubectl exec -it deploy/billforge-postgres -n billforge -- \
  psql -U billforge -c "SHOW max_connections;"
```

**Fix steps:**

1. Kill idle connections older than 10 minutes:
   ```bash
   kubectl exec -it deploy/billforge-postgres -n billforge -- \
     psql -U billforge -c "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE state='idle' AND now()-query_start > interval '10 minutes';"
   ```
2. If connection pooling is not configured, deploy PgBouncer.
3. Increase max connections as a temporary measure:
   ```bash
   # Requires PostgreSQL restart
   kubectl exec -it deploy/billforge-postgres -n billforge -- \
     psql -U billforge -c "ALTER SYSTEM SET max_connections = 200;"
   kubectl rollout restart deploy/billforge-postgres -n billforge
   ```

**Escalate if:** Connections max out even after cleanup (possible connection leak in application).

---

### PostgreSQLReplicationLag {#postgresqlreplicationlag}

**Severity:** critical
**Alert group:** `billforge_database`
**Threshold:** >10 seconds lag for 5 minutes

**Diagnosis:**

```bash
# Check replication status
kubectl exec -it deploy/billforge-postgres -n billforge -- \
  psql -U billforge -c "SELECT client_addr, state, sent_lsn, replay_lsn, (sent_lsn - replay_lsn) AS lag FROM pg_stat_replication;"

# Check if replica is under heavy load
kubectl top pods -l app=billforge-postgres-replica -n billforge
```

**Fix steps:**

1. If replica is CPU-constrained, scale vertically or relieve read traffic.
2. If replication is stalled, restart the replica:
   ```bash
   kubectl rollout restart deploy/billforge-postgres-replica -n billforge
   ```
3. If replica is unrecoverable, rebuild from primary:
   ```bash
   # Follow the disaster recovery procedure in docs/disaster_recovery.md
   ```

**Escalate if:** Lag exceeds 60 seconds (risk of stale reads affecting users).

---

### PostgreSQLDiskSpaceLow {#postgresqldiskspacelow}

**Severity:** warning
**Alert group:** `billforge_database`
**Threshold:** >85% disk usage for 10 minutes

**Diagnosis:**

```bash
# Check database sizes
kubectl exec -it deploy/billforge-postgres -n billforge -- \
  psql -U billforge -c "SELECT datname, pg_size_pretty(pg_database_size(datname)) FROM pg_database;"

# Check table sizes
kubectl exec -it deploy/billforge-postgres -n billforge -- \
  psql -U billforge -c "SELECT relname, pg_size_pretty(pg_total_relation_size(relid)) FROM pg_catalog.pg_statio_user_tables ORDER BY pg_total_relation_size(relid) DESC LIMIT 10;"

# Check if vacuum is needed
kubectl exec -it deploy/billforge-postgres -n billforge -- \
  psql -U billforge -c "SELECT relname, n_dead_tup FROM pg_stat_user_tables WHERE n_dead_tup > 10000 ORDER BY n_dead_tup DESC;"
```

**Fix steps:**

1. Run emergency vacuum on tables with many dead tuples:
   ```bash
   kubectl exec -it deploy/billforge-postgres -n billforge -- \
     psql -U billforge -c "VACUUM (VERBOSE) invoices;"
   ```
2. Expand PVC if possible:
   ```bash
   kubectl edit pvc data-billforge-postgres-0 -n billforge
   # Increase storage request
   ```
3. Archive old invoice PDFs to object storage (S3).

**Escalate if:** Disk usage exceeds 95% (immediate risk of database corruption).

---

## Redis Alerts

### RedisMemoryHigh {#redismemoryhigh}

**Severity:** warning
**Alert group:** `billforge_database`
**Threshold:** >90% memory usage for 5 minutes

**Diagnosis:**

```bash
# Check Redis memory info
kubectl exec -it deploy/billforge-redis -n billforge -- \
  redis-cli INFO memory

# Check top keys by memory
kubectl exec -it deploy/billforge-redis -n billforge -- \
  redis-cli --bigkeys

# Check if eviction policy is set
kubectl exec -it deploy/billforge-redis -n billforge -- \
  redis-cli CONFIG GET maxmemory-policy
```

**Fix steps:**

1. If eviction policy is `noeviction`, change to `allkeys-lru`:
   ```bash
   kubectl exec -it deploy/billforge-redis -n billforge -- \
     redis-cli CONFIG SET maxmemory-policy allkeys-lru
   ```
2. Flush stale cache keys:
   ```bash
   kubectl exec -it deploy/billforge-redis -n billforge -- \
     redis-cli --scan --pattern "cache:*" | xargs -L 1000 redis-cli DEL
   ```
3. Increase memory limit if sustained traffic justifies it.

**Escalate if:** Redis starts rejecting writes or OOM kills the process.

---

### RedisDown {#redisdown}

**Severity:** critical
**Alert group:** `billforge_database`
**Threshold:** Unreachable for 2+ minutes

**Diagnosis:**

```bash
kubectl get pods -l app=billforge-redis -n billforge
kubectl logs -l app=billforge-redis --tail=100 -n billforge
```

**Fix steps:**

1. If pod is CrashLoopBackOff, check for OOM or config errors.
2. Restart Redis:
   ```bash
   kubectl rollout restart deploy/billforge-redis -n billforge
   ```
3. If data persistence is enabled, verify the PVC is healthy.
4. After recovery, verify workers reconnect:
   ```bash
   kubectl logs -l app=billforge-worker --tail=20 -n billforge | grep "redis.*connect"
   ```

**Escalate if:** Redis does not recover within 10 minutes (consider failing over to replica).

---

## Business Alerts

### InvoiceProcessingBacklog {#invoiceprocessingbacklog}

**Severity:** warning
**Alert group:** `billforge_business`
**Threshold:** Pending invoices exceed hourly processing capacity for 1 hour

**Diagnosis:**

```bash
# Count pending invoices
kubectl exec -it deploy/billforge-postgres -n billforge -- \
  psql -U billforge -c "SELECT status, count(*) FROM invoices WHERE status IN ('pending', 'processing') GROUP BY status;"

# Check processing throughput
kubectl logs -l app=billforge-worker --tail=500 -n billforge | grep -c "invoice.*processed"
```

**Fix steps:**

1. Scale workers to clear the backlog:
   ```bash
   kubectl scale deploy/billforge-worker --replicas=10 -n billforge
   ```
2. If OCR is the bottleneck, see [OCRProcessingLatency](#ocrprocessinglatency).
3. If a specific step is failing, see [WorkerHighFailureRate](#workerhighfailurerate).

**Escalate if:** Backlog persists for >4 hours despite scaling (risk of SLA breach).

---

### ApprovalSLABreach {#approvalslabreach}

**Severity:** warning
**Alert group:** `billforge_business`
**Threshold:** P95 approval cycle time >48 hours for 30 minutes

**Diagnosis:**

```bash
# Find invoices waiting longest for approval
kubectl exec -it deploy/billforge-postgres -n billforge -- \
  psql -U billforge -c "SELECT id, vendor_id, created_at, now()-created_at AS age FROM invoices WHERE status='pending_approval' ORDER BY created_at ASC LIMIT 20;"
```

**Fix steps:**

1. Notify approvers via the notification system.
2. Check if approval routing rules are correct (not routing to inactive users).
3. If an approver is unavailable, reassign via admin API:
   ```bash
   curl -X POST $API_URL/api/v1/admin/approvals/reassign \
     -H "Authorization: Bearer $TOKEN" \
     -d '{"invoice_id": "...", "new_approver_id": "..."}'
   ```

**Escalate if:** SLA breach affects >10% of invoices or key customer accounts.

---

## Infrastructure Alerts

### ContainerHighCPU {#containerhighcpu}

**Severity:** warning
**Alert group:** `billforge_infrastructure`
**Threshold:** >80% CPU for 10 minutes

**Diagnosis:**

```bash
kubectl top pods -n billforge --sort-by=cpu
kubectl top containers -n billforge
```

**Fix steps:**

1. Scale the affected deployment horizontally:
   ```bash
   kubectl scale deploy/<deployment-name> --replicas=<current+2> -n billforge
   ```
2. If scaling does not help, increase CPU limit:
   ```bash
   kubectl set resources deploy/<deployment-name> -c <container> \
     --limits=cpu=2 -n billforge
   ```
3. Identify hot code paths from flame graphs (via Grafana Pyroscope if available).

**Escalate if:** CPU throttling causes request failures.

---

### ContainerHighMemory {#containerhighmemory}

**Severity:** warning
**Alert group:** `billforge_infrastructure`
**Threshold:** >90% memory for 10 minutes

**Diagnosis:**

```bash
kubectl top pods -n billforge --sort-by=memory
# Check for memory leaks
kubectl logs -l app=<service> --tail=500 -n billforge | grep -i "oom\|memory\|alloc"
```

**Fix steps:**

1. Increase memory limit:
   ```bash
   kubectl set resources deploy/<deployment-name> -c <container> \
     --limits=memory=2Gi -n billforge
   ```
2. If memory is growing linearly (leak), restart as a stopgap and file a bug:
   ```bash
   kubectl rollout restart deploy/<deployment-name> -n billforge
   ```

**Escalate if:** Pods are being OOMKilled.

---

### ContainerRestartingFrequently {#containerrestartingfrequently}

**Severity:** warning
**Alert group:** `billforge_infrastructure`
**Threshold:** >5 restarts in 1 hour

**Diagnosis:**

```bash
kubectl get pods -n billforge -o wide
kubectl describe pod <pod-name> -n billforge | grep -A5 "Last State"
kubectl logs <pod-name> --previous -n billforge
```

**Fix steps:**

1. If CrashLoopBackOff with OOMKilled, increase memory (see [ContainerHighMemory](#containerhighmemory)).
2. If application panic, analyze stack trace in logs and file a bug.
3. If health check is too aggressive, tune probe settings:
   ```bash
   kubectl patch deploy/<deployment-name> -n billforge --type=json \
     -p='[{"op":"replace","path":"/spec/template/spec/containers/0/livenessProbe/initialDelaySeconds","value":30}]'
   ```

**Escalate if:** Restart loop makes the service unavailable.

---

### DiskSpaceLow {#diskspacelow}

**Severity:** warning
**Alert group:** `billforge_infrastructure`
**Threshold:** >85% disk usage for 10 minutes

**Diagnosis:**

```bash
# Check node disk usage
kubectl get nodes -o wide
kubectl describe node <node-name> | grep -A10 "Allocated"

# SSH to node (if possible) for details
df -h
du -sh /var/lib/containerd/* | sort -rh | head -10
```

**Fix steps:**

1. Rotate logs on the node:
   ```bash
   kubectl logs --rotate <pod-name> -n billforge
   # Or on the node directly:
   journalctl --vacuum-size=500M
   ```
2. Clean up unused container images:
   ```bash
   crictl rmi --prune
   ```
3. Expand node storage or migrate workloads to a less utilized node.

**Escalate if:** Disk usage exceeds 95% (node may become unresponsive).

---

### HighNetworkTraffic {#highnetworktraffic}

**Severity:** info
**Alert group:** `billforge_infrastructure`
**Threshold:** >100MB/s receive for 10 minutes

This is an informational alert. No immediate action required unless it correlates
with performance degradation. Check for:

1. Large invoice PDF uploads or downloads.
2. Backup jobs running during peak hours.
3. Unexpected external traffic (potential security concern).

If traffic is causing latency, consider enabling CDN for static assets or rate-limiting uploads.
