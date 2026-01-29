'use client';

import { useParams, useRouter } from 'next/navigation';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { workflowsApi, invoicesApi } from '@/lib/api';
import Link from 'next/link';
import {
  ArrowLeft,
  Layers,
  Settings,
  FileText,
  Clock,
  User,
  CheckCircle,
  ChevronRight,
  Hand,
  DollarSign,
  AlertCircle,
  Loader2,
  MoreVertical,
  CheckCircle2,
  XCircle,
  PauseCircle,
} from 'lucide-react';
import { toast } from 'sonner';

const queueTypeColors: Record<string, { bg: string; text: string; border: string; dot: string }> = {
  exception: { bg: 'bg-error/10', text: 'text-error', border: 'border-error/30', dot: 'bg-error' },
  review: { bg: 'bg-capture/10', text: 'text-capture', border: 'border-capture/30', dot: 'bg-capture' },
  approval: { bg: 'bg-warning/10', text: 'text-warning', border: 'border-warning/30', dot: 'bg-warning' },
  payment: { bg: 'bg-success/10', text: 'text-success', border: 'border-success/30', dot: 'bg-success' },
  custom: { bg: 'bg-vendor/10', text: 'text-vendor', border: 'border-vendor/30', dot: 'bg-vendor' },
};

export default function WorkQueueDetailPage() {
  const params = useParams();
  const router = useRouter();
  const queryClient = useQueryClient();
  const queueId = params.id as string;

  const { data: queue, isLoading: queueLoading } = useQuery({
    queryKey: ['work-queue', queueId],
    queryFn: () => workflowsApi.getQueue(queueId),
    enabled: !!queueId,
  });

  const { data: queueItems, isLoading: itemsLoading } = useQuery({
    queryKey: ['queue-items', queueId],
    queryFn: () => workflowsApi.listQueueItems(queueId),
    enabled: !!queueId,
  });

  // Get invoice details for each queue item
  const { data: invoices } = useQuery({
    queryKey: ['invoices-list'],
    queryFn: () => invoicesApi.list(),
  });

  const claimItem = useMutation({
    mutationFn: (itemId: string) => workflowsApi.claimQueueItem(queueId, itemId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['queue-items', queueId] });
      toast.success('Item claimed');
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to claim item');
    },
  });

  const completeItem = useMutation({
    mutationFn: ({ itemId, action }: { itemId: string; action: string }) =>
      workflowsApi.completeQueueItem(queueId, itemId, action),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['queue-items', queueId] });
      toast.success('Item completed');
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to complete item');
    },
  });

  if (queueLoading) {
    return (
      <div className="flex items-center justify-center py-24">
        <div className="flex items-center gap-3 text-muted-foreground">
          <Loader2 className="w-5 h-5 animate-spin" />
          Loading queue...
        </div>
      </div>
    );
  }

  if (!queue) {
    return (
      <div className="text-center py-24">
        <div className="w-16 h-16 rounded-2xl bg-processing/10 flex items-center justify-center mx-auto mb-4">
          <Layers className="w-8 h-8 text-processing" />
        </div>
        <h2 className="text-xl font-semibold text-foreground mb-2">Queue not found</h2>
        <p className="text-muted-foreground mb-4">The queue you're looking for doesn't exist</p>
        <Link href="/processing/queues" className="btn btn-primary btn-sm">
          Back to Queues
        </Link>
      </div>
    );
  }

  // Get invoice data for a queue item
  const getInvoiceForItem = (invoiceId: string) => {
    return invoices?.data?.find((inv: any) => inv.id === invoiceId);
  };

  const colors = queueTypeColors[queue.queue_type] || queueTypeColors.custom;
  const totalItems = queueItems?.length || 0;
  const unassignedItems = queueItems?.filter((item: any) => !item.assigned_to).length || 0;
  const claimedItems = queueItems?.filter((item: any) => item.claimed_at).length || 0;

  return (
    <div className="space-y-6 max-w-6xl mx-auto">
      {/* Header */}
      <div>
        <Link
          href="/processing/queues"
          className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors mb-3"
        >
          <ArrowLeft className="w-4 h-4" />
          Back to Queues
        </Link>

        <div className="flex flex-col sm:flex-row sm:items-start sm:justify-between gap-4">
          <div className="flex items-start gap-4">
            <div className={`w-12 h-12 rounded-xl ${colors.bg} flex items-center justify-center`}>
              <Layers className={`w-6 h-6 ${colors.text}`} />
            </div>
            <div>
              <div className="flex items-center gap-3">
                <h1 className="text-2xl font-semibold text-foreground">{queue.name}</h1>
                <span className={`px-2.5 py-0.5 rounded-full text-xs font-medium ${colors.bg} ${colors.text}`}>
                  {queue.queue_type}
                </span>
                {queue.is_default && (
                  <span className="px-2 py-0.5 rounded-full text-xs font-medium bg-primary/10 text-primary">
                    Default
                  </span>
                )}
              </div>
              <p className="text-muted-foreground mt-0.5">
                {queue.description || 'No description provided'}
              </p>
            </div>
          </div>

          <button className="btn btn-secondary btn-sm">
            <Settings className="w-4 h-4 mr-1.5" />
            Settings
          </button>
        </div>
      </div>

      {/* Stats */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
        <div className="card p-4">
          <div className="flex items-center gap-3 mb-2">
            <div className="p-2 rounded-lg bg-processing/10">
              <FileText className="w-4 h-4 text-processing" />
            </div>
          </div>
          <p className="text-2xl font-semibold text-foreground">{totalItems}</p>
          <p className="text-sm text-muted-foreground">Total Items</p>
        </div>

        <div className="card p-4">
          <div className="flex items-center gap-3 mb-2">
            <div className="p-2 rounded-lg bg-warning/10">
              <Clock className="w-4 h-4 text-warning" />
            </div>
          </div>
          <p className="text-2xl font-semibold text-foreground">{unassignedItems}</p>
          <p className="text-sm text-muted-foreground">Unassigned</p>
        </div>

        <div className="card p-4">
          <div className="flex items-center gap-3 mb-2">
            <div className="p-2 rounded-lg bg-primary/10">
              <User className="w-4 h-4 text-primary" />
            </div>
          </div>
          <p className="text-2xl font-semibold text-foreground">{claimedItems}</p>
          <p className="text-sm text-muted-foreground">Claimed</p>
        </div>

        <div className="card p-4">
          <div className="flex items-center gap-3 mb-2">
            <div className="p-2 rounded-lg bg-accent/10">
              <Clock className="w-4 h-4 text-accent" />
            </div>
          </div>
          <p className="text-2xl font-semibold text-foreground">
            {queue.settings?.sla_hours ? `${queue.settings.sla_hours}h` : '—'}
          </p>
          <p className="text-sm text-muted-foreground">SLA</p>
        </div>
      </div>

      {/* Queue Items */}
      <div className="card overflow-hidden">
        <div className="h-1 bg-gradient-to-r from-processing to-processing/50" />
        <div className="p-4 border-b border-border flex items-center justify-between">
          <h2 className="text-lg font-semibold text-foreground">Queue Items</h2>
          <span className="text-sm text-muted-foreground">
            {totalItems} item{totalItems !== 1 ? 's' : ''}
          </span>
        </div>

        {itemsLoading ? (
          <div className="p-12 text-center">
            <Loader2 className="w-6 h-6 text-primary animate-spin mx-auto mb-3" />
            <p className="text-muted-foreground">Loading items...</p>
          </div>
        ) : !queueItems || queueItems.length === 0 ? (
          <div className="p-12 text-center">
            <div className="w-14 h-14 rounded-2xl bg-success/10 flex items-center justify-center mx-auto mb-4">
              <CheckCircle className="w-7 h-7 text-success" />
            </div>
            <h3 className="text-lg font-semibold text-foreground mb-1">All caught up!</h3>
            <p className="text-muted-foreground">No items in this queue</p>
          </div>
        ) : (
          <div className="divide-y divide-border">
            {queueItems.map((item: any) => {
              const invoice = getInvoiceForItem(item.invoice_id);
              const isClaimed = !!item.claimed_at;

              return (
                <div
                  key={item.id}
                  className="p-4 hover:bg-secondary/30 transition-colors"
                >
                  <div className="flex items-center justify-between gap-4">
                    <div className="flex items-center gap-4 min-w-0">
                      <div className="p-2 rounded-lg bg-secondary">
                        <FileText className="w-5 h-5 text-muted-foreground" />
                      </div>
                      <div className="min-w-0">
                        <Link
                          href={`/invoices/${item.invoice_id}`}
                          className="font-medium text-foreground hover:text-primary transition-colors block truncate"
                        >
                          {invoice?.invoice_number || item.invoice_id.slice(0, 8) + '...'}
                        </Link>
                        <p className="text-sm text-muted-foreground truncate">
                          {invoice?.vendor_name || 'Unknown vendor'}
                        </p>
                      </div>
                    </div>

                    <div className="flex items-center gap-4 flex-shrink-0">
                      {invoice && (
                        <div className="text-right hidden sm:block">
                          <p className="font-semibold text-foreground">
                            ${(invoice.total_amount.amount / 100).toLocaleString()}
                          </p>
                          <p className="text-xs text-muted-foreground">
                            {new Date(item.entered_at).toLocaleDateString()}
                          </p>
                        </div>
                      )}

                      {/* Priority indicator */}
                      {item.priority > 1 && (
                        <span className="px-2 py-0.5 rounded text-xs font-medium bg-error/10 text-error">
                          High
                        </span>
                      )}

                      <div className="flex items-center gap-2">
                        {!isClaimed ? (
                          <button
                            onClick={() => claimItem.mutate(item.id)}
                            disabled={claimItem.isPending}
                            className="btn btn-sm bg-primary/10 text-primary hover:bg-primary/20"
                          >
                            {claimItem.isPending ? (
                              <Loader2 className="w-4 h-4 animate-spin" />
                            ) : (
                              <>
                                <Hand className="w-4 h-4 mr-1" />
                                Claim
                              </>
                            )}
                          </button>
                        ) : (
                          <div className="flex items-center gap-1">
                            <button
                              onClick={() => completeItem.mutate({ itemId: item.id, action: 'approve' })}
                              disabled={completeItem.isPending}
                              className="btn btn-sm bg-success/10 text-success hover:bg-success/20"
                              title="Approve"
                            >
                              <CheckCircle2 className="w-4 h-4" />
                            </button>
                            <button
                              onClick={() => completeItem.mutate({ itemId: item.id, action: 'reject' })}
                              disabled={completeItem.isPending}
                              className="btn btn-sm bg-error/10 text-error hover:bg-error/20"
                              title="Reject"
                            >
                              <XCircle className="w-4 h-4" />
                            </button>
                            <button
                              onClick={() => completeItem.mutate({ itemId: item.id, action: 'hold' })}
                              disabled={completeItem.isPending}
                              className="btn btn-sm bg-warning/10 text-warning hover:bg-warning/20"
                              title="Hold"
                            >
                              <PauseCircle className="w-4 h-4" />
                            </button>
                          </div>
                        )}

                        <Link
                          href={`/invoices/${item.invoice_id}`}
                          className="p-2 text-muted-foreground hover:text-foreground hover:bg-secondary rounded-lg transition-colors"
                        >
                          <ChevronRight className="w-4 h-4" />
                        </Link>
                      </div>
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Bottom Section */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Assignment Rules */}
        <div className="card p-6">
          <div className="flex items-center justify-between mb-4">
            <h2 className="text-lg font-semibold text-foreground">Assignment Rules</h2>
            <Link
              href="/processing/assignment-rules"
              className="text-sm text-primary hover:underline"
            >
              Manage
            </Link>
          </div>
          <p className="text-sm text-muted-foreground mb-4">
            Rules that determine how invoices are automatically assigned when they enter this queue.
          </p>
          <div className="p-4 bg-secondary/50 rounded-lg">
            <div className="flex items-center gap-3 text-muted-foreground">
              <Layers className="w-5 h-5" />
              <span className="text-sm">
                {(queue as any).assignment_rules?.length || 0} rules configured
              </span>
            </div>
          </div>
        </div>

        {/* Queue Settings Summary */}
        <div className="card p-6">
          <h2 className="text-lg font-semibold text-foreground mb-4">Queue Settings</h2>
          <div className="space-y-3">
            <div className="flex items-center justify-between py-2 border-b border-border">
              <span className="text-sm text-muted-foreground">SLA Time</span>
              <span className="text-sm font-medium text-foreground">
                {queue.settings?.sla_hours ? `${queue.settings.sla_hours} hours` : 'Not set'}
              </span>
            </div>
            <div className="flex items-center justify-between py-2 border-b border-border">
              <span className="text-sm text-muted-foreground">Escalation</span>
              <span className="text-sm font-medium text-foreground">
                {queue.settings?.escalation_hours ? `After ${queue.settings.escalation_hours}h` : 'Disabled'}
              </span>
            </div>
            <div className="flex items-center justify-between py-2 border-b border-border">
              <span className="text-sm text-muted-foreground">Default Sort</span>
              <span className="text-sm font-medium text-foreground capitalize">
                {queue.settings?.default_sort?.replace(/_/g, ' ') || 'Date entered'}
              </span>
            </div>
            <div className="flex items-center justify-between py-2">
              <span className="text-sm text-muted-foreground">Assigned Users</span>
              <span className="text-sm font-medium text-foreground">
                {queue.assigned_users?.length || 0} users
              </span>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
