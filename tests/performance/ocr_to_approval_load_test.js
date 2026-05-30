import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate, Trend } from 'k6/metrics';

// ---------------------------------------------------------------------------
// OCR → Approval Pipeline End-to-End Load Test
//
// Exercises the full invoice lifecycle under sustained load:
//   upload → OCR processing → queue appearance → approval action
//
// Thresholds:
//   - pipeline_total_time p(95) < 30s (async OCR budget, not sub-200ms API budget)
//   - Per-API-call latencies checked against sub-200ms where applicable
//     (see docs/northstar.md:57 for the API response commitment)
// ---------------------------------------------------------------------------

// Custom Trend metrics — per-stage timing
const ocrProcessingTime = new Trend('ocr_processing_time');
const queueAppearanceTime = new Trend('queue_appearance_time');
const pipelineTotalTime = new Trend('pipeline_total_time');
const uploadLatency = new Trend('upload_latency');
const queueReadLatency = new Trend('queue_read_latency');
const approveLatency = new Trend('approve_latency');

export const options = {
    scenarios: {
        // Sustained ~1 upload/sec for 3 minutes
        constant_ocr_pipeline: {
            executor: 'constant-arrival-rate',
            rate: 1,
            timeUnit: '1s',
            duration: '3m',
            preAllocatedVUs: 5,
            maxVUs: 20,
            gracefulStop: '30s',
        },
    },
    thresholds: {
        // End-to-end pipeline budget (async OCR is excluded from sub-200ms API promise)
        pipeline_total_time: ['p(95)<30000'],
        // Error rate < 1%
        http_req_failed: ['rate<0.01'],
        // Per-API-call latency checks (sub-200ms northstar commitment for API responses)
        upload_latency: ['p(95)<1500'],
        queue_read_latency: ['p(95)<200'],
        approve_latency: ['p(95)<200'],
    },
};

// Configuration — reuse same env vars as api_load_test.js
const BASE_URL = __ENV.API_URL || 'http://localhost:8000';
const API_TOKEN = __ENV.API_TOKEN || '';
const TENANT_ID = __ENV.TENANT_ID || 'test-tenant';

const headers = {
    'Authorization': `Bearer ${API_TOKEN}`,
    'Content-Type': 'application/json',
    'X-Tenant-ID': TENANT_ID,
};

// Minimal base64-encoded PDF (same payload as api_load_test.js)
const TEST_PDF = 'JVBERi0xLjQKJcfsj6IKNSAwIG9iago8PAovVHlwZSAvQ2F0YWxvZwovUGFnZXMgMiAwIFIKPj4KZW5kb2JqCjIgMCBvYmoKPDwKL1R5cGUgL1BhZ2VzCi9LaWRzIFszIDAgUl0KL0NvdW50IDEKL01lZGlhQm94IFswIDAgNjEyIDc5Ml0KPj4KZW5kb2JqCjMgMCBvYmoKPDwKL1R5cGUgL1BhZ2UKL1BhcmVudCAyIDAgUgovUmVzb3VyY2VzIDw8Pj4KL0NvbnRlbnRzIDQgMCBSCj4+CmVuZG9iago0IDAgb2JqCjw8Ci9MZW5ndGggMjMKPj4Kc3RyZWFtCkJUIFRlc3QgSW52b2ljZSBFbmRzdHJlYW0KZW5kb2JqCnhyZWYKMCA1CjAwMDAwMDAwMDAgNjU1MzUgZiAKMDAwMDAwMDAwOCAwMDAwMCBuIAowMDAwMDAwMDM0IDAwMDAwIG4gCjAwMDAwMDAxMDcgMDAwMDAgbiAKMDAwMDAwMDE3OCAwMDAwMCBuIAp0cmFpbGVyCjw8Ci9TaXplIDUKL1Jvb3QgMSAwIFIKPj4Kc3RhcnR4cmVmCjIyNQolJUVPRgo=';

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------
export function setup() {
    const healthRes = http.get(`${BASE_URL}/health`);
    if (healthRes.status !== 200) {
        throw new Error(`API health check failed: ${healthRes.status}`);
    }
    console.log('OCR-to-approval load test: API health check passed');
}

// ---------------------------------------------------------------------------
// Main iteration: upload → poll OCR → verify queue → approve
// ---------------------------------------------------------------------------
export default function () {
    const pipelineStart = Date.now();

    // Step 1: POST invoice upload
    const uploadPayload = JSON.stringify({
        file_name: `ocr_e2e_${Date.now()}_${__VU}_${__ITER}.pdf`,
        file_content: TEST_PDF,
        vendor_id: 'test-vendor-123',
    });

    const uploadRes = http.post(`${BASE_URL}/api/v1/invoices/upload`, uploadPayload, { headers });

    check(uploadRes, {
        'upload status is 201/202': (r) => r.status === 201 || r.status === 202,
        'upload returns invoice id': (r) => {
            const body = r.json();
            return body.invoice_id !== undefined || body.id !== undefined;
        },
    });

    uploadLatency.add(uploadRes.timings.duration);

    if (uploadRes.status !== 201 && uploadRes.status !== 202) {
        return; // cannot continue without an invoice
    }

    const uploadBody = uploadRes.json();
    const invoiceId = uploadBody.invoice_id || uploadBody.id;

    if (!invoiceId) {
        return;
    }

    // Step 2: Poll GET /api/v1/invoices/{id} until status is ready_for_approval or needs_review
    const ocrStart = Date.now();
    const maxWaitMs = 30000;
    const pollIntervalMs = 500;
    let finalStatus = '';
    let ocrDone = false;

    while (Date.now() - ocrStart < maxWaitMs) {
        const pollRes = http.get(`${BASE_URL}/api/v1/invoices/${invoiceId}`, { headers });

        if (pollRes.status === 200) {
            const invoice = pollRes.json();
            finalStatus = invoice.status || '';

            if (finalStatus === 'ready_for_approval' || finalStatus === 'needs_review') {
                ocrProcessingTime.add(Date.now() - ocrStart);
                ocrDone = true;
                break;
            }
        }

        sleep(pollIntervalMs / 1000);
    }

    if (!ocrDone) {
        console.log(`Invoice ${invoiceId}: OCR did not complete within ${maxWaitMs}ms (status: ${finalStatus})`);
        pipelineTotalTime.add(Date.now() - pipelineStart);
        return;
    }

    // Step 3: GET /api/v1/workflow/queue — verify the invoice appears
    const queueStart = Date.now();
    const queueRes = http.get(`${BASE_URL}/api/v1/workflow/queue`, { headers });

    check(queueRes, {
        'workflow queue status is 200': (r) => r.status === 200,
    });

    queueReadLatency.add(queueRes.timings.duration);
    queueAppearanceTime.add(Date.now() - queueStart);

    const queue = queueRes.json('queue') || [];
    const foundInQueue = queue.some((item) => item.invoice_id === invoiceId || item.id === invoiceId);

    check(queueRes, {
        'invoice appeared in workflow queue': () => foundInQueue,
    });

    // Step 4: POST approve action
    if (foundInQueue) {
        const matchItem = queue.find((item) => item.invoice_id === invoiceId || item.id === invoiceId);
        const itemId = matchItem.id || invoiceId;

        const approvePayload = JSON.stringify({
            action: 'approve',
            comments: 'OCR-to-approval load test auto-approve',
        });

        const approveRes = http.post(
            `${BASE_URL}/api/v1/workflow/queue/${itemId}/action`,
            approvePayload,
            { headers }
        );

        check(approveRes, {
            'approve action status is 200': (r) => r.status === 200,
        });

        approveLatency.add(approveRes.timings.duration);
    }

    // Record total pipeline time
    pipelineTotalTime.add(Date.now() - pipelineStart);
}

// ---------------------------------------------------------------------------
// Teardown
// ---------------------------------------------------------------------------
export function teardown() {
    console.log('OCR-to-approval load test completed');
}

// ---------------------------------------------------------------------------
// handleSummary — write JSON results alongside api_load_test results
// ---------------------------------------------------------------------------
export function handleSummary(data) {
    const resultsDir = __ENV.RESULTS_DIR || 'tests/performance/results';
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
    const jsonPath = `${resultsDir}/ocr-e2e-run-${timestamp}.json`;

    return {
        [jsonPath]: JSON.stringify(data, null, 2),
        stdout: '\n=== OCR-to-Approval Pipeline Test Summary ===\n' + summaryText(data),
    };
}

function summaryText(data) {
    const lines = [];
    const metrics = data.metrics || {};
    for (const [name, val] of Object.entries(metrics)) {
        if (val.values) {
            const p95 = val.values['p(95)'] !== undefined ? val.values['p(95)'].toFixed(2) : 'N/A';
            const avg = val.values.avg !== undefined ? val.values.avg.toFixed(2) : 'N/A';
            lines.push(`  ${name}: p(95)=${p95}ms  avg=${avg}ms`);
        }
    }
    return lines.join('\n') + '\n';
}
