import { describe, expect, it, vi } from 'vitest';
import { invalidateInvoiceWorkflowQueries } from '@/lib/workflow-cache';

describe('invalidateInvoiceWorkflowQueries', () => {
  it('invalidates invoice workflow surfaces that can become stale after mutations', () => {
    const queryClient = {
      invalidateQueries: vi.fn(),
    };

    invalidateInvoiceWorkflowQueries(queryClient as any, 'inv-123');

    expect(queryClient.invalidateQueries).toHaveBeenCalledWith({ queryKey: ['invoice', 'inv-123'] });
    expect(queryClient.invalidateQueries).toHaveBeenCalledWith({ queryKey: ['invoices'] });
    expect(queryClient.invalidateQueries).toHaveBeenCalledWith({ queryKey: ['pending-approvals'] });
    expect(queryClient.invalidateQueries).toHaveBeenCalledWith({ queryKey: ['work-queues'] });
    expect(queryClient.invalidateQueries).toHaveBeenCalledWith({ queryKey: ['dashboard-summary'] });
    expect(queryClient.invalidateQueries).toHaveBeenCalledWith({ queryKey: ['dashboard-metrics'] });
    expect(queryClient.invalidateQueries).toHaveBeenCalledWith({ queryKey: ['dashboard-kpis'] });
  });
});
