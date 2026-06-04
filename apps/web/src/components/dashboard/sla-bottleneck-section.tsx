'use client';

import { useQuery } from '@tanstack/react-query';
import { reportsApi, dashboardApi } from '@/lib/api';
import type { ApprovalSlaSummary, ApprovalSlaItem } from '@/lib/api';
import Link from 'next/link';
import { Clock, AlertTriangle, Flame, TrendingUp, Users } from 'lucide-react';

// ---------------------------------------------------------------------------
// Types for the new dashboard endpoints
// ---------------------------------------------------------------------------

interface StageDwellRow {
  stage: string;
  median_minutes: number;
  p90_minutes: number;
  count: number;
}

interface ApproverWorkloadRow {
  approver_id: string;
  approver_name: string;
  pending_count: number;
  near_breach_count: number;
  breached_count: number;
  avg_response_hours: number;
}

interface ExceptionTrendPoint {
  date: string;
  total_invoices: number;
  exception_count: number;
  exception_rate: number;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function formatDeadline(dateStr: string): string {
  const d = new Date(dateStr);
  const now = new Date();
  const diffMs = d.getTime() - now.getTime();
  if (diffMs <= 0) return 'Overdue';
  const diffH = diffMs / 3600000;
  if (diffH < 1) return `${Math.round(diffH * 60)}m`;
  if (diffH < 24) return `${diffH.toFixed(1)}h`;
  return `${(diffH / 24).toFixed(1)}d`;
}

function slaColor(percent: number): string {
  if (percent >= 100) return 'bg-error';
  if (percent >= 70) return 'bg-warning';
  return 'bg-success';
}

function slaTextColor(percent: number): string {
  if (percent >= 100) return 'text-error';
  if (percent >= 70) return 'text-warning';
  return 'text-success';
}

function dwellColor(minutes: number): string {
  if (minutes >= 120) return 'bg-red-500';
  if (minutes >= 60) return 'bg-amber-500';
  if (minutes >= 30) return 'bg-yellow-400';
  return 'bg-emerald-500';
}

// ---------------------------------------------------------------------------
// Sub-widgets
// ---------------------------------------------------------------------------

function AtRiskInvoices({ data, isLoading, isError }: { data: ApprovalSlaSummary | undefined; isLoading?: boolean; isError?: boolean }) {
  const items = (data?.items ?? [])
    .slice()
    .sort((a, b) => b.percent_elapsed - a.percent_elapsed)
    .slice(0, 10);

  if (!items.length) {
    if (isLoading) {
      return (
        <div className="card p-5" data-testid="at-risk-loading" role="status" aria-busy="true">
          <div className="flex items-center gap-3 mb-4">
            <div className="p-2.5 rounded-xl bg-processing/10">
              <Clock className="w-5 h-5 text-processing" />
            </div>
            <div>
              <h3 className="font-semibold text-foreground">At-Risk Invoices</h3>
              <p className="text-sm text-muted-foreground">Loading...</p>
            </div>
          </div>
          <div className="space-y-2">
            {Array.from({ length: 3 }).map((_, i) => (
              <div key={i} className="animate-pulse h-14 rounded-xl bg-secondary" />
            ))}
          </div>
        </div>
      );
    }

    if (isError) {
      return (
        <div className="card p-5" data-testid="at-risk-error" role="alert">
          <div className="flex items-center gap-3 mb-4">
            <div className="p-2.5 rounded-xl bg-error/10">
              <AlertTriangle className="w-5 h-5 text-error" />
            </div>
            <div>
              <h3 className="font-semibold text-foreground">At-Risk Invoices</h3>
              <p className="text-sm text-error">Unable to load at-risk invoices</p>
            </div>
          </div>
        </div>
      );
    }

    return (
      <div className="card p-5">
        <div className="flex items-center gap-3 mb-4">
          <div className="p-2.5 rounded-xl bg-processing/10">
            <Clock className="w-5 h-5 text-processing" />
          </div>
          <div>
            <h3 className="font-semibold text-foreground">At-Risk Invoices</h3>
            <p className="text-sm text-muted-foreground">SLA countdown timers</p>
          </div>
        </div>
        <p className="text-sm text-muted-foreground">No invoices at risk</p>
      </div>
    );
  }

  const remaining = (data?.items?.length ?? 0) - 10;

  return (
    <div className="card p-5">
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-3">
          <div className="p-2.5 rounded-xl bg-processing/10">
            <Clock className="w-5 h-5 text-processing" />
          </div>
          <div>
            <h3 className="font-semibold text-foreground">At-Risk Invoices</h3>
            <p className="text-sm text-muted-foreground">
              {data?.near_breach_count ?? 0} near breach · {data?.breached_count ?? 0} breached
            </p>
          </div>
        </div>
      </div>
      <div className="space-y-3">
        {items.map((item) => (
          <Link
            key={item.approval_id}
            href={`/invoices/${item.invoice_id}`}
            className="block rounded-xl border border-border p-3 hover:bg-secondary/40 transition-colors"
          >
            <div className="flex items-start justify-between gap-3">
              <div className="min-w-0">
                <p className="font-medium text-foreground truncate">{item.invoice_number}</p>
                <p className="text-sm text-muted-foreground truncate">
                  {item.vendor_name} · {item.approver_label}
                </p>
              </div>
              <span
                className={`text-xs font-medium rounded-full px-2 py-1 ${slaTextColor(item.percent_elapsed)}`}
              >
                {Math.round(item.percent_elapsed)}%
              </span>
            </div>
            <div className="mt-3 h-2 rounded-full bg-secondary overflow-hidden">
              <div
                className={`h-full rounded-full ${slaColor(item.percent_elapsed)}`}
                style={{ width: `${Math.min(item.percent_elapsed, 100)}%` }}
              />
            </div>
            <p className="mt-2 text-xs text-muted-foreground">
              Due {formatDeadline(item.deadline_at)} · {item.hours_waiting.toFixed(1)}h of{' '}
              {item.sla_hours}h elapsed
            </p>
          </Link>
        ))}
      </div>
      {remaining > 0 && (
        <Link
          href="/reports#approval-sla"
          className="block mt-3 text-sm font-medium text-primary hover:underline text-center"
        >
          {remaining} more →
        </Link>
      )}
    </div>
  );
}

function StageDwellHeatMap({ data, isLoading, isError }: { data: StageDwellRow[] | undefined; isLoading?: boolean; isError?: boolean }) {
  if (!data || data.length === 0) {
    if (isLoading) {
      return (
        <div className="card p-5" data-testid="stage-dwell-loading" role="status" aria-busy="true">
          <div className="flex items-center gap-3 mb-4">
            <div className="p-2.5 rounded-xl bg-warning/10">
              <Flame className="w-5 h-5 text-warning" />
            </div>
            <div>
              <h3 className="font-semibold text-foreground">Stage Dwell Time</h3>
              <p className="text-sm text-muted-foreground">Loading...</p>
            </div>
          </div>
          <div className="space-y-2">
            {Array.from({ length: 3 }).map((_, i) => (
              <div key={i} className="animate-pulse h-8 rounded-full bg-secondary" />
            ))}
          </div>
        </div>
      );
    }

    if (isError) {
      return (
        <div className="card p-5" data-testid="stage-dwell-error" role="alert">
          <div className="flex items-center gap-3 mb-4">
            <div className="p-2.5 rounded-xl bg-error/10">
              <AlertTriangle className="w-5 h-5 text-error" />
            </div>
            <div>
              <h3 className="font-semibold text-foreground">Stage Dwell Time</h3>
              <p className="text-sm text-error">Unable to load stage dwell data</p>
            </div>
          </div>
        </div>
      );
    }

    return (
      <div className="card p-5">
        <div className="flex items-center gap-3 mb-4">
          <div className="p-2.5 rounded-xl bg-warning/10">
            <Flame className="w-5 h-5 text-warning" />
          </div>
          <div>
            <h3 className="font-semibold text-foreground">Stage Dwell Time</h3>
            <p className="text-sm text-muted-foreground">No stage data yet</p>
          </div>
        </div>
      </div>
    );
  }

  const maxMedian = Math.max(...data.map((d) => d.median_minutes), 1);

  return (
    <div className="card p-5">
      <div className="flex items-center gap-3 mb-4">
        <div className="p-2.5 rounded-xl bg-warning/10">
          <Flame className="w-5 h-5 text-warning" />
        </div>
        <div>
          <h3 className="font-semibold text-foreground">Stage Dwell Time</h3>
          <p className="text-sm text-muted-foreground">Bottleneck heat map (30d)</p>
        </div>
      </div>
      <div className="space-y-3">
        {data.map((row) => (
          <div key={row.stage} data-testid={`stage-row-${row.stage}`}>
            <div className="flex items-center justify-between text-sm mb-1">
              <span className="text-foreground font-medium capitalize">{row.stage}</span>
              <span className="text-muted-foreground">
                {row.median_minutes.toFixed(0)}m median · {row.count} invoices
              </span>
            </div>
            <div className="h-3 rounded-full bg-secondary overflow-hidden">
              <div
                className={`h-full rounded-full ${dwellColor(row.median_minutes)}`}
                style={{ width: `${(row.median_minutes / maxMedian) * 100}%` }}
              />
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function ApproverWorkloadChart({ data, isLoading, isError }: { data: ApproverWorkloadRow[] | undefined; isLoading?: boolean; isError?: boolean }) {
  if (!data || data.length === 0) {
    if (isLoading) {
      return (
        <div className="card p-5" data-testid="approver-workload-loading" role="status" aria-busy="true">
          <div className="flex items-center gap-3 mb-4">
            <div className="p-2.5 rounded-xl bg-accent/10">
              <Users className="w-5 h-5 text-accent" />
            </div>
            <div>
              <h3 className="font-semibold text-foreground">Approver Workload</h3>
              <p className="text-sm text-muted-foreground">Loading...</p>
            </div>
          </div>
          <div className="space-y-2">
            {Array.from({ length: 3 }).map((_, i) => (
              <div key={i} className="animate-pulse h-8 rounded-full bg-secondary" />
            ))}
          </div>
        </div>
      );
    }

    if (isError) {
      return (
        <div className="card p-5" data-testid="approver-workload-error" role="alert">
          <div className="flex items-center gap-3 mb-4">
            <div className="p-2.5 rounded-xl bg-error/10">
              <AlertTriangle className="w-5 h-5 text-error" />
            </div>
            <div>
              <h3 className="font-semibold text-foreground">Approver Workload</h3>
              <p className="text-sm text-error">Unable to load approver workload</p>
            </div>
          </div>
        </div>
      );
    }

    return (
      <div className="card p-5">
        <div className="flex items-center gap-3 mb-4">
          <div className="p-2.5 rounded-xl bg-accent/10">
            <Users className="w-5 h-5 text-accent" />
          </div>
          <div>
            <h3 className="font-semibold text-foreground">Approver Workload</h3>
            <p className="text-sm text-muted-foreground">No pending approvals</p>
          </div>
        </div>
      </div>
    );
  }

  const maxPending = Math.max(...data.map((d) => d.pending_count), 1);

  return (
    <div className="card p-5">
      <div className="flex items-center gap-3 mb-4">
        <div className="p-2.5 rounded-xl bg-accent/10">
          <Users className="w-5 h-5 text-accent" />
        </div>
        <div>
          <h3 className="font-semibold text-foreground">Approver Workload</h3>
          <p className="text-sm text-muted-foreground">Pending count per approver</p>
        </div>
      </div>
      <div className="space-y-3">
        {data.map((row) => (
          <div key={row.approver_id} data-testid={`approver-row-${row.approver_id}`}>
            <div className="flex items-center justify-between text-sm mb-1">
              <span className="text-foreground font-medium">{row.approver_name}</span>
              <div className="flex items-center gap-3 text-muted-foreground">
                {row.breached_count > 0 && (
                  <span className="text-error">{row.breached_count} breached</span>
                )}
                {row.near_breach_count > 0 && (
                  <span className="text-warning">{row.near_breach_count} near</span>
                )}
                <span>{row.pending_count} pending</span>
              </div>
            </div>
            <div className="h-3 rounded-full bg-secondary overflow-hidden">
              <div
                className="h-full rounded-full bg-accent"
                style={{ width: `${(row.pending_count / maxPending) * 100}%` }}
              />
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function ExceptionTrendChart({ data, isLoading, isError }: { data: ExceptionTrendPoint[] | undefined; isLoading?: boolean; isError?: boolean }) {
  if (!data || data.length === 0) {
    if (isLoading) {
      return (
        <div className="card p-5" data-testid="exception-trend-loading" role="status" aria-busy="true">
          <div className="flex items-center gap-3 mb-4">
            <div className="p-2.5 rounded-xl bg-error/10">
              <TrendingUp className="w-5 h-5 text-error" />
            </div>
            <div>
              <h3 className="font-semibold text-foreground">Exception Rate Trend</h3>
              <p className="text-sm text-muted-foreground">Loading...</p>
            </div>
          </div>
          <div className="animate-pulse h-32 rounded-xl bg-secondary" />
        </div>
      );
    }

    if (isError) {
      return (
        <div className="card p-5" data-testid="exception-trend-error" role="alert">
          <div className="flex items-center gap-3 mb-4">
            <div className="p-2.5 rounded-xl bg-error/10">
              <AlertTriangle className="w-5 h-5 text-error" />
            </div>
            <div>
              <h3 className="font-semibold text-foreground">Exception Rate Trend</h3>
              <p className="text-sm text-error">Unable to load exception trend data</p>
            </div>
          </div>
        </div>
      );
    }

    return (
      <div className="card p-5">
        <div className="flex items-center gap-3 mb-4">
          <div className="p-2.5 rounded-xl bg-error/10">
            <TrendingUp className="w-5 h-5 text-error" />
          </div>
          <div>
            <h3 className="font-semibold text-foreground">Exception Rate Trend</h3>
            <p className="text-sm text-muted-foreground">No data yet</p>
          </div>
        </div>
      </div>
    );
  }

  const maxRate = Math.max(...data.map((d) => d.exception_rate), 1);

  return (
    <div className="card p-5">
      <div className="flex items-center gap-3 mb-4">
        <div className="p-2.5 rounded-xl bg-error/10">
          <TrendingUp className="w-5 h-5 text-error" />
        </div>
        <div>
          <h3 className="font-semibold text-foreground">Exception Rate Trend</h3>
          <p className="text-sm text-muted-foreground">Last 14 days</p>
        </div>
      </div>
      <div className="flex items-end gap-1 h-32">
        {data.map((point) => (
          <div
            key={point.date}
            className="flex-1 flex flex-col items-center gap-1"
            data-testid={`trend-bar-${point.date}`}
          >
            <div
              className="w-full rounded-t bg-error/70 min-h-[2px]"
              style={{ height: `${(point.exception_rate / maxRate) * 100}%` }}
            />
            <span className="text-[9px] text-muted-foreground leading-none">
              {point.date.slice(5)}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main section
// ---------------------------------------------------------------------------

export function SlaBottleneckSection() {
  const { data: approvalSla, isLoading: approvalSlaLoading, isError: approvalSlaError } = useQuery({
    queryKey: ['dashboard-approval-sla'],
    queryFn: () => reportsApi.approvalSla(),
  });

  const { data: stageDwell, isLoading: stageDwellLoading, isError: stageDwellError } = useQuery({
    queryKey: ['dashboard-stage-dwell'],
    queryFn: () => dashboardApi.getStageDwell(),
  });

  const { data: approverWorkload, isLoading: approverWorkloadLoading, isError: approverWorkloadError } = useQuery({
    queryKey: ['dashboard-approver-workload'],
    queryFn: () => dashboardApi.getApproverWorkload(),
  });

  const { data: exceptionTrend, isLoading: exceptionTrendLoading, isError: exceptionTrendError } = useQuery({
    queryKey: ['dashboard-exception-trend'],
    queryFn: () => dashboardApi.getExceptionTrend(),
  });

  return (
    <div className="space-y-4">
      <h2 className="text-sm font-semibold text-muted-foreground uppercase tracking-wider flex items-center gap-2">
        <AlertTriangle className="w-4 h-4" />
        SLA &amp; Bottlenecks
      </h2>

      {/* Top row: At-Risk Invoices */}
      <AtRiskInvoices data={approvalSla} isLoading={approvalSlaLoading} isError={approvalSlaError} />

      {/* Middle row: Stage Dwell + Approver Workload */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <StageDwellHeatMap data={stageDwell} isLoading={stageDwellLoading} isError={stageDwellError} />
        <ApproverWorkloadChart data={approverWorkload} isLoading={approverWorkloadLoading} isError={approverWorkloadError} />
      </div>

      {/* Bottom row: Exception Rate Trend */}
      <ExceptionTrendChart data={exceptionTrend} isLoading={exceptionTrendLoading} isError={exceptionTrendError} />
    </div>
  );
}

export default SlaBottleneckSection;
