'use client';

import { useParams, useRouter } from 'next/navigation';
import Link from 'next/link';
import { ArrowLeft, CheckCircle, XCircle, FileText, DollarSign, Building, Calendar } from 'lucide-react';

export default function ApprovalDetailPage() {
  const params = useParams();
  const router = useRouter();
  const id = params.id as string;

  // Mock approval data
  const approval = {
    id,
    invoice_number: 'INV-2024-001',
    vendor: 'Acme Corporation',
    amount: 1500.00,
    invoice_date: '2024-01-15',
    due_date: '2024-02-15',
    requested_at: '2024-01-16',
    requested_by: 'John Smith',
    reason: 'Amount exceeds auto-approval threshold',
  };

  const handleApprove = () => {
    // In real app, would call API
    alert('Invoice approved!');
    router.push('/processing/approvals');
  };

  const handleReject = () => {
    // In real app, would call API
    alert('Invoice rejected');
    router.push('/processing/approvals');
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center space-x-4">
        <Link
          href="/processing/approvals"
          className="p-2 text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 transition-colors"
        >
          <ArrowLeft className="w-5 h-5" />
        </Link>
        <div>
          <h1 className="text-2xl font-bold text-slate-900 dark:text-white">
            Approval Request
          </h1>
          <p className="text-slate-500 dark:text-slate-400">
            Review and approve invoice {approval.invoice_number}
          </p>
        </div>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Invoice Details */}
        <div className="lg:col-span-2 bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700">
          <div className="p-6 border-b border-slate-200 dark:border-slate-700">
            <h2 className="text-lg font-semibold text-slate-900 dark:text-white">
              Invoice Details
            </h2>
          </div>
          <div className="p-6 space-y-4">
            <div className="grid grid-cols-2 gap-4">
              <div className="flex items-center space-x-3">
                <FileText className="w-5 h-5 text-slate-400" />
                <div>
                  <p className="text-sm text-slate-500">Invoice Number</p>
                  <p className="font-medium text-slate-900 dark:text-white">
                    {approval.invoice_number}
                  </p>
                </div>
              </div>
              <div className="flex items-center space-x-3">
                <Building className="w-5 h-5 text-slate-400" />
                <div>
                  <p className="text-sm text-slate-500">Vendor</p>
                  <p className="font-medium text-slate-900 dark:text-white">
                    {approval.vendor}
                  </p>
                </div>
              </div>
              <div className="flex items-center space-x-3">
                <DollarSign className="w-5 h-5 text-slate-400" />
                <div>
                  <p className="text-sm text-slate-500">Amount</p>
                  <p className="font-medium text-slate-900 dark:text-white">
                    ${approval.amount.toLocaleString()}
                  </p>
                </div>
              </div>
              <div className="flex items-center space-x-3">
                <Calendar className="w-5 h-5 text-slate-400" />
                <div>
                  <p className="text-sm text-slate-500">Due Date</p>
                  <p className="font-medium text-slate-900 dark:text-white">
                    {approval.due_date}
                  </p>
                </div>
              </div>
            </div>

            <div className="pt-4 border-t border-slate-200 dark:border-slate-700">
              <p className="text-sm text-slate-500 mb-2">Approval Reason</p>
              <p className="text-slate-900 dark:text-white">{approval.reason}</p>
            </div>
          </div>
        </div>

        {/* Action Panel */}
        <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700">
          <div className="p-6 border-b border-slate-200 dark:border-slate-700">
            <h2 className="text-lg font-semibold text-slate-900 dark:text-white">
              Your Decision
            </h2>
          </div>
          <div className="p-6 space-y-4">
            <div>
              <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">
                Comments (optional)
              </label>
              <textarea
                rows={4}
                className="w-full px-4 py-2 border border-slate-200 dark:border-slate-600 rounded-lg bg-white dark:bg-slate-700 text-slate-900 dark:text-white focus:outline-none focus:ring-2 focus:ring-processing/50"
                placeholder="Add comments..."
              />
            </div>
            <div className="flex space-x-3">
              <button
                onClick={handleReject}
                className="flex-1 px-4 py-3 bg-red-100 dark:bg-red-900/30 text-red-600 rounded-lg hover:bg-red-200 dark:hover:bg-red-900/50 transition-colors flex items-center justify-center space-x-2"
              >
                <XCircle className="w-5 h-5" />
                <span>Reject</span>
              </button>
              <button
                onClick={handleApprove}
                className="flex-1 px-4 py-3 bg-green-500 text-white rounded-lg hover:bg-green-600 transition-colors flex items-center justify-center space-x-2"
              >
                <CheckCircle className="w-5 h-5" />
                <span>Approve</span>
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
