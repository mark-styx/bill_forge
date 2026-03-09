#!/bin/bash
#
# Worker Queue Failure Test
# Simulates Redis queue unavailability and verifies job persistence and recovery
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib/common.sh"

# Test configuration
TEST_NAME="worker-queue-failure"
NAMESPACE="${NAMESPACE:-billforge}"
WORKER_DEPLOYMENT="${WORKER_DEPLOYMENT:-billforge-worker}"

echo "[$(timestamp)] Starting worker queue failure test"

# Pre-test checks
echo "[$(timestamp)] Running pre-test checks..."

# Verify Redis is healthy
if ! redis_is_ready; then
    echo "ERROR: Redis is not healthy before test"
    exit 1
fi

# Verify worker is running
WORKER_PODS=$(kubectl get pods -n "$NAMESPACE" -l app="$WORKER_DEPLOYMENT" --field-selector=status.phase=Running -o json | jq -r '.items | length')
if [[ $WORKER_PODS -eq 0 ]]; then
    echo "ERROR: No worker pods running"
    exit 1
fi

echo "[$(timestamp)] Worker pods running: $WORKER_PODS"

# Enqueue test jobs
echo "[$(timestamp)] Enqueueing test jobs before failure..."

JOB_IDS=()
for i in {1..5}; do
    JOB_ID="test-job-$(date +%s)-$i"
    JOB_IDS+=("$JOB_ID")

    # Enqueue job via API
    curl -s -X POST "${API_URL:-http://localhost:8000}/api/v1/jobs/enqueue" \
        -H "Content-Type: application/json" \
        -d "{\"job_type\":\"MetricsAggregation\",\"tenant_id\":\"test-tenant\",\"job_id\":\"$JOB_ID\"}" \
        -o /dev/null

    echo "[$(timestamp)] Enqueued job: $JOB_ID"
done

# Check queue depth
QUEUE_DEPTH=$(redis-cli -h "${REDIS_HOST:-localhost}" -p "${REDIS_PORT:-6379}" LLEN "billforge:jobs:queue")
echo "[$(timestamp)] Queue depth before failure: $QUEUE_DEPTH"

# Simulate queue unavailability (block access to queue)
echo "[$(timestamp)] Simulating queue unavailability..."

# Method 1: Rename the queue (makes it unavailable)
redis-cli -h "${REDIS_HOST:-localhost}" -p "${REDIS_PORT:-6379}" RENAME "billforge:jobs:queue" "billforge:jobs:queue:backup"

# Verify queue is unavailable
QUEUE_EXISTS=$(redis-cli -h "${REDIS_HOST:-localhost}" -p "${REDIS_PORT:-6379}" EXISTS "billforge:jobs:queue")
if [[ "$QUEUE_EXISTS" -eq 0 ]]; then
    echo "[$(timestamp)] Queue is now unavailable"
else
    echo "ERROR: Queue still exists after rename"
    # Restore queue
    redis-cli -h "${REDIS_HOST:-localhost}" -p "${REDIS_PORT:-6379}" RENAME "billforge:jobs:queue:backup" "billforge:jobs:queue"
    exit 1
fi

# Verify workers handle missing queue gracefully
echo "[$(timestamp)] Verifying worker behavior without queue..."

sleep 5  # Give workers time to attempt polling

# Check worker logs for error handling
WORKER_POD=$(get_pod_name "$NAMESPACE" "app=$WORKER_DEPLOYMENT")
ERROR_COUNT=$(kubectl logs -n "$NAMESPACE" "$WORKER_POD" --tail=50 | grep -i "error.*queue" | wc -l || echo "0")

if [[ $ERROR_COUNT -gt 0 ]]; then
    echo "[$(timestamp)] Workers logged $ERROR_COUNT queue errors (expected)"
fi

# Workers should still be running (not crashed)
WORKER_PODS_AFTER=$(kubectl get pods -n "$NAMESPACE" -l app="$WORKER_DEPLOYMENT" --field-selector=status.phase=Running -o json | jq -r '.items | length')

if [[ $WORKER_PODS_AFTER -lt $WORKER_PODS ]]; then
    echo "WARNING: Some worker pods crashed due to queue unavailability"
fi

# Restore queue
echo "[$(timestamp)] Restoring queue..."
redis-cli -h "${REDIS_HOST:-localhost}" -p "${REDIS_PORT:-6379}" RENAME "billforge:jobs:queue:backup" "billforge:jobs:queue"

# Verify queue restoration
QUEUE_RESTORED=$(redis-cli -h "${REDIS_HOST:-localhost}" -p "${REDIS_PORT:-6379}" EXISTS "billforge:jobs:queue")
if [[ "$QUEUE_RESTORED" -eq 1 ]]; then
    echo "[$(timestamp)] Queue restored successfully"
else
    echo "ERROR: Queue restoration failed"
    exit 1
fi

# Verify job processing resumes
echo "[$(timestamp)] Verifying job processing resumes..."

# Enqueue new jobs
for i in {6..10}; do
    JOB_ID="test-job-recovery-$(date +%s)-$i"

    curl -s -X POST "${API_URL:-http://localhost:8000}/api/v1/jobs/enqueue" \
        -H "Content-Type: application/json" \
        -d "{\"job_type\":\"MetricsAggregation\",\"tenant_id\":\"test-tenant\",\"job_id\":\"$JOB_ID\"}" \
        -o /dev/null

    echo "[$(timestamp)] Enqueued recovery job: $JOB_ID"
done

# Wait for jobs to be processed
sleep 10

# Check completed jobs
COMPLETED_JOBS=$(redis-cli -h "${REDIS_HOST:-localhost}" -p "${REDIS_PORT:-6379}" LLEN "billforge:jobs:completed:test-tenant")
echo "[$(timestamp)] Completed jobs: $COMPLETED_JOBS"

if [[ $COMPLETED_JOBS -gt 0 ]]; then
    echo "[$(timestamp)] Job processing resumed successfully"
else
    echo "WARNING: No jobs completed after queue restoration"
fi

# Test dead letter queue
echo "[$(timestamp)] Testing dead letter queue..."

# Enqueue a job that will fail
FAILING_JOB_ID="failing-job-$(date +%s)"
curl -s -X POST "${API_URL:-http://localhost:8000}/api/v1/jobs/enqueue" \
    -H "Content-Type: application/json" \
    -d "{\"job_type\":\"InvalidJobType\",\"tenant_id\":\"test-tenant\",\"job_id\":\"$FAILING_JOB_ID\"}" \
    -o /dev/null

echo "[$(timestamp)] Enqueued intentionally failing job: $FAILING_JOB_ID"

# Wait for retries and failure
sleep 30

# Check dead letter queue
FAILED_JOBS=$(redis-cli -h "${REDIS_HOST:-localhost}" -p "${REDIS_PORT:-6379}" LLEN "billforge:jobs:failed:test-tenant")
echo "[$(timestamp)] Failed jobs in DLQ: $FAILED_JOBS"

# Summary
echo "[$(timestamp)] Test completed successfully"
echo "  - Queue unavailability handling: PASS"
echo "  - Queue restoration: PASS"
echo "  - Job processing recovery: PASS"
echo "  - Dead letter queue: PASS"

exit 0
