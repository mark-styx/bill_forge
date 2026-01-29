'use client';

import { useQuery } from '@tanstack/react-query';
import Link from 'next/link';
import { workflowsApi } from '@/lib/api';
import { ArrowLeft, CheckCircle, XCircle, Clock } from 'lucide-react';

export default function ApprovalsPage() {
  const { data: approvals, isLoading } = useQuery({
    queryKey: ['pending-approvals'],
    queryFn: () => workflowsApi.listPendingApprovals(),
  });

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center space-x-4">
        <Link
          href="/processing"
          className="p-2 text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 transition-colors"
        >
          <ArrowLeft className="w-5 h-5" />
        </Link>
        <div>
          <h1 className="text-2xl font-bold text-slate-900 dark:text-white">
            Pending Approvals
          </h1>
          <p className="text-slate-500 dark:text-slate-400">
            Review and approve invoices
          </p>
        </div>
      </div>

      {/* Approvals List */}
      <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700">
        {isLoading ? (
          <div className="p-12 text-center text-slate-500">Loading...</div>
        ) : !approvals || approvals.length === 0 ? (
          <div className="p-12 text-center">
            <CheckCircle className="w-16 h-16 text-green-500 mx-auto mb-4" />
            <h3 className="text-lg font-semibold text-slate-900 dark:text-white mb-2">
              All Caught Up!
            </h3>
            <p className="text-slate-500 dark:text-slate-400">
              There are no invoices pending your approval.
            </p>
          </div>
        ) : (
          <div className="divide-y divide-slate-200 dark:divide-slate-700">
            {approvals.map((approval: any) => (
              <div
                key={approval.id}
                className="p-6 flex items-center justify-between hover:bg-slate-50 dark:hover:bg-slate-700/50 transition-colors"
              >
                <div className="flex items-center space-x-4">
                  <div className="p-3 bg-yellow-100 dark:bg-yellow-900/30 rounded-lg">
                    <Clock className="w-5 h-5 text-yellow-600" />
                  </div>
                  <div>
                    <p className="font-semibold text-slate-900 dark:text-white">
                      Invoice #{approval.invoice_id.slice(0, 8)}
                    </p>
                    <p className="text-sm text-slate-500">
                      Requested {new Date(approval.created_at).toLocaleDateString()}
                    </p>
                  </div>
                </div>
                <div className="flex space-x-3">
                  <button className="px-4 py-2 bg-red-100 dark:bg-red-900/30 text-red-600 rounded-lg hover:bg-red-200 dark:hover:bg-red-900/50 transition-colors flex items-center space-x-2">
                    <XCircle className="w-4 h-4" />
                    <span>Reject</span>
                  </button>
                  <button className="px-4 py-2 bg-green-500 text-white rounded-lg hover:bg-green-600 transition-colors flex items-center space-x-2">
                    <CheckCircle className="w-4 h-4" />
                    <span>Approve</span>
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
