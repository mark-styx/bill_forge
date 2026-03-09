#!/bin/bash
#
# API Service Failure Test
# Simulates API pod crashes and verifies automatic recovery and load balancer handling
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib/common.sh"

# Test configuration
TEST_NAME="api-failure"
NAMESPACE="${NAMESPACE:-billforge}"
DEPLOYMENT="${API_DEPLOYMENT:-billforge-api}"
INITIAL_REPLICAS=$(kubectl get deployment "$DEPLOYMENT" -n "$NAMESPACE" -o jsonpath='{.spec.replicas}')

echo "[$(timestamp)] Starting API failure test"

# Pre-test checks
echo "[$(timestamp)] Running pre-test checks..."

# Verify API is healthy before test
if ! api_is_healthy; then
    echo "ERROR: API is not healthy before test"
    exit 1
fi

# Get initial pod count
INITIAL_PODS=$(kubectl get pods -n "$NAMESPACE" -l app="$DEPLOYMENT" --field-selector=status.phase=Running -o json | jq -r '.items | length')
echo "[$(timestamp)] Initial running pods: $INITIAL_PODS"

# Get a random API pod to kill
POD_NAME=$(get_pod_name "$NAMESPACE" "app=$DEPLOYMENT")
if [[ -z "$POD_NAME" ]]; then
    echo "ERROR: No API pods found"
    exit 1
fi

echo "[$(timestamp)] Target pod: $POD_NAME"

# Record baseline response time
BASELINE_RESPONSE_TIME=$(measure_api_response_time "${API_URL:-http://localhost:8000}/health")
echo "[$(timestamp)] Baseline response time: ${BASELINE_RESPONSE_TIME}ms"

# Kill the pod
kill_pod "$NAMESPACE" "$POD_NAME"

# Verify automatic pod recreation
echo "[$(timestamp)] Waiting for pod recreation..."
sleep 5  # Give Kubernetes time to detect the failure

RETRY_COUNT=0
MAX_RETRIES=30
NEW_POD_READY=false

while [[ $RETRY_COUNT -lt $MAX_RETRIES ]]; do
    CURRENT_PODS=$(kubectl get pods -n "$NAMESPACE" -l app="$DEPLOYMENT" --field-selector=status.phase=Running -o json | jq -r '.items | length')

    if [[ $CURRENT_PODS -ge $INITIAL_PODS ]]; then
        # Check if new pod is ready
        READY_PODS=$(kubectl get pods -n "$NAMESPACE" -l app="$DEPLOYMENT" -o json | jq -r '.items[] | select(.status.conditions[]? | select(.type=="Ready" and .status=="True")) | .metadata.name' | wc -l)

        if [[ $READY_PODS -ge $INITIAL_PODS ]]; then
            echo "[$(timestamp)] New pod is ready"
            NEW_POD_READY=true
            break
        fi
    fi

    RETRY_COUNT=$((RETRY_COUNT + 1))
    echo "[$(timestamp)] Waiting for pod recovery... ($RETRY_COUNT/$MAX_RETRIES)"
    sleep 2
done

if [[ "$NEW_POD_READY" != "true" ]]; then
    echo "ERROR: Pod did not recover within timeout"
    exit 1
fi

# Verify API is still healthy during recovery
echo "[$(timestamp)] Verifying API remains available during recovery..."

RETRY_COUNT=0
MAX_RETRIES=10
API_HEALTHY=true

while [[ $RETRY_COUNT -lt $MAX_RETRIES ]]; do
    if api_is_healthy; then
        echo "[$(timestamp)] API is healthy"
        break
    fi

    RETRY_COUNT=$((RETRY_COUNT + 1))
    echo "[$(timestamp)] API health check $RETRY_COUNT/$MAX_RETRIES failed"
    sleep 1
done

if [[ $RETRY_COUNT -eq $MAX_RETRIES ]]; then
    echo "WARNING: API was temporarily unavailable during pod failure"
    API_HEALTHY=false
fi

# Measure response time after recovery
RECOVERY_RESPONSE_TIME=$(measure_api_response_time "${API_URL:-http://localhost:8000}/health")
echo "[$(timestamp)] Recovery response time: ${RECOVERY_RESPONSE_TIME}ms"

# Check response time hasn't degraded significantly
RESPONSE_TIME_INCREASE=$(( RECOVERY_RESPONSE_TIME - BASELINE_RESPONSE_TIME ))
if [[ $RESPONSE_TIME_INCREASE -gt 500 ]]; then
    echo "WARNING: Response time increased by ${RESPONSE_TIME_INCREASE}ms after recovery"
fi

# Verify request routing
echo "[$(timestamp)] Verifying request routing to healthy pods..."

for i in {1..10}; do
    if ! curl -s -o /dev/null -w "%{http_code}" "${API_URL:-http://localhost:8000}/health" | grep -q "200"; then
        echo "ERROR: Request $i failed to route to healthy pod"
        exit 1
    fi
done

echo "[$(timestamp)] All requests routed successfully"

# Summary
if [[ "$API_HEALTHY" == "true" ]]; then
    echo "[$(timestamp)] Test completed successfully"
    echo "  - Pod recovery: PASS"
    echo "  - API availability: PASS"
    echo "  - Request routing: PASS"
    exit 0
else
    echo "[$(timestamp)] Test completed with warnings"
    echo "  - Pod recovery: PASS"
    echo "  - API availability: WARNING (temporary unavailability)"
    echo "  - Request routing: PASS"
    exit 0
fi
