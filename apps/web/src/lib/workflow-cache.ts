import type { QueryClient } from '@tanstack/react-query';

export function invalidateInvoiceWorkflowQueries(queryClient: QueryClient, invoiceId: string) {
  queryClient.invalidateQueries({ queryKey: ['invoice', invoiceId] });
  queryClient.invalidateQueries({ queryKey: ['invoices'] });
  queryClient.invalidateQueries({ queryKey: ['pending-approvals'] });
  queryClient.invalidateQueries({ queryKey: ['work-queues'] });
  queryClient.invalidateQueries({ queryKey: ['dashboard-summary'] });
  queryClient.invalidateQueries({ queryKey: ['dashboard-metrics'] });
  queryClient.invalidateQueries({ queryKey: ['dashboard-kpis'] });
}
