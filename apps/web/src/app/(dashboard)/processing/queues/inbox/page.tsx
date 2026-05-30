'use client';

import { useQuery, useQueryClient } from '@tanstack/react-query';
import { workflowsApi, InboxItem } from '@/lib/api';
import { useRouter } from 'next/navigation';
import { useState, useEffect, useCallback } from 'react';
import {
  Inbox,
  ChevronRight,
  CheckCircle,
  XCircle,
  UserPlus,
  ExternalLink,
  HelpCircle,
  Loader2,
  Clock,
} from 'lucide-react';

const queueTypeColors: Record<string, string> = {
  exception: 'bg-red-100 text-red-700',
  review: 'bg-blue-100 text-blue-700',
  approval: 'bg-yellow-100 text-yellow-700',
  payment: 'bg-green-100 text-green-700',
  custom: 'bg-purple-100 text-purple-700',
  ocr_error: 'bg-orange-100 text-orange-700',
};

function formatAge(enteredAt: string): string {
  const entered = new Date(enteredAt);
  const now = new Date();
  const diffMs = now.getTime() - entered.getTime();
  const diffMin = Math.floor(diffMs / 60000);
  if (diffMin < 60) return `${diffMin}m ago`;
  const diffHrs = Math.floor(diffMin / 60);
  if (diffHrs < 24) return `${diffHrs}h ago`;
  const diffDays = Math.floor(diffHrs / 24);
  return `${diffDays}d ago`;
}

function formatAmount(cents?: number, currency?: string): string {
  if (cents == null) return '--';
  const cur = currency ?? 'USD';
  return new Intl.NumberFormat('en-US', { style: 'currency', currency: cur }).format(cents / 100);
}

export default function InboxPage() {
  const router = useRouter();
  const queryClient = useQueryClient();
  const [focusedIndex, setFocusedIndex] = useState(0);
  const [showHelp, setShowHelp] = useState(false);

  const { data: result, isLoading } = useQuery({
    queryKey: ['inbox'],
    queryFn: () => workflowsApi.listInboxItems({ per_page: 100 }),
  });

  const inboxItems: InboxItem[] = result?.data ?? [];
  const totalCount = result?.pagination.total_items ?? 0;

  const advanceFocus = useCallback(() => {
    setFocusedIndex((prev) => {
      if (inboxItems.length <= 1) return 0;
      return prev >= inboxItems.length - 1 ? prev - 1 : prev;
    });
  }, [inboxItems.length]);

  const focusedItem = inboxItems[focusedIndex] as InboxItem | undefined;

  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      const tag = (document.activeElement as HTMLElement)?.tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA') return;

      switch (e.key) {
        case 'j':
        case 'ArrowDown':
          e.preventDefault();
          setFocusedIndex((prev) => Math.min(prev + 1, inboxItems.length - 1));
          break;
        case 'k':
        case 'ArrowUp':
          e.preventDefault();
          setFocusedIndex((prev) => Math.max(prev - 1, 0));
          break;
        case 'o':
        case 'Enter':
          if (focusedItem) {
            e.preventDefault();
            router.push(`/processing/invoices/${focusedItem.invoice_id}`);
          }
          break;
        case 'a':
          if (focusedItem) {
            e.preventDefault();
            workflowsApi
              .completeQueueItem(focusedItem.queue_id, focusedItem.id, 'approve')
              .then(() => queryClient.invalidateQueries({ queryKey: ['inbox'] }))
              .then(() => advanceFocus());
          }
          break;
        case 'r':
          if (focusedItem) {
            e.preventDefault();
            workflowsApi
              .completeQueueItem(focusedItem.queue_id, focusedItem.id, 'reject')
              .then(() => queryClient.invalidateQueries({ queryKey: ['inbox'] }))
              .then(() => advanceFocus());
          }
          break;
        case 'c':
          if (focusedItem) {
            e.preventDefault();
            workflowsApi
              .claimQueueItem(focusedItem.queue_id, focusedItem.id)
              .then(() => queryClient.invalidateQueries({ queryKey: ['inbox'] }));
          }
          break;
        case '?':
          e.preventDefault();
          setShowHelp((prev) => !prev);
          break;
      }
    }

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [focusedItem, inboxItems.length, router, queryClient, advanceFocus]);

  return (
    <div className="space-y-6 max-w-7xl mx-auto">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="p-2.5 rounded-xl bg-primary/10">
            <Inbox className="w-6 h-6 text-primary" />
          </div>
          <div>
            <h1 className="text-2xl font-semibold text-foreground">
              My Inbox
              {totalCount > 0 && (
                <span className="ml-2 px-2 py-0.5 text-sm font-medium bg-primary/10 text-primary rounded-full">
                  {totalCount}
                </span>
              )}
            </h1>
            <p className="text-muted-foreground text-sm">
              All queue items assigned to you across all queues
            </p>
          </div>
        </div>
        <button
          onClick={() => setShowHelp((prev) => !prev)}
          className="btn btn-secondary btn-sm"
        >
          <HelpCircle className="w-4 h-4 mr-1.5" />
          Shortcuts
        </button>
      </div>

      {/* Shortcuts cheatsheet */}
      {showHelp && (
        <div className="card p-4 bg-muted/50 border border-border">
          <h3 className="text-sm font-semibold text-foreground mb-2">Keyboard Shortcuts</h3>
          <div className="grid grid-cols-2 sm:grid-cols-3 gap-2 text-sm">
            {[
              ['j / ↓', 'Next item'],
              ['k / ↑', 'Previous item'],
              ['o / Enter', 'Open invoice'],
              ['a', 'Approve'],
              ['r', 'Reject'],
              ['c', 'Claim'],
            ].map(([key, desc]) => (
              <div key={key} className="flex items-center gap-2">
                <kbd className="px-1.5 py-0.5 bg-secondary text-secondary-foreground rounded text-xs font-mono">
                  {key}
                </kbd>
                <span className="text-muted-foreground">{desc}</span>
              </div>
            ))}
          </div>
        </div>
      )}

      {isLoading ? (
        <div className="flex items-center justify-center py-20">
          <Loader2 className="w-8 h-8 animate-spin text-muted-foreground" />
        </div>
      ) : inboxItems.length === 0 ? (
        <div className="card p-12 text-center">
          <div className="w-14 h-14 rounded-xl bg-primary/10 flex items-center justify-center mx-auto mb-4">
            <Inbox className="w-7 h-7 text-primary" />
          </div>
          <h3 className="text-lg font-semibold text-foreground mb-2">Nothing assigned to you right now.</h3>
          <p className="text-muted-foreground">New queue items will appear here when they are assigned to you.</p>
        </div>
      ) : (
        <div className="grid grid-cols-1 lg:grid-cols-5 gap-4">
          {/* Left column: item list */}
          <div className="lg:col-span-3 space-y-1">
            {inboxItems.map((item, idx) => (
              <button
                key={item.id}
                onClick={() => setFocusedIndex(idx)}
                className={`w-full text-left card p-4 transition-all ${
                  idx === focusedIndex
                    ? 'ring-2 ring-primary bg-primary/5'
                    : 'hover:bg-secondary/50'
                }`}
              >
                <div className="flex items-center justify-between gap-3">
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 mb-1">
                      <span className="font-medium text-foreground truncate">
                        {item.vendor_name ?? 'Unknown vendor'}
                      </span>
                      <span
                        className={`text-xs font-medium px-1.5 py-0.5 rounded-full capitalize ${
                          queueTypeColors[item.queue_type] ?? queueTypeColors.custom
                        }`}
                      >
                        {item.queue_name}
                      </span>
                    </div>
                    <div className="flex items-center gap-3 text-sm text-muted-foreground">
                      <span>{item.invoice_number ?? 'No invoice #'}</span>
                      <span>{formatAmount(item.total_amount_cents, item.currency)}</span>
                      <span className="flex items-center gap-1">
                        <Clock className="w-3 h-3" />
                        {formatAge(item.entered_at)}
                      </span>
                    </div>
                  </div>
                  <ChevronRight
                    className={`w-4 h-4 flex-shrink-0 ${
                      idx === focusedIndex ? 'text-primary' : 'text-muted-foreground'
                    }`}
                  />
                </div>
              </button>
            ))}
          </div>

          {/* Right column: detail preview */}
          <div className="lg:col-span-2">
            {focusedItem ? (
              <div className="card p-6 sticky top-6">
                <h2 className="text-lg font-semibold text-foreground mb-4">Invoice Details</h2>

                <dl className="space-y-3 text-sm">
                  <div className="flex justify-between">
                    <dt className="text-muted-foreground">Vendor</dt>
                    <dd className="font-medium text-foreground">{focusedItem.vendor_name ?? '--'}</dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-muted-foreground">Invoice #</dt>
                    <dd className="font-medium text-foreground">{focusedItem.invoice_number ?? '--'}</dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-muted-foreground">Amount</dt>
                    <dd className="font-medium text-foreground">
                      {formatAmount(focusedItem.total_amount_cents, focusedItem.currency)}
                    </dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-muted-foreground">Status</dt>
                    <dd className="font-medium text-foreground capitalize">
                      {focusedItem.invoice_status ?? '--'}
                    </dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-muted-foreground">Queue</dt>
                    <dd>
                      <span
                        className={`text-xs font-medium px-1.5 py-0.5 rounded-full capitalize ${
                          queueTypeColors[focusedItem.queue_type] ?? queueTypeColors.custom
                        }`}
                      >
                        {focusedItem.queue_name}
                      </span>
                    </dd>
                  </div>
                  <div className="flex justify-between">
                    <dt className="text-muted-foreground">Priority</dt>
                    <dd className="font-medium text-foreground">{focusedItem.priority}</dd>
                  </div>
                </dl>

                <div className="mt-6 space-y-2">
                  <button
                    onClick={() => router.push(`/processing/invoices/${focusedItem.invoice_id}`)}
                    className="btn btn-primary w-full"
                  >
                    <ExternalLink className="w-4 h-4 mr-1.5" />
                    Open Invoice
                  </button>
                  <div className="grid grid-cols-3 gap-2">
                    <button
                      onClick={() => {
                        workflowsApi
                          .completeQueueItem(focusedItem.queue_id, focusedItem.id, 'approve')
                          .then(() => queryClient.invalidateQueries({ queryKey: ['inbox'] }))
                          .then(() => advanceFocus());
                      }}
                      className="btn btn-sm bg-green-50 text-green-700 hover:bg-green-100 border border-green-200"
                    >
                      <CheckCircle className="w-4 h-4 mr-1" />
                      Approve
                    </button>
                    <button
                      onClick={() => {
                        workflowsApi
                          .completeQueueItem(focusedItem.queue_id, focusedItem.id, 'reject')
                          .then(() => queryClient.invalidateQueries({ queryKey: ['inbox'] }))
                          .then(() => advanceFocus());
                      }}
                      className="btn btn-sm bg-red-50 text-red-700 hover:bg-red-100 border border-red-200"
                    >
                      <XCircle className="w-4 h-4 mr-1" />
                      Reject
                    </button>
                    <button
                      onClick={() => {
                        workflowsApi
                          .claimQueueItem(focusedItem.queue_id, focusedItem.id)
                          .then(() => queryClient.invalidateQueries({ queryKey: ['inbox'] }));
                      }}
                      className="btn btn-sm bg-blue-50 text-blue-700 hover:bg-blue-100 border border-blue-200"
                    >
                      <UserPlus className="w-4 h-4 mr-1" />
                      Claim
                    </button>
                  </div>
                </div>
              </div>
            ) : (
              <div className="card p-12 text-center text-muted-foreground">
                Select an item to preview
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
