'use client';

import { useEffect, useState } from 'react';
import Link from 'next/link';
import {
  AlertTriangle,
  ArrowDownToLine,
  ArrowUpFromLine,
  Cable,
  Clock,
  Inbox,
  Loader2,
  Lock,
} from 'lucide-react';
import { ediApi, EdiStatusResponse } from '@/lib/api';
import { useAuthStore } from '@/stores/auth';

function formatRelative(iso: string | null): string {
  if (!iso) return 'Never';
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return 'Unknown';
  return date.toLocaleString();
}

interface StatCardProps {
  label: string;
  value: number | string;
  icon: React.ReactNode;
  tone?: 'default' | 'warning';
  testId?: string;
}

function StatCard({ label, value, icon, tone = 'default', testId }: StatCardProps) {
  const ring =
    tone === 'warning'
      ? 'border-amber-200 dark:border-amber-900/50 bg-amber-50/40 dark:bg-amber-900/10'
      : 'border-zinc-200 dark:border-zinc-800 bg-white dark:bg-zinc-900';
  return (
    <div
      data-testid={testId}
      className={`flex flex-col gap-3 p-5 rounded-xl border ${ring}`}
    >
      <div className="flex items-center justify-between">
        <span className="text-xs font-medium uppercase tracking-wider text-zinc-500 dark:text-zinc-400">
          {label}
        </span>
        <div className="text-zinc-400 dark:text-zinc-500">{icon}</div>
      </div>
      <div className="text-2xl font-semibold text-zinc-900 dark:text-zinc-100" data-testid={`${testId}-value`}>
        {value}
      </div>
    </div>
  );
}

function EntitlementBlocked() {
  return (
    <div className="max-w-2xl mx-auto px-4 sm:px-6 py-12">
      <div className="rounded-xl border border-amber-200 dark:border-amber-900/50 bg-amber-50 dark:bg-amber-900/20 p-6">
        <div className="flex items-start gap-3">
          <Lock className="h-5 w-5 text-amber-600 dark:text-amber-300 mt-0.5" />
          <div>
            <h2 className="text-base font-semibold text-amber-800 dark:text-amber-100">
              EDI add-on required
            </h2>
            <p className="mt-1 text-sm text-amber-700 dark:text-amber-200">
              The EDI integration is not enabled on this tenant. Add the EDI module to your
              subscription to receive 810 invoices, send 820 remittances, and run ack-timeout
              detection.
            </p>
            <Link
              href="/settings?tab=billing"
              className="mt-3 inline-flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium text-amber-800 hover:text-amber-900 dark:text-amber-100 dark:hover:text-white bg-amber-100 dark:bg-amber-900/40 hover:bg-amber-200 dark:hover:bg-amber-900/60 rounded-lg transition-colors"
            >
              Manage billing
            </Link>
          </div>
        </div>
      </div>
    </div>
  );
}

export default function EdiStatusPage() {
  const hasModule = useAuthStore((s) => s.hasModule);
  const [status, setStatus] = useState<EdiStatusResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    ediApi
      .status()
      .then((data) => {
        if (!cancelled) {
          setStatus(data);
          setError(null);
        }
      })
      .catch((e: unknown) => {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : 'Failed to load EDI status');
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (loading) {
    return (
      <div className="flex items-center justify-center py-16">
        <Loader2 className="h-8 w-8 text-zinc-400 animate-spin" data-testid="edi-status-loading" />
      </div>
    );
  }

  // Entitlement is the source of truth from the server. If the request itself
  // failed before we got a response, fall back to the auth-store flag so a
  // downgraded tenant still sees the upgrade prompt rather than an error.
  const entitled = status?.entitled ?? hasModule('edi');
  if (!entitled) {
    return <EntitlementBlocked />;
  }

  return (
    <div className="max-w-5xl mx-auto px-4 sm:px-6 py-8">
      <div className="flex items-start justify-between mb-8">
        <div>
          <h1 className="text-2xl font-bold text-zinc-900 dark:text-zinc-100">EDI Status</h1>
          <p className="mt-1 text-sm text-zinc-500 dark:text-zinc-400">
            Real-time view of inbound and outbound EDI documents, pending acknowledgments, and
            ack-timeout alerts.
          </p>
        </div>
        <Link
          href="/integrations?category=erp"
          className="inline-flex items-center gap-1.5 px-3 py-1.5 text-sm text-zinc-500 hover:text-zinc-700 dark:text-zinc-400 dark:hover:text-zinc-200 transition-colors"
        >
          <Cable className="h-4 w-4" />
          Manage integrations
        </Link>
      </div>

      {error && (
        <div className="mb-6 rounded-lg border border-amber-200 bg-amber-50 dark:bg-amber-900/20 dark:border-amber-900/50 px-4 py-3 text-sm text-amber-800 dark:text-amber-200">
          {error}
        </div>
      )}

      {status && (
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
          <StatCard
            label="Pending acks"
            value={status.pending_acks}
            icon={<Inbox className="h-4 w-4" />}
            testId="edi-stat-pending-acks"
          />
          <StatCard
            label="Ack timeouts (24h)"
            value={status.ack_timeouts_last_24h}
            icon={<AlertTriangle className="h-4 w-4" />}
            tone={status.ack_timeouts_last_24h > 0 ? 'warning' : 'default'}
            testId="edi-stat-ack-timeouts"
          />
          <StatCard
            label="Last inbound"
            value={formatRelative(status.last_inbound_at)}
            icon={<ArrowDownToLine className="h-4 w-4" />}
            testId="edi-stat-last-inbound"
          />
          <StatCard
            label="Last outbound"
            value={formatRelative(status.last_outbound_at)}
            icon={<ArrowUpFromLine className="h-4 w-4" />}
            testId="edi-stat-last-outbound"
          />
        </div>
      )}

      {status && (
        <div className="mt-8 grid grid-cols-1 sm:grid-cols-3 gap-4">
          <StatCard
            label="Connection"
            value={status.connected ? (status.provider ?? 'connected') : 'Disconnected'}
            icon={<Cable className="h-4 w-4" />}
            testId="edi-stat-connection"
          />
          <StatCard
            label="Trading partners"
            value={status.partner_count}
            icon={<Cable className="h-4 w-4" />}
            testId="edi-stat-partners"
          />
          <StatCard
            label="Documents (lifetime)"
            value={status.document_count}
            icon={<Clock className="h-4 w-4" />}
            testId="edi-stat-documents"
          />
        </div>
      )}
    </div>
  );
}
