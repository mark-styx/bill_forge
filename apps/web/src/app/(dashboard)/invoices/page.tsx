'use client';

import { useState, useEffect, useMemo } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import Link from 'next/link';
import { invoicesApi, workflowsApi } from '@/lib/api';
import { useAuthStore } from '@/stores/auth';
import { useStatusConfig } from '@/hooks/useStatusConfig';
import InvoicePanel from '@/components/InvoicePanel';
import { ConfidenceBadge } from '@/components/ConfidenceBadge';
import { AdvancedDataTable, ColumnDef } from '@/components/ui/advanced-data-table';
import { useInvoiceEvents } from '@/hooks/useInvoiceEvents';
import { toast } from 'sonner';
import {
  Plus,
  Download,
  FileText,
  AlertTriangle,
} from 'lucide-react';

function escapeCsvField(value: string): string {
  if (value.includes(',') || value.includes('"') || value.includes('\n')) {
    return `"${value.replace(/"/g, '""')}"`;
  }
  return value;
}

export default function InvoicesPage() {
  useInvoiceEvents();
  const queryClient = useQueryClient();
  const [page, setPage] = useState(1);
  const [search, setSearch] = useState('');
  const [statusFilter, setStatusFilter] = useState<string>('');
  const [selectedInvoiceId, setSelectedInvoiceId] = useState<string | null>(null);
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const { hasModule } = useAuthStore();
  const { getStatusDisplay, getProcessingStatuses } = useStatusConfig();
  const processingStatuses = getProcessingStatuses();

  // Reset bulk selection when page, search, or status filter changes
  useEffect(() => {
    setSelectedIds([]);
  }, [page, search, statusFilter]);

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

  const bulkApproveMutation = useMutation({
    mutationFn: (invoiceIds: string[]) =>
      workflowsApi.bulkOperation({ operation: 'approve', invoice_ids: invoiceIds }),
    onSuccess: (result) => {
      queryClient.invalidateQueries({ queryKey: ['invoices'] });
      setSelectedIds([]);
      if (result.failed === 0) {
        toast.success(`${result.successful} invoice(s) approved`);
      } else {
        toast.warning(`${result.successful} approved, ${result.failed} failed`);
      }
    },
    onError: (error: Error) => {
      toast.error(error.message || 'Bulk approve failed');
    },
  });

  const handleBulkExport = () => {
    if (selectedIds.length === 0) return;
    const selected = invoices.filter((inv: Record<string, any>) => selectedIds.includes(inv.id));
    if (selected.length === 0) return;

    const headers = ['id', 'vendor', 'invoice_number', 'amount', 'status', 'due_date'];
    const rows = selected.map((inv: Record<string, any>) => [
      inv.id,
      inv.vendor_name ?? '',
      inv.invoice_number ?? '',
      inv.total_amount ? `${(inv.total_amount.amount / 100).toFixed(2)} ${inv.total_amount.currency}` : '',
      inv.processing_status ?? '',
      inv.due_date ?? inv.invoice_date ?? '',
    ]);
    const csv = [headers.join(','), ...rows.map((r: string[]) => r.map(escapeCsvField).join(','))].join('\n');

    const blob = new Blob([csv], { type: 'text/csv;charset=utf-8;' });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = 'invoices-export.csv';
    document.body.appendChild(anchor);
    anchor.click();
    document.body.removeChild(anchor);
    URL.revokeObjectURL(url);
    setSelectedIds([]);
  };

  // Check if invoice capture module is enabled for upload functionality
  const canUpload = hasModule('invoice_capture');

  const columns: ColumnDef<Record<string, any>>[] = useMemo(() => [
    {
      id: 'invoice',
      header: 'Invoice',
      cell: (invoice) => {
        const isError = invoice.capture_status === 'failed';
        return (
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
        );
      },
    },
    {
      id: 'vendor',
      header: 'Vendor',
      cell: (invoice) => <span className="text-foreground">{invoice.vendor_name}</span>,
    },
    {
      id: 'amount',
      header: 'Amount',
      cell: (invoice) => (
        <>
          <span className="font-medium text-foreground">
            ${(invoice.total_amount.amount / 100).toLocaleString()}
          </span>
          <span className="text-muted-foreground ml-1 text-xs">
            {invoice.total_amount.currency}
          </span>
        </>
      ),
    },
    {
      id: 'status',
      header: 'Status',
      cell: (invoice) => {
        const statusDisplay = getStatusDisplay(invoice.processing_status);
        return (
          <div className="flex flex-col gap-1">
            <span className={`inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium ${statusDisplay.bg} ${statusDisplay.text}`}>
              {statusDisplay.label}
            </span>
            {invoice.ocr_confidence !== undefined &&
             invoice.ocr_confidence !== null &&
             invoice.ocr_confidence < 0.85 && (
              <ConfidenceBadge confidence={invoice.ocr_confidence} size="sm" showLabel={false} />
            )}
          </div>
        );
      },
    },
    {
      id: 'date',
      header: 'Date',
      cell: (invoice) => (
        <span className="text-muted-foreground">{invoice.invoice_date || '—'}</span>
      ),
    },
    {
      id: 'department',
      header: 'Department',
      cell: (invoice) => (
        <span className="text-muted-foreground">{invoice.department || '—'}</span>
      ),
    },
  ], [getStatusDisplay]);

  const emptyState = (
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
  );

  const toolbarActions = selectedIds.length > 0 ? (
    <div className="flex items-center gap-2 text-sm">
      <button
        className="btn btn-secondary btn-sm"
        disabled={selectedIds.length === 0 || bulkApproveMutation.isPending}
        onClick={() => {
          if (selectedIds.length === 0) return;
          bulkApproveMutation.mutate(selectedIds);
        }}
      >
        {bulkApproveMutation.isPending ? 'Loading...' : 'Bulk Approve'}
      </button>
      <button
        className="btn btn-secondary btn-sm"
        disabled={selectedIds.length === 0}
        onClick={handleBulkExport}
      >
        Bulk Export
      </button>
      <button
        className="text-sm text-muted-foreground hover:text-foreground"
        onClick={() => setSelectedIds([])}
      >
        Clear
      </button>
    </div>
  ) : null;

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

      {/* Status filter (above table) */}
      <div className="flex items-center gap-3">
        <select
          value={statusFilter}
          onChange={(e) => setStatusFilter(e.target.value)}
          className="input w-auto"
        >
          <option value="">All Statuses</option>
          {processingStatuses.map(s => (
            <option key={s.key} value={s.key}>{s.label}</option>
          ))}
        </select>
      </div>

      {/* Advanced Data Table */}
      <AdvancedDataTable
        columns={columns}
        data={invoices}
        isLoading={isLoading}
        getRowKey={(invoice) => invoice.id}
        onRowClick={(invoice) => setSelectedInvoiceId(invoice.id)}
        selectable
        selectedRows={selectedIds}
        onSelectionChange={setSelectedIds}
        searchValue={search}
        onSearchChange={setSearch}
        searchPlaceholder="Search invoices..."
        pagination={
          pagination
            ? {
                page: pagination.page,
                perPage: pagination.per_page,
                totalItems: pagination.total_items,
                totalPages: pagination.total_pages,
                onPageChange: setPage,
              }
            : undefined
        }
        toolbarActions={toolbarActions}
        emptyState={emptyState}
      />

      {/* Invoice Panel */}
      <InvoicePanel
        invoiceId={selectedInvoiceId}
        onClose={() => setSelectedInvoiceId(null)}
      />
    </div>
  );
}
