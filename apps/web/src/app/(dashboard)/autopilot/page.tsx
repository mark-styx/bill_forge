'use client';

import { useState, useEffect, useCallback } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import Link from 'next/link';
import {
  autopilotApi,
  type AutopilotExceptionType,
  type AutopilotQueueItem,
} from '@/lib/api';
import { ConfidenceBadge } from '@/components/ConfidenceBadge';
import { FileText, Sliders, BarChart3, ListChecks } from 'lucide-react';

// ---------------------------------------------------------------------------
// Static config
// ---------------------------------------------------------------------------

const EXCEPTION_TYPE_LABELS: Record<AutopilotExceptionType, string> = {
  missing_po: 'Missing PO',
  vendor_mismatch: 'Vendor Mismatch',
  duplicate: 'Duplicate',
  gl_ambiguity: 'GL Ambiguity',
  policy_violation: 'Policy Violation',
  ocr_low_confidence: 'OCR Low Confidence',
};

const EXCEPTION_TYPE_COLORS: Record<AutopilotExceptionType, string> = {
  missing_po: 'bg-blue-100 text-blue-800 dark:bg-blue-900/30 dark:text-blue-300',
  vendor_mismatch: 'bg-purple-100 text-purple-800 dark:bg-purple-900/30 dark:text-purple-300',
  duplicate: 'bg-orange-100 text-orange-800 dark:bg-orange-900/30 dark:text-orange-300',
  gl_ambiguity: 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900/30 dark:text-yellow-300',
  policy_violation: 'bg-red-100 text-red-800 dark:bg-red-900/30 dark:text-red-300',
  ocr_low_confidence: 'bg-gray-100 text-gray-800 dark:bg-gray-900/30 dark:text-gray-300',
};

const ALL_EXCEPTION_TYPES = Object.keys(EXCEPTION_TYPE_LABELS) as AutopilotExceptionType[];

type Tab = 'queue' | 'report';

export default function AutopilotPage() {
  const [tab, setTab] = useState<Tab>('queue');
  return (
    <div className="space-y-6 max-w-7xl mx-auto">
      <div>
        <h1 className="text-2xl font-semibold text-foreground">Autopilot</h1>
        <p className="text-muted-foreground mt-0.5">
          One-keystroke confirm/override for every queued exception. Per-tenant
          threshold auto-resolves above N% confidence.
        </p>
      </div>

      <div className="flex gap-1 border-b border-border">
        <TabButton active={tab === 'queue'} onClick={() => setTab('queue')} icon={<ListChecks className="w-4 h-4" />}>
          Queue
        </TabButton>
        <TabButton active={tab === 'report'} onClick={() => setTab('report')} icon={<BarChart3 className="w-4 h-4" />}>
          Daily Report
        </TabButton>
      </div>

      {tab === 'queue' ? <QueueTab /> : <ReportTab />}
    </div>
  );
}

function TabButton({
  active,
  onClick,
  icon,
  children,
}: {
  active: boolean;
  onClick: () => void;
  icon: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      className={`px-4 py-2 -mb-px border-b-2 flex items-center gap-2 text-sm font-medium transition-colors ${
        active
          ? 'border-primary text-primary'
          : 'border-transparent text-muted-foreground hover:text-foreground'
      }`}
    >
      {icon}
      {children}
    </button>
  );
}

// ---------------------------------------------------------------------------
// Settings (per-tenant threshold + enabled types)
// ---------------------------------------------------------------------------

function SettingsPanel() {
  const queryClient = useQueryClient();
  const { data: settings, isLoading } = useQuery({
    queryKey: ['autopilot', 'settings'],
    queryFn: () => autopilotApi.getSettings(),
  });

  const updateMutation = useMutation({
    mutationFn: (next: { autopilot_threshold?: number; autopilot_enabled_types?: AutopilotExceptionType[] }) =>
      autopilotApi.updateSettings(next),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['autopilot', 'settings'] });
      queryClient.invalidateQueries({ queryKey: ['autopilot', 'queue'] });
    },
  });

  if (isLoading || !settings) {
    return null;
  }

  const toggleType = (t: AutopilotExceptionType) => {
    const has = settings.autopilot_enabled_types.includes(t);
    const next = has
      ? settings.autopilot_enabled_types.filter((x) => x !== t)
      : [...settings.autopilot_enabled_types, t];
    updateMutation.mutate({ autopilot_enabled_types: next });
  };

  return (
    <div className="card p-4 space-y-4" data-testid="autopilot-settings-panel">
      <div className="flex items-center gap-2">
        <Sliders className="w-4 h-4 text-muted-foreground" />
        <span className="text-sm font-medium text-foreground">Autopilot Settings</span>
      </div>

      <div className="flex flex-col sm:flex-row gap-4 sm:items-center">
        <label htmlFor="autopilot-threshold" className="text-sm text-muted-foreground min-w-32">
          Auto-resolve threshold
        </label>
        <input
          id="autopilot-threshold"
          type="range"
          min={0}
          max={1}
          step={0.01}
          value={settings.autopilot_threshold}
          onChange={(e) => {
            // Optimistic update is overkill; the PUT mutation invalidates.
            updateMutation.mutate({ autopilot_threshold: parseFloat(e.target.value) });
          }}
          className="flex-1 max-w-xs"
          aria-label="Autopilot auto-resolve threshold"
        />
        <span className="text-sm font-medium text-foreground tabular-nums">
          {Math.round(settings.autopilot_threshold * 100)}%
        </span>
      </div>

      <div className="space-y-2">
        <p className="text-sm text-muted-foreground">
          Exception types the background sweep may auto-resolve
        </p>
        <div className="flex flex-wrap gap-2">
          {ALL_EXCEPTION_TYPES.map((t) => {
            const on = settings.autopilot_enabled_types.includes(t);
            return (
              <button
                key={t}
                onClick={() => toggleType(t)}
                aria-pressed={on}
                aria-label={`Toggle autopilot auto-resolve for ${EXCEPTION_TYPE_LABELS[t]}`}
                className={`px-3 py-1 rounded-full text-xs font-medium border transition-colors ${
                  on
                    ? 'bg-primary/10 text-primary border-primary/40'
                    : 'bg-secondary text-muted-foreground border-border'
                }`}
              >
                {EXCEPTION_TYPE_LABELS[t]}
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Queue tab
// ---------------------------------------------------------------------------

function QueueTab() {
  const queryClient = useQueryClient();
  const [selectedIndex, setSelectedIndex] = useState(0);

  const { data, isLoading } = useQuery({
    queryKey: ['autopilot', 'queue'],
    queryFn: () => autopilotApi.getQueue(),
  });

  const resolveMutation = useMutation({
    mutationFn: ({
      item,
      decision,
      overrideAction,
    }: {
      item: AutopilotQueueItem;
      decision: 'confirm' | 'override';
      overrideAction?: { action: string };
    }) => autopilotApi.resolve(item.id, decision, overrideAction),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['autopilot', 'queue'] });
      queryClient.invalidateQueries({ queryKey: ['autopilot', 'report'] });
    },
  });

  const items = data?.items ?? [];

  // Keep selection in bounds as items are removed.
  useEffect(() => {
    if (selectedIndex >= items.length) {
      setSelectedIndex(Math.max(0, items.length - 1));
    }
  }, [items.length, selectedIndex]);

  const confirmSelected = useCallback(() => {
    const item = items[selectedIndex];
    if (!item) return;
    resolveMutation.mutate({ item, decision: 'confirm' });
  }, [items, selectedIndex, resolveMutation]);

  const overrideSelected = useCallback(() => {
    const item = items[selectedIndex];
    if (!item) return;
    // One-keystroke override: choose a sensible default action that differs
    // from the proposal. The user can refine via the inline panel.
    resolveMutation.mutate({
      item,
      decision: 'override',
      overrideAction: { action: item.proposed_resolution.action === 'approve' ? 'reject' : 'approve' },
    });
  }, [items, selectedIndex, resolveMutation]);

  // Keyboard handlers: Y confirm, N reject/override, J/K move selection.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // Ignore when the user is typing into an input/textarea/select.
      const target = e.target as HTMLElement | null;
      if (target && /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName)) return;
      if (e.key === 'y' || e.key === 'Y') {
        e.preventDefault();
        confirmSelected();
      } else if (e.key === 'n' || e.key === 'N') {
        e.preventDefault();
        overrideSelected();
      } else if (e.key === 'j' || e.key === 'J') {
        e.preventDefault();
        setSelectedIndex((i) => Math.min(i + 1, Math.max(0, items.length - 1)));
      } else if (e.key === 'k' || e.key === 'K') {
        e.preventDefault();
        setSelectedIndex((i) => Math.max(0, i - 1));
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [confirmSelected, overrideSelected, items.length]);

  return (
    <div className="space-y-4">
      <SettingsPanel />

      <div className="card">
        <div className="px-4 py-3 border-b border-border flex items-center justify-between">
          <p className="text-sm text-muted-foreground">
            <span className="font-medium text-foreground">{items.length}</span> pending exception{items.length === 1 ? '' : 's'}
            {' '}· sorted lowest-confidence first
          </p>
          <p className="text-xs text-muted-foreground hidden sm:block">
            <Kbd>Y</Kbd> confirm · <Kbd>N</Kbd> override · <Kbd>J/K</Kbd> navigate
          </p>
        </div>

        {isLoading ? (
          <div className="text-center py-12 text-muted-foreground">
            <div className="flex items-center justify-center gap-2">
              <div className="w-4 h-4 border-2 border-primary border-t-transparent rounded-full animate-spin" />
              Loading queue...
            </div>
          </div>
        ) : items.length === 0 ? (
          <div className="text-center py-12">
            <div className="w-12 h-12 rounded-xl bg-secondary flex items-center justify-center mb-3 mx-auto">
              <FileText className="w-6 h-6 text-muted-foreground" />
            </div>
            <p className="text-foreground font-medium mb-1">Queue is clear</p>
            <p className="text-sm text-muted-foreground">
              No exceptions are pending review. Auto-resolved items appear in the Daily Report.
            </p>
          </div>
        ) : (
          <ul className="divide-y divide-border">
            {items.map((item, i) => {
              const isSelected = i === selectedIndex;
              return (
                <li
                  key={item.id}
                  aria-selected={isSelected}
                  data-testid={`autopilot-queue-item-${i}`}
                  className={`px-4 py-3 transition-colors cursor-pointer ${
                    isSelected ? 'bg-primary/5' : 'hover:bg-secondary/40'
                  }`}
                  onClick={() => setSelectedIndex(i)}
                >
                  <div className="flex items-start gap-3">
                    <div className="flex flex-col items-center gap-1 pt-1 min-w-12">
                      <span
                        className={`inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium ${
                          EXCEPTION_TYPE_COLORS[item.exception_type]
                        }`}
                      >
                        {EXCEPTION_TYPE_LABELS[item.exception_type]}
                      </span>
                      {item.auto_resolve_eligible && (
                        <span
                          title="Above threshold and in enabled-types list"
                          className="text-[10px] text-primary font-medium"
                        >
                          auto
                        </span>
                      )}
                    </div>

                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        <Link
                          href={`/invoices/${item.invoice_id}`}
                          className="font-medium text-foreground hover:underline truncate"
                          onClick={(e) => e.stopPropagation()}
                        >
                          Invoice {item.invoice_id.slice(0, 8)}
                        </Link>
                        <span className="text-xs text-muted-foreground">
                          {item.proposed_resolution.action}
                        </span>
                      </div>
                      <p className="text-sm text-muted-foreground mt-0.5 line-clamp-2">
                        {item.proposed_resolution.rationale}
                      </p>
                    </div>

                    <div className="flex items-center gap-3">
                      <div data-testid={`autopilot-confidence-${i}`}>
                        <ConfidenceBadge confidence={item.confidence} size="sm" />
                      </div>
                      <div className="flex gap-1">
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            setSelectedIndex(i);
                            resolveMutation.mutate({ item, decision: 'confirm' });
                          }}
                          disabled={resolveMutation.isPending}
                          className="btn btn-sm bg-success/10 text-success hover:bg-success/20"
                          aria-label={`Confirm proposed resolution for ${item.id}`}
                        >
                          Confirm
                        </button>
                        <button
                          onClick={(e) => {
                            e.stopPropagation();
                            setSelectedIndex(i);
                            overrideSelected();
                          }}
                          disabled={resolveMutation.isPending}
                          className="btn btn-sm bg-error/10 text-error hover:bg-error/20"
                          aria-label={`Override proposed resolution for ${item.id}`}
                        >
                          Override
                        </button>
                      </div>
                    </div>
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </div>
  );
}

function Kbd({ children }: { children: React.ReactNode }) {
  return (
    <kbd className="inline-flex items-center justify-center min-w-5 h-5 px-1 rounded border border-border bg-secondary text-[10px] font-mono text-foreground">
      {children}
    </kbd>
  );
}

// ---------------------------------------------------------------------------
// Report tab
// ---------------------------------------------------------------------------

function ReportTab() {
  const today = new Date().toISOString().slice(0, 10);
  const [date, setDate] = useState(today);

  const { data: report, isLoading } = useQuery({
    queryKey: ['autopilot', 'report', date],
    queryFn: () => autopilotApi.getReport(date),
  });

  return (
    <div className="space-y-4">
      <div className="card p-4">
        <div className="flex items-center gap-3">
          <label htmlFor="autopilot-report-date" className="text-sm font-medium text-foreground">
            Date
          </label>
          <input
            id="autopilot-report-date"
            type="date"
            value={date}
            onChange={(e) => setDate(e.target.value)}
            className="input w-44"
            aria-label="Autopilot report date"
          />
        </div>
      </div>

      <div className="table-container bg-card">
        <table className="table">
          <thead>
            <tr>
              <th>Exception Type</th>
              <th className="text-right">Auto-resolved</th>
              <th className="text-right">Human confirmed</th>
              <th className="text-right">Overridden</th>
              <th className="text-right">Still open</th>
            </tr>
          </thead>
          <tbody>
            {isLoading || !report ? (
              <tr>
                <td colSpan={5} className="text-center py-12 text-muted-foreground">
                  <div className="flex items-center justify-center gap-2">
                    <div className="w-4 h-4 border-2 border-primary border-t-transparent rounded-full animate-spin" />
                    Loading report...
                  </div>
                </td>
              </tr>
            ) : report.rows.length === 0 ? (
              <tr>
                <td colSpan={5} className="text-center py-12 text-muted-foreground">
                  No decisions recorded for {date}.
                </td>
              </tr>
            ) : (
              report.rows.map((row) => (
                <tr key={row.exception_type}>
                  <td>
                    <span
                      className={`inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium ${
                        EXCEPTION_TYPE_COLORS[row.exception_type]
                      }`}
                    >
                      {EXCEPTION_TYPE_LABELS[row.exception_type]}
                    </span>
                  </td>
                  <td className="text-right tabular-nums">{row.auto_resolved}</td>
                  <td className="text-right tabular-nums">{row.human_confirmed}</td>
                  <td className="text-right tabular-nums">{row.overridden}</td>
                  <td className="text-right tabular-nums">{row.still_open}</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      {report && report.uncertain_types.length > 0 && (
        <div className="card p-4">
          <p className="text-sm font-medium text-foreground mb-2">
            Where the model is uncertain
          </p>
          <p className="text-xs text-muted-foreground mb-3">
            Lowest-confidence exception types still pending review.
          </p>
          <ul className="space-y-1">
            {report.uncertain_types.map((b) => (
              <li
                key={b.exception_type}
                className="flex items-center justify-between text-sm"
              >
                <span className="text-foreground">
                  {EXCEPTION_TYPE_LABELS[b.exception_type]}
                </span>
                <span className="text-muted-foreground tabular-nums">
                  avg {Math.round(b.avg_confidence * 100)}% · {b.open_count} open
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
