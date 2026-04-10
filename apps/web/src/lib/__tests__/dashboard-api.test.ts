import { describe, it, expect, vi, beforeEach } from 'vitest';
import { dashboardApi } from '../api';

const dashboardMetricsFixture = {
  invoices: {
    total_invoices: 120,
    pending_ocr: 5,
    ready_for_review: 12,
    submitted: 8,
    approved: 80,
    rejected: 10,
    paid: 60,
    avg_processing_time_hours: 4.5,
    total_value: 2500000,
    this_month: 35,
    trend_vs_last_month: 12.3,
  },
  approvals: {
    pending_approvals: 8,
    approved_today: 5,
    rejected_today: 1,
    avg_approval_time_hours: 3.2,
    approval_rate: 87.5,
    escalated: 2,
    overdue: 3,
  },
  vendors: {
    total_vendors: 42,
    new_this_month: 3,
    top_vendors: [
      {
        vendor_id: 'v-001',
        vendor_name: 'Acme Corp',
        invoice_count: 28,
        total_amount: 450000,
      },
      {
        vendor_id: 'v-002',
        vendor_name: 'Globex Inc',
        invoice_count: 15,
        total_amount: 225000,
      },
    ],
    concentration_percentage: 34.2,
  },
  team: {
    members: [
      {
        user_id: 'u-001',
        user_name: 'Jane Smith',
        approvals_this_month: 22,
        rejections_this_month: 3,
        avg_response_time_hours: 2.1,
      },
    ],
    avg_approvals_per_member: 11.0,
    total_pending_actions: 7,
  },
};

function mockOk(body: unknown) {
  return vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
    ok: true,
    status: 200,
    text: async () => JSON.stringify(body),
  } as Response);
}

describe('dashboardApi', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('metrics() calls GET /api/v1/dashboard/metrics and returns parsed DashboardMetrics', async () => {
    const spy = mockOk(dashboardMetricsFixture);

    const result = await dashboardApi.metrics();

    expect(spy).toHaveBeenCalledWith(
      expect.stringContaining('/api/v1/dashboard/metrics'),
      expect.objectContaining({ method: 'GET' }),
    );

    // Type smoke tests: verify JSON keys match backend snake_case fields
    expect(result.invoices.total_invoices).toBe(120);
    expect(result.approvals.approval_rate).toBe(87.5);
    expect(result.vendors.top_vendors[0].vendor_name).toBe('Acme Corp');
    expect(result.team.members).toHaveLength(1);
    expect(result.team.members[0].user_name).toBe('Jane Smith');
  });

  it('returns the full shape expected by DashboardMetrics interface', async () => {
    mockOk(dashboardMetricsFixture);

    const result = await dashboardApi.metrics();

    // Invoice fields
    expect(result.invoices).toMatchObject({
      pending_ocr: 5,
      ready_for_review: 12,
      avg_processing_time_hours: 4.5,
      trend_vs_last_month: 12.3,
    });

    // Approval fields
    expect(result.approvals).toMatchObject({
      overdue: 3,
      avg_approval_time_hours: 3.2,
      escalated: 2,
    });

    // Vendor fields
    expect(result.vendors).toMatchObject({
      total_vendors: 42,
      concentration_percentage: 34.2,
    });
    expect(result.vendors.top_vendors[0].total_amount).toBe(450000);

    // Team fields
    expect(result.team.avg_approvals_per_member).toBe(11.0);
    expect(result.team.total_pending_actions).toBe(7);
  });
});
