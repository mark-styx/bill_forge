import { describe, it, expect, vi, beforeEach } from 'vitest';
import { routingApi } from '../api';

function mockOk(body: unknown, status = 200) {
  return vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce({
    ok: true,
    status,
    text: async () => JSON.stringify(body),
  } as Response);
}

const routingDecisionFixture = {
  approver_id: 'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee',
  strategy: 'hybrid',
  score: 0.92,
  candidates: [
    {
      user_id: 'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee',
      score: 0.92,
      workload_score: 0.8,
      expertise_score: 0.95,
      availability_score: 1.0,
      reason: 'Best overall score',
    },
  ],
  factors: {
    workload_weight: 0.4,
    expertise_weight: 0.3,
    availability_weight: 0.3,
    invoice_amount: 50000,
    vendor_id: '11111111-2222-3333-4444-555555555555',
    department: 'Engineering',
    gl_code: '6000',
  },
  delegated_from: null,
};

const workloadFixture = {
  stats: { total_approvers: 5, avg_workload: 0.45 },
  approvers: [
    {
      user_id: 'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee',
      active_approvals: 12,
      pending_approvals: 3,
      completed_this_week: 8,
      workload_score: 0.6,
    },
  ],
};

const routingConfigFixture = {
  workload_weight: 0.4,
  expertise_weight: 0.3,
  availability_weight: 0.3,
  max_workload_score: 0.85,
  min_expertise_score: 0.5,
  enable_auto_delegation: true,
  enable_pattern_learning: true,
  enable_calendar_sync: false,
  working_hours_start: '09:00:00',
  working_hours_end: '17:00:00',
  working_timezone: 'America/New_York',
  working_days: [1, 2, 3, 4, 5],
};

describe('routingApi', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('routeInvoice() calls POST /api/v1/routing/invoices/:id/route and returns decision', async () => {
    const spy = mockOk({ decision: routingDecisionFixture });

    const result = await routingApi.routeInvoice(
      'inv-001',
      { queue_id: 'qq-001' },
    );

    expect(spy).toHaveBeenCalledWith(
      expect.stringContaining('/api/v1/routing/invoices/inv-001/route'),
      expect.objectContaining({ method: 'POST' }),
    );

    // Verify request body was serialized
    const callArgs = spy.mock.calls[0][1] as RequestInit;
    expect(JSON.parse(callArgs.body as string)).toEqual({ queue_id: 'qq-001' });

    // Verify response shape
    expect(result.decision.strategy).toBe('hybrid');
    expect(result.decision.approver_id).toBe(
      'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee',
    );
    expect(result.decision.candidates).toHaveLength(1);
    expect(result.decision.factors.department).toBe('Engineering');
  });

  it('getWorkload() calls GET /api/v1/routing/workload and returns stats + approvers', async () => {
    const spy = mockOk(workloadFixture);

    const result = await routingApi.getWorkload();

    expect(spy).toHaveBeenCalledWith(
      expect.stringContaining('/api/v1/routing/workload'),
      expect.objectContaining({ method: 'GET' }),
    );

    expect(result.stats.total_approvers).toBe(5);
    expect(result.approvers).toHaveLength(1);
    expect(result.approvers[0].user_id).toBe(
      'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee',
    );
    expect(result.approvers[0].workload_score).toBe(0.6);
  });

  it('setAvailability() calls POST /api/v1/routing/availability with correct body', async () => {
    const spy = mockOk(null, 204);

    await routingApi.setAvailability({
      user_id: 'uu-001',
      status: 'out_of_office',
      start_at: '2026-05-21T00:00:00Z',
      end_at: '2026-05-23T00:00:00Z',
      reason: 'Conference',
    });

    expect(spy).toHaveBeenCalledWith(
      expect.stringContaining('/api/v1/routing/availability'),
      expect.objectContaining({ method: 'POST' }),
    );

    const callArgs = spy.mock.calls[0][1] as RequestInit;
    expect(JSON.parse(callArgs.body as string)).toEqual({
      user_id: 'uu-001',
      status: 'out_of_office',
      start_at: '2026-05-21T00:00:00Z',
      end_at: '2026-05-23T00:00:00Z',
      reason: 'Conference',
    });
  });

  it('getConfig() calls GET /api/v1/routing/config and returns RoutingConfig', async () => {
    const spy = mockOk(routingConfigFixture);

    const result = await routingApi.getConfig();

    expect(spy).toHaveBeenCalledWith(
      expect.stringContaining('/api/v1/routing/config'),
      expect.objectContaining({ method: 'GET' }),
    );

    expect(result.workload_weight).toBe(0.4);
    expect(result.enable_auto_delegation).toBe(true);
    expect(result.working_days).toEqual([1, 2, 3, 4, 5]);
    expect(result.working_timezone).toBe('America/New_York');
  });

  it('updateConfig() calls PUT /api/v1/routing/config with partial body', async () => {
    const spy = mockOk(routingConfigFixture);

    const result = await routingApi.updateConfig({
      workload_weight: 0.5,
      enable_calendar_sync: true,
    });

    expect(spy).toHaveBeenCalledWith(
      expect.stringContaining('/api/v1/routing/config'),
      expect.objectContaining({ method: 'PUT' }),
    );

    const callArgs = spy.mock.calls[0][1] as RequestInit;
    expect(JSON.parse(callArgs.body as string)).toEqual({
      workload_weight: 0.5,
      enable_calendar_sync: true,
    });

    // Response is the full config
    expect(result.working_hours_start).toBe('09:00:00');
    expect(result.expertise_weight).toBe(0.3);
  });
});
