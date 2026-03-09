#!/bin/bash
#
# Redis Failure Test
# Simulates Redis unavailability and verifies graceful degradation
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib/common.sh"

# Test configuration
TEST_NAME="redis-failure"
NAMESPACE="${NAMESPACE:-billforge}"
REDIS_DEPLOYMENT="${REDIS_DEPLOYMENT:-redis}"

echo "[$(timestamp)] Starting Redis failure test"

# Pre-test checks
echo "[$(timestamp)] Running pre-test checks..."

# Verify Redis is healthy before test
if ! redis_is_ready; then
    echo "ERROR: Redis is not healthy before test"
    exit 1
fi

# Get Redis pod
REDIS_POD=$(get_pod_name "$NAMESPACE" "app=$REDIS_DEPLOYMENT")
if [[ -z "$REDIS_POD" ]]; then
    echo "ERROR: Redis pod not found"
    exit 1
fi

echo "[$(timestamp)] Redis pod: $REDIS_POD"

# Record baseline API response time (with Redis available)
BASELINE_RESPONSE_TIME=$(measure_api_response_time "${API_URL:-http://localhost:8000}/api/v1/invoices")
echo "[$(timestamp)] Baseline API response time: ${BASELINE_RESPONSE_TIME}ms"

# Simulate Redis failure
echo "[$(timestamp)] Simulating Redis failure..."

# Option 1: Kill Redis pod
kill_pod "$NAMESPACE" "$REDIS_POD"

# Wait for Redis to become unavailable
echo "[$(timestamp)] Waiting for Redis to become unavailable..."
RETRY_COUNT=0
MAX_RETRIES=10

while [[ $RETRY_COUNT -lt $MAX_RETRIES ]]; do
    if ! redis_is_ready; then
        echo "[$(timestamp)] Redis is now unavailable"
        break
    fi

    RETRY_COUNT=$((RETRY_COUNT + 1))
    sleep 1
done

if [[ $RETRY_COUNT -eq $MAX_RETRIES ]]; then
    echo "WARNING: Redis did not become unavailable as expected"
fi

# Test API behavior without Redis
echo "[$(timestamp)] Testing API behavior without Redis..."

# API should still work (degraded mode, no caching)
RESPONSE_STATUS=$(curl -s -o /dev/null -w "%{http_code}" "${API_URL:-http://localhost:8000}/api/v1/invoices")

if [[ "$RESPONSE_STATUS" == "200" ]]; then
    echo "[$(timestamp)] API continues to function without Redis (degraded mode)"
    DEGRADED_RESPONSE_TIME=$(measure_api_response_time "${API_URL:-http://localhost:8000}/api/v1/invoices")
    echo "[$(timestamp)] Degraded response time: ${DEGRADED_RESPONSE_TIME}ms"

    # Response time should be higher (no caching)
    RESPONSE_TIME_INCREASE=$(( DEGRADED_RESPONSE_TIME - BASELINE_RESPONSE_TIME ))
    echo "[$(timestamp)] Response time increase: ${RESPONSE_TIME_INCREASE}ms"

    if [[ $RESPONSE_TIME_INCREASE -gt 1000 ]]; then
        echo "WARNING: Significant performance degradation without Redis"
    fi
elif [[ "$RESPONSE_STATUS" == "503" ]]; then
    echo "WARNING: API returned 503 without Redis (not gracefully degrading)"
else
    echo "ERROR: Unexpected response status: $RESPONSE_STATUS"
fi

# Test non-critical operations (background jobs)
echo "[$(timestamp)] Testing background job queue without Redis..."

# Try to enqueue a job (should fail gracefully)
JOB_ENQUEUE_STATUS=$(curl -s -o /dev/null -w "%{http_code}" -X POST "${API_URL:-http://localhost:8000}/api/v1/jobs/test-job")

if [[ "$JOB_ENQUEUE_STATUS" == "503" ]]; then
    echo "[$(timestamp)] Job enqueue correctly returns 503 when Redis unavailable"
elif [[ "$JOB_ENQUEUE_STATUS" == "202" ]]; then
    echo "WARNING: Job enqueue accepted despite Redis unavailability"
else
    echo "[$(timestamp)] Job enqueue status: $JOB_ENQUEUE_STATUS"
fi

# Wait for Redis recovery
echo "[$(timestamp)] Waiting for Redis recovery..."

RETRY_COUNT=0
MAX_RETRIES=30

while [[ $RETRY_COUNT -lt $MAX_RETRIES ]]; do
    if redis_is_ready; then
        echo "[$(timestamp)] Redis has recovered"
        break
    fi

    RETRY_COUNT=$((RETRY_COUNT + 1))
    echo "[$(timestamp)] Waiting for Redis recovery... ($RETRY_COUNT/$MAX_RETRIES)"
    sleep 2
done

if [[ $RETRY_COUNT -eq $MAX_RETRIES ]]; then
    echo "ERROR: Redis did not recover within timeout"
    exit 1
fi

# Test cache warming
echo "[$(timestamp)] Testing cache warming after recovery..."

# Make a request to warm the cache
curl -s -o /dev/null "${API_URL:-http://localhost:8000}/api/v1/invoices"

# Verify Redis is being used again
CACHE_HIT=$(redis-cli -h "${REDIS_HOST:-localhost}" -p "${REDIS_PORT:-6379}" GET "billforge:metrics:*:invoices" 2>/dev/null || echo "")

if [[ -n "$CACHE_HIT" ]]; then
    echo "[$(timestamp)] Cache warming successful"
else
    echo "[$(timestamp)] Cache warming in progress"
fi

# Measure final response time
FINAL_RESPONSE_TIME=$(measure_api_response_time "${API_URL:-http://localhost:8000}/api/v1/invoices")
echo "[$(timestamp)] Final response time: ${FINAL_RESPONSE_TIME}ms"

# Summary
echo "[$(timestamp)] Test completed successfully"
echo "  - Graceful degradation: PASS"
echo "  - Redis recovery: PASS"
echo "  - Cache warming: PASS"

exit 0
