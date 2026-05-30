'use client';

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { closePeriodsApi, type ClosePeriod, type RunCloseResponse } from '@/lib/api';
import { useAuthStore } from '@/stores/auth';
import {
  Calendar,
  Plus,
  Lock,
  Unlock,
  AlertTriangle,
  CheckCircle2,
  XCircle,
  Loader2,
} from 'lucide-react';

// ---------------------------------------------------------------------------
// Status badge
// ---------------------------------------------------------------------------

function StatusBadge({ status }: { status: ClosePeriod['status'] }) {
  const config = {
    open: { icon: Unlock, label: 'Open', cls: 'bg-blue-100 text-blue-800 dark:bg-blue-900/30 dark:text-blue-300' },
    cutoff_passed: { icon: AlertTriangle, label: 'Cutoff Passed', cls: 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900/30 dark:text-yellow-300' },
    locked: { icon: Lock, label: 'Locked', cls: 'bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-300' },
  }[status];

  const Icon = config.icon;
  return (
    <span className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium ${config.cls}`}>
      <Icon className="w-3 h-3" />
      {config.label}
    </span>
  );
}

// ---------------------------------------------------------------------------
// New Period Dialog
// ---------------------------------------------------------------------------

function NewPeriodDialog({
  open,
  onClose,
  onSubmit,
  loading,
}: {
  open: boolean;
  onClose: () => void;
  onSubmit: (data: { period_label: string; period_start: string; period_end: string; cutoff_date: string }) => void;
  loading: boolean;
}) {
  const [label, setLabel] = useState('');
  const [start, setStart] = useState('');
  const [end, setEnd] = useState('');
  const [cutoff, setCutoff] = useState('');

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-card rounded-xl shadow-lg border border-border p-6 w-full max-w-md">
        <h3 className="text-lg font-semibold text-foreground mb-4">Create Close Period</h3>
        <div className="space-y-3">
          <div>
            <label className="block text-sm font-medium text-muted-foreground mb-1">Period Label</label>
            <input
              type="text"
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              placeholder="e.g. 2026-05"
              className="w-full px-3 py-2 rounded-lg border border-border bg-background text-foreground text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
            />
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-sm font-medium text-muted-foreground mb-1">Period Start</label>
              <input
                type="date"
                value={start}
                onChange={(e) => setStart(e.target.value)}
                className="w-full px-3 py-2 rounded-lg border border-border bg-background text-foreground text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-muted-foreground mb-1">Period End</label>
              <input
                type="date"
                value={end}
                onChange={(e) => setEnd(e.target.value)}
                className="w-full px-3 py-2 rounded-lg border border-border bg-background text-foreground text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
              />
            </div>
          </div>
          <div>
            <label className="block text-sm font-medium text-muted-foreground mb-1">Cutoff Date</label>
            <input
              type="date"
              value={cutoff}
              onChange={(e) => setCutoff(e.target.value)}
              className="w-full px-3 py-2 rounded-lg border border-border bg-background text-foreground text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
            />
          </div>
        </div>
        <div className="flex justify-end gap-2 mt-5">
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm text-muted-foreground hover:text-foreground transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={() => onSubmit({ period_label: label, period_start: start, period_end: end, cutoff_date: cutoff })}
            disabled={loading || !label || !start || !end || !cutoff}
            className="px-4 py-2 text-sm bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 disabled:opacity-50 transition-colors"
          >
            {loading ? 'Creating...' : 'Create Period'}
          </button>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Confirm Close Dialog
// ---------------------------------------------------------------------------

function ConfirmCloseDialog({
  open,
  period,
  onClose,
  onConfirm,
  loading,
}: {
  open: boolean;
  period: ClosePeriod | null;
  onClose: () => void;
  onConfirm: () => void;
  loading: boolean;
}) {
  if (!open || !period) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-card rounded-xl shadow-lg border border-border p-6 w-full max-w-md">
        <h3 className="text-lg font-semibold text-foreground mb-2">Run Month-End Close</h3>
        <p className="text-sm text-muted-foreground mb-4">
          This will generate accrual entries for unapproved invoices dated on or before{' '}
          <strong className="text-foreground">{period.period_end}</strong>, attempt to post them to your ERP, and lock
          the period.
        </p>
        <p className="text-sm text-muted-foreground mb-4">
          Once locked, no further invoice changes can be made for this period.
        </p>
        <div className="flex justify-end gap-2">
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm text-muted-foreground hover:text-foreground transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={onConfirm}
            disabled={loading}
            className="px-4 py-2 text-sm bg-destructive text-destructive-foreground rounded-lg hover:bg-destructive/90 disabled:opacity-50 transition-colors"
          >
            {loading ? 'Running Close...' : 'Run Close'}
          </button>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export default function CloseCalendarPage() {
  const { hasModule } = useAuthStore();
  const queryClient = useQueryClient();
  const [newDialogOpen, setNewDialogOpen] = useState(false);
  const [closeTarget, setCloseTarget] = useState<ClosePeriod | null>(null);
  const [closeResult, setCloseResult] = useState<RunCloseResponse | null>(null);

  // Fetch periods
  const { data: periods = [], isLoading, error } = useQuery({
    queryKey: ['close-periods'],
    queryFn: () => closePeriodsApi.list(),
  });

  // Create period mutation
  const createMutation = useMutation({
    mutationFn: closePeriodsApi.create,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['close-periods'] });
      setNewDialogOpen(false);
    },
  });

  // Update cutoff mutation
  const updateMutation = useMutation({
    mutationFn: ({ id, cutoff_date }: { id: string; cutoff_date: string }) =>
      closePeriodsApi.update(id, { cutoff_date }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['close-periods'] });
    },
  });

  // Run close mutation
  const closeMutation = useMutation({
    mutationFn: closePeriodsApi.runClose,
    onSuccess: (result) => {
      queryClient.invalidateQueries({ queryKey: ['close-periods'] });
      setCloseResult(result);
      setCloseTarget(null);
    },
    onError: () => {
      setCloseTarget(null);
    },
  });

  if (error) {
    return (
      <div className="flex items-center justify-center h-64">
        <p className="text-destructive">Failed to load close periods.</p>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-foreground flex items-center gap-2">
            <Calendar className="w-6 h-6" />
            Month-End Close
          </h1>
          <p className="text-sm text-muted-foreground mt-1">
            Define cutoff dates, run close to generate accrual entries, and lock periods.
          </p>
        </div>
        <button
          onClick={() => setNewDialogOpen(true)}
          className="flex items-center gap-2 px-4 py-2 text-sm bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors"
        >
          <Plus className="w-4 h-4" />
          New Period
        </button>
      </div>

      {/* Close result banner */}
      {closeResult && (
        <div
          className={`rounded-lg border p-4 flex items-start gap-3 ${
            closeResult.erp_post_status === 'posted' || closeResult.erp_post_status === 'pending'
              ? 'bg-green-50 border-green-200 dark:bg-green-900/20 dark:border-green-800'
              : 'bg-red-50 border-red-200 dark:bg-red-900/20 dark:border-red-800'
          }`}
        >
          {closeResult.erp_post_status === 'posted' || closeResult.erp_post_status === 'pending' ? (
            <CheckCircle2 className="w-5 h-5 text-green-600 dark:text-green-400 flex-shrink-0 mt-0.5" />
          ) : (
            <XCircle className="w-5 h-5 text-red-600 dark:text-red-400 flex-shrink-0 mt-0.5" />
          )}
          <div>
            <p className="text-sm font-medium text-foreground">
              Close completed: {closeResult.accrual_entries_created} accrual entries created.
            </p>
            <p className="text-xs text-muted-foreground mt-0.5">
              ERP post status: {closeResult.erp_post_status}
              {closeResult.erp_post_error && ` - ${closeResult.erp_post_error}`}
            </p>
          </div>
          <button
            onClick={() => setCloseResult(null)}
            className="ml-auto text-muted-foreground hover:text-foreground text-sm"
          >
            Dismiss
          </button>
        </div>
      )}

      {/* Periods table */}
      {isLoading ? (
        <div className="flex items-center justify-center h-40">
          <Loader2 className="w-6 h-6 text-muted-foreground animate-spin" />
        </div>
      ) : periods.length === 0 ? (
        <div className="text-center py-16">
          <Calendar className="w-12 h-12 text-muted-foreground mx-auto mb-3" />
          <h3 className="text-lg font-medium text-foreground">No close periods</h3>
          <p className="text-sm text-muted-foreground mt-1">
            Create your first close period to start managing month-end close.
          </p>
        </div>
      ) : (
        <div className="rounded-xl border border-border overflow-hidden">
          <table className="w-full text-sm">
            <thead className="bg-muted/50">
              <tr>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Period</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Range</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Cutoff Date</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Status</th>
                <th className="text-right px-4 py-3 font-medium text-muted-foreground">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {periods.map((period) => (
                <tr key={period.id} className="hover:bg-muted/30 transition-colors">
                  <td className="px-4 py-3 font-medium text-foreground">{period.period_label}</td>
                  <td className="px-4 py-3 text-muted-foreground">
                    {period.period_start} to {period.period_end}
                  </td>
                  <td className="px-4 py-3">
                    {period.status === 'open' ? (
                      <input
                        type="date"
                        value={period.cutoff_date}
                        onChange={(e) =>
                          updateMutation.mutate({ id: period.id, cutoff_date: e.target.value })
                        }
                        disabled={updateMutation.isPending}
                        className="px-2 py-1 rounded border border-border bg-background text-foreground text-sm focus:outline-none focus:ring-2 focus:ring-primary/50"
                      />
                    ) : (
                      <span className="text-muted-foreground">{period.cutoff_date}</span>
                    )}
                  </td>
                  <td className="px-4 py-3">
                    <StatusBadge status={period.status} />
                  </td>
                  <td className="px-4 py-3 text-right">
                    {period.status !== 'locked' && (
                      <button
                        onClick={() => setCloseTarget(period)}
                        className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium bg-destructive/10 text-destructive rounded-lg hover:bg-destructive/20 transition-colors"
                      >
                        <Lock className="w-3 h-3" />
                        Run Close
                      </button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* Dialogs */}
      <NewPeriodDialog
        open={newDialogOpen}
        onClose={() => setNewDialogOpen(false)}
        onSubmit={(data) => createMutation.mutate(data)}
        loading={createMutation.isPending}
      />
      <ConfirmCloseDialog
        open={!!closeTarget}
        period={closeTarget}
        onClose={() => setCloseTarget(null)}
        onConfirm={() => closeTarget && closeMutation.mutate(closeTarget.id)}
        loading={closeMutation.isPending}
      />
    </div>
  );
}
