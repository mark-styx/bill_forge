'use client';

import { useMemo, useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import Link from 'next/link';
import { toast } from 'sonner';
import { invoicesApi } from '@/lib/api';
import type { Invoice } from '@billforge/shared-types';
import { ConfidenceBadge } from '@/components/ConfidenceBadge';
import {
  AdvancedDataTable,
  ColumnDef,
} from '@/components/ui/advanced-data-table';
import { FileText, Sliders } from 'lucide-react';

type PendingRow = { id: string; action: 'approve' | 'reject' };

export default function OcrExceptionsPage() {
  const [page, setPage] = useState(1);
  const [threshold, setThreshold] = useState(0.85);
  const [appliedThreshold, setAppliedThreshold] = useState(0.85);
  const [pendingRowId, setPendingRowId] = useState<PendingRow | null>(null);
  const queryClient = useQueryClient();

  const queryKey = ['invoices', 'ocr-exceptions', page, appliedThreshold];

  const { data, isLoading, isError, error } = useQuery({
    queryKey,
    queryFn: () =>
      invoicesApi.list({
        page,
        per_page: 25,
        max_ocr_confidence: appliedThreshold,
        ocr_exception_status: 'pending',
      }),
  });

  const errorMessage = isError
    ? error instanceof Error
      ? error.message
      : 'Failed to load OCR exceptions'
    : undefined;

  const resolveMutation = useMutation({
    mutationFn: ({ id, action }: PendingRow) =>
      invoicesApi.resolveOcrException(id, action),
    onMutate: (vars) => {
      setPendingRowId(vars);
    },
    onSuccess: (_data, vars) => {
      queryClient.invalidateQueries({ queryKey: ['invoices', 'ocr-exceptions'] });
      toast.success(`Invoice ${vars.action}d`);
    },
    onError: (err: unknown) => {
      toast.error(
        err instanceof Error ? err.message : 'Failed to resolve OCR exception',
      );
    },
    onSettled: () => {
      setPendingRowId(null);
    },
  });

  const invoices = (data?.data ?? []) as Invoice[];
  const pagination = data?.pagination;

  const handleApply = () => {
    setPage(1);
    setAppliedThreshold(threshold);
  };

  const columns: ColumnDef<Invoice>[] = useMemo(
    () => [
      {
        id: 'invoice',
        header: 'Invoice',
        cell: (invoice) => (
          <Link href={`/invoices/${invoice.id}`} className="block">
            <div className="flex items-center gap-2">
              <FileText className="w-4 h-4 text-muted-foreground flex-shrink-0" />
              <div>
                <p className="font-medium text-foreground">
                  {invoice.invoice_number}
                </p>
                <p className="text-xs text-muted-foreground">
                  {invoice.id.slice(0, 8)}...
                </p>
              </div>
            </div>
          </Link>
        ),
      },
      {
        id: 'vendor',
        header: 'Vendor',
        cell: (invoice) => (
          <span className="text-foreground">{invoice.vendor_name}</span>
        ),
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
        id: 'confidence',
        header: 'Confidence',
        cell: (invoice) =>
          invoice.ocr_confidence !== undefined &&
          invoice.ocr_confidence !== null ? (
            <Link href={`/invoices/${invoice.id}`}>
              <ConfidenceBadge confidence={invoice.ocr_confidence} size="sm" />
            </Link>
          ) : (
            <span className="text-muted-foreground">-</span>
          ),
      },
      {
        id: 'date',
        header: 'Date',
        cell: (invoice) => (
          <span className="text-muted-foreground">
            {invoice.invoice_date || '—'}
          </span>
        ),
      },
      {
        id: 'actions',
        header: 'Actions',
        cell: (invoice) => {
          const isRowPending = pendingRowId?.id === invoice.id;
          return (
            <div className="flex gap-1">
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  resolveMutation.mutate({ id: invoice.id, action: 'approve' });
                }}
                disabled={isRowPending}
                className="btn btn-sm bg-success/10 text-success hover:bg-success/20"
                aria-label={`Approve invoice ${invoice.invoice_number}`}
              >
                Approve
              </button>
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  resolveMutation.mutate({ id: invoice.id, action: 'reject' });
                }}
                disabled={isRowPending}
                className="btn btn-sm bg-error/10 text-error hover:bg-error/20"
                aria-label={`Reject invoice ${invoice.invoice_number}`}
              >
                Reject
              </button>
            </div>
          );
        },
      },
    ],
    [pendingRowId, resolveMutation],
  );

  const emptyState = (
    <div className="flex flex-col items-center">
      <div className="w-12 h-12 rounded-xl bg-secondary flex items-center justify-center mb-3">
        <FileText className="w-6 h-6 text-muted-foreground" />
      </div>
      <p className="text-foreground font-medium mb-1">No exceptions found</p>
      <p className="text-sm text-muted-foreground">
        All invoices are above the {Math.round(appliedThreshold * 100)}% confidence threshold
      </p>
    </div>
  );

  return (
    <div className="space-y-6 max-w-7xl mx-auto">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold text-foreground">
            OCR Exceptions
          </h1>
          <p className="text-muted-foreground mt-0.5">
            Review invoices with low OCR confidence scores
          </p>
        </div>
      </div>

      {/* Threshold Filter */}
      <div className="card p-4">
        <div className="flex flex-col sm:flex-row gap-3 items-start sm:items-center">
          <div className="flex items-center gap-2">
            <Sliders className="w-4 h-4 text-muted-foreground" />
            <span className="text-sm font-medium text-foreground">
              Confidence Threshold
            </span>
          </div>
          <div className="flex gap-2 items-center">
            <input
              type="number"
              min={0}
              max={1}
              step={0.01}
              value={threshold}
              onChange={(e) => setThreshold(parseFloat(e.target.value) || 0)}
              className="input w-24"
              aria-label="OCR confidence threshold"
            />
            <span className="text-sm text-muted-foreground">
              Show invoices below this confidence
            </span>
            <button onClick={handleApply} className="btn btn-primary btn-sm">
              Apply
            </button>
          </div>
        </div>
      </div>

      {/* Table */}
      <AdvancedDataTable<Invoice>
        columns={columns}
        data={invoices}
        isLoading={isLoading}
        error={errorMessage}
        getRowKey={(invoice) => invoice.id}
        emptyState={emptyState}
        showToolbar={false}
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
      />
    </div>
  );
}
