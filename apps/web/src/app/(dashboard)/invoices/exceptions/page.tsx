'use client';

import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import Link from 'next/link';
import { invoicesApi } from '@/lib/api';
import { ConfidenceBadge } from '@/components/ConfidenceBadge';
import {
  ChevronLeft,
  ChevronRight,
  FileText,
  Sliders,
} from 'lucide-react';

export default function OcrExceptionsPage() {
  const [page, setPage] = useState(1);
  const [threshold, setThreshold] = useState(0.85);
  const [appliedThreshold, setAppliedThreshold] = useState(0.85);
  const queryClient = useQueryClient();

  const queryKey = ['invoices', 'ocr-exceptions', page, appliedThreshold];

  const { data, isLoading } = useQuery({
    queryKey,
    queryFn: () =>
      invoicesApi.list({
        page,
        per_page: 25,
        max_ocr_confidence: appliedThreshold,
        ocr_exception_status: 'pending',
      }),
  });

  const resolveMutation = useMutation({
    mutationFn: ({ id, action }: { id: string; action: 'approve' | 'reject' }) =>
      invoicesApi.resolveOcrException(id, action),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['invoices', 'ocr-exceptions'] });
    },
  });

  const invoices = data?.data ?? [];
  const pagination = data?.pagination;

  const handleApply = () => {
    setPage(1);
    setAppliedThreshold(threshold);
  };

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
      <div className="table-container bg-card">
        <table className="table">
          <thead>
            <tr>
              <th>Invoice</th>
              <th>Vendor</th>
              <th>Amount</th>
              <th>Confidence</th>
              <th>Date</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {isLoading ? (
              <tr>
                <td colSpan={6} className="text-center py-12 text-muted-foreground">
                  <div className="flex items-center justify-center gap-2">
                    <div className="w-4 h-4 border-2 border-primary border-t-transparent rounded-full animate-spin" />
                    Loading exceptions...
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
                    <p className="text-foreground font-medium mb-1">
                      No exceptions found
                    </p>
                    <p className="text-sm text-muted-foreground">
                      All invoices are above the {Math.round(appliedThreshold * 100)}% confidence threshold
                    </p>
                  </div>
                </td>
              </tr>
            ) : (
              invoices.map((invoice) => (
                <tr
                  key={invoice.id}
                  className="cursor-pointer hover:bg-secondary/50 transition-colors"
                >
                  <td>
                    <Link href={`/invoices/${invoice.id}`} className="block">
                      <p className="font-medium text-foreground">
                        {invoice.invoice_number}
                      </p>
                      <p className="text-xs text-muted-foreground">
                        {invoice.id.slice(0, 8)}...
                      </p>
                    </Link>
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
                    {invoice.ocr_confidence !== undefined &&
                    invoice.ocr_confidence !== null ? (
                      <Link href={`/invoices/${invoice.id}`}>
                        <ConfidenceBadge
                          confidence={invoice.ocr_confidence}
                          size="sm"
                        />
                      </Link>
                    ) : (
                      <span className="text-muted-foreground">-</span>
                    )}
                  </td>
                  <td className="text-muted-foreground">
                    {invoice.invoice_date || '\u2014'}
                  </td>
                  <td>
                    <div className="flex gap-1">
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          resolveMutation.mutate({ id: invoice.id, action: 'approve' });
                        }}
                        disabled={resolveMutation.isPending}
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
                        disabled={resolveMutation.isPending}
                        className="btn btn-sm bg-error/10 text-error hover:bg-error/20"
                        aria-label={`Reject invoice ${invoice.invoice_number}`}
                      >
                        Reject
                      </button>
                    </div>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>

        {/* Pagination */}
        {pagination && pagination.total_pages > 1 && (
          <div className="px-4 py-3 border-t border-border flex items-center justify-between">
            <p className="text-sm text-muted-foreground">
              Showing {((pagination.page - 1) * pagination.per_page) + 1} to{' '}
              {Math.min(pagination.page * pagination.per_page, pagination.total_items)} of{' '}
              {pagination.total_items} invoices
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
    </div>
  );
}
