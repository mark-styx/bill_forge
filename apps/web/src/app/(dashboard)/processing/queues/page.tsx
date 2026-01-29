'use client';

import { useQuery } from '@tanstack/react-query';
import Link from 'next/link';
import { workflowsApi } from '@/lib/api';
import { Layers, Plus, Search, ChevronRight, AlertCircle, CheckCircle, Clock, CreditCard } from 'lucide-react';

const queueTypeIcons: Record<string, React.ReactNode> = {
  exception: <AlertCircle className="w-5 h-5 text-red-500" />,
  review: <Clock className="w-5 h-5 text-blue-500" />,
  approval: <CheckCircle className="w-5 h-5 text-yellow-500" />,
  payment: <CreditCard className="w-5 h-5 text-green-500" />,
  custom: <Layers className="w-5 h-5 text-purple-500" />,
};

const queueTypeColors: Record<string, string> = {
  exception: 'bg-red-100 dark:bg-red-900/30 border-red-200 dark:border-red-800',
  review: 'bg-blue-100 dark:bg-blue-900/30 border-blue-200 dark:border-blue-800',
  approval: 'bg-yellow-100 dark:bg-yellow-900/30 border-yellow-200 dark:border-yellow-800',
  payment: 'bg-green-100 dark:bg-green-900/30 border-green-200 dark:border-green-800',
  custom: 'bg-purple-100 dark:bg-purple-900/30 border-purple-200 dark:border-purple-800',
};

export default function WorkQueuesPage() {
  const { data: queues, isLoading } = useQuery({
    queryKey: ['work-queues'],
    queryFn: () => workflowsApi.listQueues(),
  });

  // Sort queues by type for a logical flow: exception -> review -> approval -> payment
  const sortedQueues = queues?.sort((a: any, b: any) => {
    const order = ['exception', 'review', 'approval', 'payment', 'custom'];
    return order.indexOf(a.queue_type) - order.indexOf(b.queue_type);
  });

  return (
    <div className="space-y-6">
      {/* Page header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-slate-900 dark:text-white">Work Queues</h1>
          <p className="text-slate-500 dark:text-slate-400">
            Manage invoice workflow and processing queues
          </p>
        </div>
        <Link
          href="/processing/queues/new"
          className="px-4 py-2 bg-processing text-white rounded-lg hover:bg-processing/90 transition-colors flex items-center space-x-2"
        >
          <Plus className="w-4 h-4" />
          <span>Create Queue</span>
        </Link>
      </div>

      {/* Queue Flow Diagram */}
      <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700 p-6">
        <h2 className="text-sm font-medium text-slate-500 dark:text-slate-400 uppercase tracking-wider mb-4">
          Queue Flow
        </h2>
        <div className="flex items-center justify-between overflow-x-auto pb-2">
          {['OCR Error', 'AP Queue', 'Pending Approval', 'Ready for Payment'].map((stage, idx) => (
            <div key={stage} className="flex items-center">
              <div className="px-4 py-2 bg-slate-100 dark:bg-slate-700 rounded-lg text-sm font-medium text-slate-700 dark:text-slate-200 whitespace-nowrap">
                {stage}
              </div>
              {idx < 3 && (
                <ChevronRight className="w-5 h-5 text-slate-400 mx-2" />
              )}
            </div>
          ))}
        </div>
      </div>

      {/* Queues Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-6">
        {isLoading ? (
          // Loading skeleton
          [...Array(4)].map((_, i) => (
            <div
              key={i}
              className="bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700 p-6 animate-pulse"
            >
              <div className="h-10 bg-slate-200 dark:bg-slate-700 rounded mb-4"></div>
              <div className="h-4 bg-slate-200 dark:bg-slate-700 rounded w-2/3 mb-2"></div>
              <div className="h-4 bg-slate-200 dark:bg-slate-700 rounded w-1/2"></div>
            </div>
          ))
        ) : !sortedQueues || sortedQueues.length === 0 ? (
          <div className="col-span-full text-center py-12">
            <Layers className="w-12 h-12 text-slate-300 dark:text-slate-600 mx-auto mb-4" />
            <p className="text-slate-500 dark:text-slate-400 mb-4">
              No work queues configured.
            </p>
            <Link
              href="/processing/queues/new"
              className="px-4 py-2 bg-processing text-white rounded-lg hover:bg-processing/90 transition-colors inline-flex items-center space-x-2"
            >
              <Plus className="w-4 h-4" />
              <span>Create your first queue</span>
            </Link>
          </div>
        ) : (
          sortedQueues.map((queue: any) => (
            <Link
              key={queue.id}
              href={`/processing/queues/${queue.id}`}
              className={`block rounded-xl border p-6 transition-all hover:shadow-lg ${queueTypeColors[queue.queue_type] || 'bg-white dark:bg-slate-800 border-slate-200 dark:border-slate-700'}`}
            >
              <div className="flex items-center justify-between mb-4">
                <div className="p-2 bg-white dark:bg-slate-800 rounded-lg shadow-sm">
                  {queueTypeIcons[queue.queue_type] || <Layers className="w-5 h-5 text-slate-500" />}
                </div>
                {queue.is_default && (
                  <span className="px-2 py-1 bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300 text-xs rounded-full">
                    Default
                  </span>
                )}
              </div>
              <h3 className="text-lg font-semibold text-slate-900 dark:text-white mb-1">
                {queue.name}
              </h3>
              <p className="text-sm text-slate-500 dark:text-slate-400 mb-4 line-clamp-2">
                {queue.description || 'No description'}
              </p>
              <div className="flex items-center justify-between text-sm">
                <span className="text-slate-500 dark:text-slate-400 capitalize">
                  {queue.queue_type} Queue
                </span>
                <ChevronRight className="w-4 h-4 text-slate-400" />
              </div>
            </Link>
          ))
        )}
      </div>

      {/* Assignment Rules Link */}
      <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700 p-6">
        <div className="flex items-center justify-between">
          <div>
            <h2 className="text-lg font-semibold text-slate-900 dark:text-white">
              Assignment Rules
            </h2>
            <p className="text-sm text-slate-500 dark:text-slate-400">
              Configure automatic invoice routing and assignment
            </p>
          </div>
          <Link
            href="/processing/assignment-rules"
            className="px-4 py-2 bg-slate-100 dark:bg-slate-700 text-slate-700 dark:text-slate-200 rounded-lg hover:bg-slate-200 dark:hover:bg-slate-600 transition-colors flex items-center space-x-2"
          >
            <span>Manage Rules</span>
            <ChevronRight className="w-4 h-4" />
          </Link>
        </div>
      </div>
    </div>
  );
}
