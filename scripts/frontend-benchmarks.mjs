#!/usr/bin/env node
import { performance } from 'node:perf_hooks';

const invoiceCount = Number.parseInt(process.env.BF_BENCH_INVOICES || '5000', 10);
const iterations = Number.parseInt(process.env.BF_BENCH_ITERATIONS || '100', 10);

function makeInvoices(count) {
  const statuses = ['received', 'in_review', 'pending_approval', 'approved', 'paid', 'rejected'];
  const departments = ['Finance', 'Operations', 'Sales', 'Engineering', 'Facilities'];
  return Array.from({ length: count }, (_, index) => ({
    id: `inv-${index}`,
    status: statuses[index % statuses.length],
    department: departments[index % departments.length],
    total_amount: 100 + (index % 2500),
    due_days: index % 45,
  }));
}

function summarizeDashboard(invoices) {
  return invoices.reduce(
    (summary, invoice) => {
      summary.totalAmount += invoice.total_amount;
      summary.byStatus[invoice.status] = (summary.byStatus[invoice.status] || 0) + 1;
      summary.byDepartment[invoice.department] = (summary.byDepartment[invoice.department] || 0) + invoice.total_amount;
      if (invoice.due_days <= 7 && invoice.status !== 'paid') {
        summary.dueSoon += 1;
      }
      return summary;
    },
    { totalAmount: 0, dueSoon: 0, byStatus: {}, byDepartment: {} },
  );
}

function percentile(values, p) {
  const sorted = [...values].sort((a, b) => a - b);
  const index = Math.min(sorted.length - 1, Math.floor((p / 100) * sorted.length));
  return sorted[index];
}

const invoices = makeInvoices(invoiceCount);
const timings = [];
let checksum = 0;

for (let i = 0; i < iterations; i += 1) {
  const start = performance.now();
  const summary = summarizeDashboard(invoices);
  timings.push(performance.now() - start);
  checksum += summary.totalAmount + summary.dueSoon;
}

const total = timings.reduce((sum, value) => sum + value, 0);
const result = {
  benchmark: 'frontend-dashboard-data-shaping',
  invoices: invoiceCount,
  iterations,
  mean_ms: total / timings.length,
  p95_ms: percentile(timings, 95),
  max_ms: Math.max(...timings),
  checksum,
};

console.log(JSON.stringify(result, null, 2));
