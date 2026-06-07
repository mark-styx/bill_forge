'use client';

import { useState, useCallback } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  apCommandCenterApi,
  approvalsActions,
  dashboardApi,
  type ApCommandCenterResponse,
  type ApCommandCenterInvoice,
  type ApCommandCenterBucket,
  type DashboardTeamMemberStats,
} from '@/lib/api';
import {
  DollarSign,
  AlertTriangle,
  Clock,
  RefreshCw,
  UserX,
  MessageSquare,
  ArrowRightLeft,
  Users,
  X,
} from 'lucide-react';
import PeerBenchmarkPanel from '@/components/ap-command-center/PeerBenchmarkPanel';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const CENTS = (cents: number) =>
  new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
    maximumFractionDigits: 0,
  }).format(cents / 100);

const DATE_SHORT = (iso: string) => {
  const d = new Date(iso + 'T00:00:00');
  return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
};

// ---------------------------------------------------------------------------
// KPI tiles
// ---------------------------------------------------------------------------

function KpiTile({
  label,
  value,
  sub,
  icon: Icon,
  color,
}: {
  label: string;
  value: string;
  sub?: string;
  icon: React.ComponentType<{ className?: string }>;
  color: string;
}) {
  return (
    <div className="bg-card rounded-lg border border-border p-4 flex items-start gap-3">
      <div className={`rounded-lg p-2 ${color}`}>
        <Icon className="w-5 h-5" />
      </div>
      <div>
        <p className="text-sm text-muted-foreground">{label}</p>
        <p className="text-lg font-semibold text-foreground">{value}</p>
        {sub && <p className="text-xs text-muted-foreground">{sub}</p>}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Stuck badge
// ---------------------------------------------------------------------------

function DaysStuckBadge({ days }: { days: number }) {
  if (days <= 0) return null;
  if (days >= 5)
    return (
      <span className="inline-flex items-center rounded-full bg-red-100 text-red-700 px-2 py-0.5 text-xs font-medium">
        {days}d stuck
      </span>
    );
  return (
    <span className="inline-flex items-center rounded-full bg-amber-100 text-amber-700 px-2 py-0.5 text-xs font-medium">
      {days}d stuck
    </span>
  );
}

// ---------------------------------------------------------------------------
// Inline actions per row
// ---------------------------------------------------------------------------

function InlineActions({ inv }: { inv: ApCommandCenterInvoice }) {
  const queryClient = useQueryClient();
  const [showNudge, setShowNudge] = useState(false);
  const [nudgeText, setNudgeText] = useState('');
  const [showReassign, setShowReassign] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const [toastType, setToastType] = useState<'success' | 'error'>('success');

  const setFeedback = (msg: string, type: 'success' | 'error') => {
    setToast(msg);
    setToastType(type);
    setTimeout(() => setToast(null), 3000);
  };

  // Fetch team members for the reassign picker
  const { data: teamData } = useQuery({
    queryKey: ['dashboard-team'],
    queryFn: () => dashboardApi.getTeamMetrics(),
    enabled: showReassign,
  });

  const teamMembers: DashboardTeamMemberStats[] = (teamData?.members ?? []).filter(
    (m) => m.user_id !== inv.blocking_approver_id,
  );

  const nudgeMut = useMutation({
    mutationFn: () =>
      approvalsActions.nudge(
        inv.invoice_id,
        nudgeText || 'Gentle reminder: please review this invoice.',
      ),
    onSuccess: () => {
      setFeedback('Nudge sent', 'success');
      setNudgeText('');
      setShowNudge(false);
      queryClient.invalidateQueries({ queryKey: ['ap-command-center'] });
    },
    onError: () => {
      setFeedback('Failed to send nudge', 'error');
    },
  });

  const reassignMut = useMutation({
    mutationFn: (newUserId: string) =>
      approvalsActions.reassign(inv.invoice_id, newUserId),
    onSuccess: () => {
      setFeedback('Reassigned', 'success');
      setShowReassign(false);
      queryClient.invalidateQueries({ queryKey: ['ap-command-center'] });
    },
    onError: () => {
      setFeedback('Failed to reassign', 'error');
    },
  });

  const handleNudge = useCallback(() => {
    if (!nudgeMut.isPending) nudgeMut.mutate();
  }, [nudgeMut]);

  return (
    <div className="flex flex-col gap-1">
      {toast && (
        <span
          className={`text-xs font-medium ${
            toastType === 'success' ? 'text-green-600' : 'text-red-600'
          }`}
        >
          {toast}
        </span>
      )}
      <div className="flex items-center gap-1 flex-wrap">
        {inv.blocking_approver_id && (
          <>
            <button
              onClick={() => {
                setShowNudge(!showNudge);
                setShowReassign(false);
              }}
              className="inline-flex items-center gap-1 rounded-md bg-amber-50 text-amber-700 px-2 py-1 text-xs font-medium hover:bg-amber-100 transition-colors"
            >
              <MessageSquare className="w-3 h-3" />
              Nudge
            </button>
            <button
              onClick={() => {
                setShowReassign(!showReassign);
                setShowNudge(false);
              }}
              className="inline-flex items-center gap-1 rounded-md bg-blue-50 text-blue-700 px-2 py-1 text-xs font-medium hover:bg-blue-100 transition-colors"
            >
              <Users className="w-3 h-3" />
              Reassign
            </button>
          </>
        )}
      </div>
      {showNudge && (
        <div className="flex items-center gap-1 w-full">
          <input
            type="text"
            value={nudgeText}
            onChange={(e) => setNudgeText(e.target.value)}
            placeholder="Reminder message..."
            className="flex-1 rounded border border-border px-2 py-1 text-xs bg-background"
          />
          <button
            onClick={handleNudge}
            disabled={nudgeMut.isPending}
            className="rounded bg-primary text-primary-foreground px-2 py-1 text-xs font-medium disabled:opacity-50"
          >
            Send
          </button>
        </div>
      )}
      {showReassign && (
        <div className="border border-border rounded bg-card p-2 w-full max-h-32 overflow-y-auto space-y-1">
          <div className="flex items-center justify-between">
            <span className="text-xs text-muted-foreground font-medium">Pick approver</span>
            <button onClick={() => setShowReassign(false)} className="text-muted-foreground hover:text-foreground">
              <X className="w-3 h-3" />
            </button>
          </div>
          {teamMembers.length === 0 ? (
            <span className="text-xs text-muted-foreground">No other team members found.</span>
          ) : (
            teamMembers.map((m) => (
              <button
                key={m.user_id}
                onClick={() => reassignMut.mutate(m.user_id)}
                disabled={reassignMut.isPending}
                className="w-full text-left px-2 py-1 rounded text-xs hover:bg-muted transition-colors disabled:opacity-50"
              >
                {m.user_name}
              </button>
            ))
          )}
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Bucket table
// ---------------------------------------------------------------------------

function BucketTable({ bucket }: { bucket: ApCommandCenterBucket }) {
  if (bucket.invoices.length === 0) {
    return (
      <div className="bg-card rounded-lg border border-border p-6 text-center text-muted-foreground">
        No invoices due in this period.
      </div>
    );
  }

  return (
    <div className="bg-card rounded-lg border border-border shadow-sm">
      <div className="px-6 py-4 border-b border-border flex items-center justify-between">
        <div>
          <h2 className="text-lg font-semibold text-foreground">{bucket.label}</h2>
          <p className="text-sm text-muted-foreground">
            {DATE_SHORT(bucket.range_start)} &ndash; {DATE_SHORT(bucket.range_end)}{' '}
            &middot;{' '}
            <span className="font-medium text-foreground">
              {CENTS(bucket.total_payable_cents)}
            </span>
          </p>
        </div>
        <span className="text-sm text-muted-foreground">
          {bucket.invoices.length} invoice{bucket.invoices.length !== 1 ? 's' : ''}
        </span>
      </div>
      <div className="overflow-x-auto">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-border bg-muted/30">
              <th className="text-left px-4 py-3 font-medium text-muted-foreground">Vendor</th>
              <th className="text-left px-4 py-3 font-medium text-muted-foreground">Invoice</th>
              <th className="text-right px-4 py-3 font-medium text-muted-foreground">Amount</th>
              <th className="text-center px-4 py-3 font-medium text-muted-foreground">Due</th>
              <th className="text-left px-4 py-3 font-medium text-muted-foreground">Blocked by</th>
              <th className="text-right px-4 py-3 font-medium text-muted-foreground">
                Late-fee risk
              </th>
              <th className="text-right px-4 py-3 font-medium text-muted-foreground">
                Discount expiring
              </th>
              <th className="text-center px-4 py-3 font-medium text-muted-foreground">Actions</th>
            </tr>
          </thead>
          <tbody>
            {bucket.invoices.map((inv) => (
              <tr
                key={inv.invoice_id}
                className="border-b border-border hover:bg-muted/20 transition-colors"
              >
                <td className="px-4 py-3 font-medium text-foreground">{inv.vendor_name}</td>
                <td className="px-4 py-3 text-muted-foreground">{inv.invoice_number}</td>
                <td className="px-4 py-3 text-right text-foreground font-medium">
                  {CENTS(inv.amount_cents)}
                </td>
                <td className="px-4 py-3 text-center text-muted-foreground">
                  {DATE_SHORT(inv.due_date)}
                </td>
                <td className="px-4 py-3">
                  {inv.blocking_approver_name ? (
                    <div className="flex items-center gap-2">
                      <UserX className="w-3.5 h-3.5 text-red-500" />
                      <span className="text-foreground">{inv.blocking_approver_name}</span>
                      <DaysStuckBadge days={inv.days_stuck} />
                    </div>
                  ) : (
                    <span className="text-muted-foreground">&mdash;</span>
                  )}
                </td>
                <td
                  className={`px-4 py-3 text-right ${
                    inv.late_fee_risk_cents > 0
                      ? 'text-red-600 font-medium'
                      : 'text-muted-foreground'
                  }`}
                >
                  {inv.late_fee_risk_cents > 0 ? CENTS(inv.late_fee_risk_cents) : '\u2014'}
                </td>
                <td
                  className={`px-4 py-3 text-right ${
                    inv.discount_expiring_cents > 0
                      ? 'text-amber-600 font-medium'
                      : 'text-muted-foreground'
                  }`}
                >
                  {inv.discount_expiring_cents > 0 ? (
                    <div>
                      {CENTS(inv.discount_expiring_cents)}
                      {inv.discount_expires_at && (
                        <div className="text-xs text-muted-foreground">
                          by {DATE_SHORT(inv.discount_expires_at)}
                        </div>
                      )}
                    </div>
                  ) : (
                    '\u2014'
                  )}
                </td>
                <td className="px-4 py-3">
                  <InlineActions inv={inv} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export default function ApCommandCenterPage() {
  const [lastRefresh, setLastRefresh] = useState<Date>(new Date());

  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ['ap-command-center'],
    queryFn: async () => {
      const resp = await apCommandCenterApi.thisWeek();
      setLastRefresh(new Date());
      return resp;
    },
    refetchInterval: 60_000,
  });

  const handleManualRefresh = useCallback(() => {
    refetch();
  }, [refetch]);

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-96">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex items-center justify-center h-96">
        <p className="text-destructive">Failed to load AP Command Center. Please try again.</p>
      </div>
    );
  }

  const resp = data as ApCommandCenterResponse | undefined;
  const thisWeek = resp?.week_buckets?.[0];
  const nextWeek = resp?.week_buckets?.[1];

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-foreground flex items-center gap-2">
            <DollarSign className="w-6 h-6" />
            AP Command Center
          </h1>
          <p className="text-muted-foreground mt-1">
            Standup view &mdash; invoices due this week and next, blockers, and at-risk dollars.
          </p>
        </div>
        <div className="flex items-center gap-3">
          <span className="text-xs text-muted-foreground">
            Last refresh: {lastRefresh.toLocaleTimeString()}
          </span>
          <button
            onClick={handleManualRefresh}
            className="inline-flex items-center gap-1.5 rounded-lg border border-border bg-background px-3 py-1.5 text-xs font-medium hover:bg-muted transition-colors"
          >
            <RefreshCw className="w-3.5 h-3.5" />
            Refresh
          </button>
        </div>
      </div>

      {/* KPI tiles */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        <KpiTile
          label="This week payable"
          value={thisWeek ? CENTS(thisWeek.total_payable_cents) : '\u2014'}
          sub={
            thisWeek
              ? `${thisWeek.invoices.length} invoice${thisWeek.invoices.length !== 1 ? 's' : ''}`
              : undefined
          }
          icon={DollarSign}
          color="bg-blue-50 text-blue-600"
        />
        <KpiTile
          label="Next week payable"
          value={nextWeek ? CENTS(nextWeek.total_payable_cents) : '\u2014'}
          sub={
            nextWeek
              ? `${nextWeek.invoices.length} invoice${nextWeek.invoices.length !== 1 ? 's' : ''}`
              : undefined
          }
          icon={Clock}
          color="bg-indigo-50 text-indigo-600"
        />
        <KpiTile
          label="Late-fee risk $"
          value={resp ? CENTS(resp.late_fee_risk_total_cents) : '\u2014'}
          sub="Aggregate exposure"
          icon={AlertTriangle}
          color="bg-red-50 text-red-600"
        />
        <KpiTile
          label="Discount $ expiring"
          value={resp ? CENTS(resp.discount_expiring_total_cents) : '\u2014'}
          sub="Capture before deadline"
          icon={ArrowRightLeft}
          color="bg-amber-50 text-amber-600"
        />
      </div>

      {/* Peer Benchmark Panel */}
      <PeerBenchmarkPanel />

      {/* Bucket tables */}
      {thisWeek && <BucketTable bucket={thisWeek} />}
      {nextWeek && <BucketTable bucket={nextWeek} />}
    </div>
  );
}
