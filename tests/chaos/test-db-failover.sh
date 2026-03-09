#!/bin/bash
#
# Database Failover Test
# Simulates PostgreSQL primary failure and verifies automatic failover
#
# This test:
# 1. Stops the primary PostgreSQL instance
# 2. Waits for automatic failover to standby
# 3. Verifies the application can still connect and query
# 4. Verifies data integrity after failover
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/lib/common.sh"

# Test configuration
TEST_NAME="database-failover"
TIMEOUT_SECONDS=120
RETRY_INTERVAL=5

echo "[$(timestamp)] Starting database failover test"

# Pre-test checks
echo "[$(timestamp)] Running pre-test checks..."

# Verify database is healthy before test
if ! pg_isready -h "${DB_HOST:-localhost}" -p "${DB_PORT:-5432}"; then
    echo "ERROR: Database is not healthy before test"
    exit 1
fi

# Get current primary
PRIMARY_HOST=$(kubectl get pods -n billforge -l app=postgres,role=primary -o jsonpath='{.items[0].metadata.name}')
if [[ -z "$PRIMARY_HOST" ]]; then
    echo "ERROR: Could not find primary PostgreSQL pod"
    exit 1
fi

echo "[$(timestamp)] Current primary: $PRIMARY_HOST"

# Record test data before failover
echo "[$(timestamp)] Recording pre-failover state..."
PRE_FAILOVER_COUNT=$(psql -h "${DB_HOST:-localhost}" -U "${DB_USER:-billforge}" -d "${DB_NAME:-billforge_control}" -t -c "SELECT COUNT(*) FROM tenants")

# Simulate primary failure
echo "[$(timestamp)] Simulating primary failure by stopping $PRIMARY_HOST..."
kubectl delete pod "$PRIMARY_HOST" -n billforge --force --grace-period=0

# Wait for failover
echo "[$(timestamp)] Waiting for automatic failover (timeout: ${TIMEOUT_SECONDS}s)..."

START_TIME=$(date +%s)
FAILOVER_COMPLETE=false

while true; do
    CURRENT_TIME=$(date +%s)
    ELAPSED=$((CURRENT_TIME - START_TIME))

    if [[ $ELAPSED -ge $TIMEOUT_SECONDS ]]; then
        echo "ERROR: Failover did not complete within ${TIMEOUT_SECONDS} seconds"
        exit 1
    fi

    # Check if new primary is ready
    NEW_PRIMARY=$(kubectl get pods -n billforge -l app=postgres,role=primary -o jsonpath='{.items[0].metadata.name}' 2>/dev/null || echo "")

    if [[ -n "$NEW_PRIMARY" && "$NEW_PRIMARY" != "$PRIMARY_HOST" ]]; then
        # Verify new primary is ready
        if kubectl exec -n billforge "$NEW_PRIMARY" -- pg_isready -U "${DB_USER:-billforge}"; then
            echo "[$(timestamp)] New primary ready: $NEW_PRIMARY"
            FAILOVER_COMPLETE=true
            break
        fi
    fi

    echo "[$(timestamp)] Waiting for failover... (${ELAPSED}s elapsed)"
    sleep $RETRY_INTERVAL
done

if [[ "$FAILOVER_COMPLETE" != "true" ]]; then
    echo "ERROR: Failover did not complete"
    exit 1
fi

# Verify application connectivity
echo "[$(timestamp)] Verifying application can connect to new primary..."

RETRY_COUNT=0
MAX_RETRIES=6

while [[ $RETRY_COUNT -lt $MAX_RETRIES ]]; do
    if pg_isready -h "${DB_HOST:-localhost}" -p "${DB_PORT:-5432}"; then
        echo "[$(timestamp)] Application can connect to database"
        break
    fi

    RETRY_COUNT=$((RETRY_COUNT + 1))
    echo "[$(timestamp)] Connection attempt $RETRY_COUNT/$MAX_RETRIES failed, retrying..."
    sleep $RETRY_INTERVAL
done

if [[ $RETRY_COUNT -eq $MAX_RETRIES ]]; then
    echo "ERROR: Application cannot connect to database after failover"
    exit 1
fi

# Verify data integrity
echo "[$(timestamp)] Verifying data integrity..."
POST_FAILOVER_COUNT=$(psql -h "${DB_HOST:-localhost}" -U "${DB_USER:-billforge}" -d "${DB_NAME:-billforge_control}" -t -c "SELECT COUNT(*) FROM tenants")

if [[ "$PRE_FAILOVER_COUNT" != "$POST_FAILOVER_COUNT" ]]; then
    echo "ERROR: Data integrity check failed: tenant count mismatch"
    echo "  Before: $PRE_FAILOVER_COUNT"
    echo "  After:  $POST_FAILOVER_COUNT"
    exit 1
fi

echo "[$(timestamp)] Data integrity verified: tenant counts match"

# Test write operation
echo "[$(timestamp)] Testing write operations..."
psql -h "${DB_HOST:-localhost}" -U "${DB_USER:-billforge}" -d "${DB_NAME:-billforge_control}" -c "UPDATE system_config SET value = 'failover-test-$(date +%s)' WHERE key = 'last_failover_test'" || {
    echo "ERROR: Write operation failed after failover"
    exit 1
}

echo "[$(timestamp)] Write operations successful"

# Cleanup
echo "[$(timestamp)] Test completed successfully"

exit 0
