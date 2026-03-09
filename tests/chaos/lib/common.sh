#!/bin/bash
#
# Common functions for chaos tests
#

# Timestamp function
timestamp() {
    date -u +"%Y-%m-%dT%H:%M:%SZ"
}

# Check if a command exists
command_exists() {
    command -v "$1" &> /dev/null
}

# Wait for condition with timeout
wait_for() {
    local description="$1"
    local timeout="$2"
    local interval="$3"
    shift 3
    local cmd=("$@")

    local start_time=$(date +%s)

    echo "[$(timestamp)] Waiting for: $description (timeout: ${timeout}s)"

    while true; do
        local current_time=$(date +%s)
        local elapsed=$((current_time - start_time))

        if [[ $elapsed -ge $timeout ]]; then
            echo "[$(timestamp)] ERROR: Timeout waiting for: $description"
            return 1
        fi

        if "${cmd[@]}"; then
            echo "[$(timestamp)] Condition met: $description (${elapsed}s)"
            return 0
        fi

        sleep "$interval"
    done
}

# Check if environment is production
assert_not_production() {
    if [[ "${ENVIRONMENT:-}" == "production" ]]; then
        echo "ERROR: This test cannot run in production!"
        exit 1
    fi
}

# Get pod name by selector
get_pod_name() {
    local namespace="$1"
    local selector="$2"

    kubectl get pods -n "$namespace" -l "$selector" -o jsonpath='{.items[0].metadata.name}'
}

# Kill pod forcefully
kill_pod() {
    local namespace="$1"
    local pod_name="$2"

    echo "[$(timestamp)] Killing pod: $pod_name in namespace: $namespace"
    kubectl delete pod "$pod_name" -n "$namespace" --force --grace-period=0
}

# Scale deployment
scale_deployment() {
    local namespace="$1"
    local deployment="$2"
    local replicas="$3"

    echo "[$(timestamp)] Scaling $deployment to $replicas replicas"
    kubectl scale deployment "$deployment" -n "$namespace" --replicas="$replicas"
}

# Add network latency using toxiproxy
add_network_latency() {
    local service="$1"
    local latency_ms="$2"

    echo "[$(timestamp)] Adding ${latency_ms}ms latency to $service"

    if command_exists toxiproxy-cli; then
        toxiproxy-cli toxic add -t latency -a "latency=${latency_ms}" "$service"
    else
        echo "WARNING: toxiproxy-cli not found, skipping latency injection"
    fi
}

# Remove network latency
remove_network_latency() {
    local service="$1"

    echo "[$(timestamp)] Removing latency from $service"

    if command_exists toxiproxy-cli; then
        toxiproxy-cli toxic remove -n latency_downstream "$service"
    fi
}

# Simulate network partition
simulate_partition() {
    local namespace="$1"
    local pod1="$2"
    local pod2="$3"

    echo "[$(timestamp)] Simulating network partition between $pod1 and $pod2"

    # Use iptables in pod to drop traffic
    kubectl exec -n "$namespace" "$pod1" -- iptables -A INPUT -s "$pod2" -j DROP
    kubectl exec -n "$namespace" "$pod2" -- iptables -A INPUT -s "$pod1" -j DROP
}

# Heal network partition
heal_partition() {
    local namespace="$1"
    local pod1="$2"
    local pod2="$3"

    echo "[$(timestamp)] Healing network partition between $pod1 and $pod2"

    kubectl exec -n "$namespace" "$pod1" -- iptables -D INPUT -s "$pod2" -j DROP || true
    kubectl exec -n "$namespace" "$pod2" -- iptables -D INPUT -s "$pod1" -j DROP || true
}

# Measure API response time
measure_api_response_time() {
    local url="$1"
    local expected_status="${2:-200}"

    local start_time=$(date +%s%N)
    local status=$(curl -s -o /dev/null -w "%{http_code}" "$url")
    local end_time=$(date +%s%N)

    local duration_ms=$(( (end_time - start_time) / 1000000 ))

    if [[ "$status" != "$expected_status" ]]; then
        echo "ERROR: Expected status $expected_status, got $status"
        return 1
    fi

    echo "$duration_ms"
}

# Check API health
api_is_healthy() {
    local url="${API_URL:-http://localhost:8000}/health"

    local status=$(curl -s -o /dev/null -w "%{http_code}" "$url")
    [[ "$status" == "200" ]]
}

# Check database is ready
db_is_ready() {
    pg_isready -h "${DB_HOST:-localhost}" -p "${DB_PORT:-5432}" -U "${DB_USER:-billforge}"
}

# Check redis is ready
redis_is_ready() {
    redis-cli -h "${REDIS_HOST:-localhost}" -p "${REDIS_PORT:-6379}" ping | grep -q PONG
}
