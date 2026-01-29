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
  AlertCircle
} from 'lucide-react';
import { toast } from 'sonner';

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
      <div className="flex items-center justify-center py-12">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-500"></div>
      </div>
    );
  }

  if (!queue) {
    return (
      <div className="text-center py-12">
        <p className="text-slate-500">Queue not found.</p>
        <Link href="/processing/queues" className="text-blue-500 hover:underline mt-4 inline-block">
          Back to queues
        </Link>
      </div>
    );
  }

  // Get invoice data for a queue item
  const getInvoiceForItem = (invoiceId: string) => {
    return invoices?.data?.find((inv: any) => inv.id === invoiceId);
  };

  const queueTypeColors: Record<string, string> = {
    exception: 'bg-red-500',
    review: 'bg-blue-500',
    approval: 'bg-yellow-500',
    payment: 'bg-green-500',
    custom: 'bg-purple-500',
  };

  return (
    <div className="space-y-6">
      {/* Page header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center space-x-4">
          <Link
            href="/processing/queues"
            className="p-2 text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 transition-colors"
          >
            <ArrowLeft className="w-5 h-5" />
          </Link>
          <div>
            <div className="flex items-center space-x-2">
              <span className={`w-3 h-3 rounded-full ${queueTypeColors[queue.queue_type] || 'bg-slate-500'}`}></span>
              <h1 className="text-2xl font-bold text-slate-900 dark:text-white">
                {queue.name}
              </h1>
            </div>
            <p className="text-slate-500 dark:text-slate-400">
              {queue.description || 'No description'}
            </p>
          </div>
        </div>
        <button className="px-4 py-2 bg-slate-100 dark:bg-slate-700 text-slate-700 dark:text-slate-200 rounded-lg hover:bg-slate-200 dark:hover:bg-slate-600 transition-colors flex items-center space-x-2">
          <Settings className="w-4 h-4" />
          <span>Queue Settings</span>
        </button>
      </div>

      {/* Stats */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
        <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700 p-4">
          <div className="flex items-center space-x-3">
            <div className="p-2 bg-blue-100 dark:bg-blue-900/30 rounded-lg">
              <FileText className="w-5 h-5 text-blue-500" />
            </div>
            <div>
              <p className="text-sm text-slate-500 dark:text-slate-400">Total Items</p>
              <p className="text-2xl font-bold text-slate-900 dark:text-white">
                {queueItems?.length || 0}
              </p>
            </div>
          </div>
        </div>
        <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700 p-4">
          <div className="flex items-center space-x-3">
            <div className="p-2 bg-yellow-100 dark:bg-yellow-900/30 rounded-lg">
              <Clock className="w-5 h-5 text-yellow-500" />
            </div>
            <div>
              <p className="text-sm text-slate-500 dark:text-slate-400">Unassigned</p>
              <p className="text-2xl font-bold text-slate-900 dark:text-white">
                {queueItems?.filter((item: any) => !item.assigned_to).length || 0}
              </p>
            </div>
          </div>
        </div>
        <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700 p-4">
          <div className="flex items-center space-x-3">
            <div className="p-2 bg-green-100 dark:bg-green-900/30 rounded-lg">
              <User className="w-5 h-5 text-green-500" />
            </div>
            <div>
              <p className="text-sm text-slate-500 dark:text-slate-400">My Items</p>
              <p className="text-2xl font-bold text-slate-900 dark:text-white">
                {queueItems?.filter((item: any) => item.claimed_at).length || 0}
              </p>
            </div>
          </div>
        </div>
        <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700 p-4">
          <div className="flex items-center space-x-3">
            <div className="p-2 bg-purple-100 dark:bg-purple-900/30 rounded-lg">
              <Layers className="w-5 h-5 text-purple-500" />
            </div>
            <div>
              <p className="text-sm text-slate-500 dark:text-slate-400">Type</p>
              <p className="text-lg font-semibold text-slate-900 dark:text-white capitalize">
                {queue.queue_type}
              </p>
            </div>
          </div>
        </div>
      </div>

      {/* Queue Items */}
      <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700 overflow-hidden">
        <div className="p-4 border-b border-slate-200 dark:border-slate-700 flex items-center justify-between">
          <h2 className="text-lg font-semibold text-slate-900 dark:text-white">
            Queue Items
          </h2>
          <div className="flex items-center space-x-2">
            <span className="text-sm text-slate-500">
              Showing {queueItems?.length || 0} items
            </span>
          </div>
        </div>
        
        {itemsLoading ? (
          <div className="p-8 text-center">
            <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-500 mx-auto"></div>
          </div>
        ) : !queueItems || queueItems.length === 0 ? (
          <div className="p-12 text-center">
            <CheckCircle className="w-12 h-12 text-green-500 mx-auto mb-4" />
            <p className="text-slate-500 dark:text-slate-400">
              All caught up! No items in this queue.
            </p>
          </div>
        ) : (
          <div className="divide-y divide-slate-200 dark:divide-slate-700">
            {queueItems.map((item: any) => {
              const invoice = getInvoiceForItem(item.invoice_id);
              return (
                <div
                  key={item.id}
                  className="p-4 hover:bg-slate-50 dark:hover:bg-slate-700/50 transition-colors"
                >
                  <div className="flex items-center justify-between">
                    <div className="flex items-center space-x-4">
                      <div className="p-2 bg-slate-100 dark:bg-slate-700 rounded-lg">
                        <FileText className="w-5 h-5 text-slate-500" />
                      </div>
                      <div>
                        <Link
                          href={`/invoices/${item.invoice_id}`}
                          className="font-medium text-slate-900 dark:text-white hover:text-blue-500 transition-colors"
                        >
                          {invoice?.invoice_number || item.invoice_id.slice(0, 8) + '...'}
                        </Link>
                        <p className="text-sm text-slate-500 dark:text-slate-400">
                          {invoice?.vendor_name || 'Unknown vendor'}
                        </p>
                      </div>
                    </div>
                    
                    <div className="flex items-center space-x-4">
                      {invoice && (
                        <div className="text-right">
                          <p className="font-semibold text-slate-900 dark:text-white">
                            ${(invoice.total_amount.amount / 100).toLocaleString()}
                          </p>
                          <p className="text-xs text-slate-500">
                            {new Date(item.entered_at).toLocaleDateString()}
                          </p>
                        </div>
                      )}
                      
                      <div className="flex items-center space-x-2">
                        {!item.claimed_at ? (
                          <button
                            onClick={() => claimItem.mutate(item.id)}
                            disabled={claimItem.isPending}
                            className="px-3 py-1.5 bg-blue-100 text-blue-700 rounded-lg hover:bg-blue-200 transition-colors text-sm flex items-center space-x-1"
                          >
                            <Hand className="w-4 h-4" />
                            <span>Claim</span>
                          </button>
                        ) : (
                          <span className="px-3 py-1.5 bg-green-100 text-green-700 rounded-lg text-sm">
                            Claimed
                          </span>
                        )}
                        
                        <Link
                          href={`/invoices/${item.invoice_id}`}
                          className="p-2 text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 transition-colors"
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

      {/* Assignment Rules for this Queue */}
      <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700 p-6">
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-lg font-semibold text-slate-900 dark:text-white">
            Assignment Rules
          </h2>
          <Link
            href="/processing/assignment-rules"
            className="text-sm text-blue-500 hover:underline"
          >
            Manage Rules
          </Link>
        </div>
        <p className="text-sm text-slate-500 dark:text-slate-400">
          Rules that determine how invoices are automatically assigned when they enter this queue.
        </p>
      </div>
    </div>
  );
}
