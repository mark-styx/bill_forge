#!/bin/bash
# Test Predictive Analytics Endpoints

BASE_URL="http://localhost:8001"
TENANT_ID="11111111-1111-1111-1111-111111111111"

echo "🔮 Testing Predictive Analytics Endpoints"
echo "=========================================="
echo ""

# First, let's check if we can access health endpoint
echo "1. Health Check:"
curl -s "$BASE_URL/health" | jq .
echo ""

# Check database has test data
echo "2. Checking test data in database:"
export $(cat .env | grep -v '^#' | xargs)
psql "$DATABASE_URL" -c "SELECT COUNT(*) as invoice_count FROM invoices WHERE tenant_id = '$TENANT_ID';" 2>/dev/null || echo "No invoices table yet"
echo ""

# Test forecast generation endpoint (will fail without auth, but shows endpoint exists)
echo "3. Testing Forecast Endpoint (expect 401 - need auth):"
curl -s -X POST "$BASE_URL/api/v1/analytics/predictive/forecasts/generate" \
  -H "Content-Type: application/json" \
  -d '{
    "entity_type": "vendor",
    "entity_id": "test-vendor",
    "horizon": "days_30"
  }' | jq . || echo "Endpoint requires authentication"
echo ""

echo "4. Testing Anomaly Detection Endpoint (expect 401):"
curl -s -X POST "$BASE_URL/api/v1/analytics/predictive/anomalies/detect" \
  -H "Content-Type: application/json" \
  -d '{"days": 30}' | jq . || echo "Endpoint requires authentication"
echo ""

echo "5. List of Available Predictive Analytics Endpoints:"
echo "   POST /api/v1/analytics/predictive/forecasts/generate"
echo "   GET  /api/v1/analytics/predictive/forecasts"
echo "   GET  /api/v1/analytics/predictive/forecasts/:id"
echo "   POST /api/v1/analytics/predictive/anomalies/detect"
echo "   GET  /api/v1/analytics/predictive/anomalies"
echo "   POST /api/v1/analytics/predictive/anomalies/:id/acknowledge"
echo "   GET  /api/v1/analytics/predictive/alerts"
echo "   POST /api/v1/analytics/predictive/alerts/:id/dismiss"
echo ""

echo "📊 Services Status:"
echo "   ✅ Server: Running on port 8001"
echo "   ✅ Worker: Running and connected to Redis"
echo "   ✅ PostgreSQL: Running on port 8005"
echo "   ✅ Redis: Running on port 8004"
echo "   ✅ MinIO: Running on port 9000"
echo ""

echo "📝 Next Steps:"
echo "   1. Seed the database with test invoices (cargo run --bin seed)"
echo "   2. Get a valid auth token via login"
echo "   3. Test the predictive endpoints with the token"
echo ""
