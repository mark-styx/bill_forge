import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate, Trend, Counter } from 'k6/metrics';

// Custom metrics
const errorRate = new Rate('errors');
const apiLatency = new Trend('api_latency');
const invoiceUploadTime = new Trend('invoice_upload_time');
const approvalTime = new Trend('approval_time');
const dashboardLoadTime = new Trend('dashboard_load_time');

// ---------------------------------------------------------------------------
// Scenario helpers - shared thresholds applied across all scenarios
// ---------------------------------------------------------------------------
const sharedThresholds = {
    // Overall error rate must be < 1%
    errors: ['rate<0.01'],
    // P95 read latency < 300ms, P99 < 800ms
    api_latency: ['p(95)<300', 'p(99)<800'],
    // Invoice upload P95 < 1.5s
    invoice_upload_time: ['p(95)<1500'],
    // Approval actions P95 < 300ms
    approval_time: ['p(95)<300'],
    // Dashboard load P95 < 800ms
    dashboard_load_time: ['p(95)<800'],
    // HTTP requests
    http_req_duration: ['p(95)<500', 'p(99)<1000'],
    http_req_failed: ['rate<0.01'],
};

// ---------------------------------------------------------------------------
// 5,000 invoices/month throughput model
//
// 5000 / 30 days / 8 business hours / 60 min = ~0.35 uploads/sec sustained
// With 3x peak multiplier = ~1.04 uploads/sec
// Correlated read traffic (list/search) ~3x upload rate = ~3.1 reads/sec
// Approval traffic ~0.5x upload rate = ~0.5/sec
// ---------------------------------------------------------------------------
const TARGET_UPLOAD_RPS = 1.04;   // 3x peak multiplier of sustained rate
const TARGET_READ_RPS = 3.1;      // correlated read traffic
const TARGET_APPROVAL_RPS = 0.5;  // approval actions

// ---------------------------------------------------------------------------
// Scenarios
// ---------------------------------------------------------------------------
// Default: run the 5K/month sustained throughput model.
// Override with: k6 run -e SCENARIO=ocr_stress  or  --scenarioName
// ---------------------------------------------------------------------------
const scenarios = {
    // Sustained 5K/month throughput using arrival-rate (not VU-based)
    monthly_5k_throughput: {
        executor: 'constant-arrival-rate',
        rate: TARGET_UPLOAD_RPS + TARGET_READ_RPS + TARGET_APPROVAL_RPS,
        timeUnit: '1s',
        duration: '10m',
        preAllocatedVUs: 20,
        maxVUs: 50,
        gracefulStop: '30s',
        exec: 'sustainedTraffic',
    },
    // Stress test: hammer upload→OCR path at 2x peak to find ceiling
    ocr_pipeline_stress: {
        executor: 'constant-arrival-rate',
        rate: TARGET_UPLOAD_RPS * 2,   // ~2.08 uploads/sec
        timeUnit: '1s',
        duration: '5m',
        preAllocatedVUs: 10,
        maxVUs: 40,
        gracefulStop: '30s',
        exec: 'ocrPipelineStress',
    },
    // Legacy VU-ramp scenario (kept for backward compat with run-performance-tests.sh)
    vu_ramp: {
        executor: 'ramping-vus',
        startVUs: 0,
        stages: [
            { duration: '1m', target: 50 },
            { duration: '3m', target: 50 },
            { duration: '1m', target: 100 },
            { duration: '5m', target: 100 },
            { duration: '2m', target: 200 },
            { duration: '5m', target: 200 },
            { duration: '1m', target: 0 },
        ],
        gracefulRampDown: '30s',
    },
};

// Select scenario via environment variable; default runs all
const scenarioName = __ENV.SCENARIO || '';

export let options = scenarioName
    ? { scenarios: { [scenarioName]: scenarios[scenarioName] }, thresholds: sharedThresholds }
    : { scenarios, thresholds: sharedThresholds };

// Configuration from environment
const BASE_URL = __ENV.API_URL || 'http://localhost:8000';
const API_TOKEN = __ENV.API_TOKEN || '';
const TENANT_ID = __ENV.TENANT_ID || 'test-tenant';

// Default headers
const headers = {
    'Authorization': `Bearer ${API_TOKEN}`,
    'Content-Type': 'application/json',
    'X-Tenant-ID': TENANT_ID,
};

// ---------------------------------------------------------------------------
// Scenario entry points
// ---------------------------------------------------------------------------

// Sustained 5K/month traffic mix (arrival-rate driven)
export function sustainedTraffic() {
    const roll = Math.random();
    const uploadShare = TARGET_UPLOAD_RPS / (TARGET_UPLOAD_RPS + TARGET_READ_RPS + TARGET_APPROVAL_RPS);
    const readShare = TARGET_READ_RPS / (TARGET_UPLOAD_RPS + TARGET_READ_RPS + TARGET_APPROVAL_RPS);

    if (roll < uploadShare) {
        testInvoiceUpload();
    } else if (roll < uploadShare + readShare) {
        testInvoiceList();
    } else {
        testApprovalWorkflow();
    }
}

// OCR pipeline stress - uploads only at 2x peak rate
export function ocrPipelineStress() {
    testInvoiceUpload();
}

// ---------------------------------------------------------------------------
// Setup / Teardown / Default
// ---------------------------------------------------------------------------

// Setup function (runs once per VU)
export function setup() {
    // Verify API is accessible
    const healthRes = http.get(`${BASE_URL}/health`);
    if (healthRes.status !== 200) {
        throw new Error(`API health check failed: ${healthRes.status}`);
    }

    console.log('API health check passed, starting load test');
}

// Teardown function (runs once after all VUs finish)
export function teardown() {
    console.log('Load test completed');
}

// Main test function (runs in a loop for each VU) - used by vu_ramp scenario
export default function () {
    // Randomly select a test scenario
    const scenario = Math.random();

    if (scenario < 0.3) {
        // 30% - Invoice list and search
        testInvoiceList();
    } else if (scenario < 0.5) {
        // 20% - Invoice upload
        testInvoiceUpload();
    } else if (scenario < 0.7) {
        // 20% - Approval workflow
        testApprovalWorkflow();
    } else if (scenario < 0.85) {
        // 15% - Dashboard metrics
        testDashboardMetrics();
    } else {
        // 15% - Vendor operations
        testVendorOperations();
    }

    // Think time between requests (1-3 seconds)
    sleep(Math.random() * 2 + 1);
}

// ---------------------------------------------------------------------------
// handleSummary - write JSON results for baseline comparison
// ---------------------------------------------------------------------------
export function handleSummary(data) {
    const resultsDir = __ENV.RESULTS_DIR || 'tests/performance/results';
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
    const jsonPath = `${resultsDir}/run-${timestamp}.json`;

    return {
        [jsonPath]: JSON.stringify(data, null, 2),
        stdout: textSummary(data, { indent: ' ', enableColors: true }),
    };
}

function textSummary(data, opts) {
    // Minimal plaintext summary (k6 ships its own but handleSummary needs raw text)
    const lines = [];
    lines.push('\n=== Load Test Summary ===\n');

    for (const [name, scenario] of Object.entries(data.scenarios || {})) {
        lines.push(`Scenario: ${name}`);
        lines.push(`  Duration: ${scenario.state?.testRunDurationMs || 'N/A'}ms`);
    }

    const metrics = data.metrics || {};
    for (const [metric, val] of Object.entries(metrics)) {
        if (val.values) {
            lines.push(`Metric: ${metric}  p(95)=${val.values['p(95)'] !== undefined ? val.values['p(95)'].toFixed(2) : 'N/A'}  avg=${val.values.avg !== undefined ? val.values.avg.toFixed(2) : 'N/A'}`);
        }
    }

    lines.push('\nResults written to: ' + (data.resultsPath || 'N/A'));
    return lines.join('\n');
}

// Test: Invoice list and search
function testInvoiceList() {
    // Get invoice list
    let res = http.get(`${BASE_URL}/api/v1/invoices?limit=20&offset=0`, { headers });

    check(res, {
        'invoice list status is 200': (r) => r.status === 200,
        'invoice list has data': (r) => r.json('invoices') !== undefined,
    });

    apiLatency.add(res.timings.duration);
    errorRate.add(res.status !== 200);

    // Search invoices
    res = http.get(`${BASE_URL}/api/v1/invoices?search=test&limit=10`, { headers });

    check(res, {
        'invoice search status is 200': (r) => r.status === 200,
    });

    apiLatency.add(res.timings.duration);
}

// Test: Invoice upload
function testInvoiceUpload() {
    // Create a test PDF (base64 encoded minimal PDF)
    const testPdf = 'JVBERi0xLjQKJcfsj6IKNSAwIG9iago8PAovVHlwZSAvQ2F0YWxvZwovUGFnZXMgMiAwIFIKPj4KZW5kb2JqCjIgMCBvYmoKPDwKL1R5cGUgL1BhZ2VzCi9LaWRzIFszIDAgUl0KL0NvdW50IDEKL01lZGlhQm94IFswIDAgNjEyIDc5Ml0KPj4KZW5kb2JqCjMgMCBvYmoKPDwKL1R5cGUgL1BhZ2UKL1BhcmVudCAyIDAgUgovUmVzb3VyY2VzIDw8Pj4KL0NvbnRlbnRzIDQgMCBSCj4+CmVuZG9iago0IDAgb2JqCjw8Ci9MZW5ndGggMjMKPj4Kc3RyZWFtCkJUIFRlc3QgSW52b2ljZSBFbmRzdHJlYW0KZW5kb2JqCnhyZWYKMCA1CjAwMDAwMDAwMDAgNjU1MzUgZiAKMDAwMDAwMDAwOCAwMDAwMCBuIAowMDAwMDAwMDM0IDAwMDAwIG4gCjAwMDAwMDAxMDcgMDAwMDAgbiAKMDAwMDAwMDE3OCAwMDAwMCBuIAp0cmFpbGVyCjw8Ci9TaXplIDUKL1Jvb3QgMSAwIFIKPj4Kc3RhcnR4cmVmCjIyNQolJUVPRgo=';

    const payload = JSON.stringify({
        file_name: `test_invoice_${Date.now()}.pdf`,
        file_content: testPdf,
        vendor_id: 'test-vendor-123',
    });

    const res = http.post(`${BASE_URL}/api/v1/invoices/upload`, payload, { headers });

    check(res, {
        'invoice upload status is 201': (r) => r.status === 201 || r.status === 202,
        'invoice upload returns id': (r) => r.json('invoice_id') !== undefined,
    });

    invoiceUploadTime.add(res.timings.duration);
    errorRate.add(res.status !== 201 && res.status !== 202);
}

// Test: Approval workflow
function testApprovalWorkflow() {
    // Get pending approvals
    let res = http.get(`${BASE_URL}/api/v1/approvals/pending`, { headers });

    check(res, {
        'pending approvals status is 200': (r) => r.status === 200,
    });

    apiLatency.add(res.timings.duration);

    // Get approval queue
    res = http.get(`${BASE_URL}/api/v1/workflow/queue`, { headers });

    check(res, {
        'approval queue status is 200': (r) => r.status === 200,
    });

    // If there are pending items, try to approve one
    const queue = res.json('queue');
    if (queue && queue.length > 0) {
        const item = queue[0];
        const approvePayload = JSON.stringify({
            action: 'approve',
            comments: 'Performance test approval',
        });

        const approveRes = http.post(
            `${BASE_URL}/api/v1/workflow/queue/${item.id}/action`,
            approvePayload,
            { headers }
        );

        check(approveRes, {
            'approval action status is 200': (r) => r.status === 200,
        });

        approvalTime.add(approveRes.timings.duration);
    }
}

// Test: Dashboard metrics
function testDashboardMetrics() {
    const res = http.get(`${BASE_URL}/api/v1/metrics/dashboard`, { headers });

    check(res, {
        'dashboard metrics status is 200': (r) => r.status === 200,
        'dashboard has invoice metrics': (r) => r.json('invoice_metrics') !== undefined,
        'dashboard has approval metrics': (r) => r.json('approval_metrics') !== undefined,
        'dashboard has vendor metrics': (r) => r.json('vendor_metrics') !== undefined,
    });

    dashboardLoadTime.add(res.timings.duration);
    errorRate.add(res.status !== 200);
}

// Test: Vendor operations
function testVendorOperations() {
    // Get vendor list
    let res = http.get(`${BASE_URL}/api/v1/vendors?limit=20`, { headers });

    check(res, {
        'vendor list status is 200': (r) => r.status === 200,
        'vendor list has data': (r) => r.json('vendors') !== undefined,
    });

    apiLatency.add(res.timings.duration);
    errorRate.add(res.status !== 200);

    // Get vendor metrics
    res = http.get(`${BASE_URL}/api/v1/vendors/metrics`, { headers });

    check(res, {
        'vendor metrics status is 200': (r) => r.status === 200,
    });

    apiLatency.add(res.timings.duration);
}
