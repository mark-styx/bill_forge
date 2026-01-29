'use client';

import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import Link from 'next/link';
import { invoicesApi } from '@/lib/api';
import { useAuthStore } from '@/stores/auth';
import InvoicePanel from '@/components/InvoicePanel';
import {
  Plus,
  Search,
  Filter,
  Download,
  FileText,
  ChevronLeft,
  ChevronRight,
  AlertTriangle,
} from 'lucide-react';

const statusStyles: Record<string, { bg: string; text: string }> = {
  pending: { bg: 'bg-warning/10', text: 'text-warning' },
  processing: { bg: 'bg-primary/10', text: 'text-primary' },
  ready_for_review: { bg: 'bg-warning/10', text: 'text-warning' },
  reviewed: { bg: 'bg-success/10', text: 'text-success' },
  submitted: { bg: 'bg-primary/10', text: 'text-primary' },
  pending_approval: { bg: 'bg-warning/10', text: 'text-warning' },
  approved: { bg: 'bg-success/10', text: 'text-success' },
  rejected: { bg: 'bg-error/10', text: 'text-error' },
  on_hold: { bg: 'bg-warning/10', text: 'text-warning' },
  ready_for_payment: { bg: 'bg-success/10', text: 'text-success' },
  paid: { bg: 'bg-success/10', text: 'text-success' },
  draft: { bg: 'bg-secondary', text: 'text-muted-foreground' },
  failed: { bg: 'bg-error/10', text: 'text-error' },
};

export default function InvoicesPage() {
  const [page, setPage] = useState(1);
  const [search, setSearch] = useState('');
  const [statusFilter, setStatusFilter] = useState<string>('');
  const [selectedInvoiceId, setSelectedInvoiceId] = useState<string | null>(null);
  const { hasModule } = useAuthStore();

  const { data, isLoading } = useQuery({
    queryKey: ['invoices', page, search, statusFilter],
    queryFn: () => invoicesApi.list({ 
      page, 
      per_page: 25,
      ...(statusFilter && { processing_status: statusFilter }),
    }),
  });

  const invoices = data?.data ?? [];
  const pagination = data?.pagination;

  // Check if invoice capture module is enabled for upload functionality
  const canUpload = hasModule('invoice_capture');

  return (
    <div className="space-y-6 max-w-7xl mx-auto">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold text-foreground">Invoices</h1>
          <p className="text-muted-foreground mt-0.5">
            Manage and process your invoices
          </p>
        </div>
        <div className="flex gap-2">
          <button className="btn btn-secondary btn-sm">
            <Download className="w-4 h-4 mr-1.5" />
            Export
          </button>
          {canUpload && (
            <Link href="/invoices/upload" className="btn btn-primary btn-sm">
              <Plus className="w-4 h-4 mr-1.5" />
              Upload Invoice
            </Link>
          )}
        </div>
      </div>

      {/* Search & Filters */}
      <div className="card p-4">
        <div className="flex flex-col sm:flex-row gap-3">
          <div className="flex-1 relative">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
            <input
              type="text"
              placeholder="Search invoices..."
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="input pl-9"
            />
          </div>
          <select
            value={statusFilter}
            onChange={(e) => setStatusFilter(e.target.value)}
            className="input w-auto"
          >
            <option value="">All Statuses</option>
            <option value="draft">Draft</option>
            <option value="submitted">Submitted</option>
            <option value="pending_approval">Pending Approval</option>
            <option value="approved">Approved</option>
            <option value="rejected">Rejected</option>
            <option value="on_hold">On Hold</option>
            <option value="ready_for_payment">Ready for Payment</option>
            <option value="paid">Paid</option>
          </select>
          <button className="btn btn-secondary btn-sm">
            <Filter className="w-4 h-4 mr-1.5" />
            More Filters
          </button>
        </div>
      </div>

      {/* Table */}
      <div className="table-container bg-card">
        <table className="table">
          <thead>
            <tr>
              <th>Invoice</th>
              <th>Vendor</th>
              <th>Amount</th>
              <th>Status</th>
              <th>Date</th>
              <th>Department</th>
            </tr>
          </thead>
          <tbody>
            {isLoading ? (
              <tr>
                <td colSpan={6} className="text-center py-12 text-muted-foreground">
                  <div className="flex items-center justify-center gap-2">
                    <div className="w-4 h-4 border-2 border-primary border-t-transparent rounded-full animate-spin" />
                    Loading invoices...
                  </div>
                </td>
              </tr>
            ) : invoices.length === 0 ? (
              <tr>
                <td colSpan={6} className="text-center py-12">
                  <div className="flex flex-col items-center">
                    <div className="w-12 h-12 rounded-xl bg-secondary flex items-center justify-center mb-3">
                      <FileText className="w-6 h-6 text-muted-foreground" />
                    </div>
                    <p className="text-foreground font-medium mb-1">No invoices found</p>
                    <p className="text-sm text-muted-foreground mb-4">
                      {statusFilter ? 'Try adjusting your filters' : 'Get started by uploading your first invoice'}
                    </p>
                    {canUpload && !statusFilter && (
                      <Link href="/invoices/upload" className="btn btn-primary btn-sm">
                        <Plus className="w-4 h-4 mr-1.5" />
                        Upload Invoice
                      </Link>
                    )}
                  </div>
                </td>
              </tr>
            ) : (
              invoices.map((invoice) => {
                const status = statusStyles[invoice.processing_status] || statusStyles.draft;
                const isError = invoice.capture_status === 'failed';
                
                return (
                  <tr 
                    key={invoice.id}
                    onClick={() => setSelectedInvoiceId(invoice.id)}
                    className="cursor-pointer hover:bg-secondary/50 transition-colors"
                  >
                    <td>
                      <div className="flex items-center gap-2">
                        {isError && (
                          <AlertTriangle className="w-4 h-4 text-error flex-shrink-0" />
                        )}
                        <div>
                          <p className="font-medium text-foreground">{invoice.invoice_number}</p>
                          <p className="text-xs text-muted-foreground">
                            {invoice.id.slice(0, 8)}...
                          </p>
                        </div>
                      </div>
                    </td>
                    <td className="text-foreground">{invoice.vendor_name}</td>
                    <td>
                      <span className="font-medium text-foreground">
                        ${(invoice.total_amount.amount / 100).toLocaleString()}
                      </span>
                      <span className="text-muted-foreground ml-1 text-xs">
                        {invoice.total_amount.currency}
                      </span>
                    </td>
                    <td>
                      <span className={`inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium ${status.bg} ${status.text}`}>
                        {invoice.processing_status.replace(/_/g, ' ')}
                      </span>
                    </td>
                    <td className="text-muted-foreground">
                      {invoice.invoice_date || '—'}
                    </td>
                    <td className="text-muted-foreground">
                      {(invoice as any).department || '—'}
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>

        {/* Pagination */}
        {pagination && pagination.total_pages > 1 && (
          <div className="px-4 py-3 border-t border-border flex items-center justify-between">
            <p className="text-sm text-muted-foreground">
              Showing {((pagination.page - 1) * pagination.per_page) + 1} to {Math.min(pagination.page * pagination.per_page, pagination.total_items)} of {pagination.total_items} invoices
            </p>
            <div className="flex gap-1">
              <button
                onClick={() => setPage(page - 1)}
                disabled={page === 1}
                className="btn btn-ghost btn-sm disabled:opacity-50"
              >
                <ChevronLeft className="w-4 h-4" />
              </button>
              <button
                onClick={() => setPage(page + 1)}
                disabled={page >= pagination.total_pages}
                className="btn btn-ghost btn-sm disabled:opacity-50"
              >
                <ChevronRight className="w-4 h-4" />
              </button>
            </div>
          </div>
        )}
      </div>

      {/* Invoice Panel */}
      <InvoicePanel 
        invoiceId={selectedInvoiceId} 
        onClose={() => setSelectedInvoiceId(null)} 
      />
    </div>
  );
}
