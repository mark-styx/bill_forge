'use client';

import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  vendorRiskApi,
  type VendorRiskAlert,
  type VendorRiskSeverity,
} from '@/lib/api';
import { useAuthStore } from '@/stores/auth';
import { AlertTriangle, ShieldAlert, CheckCircle2, Loader2 } from 'lucide-react';

// ---------------------------------------------------------------------------
// Severity styling
// ---------------------------------------------------------------------------

const SEVERITY_ORDER: VendorRiskSeverity[] = ['critical', 'high', 'medium', 'low'];

const SEVERITY_STYLES: Record<
  VendorRiskSeverity,
  { badge: string; row: string; icon: typeof ShieldAlert }
> = {
  critical: {
    badge: 'bg-red-100 text-red-700',
    row: 'border-l-4 border-red-500',
    icon: ShieldAlert,
  },
  high: {
    badge: 'bg-orange-100 text-orange-700',
    row: 'border-l-4 border-orange-500',
    icon: AlertTriangle,
  },
  medium: {
    badge: 'bg-amber-100 text-amber-700',
    row: 'border-l-4 border-amber-500',
    icon: AlertTriangle,
  },
  low: {
    badge: 'bg-blue-100 text-blue-700',
    row: 'border-l-4 border-blue-500',
    icon: AlertTriangle,
  },
};

const ALERT_TYPE_LABELS: Record<string, string> = {
  sanctions_hit: 'Sanctions / OFAC hit',
  pep_hit: 'PEP match',
  banking_change: 'Banking details changed',
  address_drift: 'Address drift detected',
  tax_id_reverify_failed: 'Tax ID re-verification failed',
  beneficial_owner_change: 'Beneficial owner changed',
  w9_expiring: 'W-9 expiring soon',
  w9_expired: 'W-9 expired',
  w8_expiring: 'W-8 expiring soon',
  w8_expired: 'W-8 expired',
  coi_expiring: 'Certificate of insurance expiring',
  coi_expired: 'Certificate of insurance expired',
  threshold_1099_no_w9: '1099 threshold crossed without W-9 on file',
};

function formatTimestamp(iso: string): string {
  try {
    return new Date(iso).toLocaleString();
  } catch {
    return iso;
  }
}

/**
 * Reduce a JSONB payload to a short human-readable preview for the table.
 * Banking-change alerts carry masked last-four pairs; sanctions hits carry
 * the screener match list. Everything else falls back to a compact JSON view.
 */
function payloadPreview(alert: VendorRiskAlert): string {
  const p = alert.payload;
  if (alert.alert_type === 'banking_change') {
    const oldFour = (p.old_account_last_four as string | null | undefined) ?? null;
    const newFour = (p.new_account_last_four as string | null | undefined) ?? null;
    if (oldFour && newFour) return `acct ****${oldFour} -> ****${newFour}`;
    if (newFour) return `new acct ****${newFour}`;
    return 'Banking details updated';
  }
  if (alert.alert_type === 'sanctions_hit') {
    const matches = Array.isArray(p.matches) ? p.matches : [];
    if (matches.length > 0) {
      const names = matches
        .slice(0, 2)
        .map((m) => (m as { matched_name?: string }).matched_name)
        .filter(Boolean)
        .join(', ');
      return names ? `Matched: ${names}` : `${matches.length} match(es)`;
    }
    return 'Sanctions screening flagged';
  }
  if (
    alert.alert_type === 'w9_expiring' ||
    alert.alert_type === 'w8_expiring' ||
    alert.alert_type === 'coi_expiring'
  ) {
    const expiresOn = p.expires_on as string | undefined;
    const daysUntil = p.days_until_expiry as number | undefined;
    if (expiresOn && typeof daysUntil === 'number') {
      return `Expires ${expiresOn} (${daysUntil} day${daysUntil === 1 ? '' : 's'} left)`;
    }
    return 'Expiring soon';
  }
  if (
    alert.alert_type === 'w9_expired' ||
    alert.alert_type === 'w8_expired' ||
    alert.alert_type === 'coi_expired'
  ) {
    const expiresOn = p.expires_on as string | undefined;
    const daysOverdue = p.days_overdue as number | undefined;
    if (expiresOn && typeof daysOverdue === 'number') {
      return `Expired ${expiresOn} (${daysOverdue} day${daysOverdue === 1 ? '' : 's'} overdue)`;
    }
    return 'Expired';
  }
  if (alert.alert_type === 'threshold_1099_no_w9') {
    const ytd = p.ytd_paid_cents as number | undefined;
    if (typeof ytd === 'number') {
      return `YTD paid $${(ytd / 100).toFixed(2)} - W-9 not on file`;
    }
    return '$600 1099 threshold crossed without W-9';
  }
  return JSON.stringify(p).slice(0, 120);
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export default function VendorRiskDashboardPage() {
  const { hasModule, isAuthenticated } = useAuthStore();
  const queryClient = useQueryClient();
  const [statusFilter, setStatusFilter] = useState<'open' | 'acknowledged'>('open');

  const { data, isLoading, error } = useQuery({
    queryKey: ['vendor-risk-alerts', statusFilter],
    queryFn: () => vendorRiskApi.list({ status: statusFilter }),
    enabled: isAuthenticated && hasModule('vendor_management'),
  });

  const acknowledgeMutation = useMutation({
    mutationFn: (alertId: string) => vendorRiskApi.acknowledge(alertId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['vendor-risk-alerts'] });
    },
  });

  // Entitlement guard: hide the page entirely when the tenant lacks the
  // Vendor Management module (the API rejects these calls anyway, but hiding
  // prevents a confusing 403 flash).
  if (!hasModule('vendor_management')) {
    return (
      <div className="flex items-center justify-center h-96">
        <p className="text-muted-foreground">
          Vendor Risk monitoring requires the Vendor Management module.
        </p>
      </div>
    );
  }

  const alerts = data?.items ?? [];

  // Group by severity (preserving the critical->low sort from the API).
  const grouped: Record<VendorRiskSeverity, VendorRiskAlert[]> = {
    critical: [],
    high: [],
    medium: [],
    low: [],
  };
  for (const a of alerts) {
    if (grouped[a.severity]) grouped[a.severity].push(a);
  }

  return (
    <div className="space-y-6 max-w-7xl mx-auto">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold text-foreground flex items-center gap-2">
            <ShieldAlert className="w-6 h-6" />
            Vendor Risk Monitor
          </h1>
          <p className="text-muted-foreground mt-0.5">
            Continuous sanctions, banking-change, beneficial-ownership, and
            tax-compliance (W-9/W-8/COI/1099-threshold) alerts. Open critical
            alerts block payment release until acknowledged; soft hits warn
            without blocking.
          </p>
        </div>
        <div className="flex items-center gap-2">
          {(['open', 'acknowledged'] as const).map((s) => (
            <button
              key={s}
              onClick={() => setStatusFilter(s)}
              className={`btn btn-sm ${
                statusFilter === s ? 'btn-primary' : 'btn-secondary'
              }`}
            >
              {s === 'open' ? 'Open' : 'Acknowledged'}
            </button>
          ))}
        </div>
      </div>

      {/* Summary strip */}
      <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
        {SEVERITY_ORDER.map((sev) => {
          const count = grouped[sev].length;
          const style = SEVERITY_STYLES[sev];
          const Icon = style.icon;
          return (
            <div
              key={sev}
              className={`card p-4 flex items-center gap-3 ${style.row}`}
            >
              <Icon className="w-5 h-5 text-muted-foreground" />
              <div>
                <div className="text-2xl font-bold text-foreground">{count}</div>
                <div className="text-xs uppercase text-muted-foreground">
                  {sev}
                </div>
              </div>
            </div>
          );
        })}
      </div>

      {/* Alert table */}
      {isLoading ? (
        <div className="card p-12 text-center text-muted-foreground">
          <Loader2 className="w-5 h-5 mx-auto mb-2 animate-spin" />
          Loading alerts...
        </div>
      ) : error ? (
        <div className="card p-12 text-center text-destructive">
          Failed to load vendor-risk alerts. Please try again.
        </div>
      ) : alerts.length === 0 ? (
        <div className="card p-12 text-center">
          <CheckCircle2 className="w-10 h-10 mx-auto mb-3 text-green-500" />
          <p className="text-foreground font-medium mb-1">
            No {statusFilter} vendor-risk alerts
          </p>
          <p className="text-sm text-muted-foreground">
            {statusFilter === 'open'
              ? 'All clear. The VendorRiskRescan worker re-screens daily.'
              : 'No alerts have been acknowledged yet.'}
          </p>
        </div>
      ) : (
        <div className="card overflow-hidden">
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead className="bg-muted/50 text-muted-foreground">
                <tr>
                  <th className="text-left px-4 py-3 font-medium">Severity</th>
                  <th className="text-left px-4 py-3 font-medium">Vendor</th>
                  <th className="text-left px-4 py-3 font-medium">Alert</th>
                  <th className="text-left px-4 py-3 font-medium">Detail</th>
                  <th className="text-left px-4 py-3 font-medium">Detected</th>
                  <th className="text-right px-4 py-3 font-medium">Action</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border">
                {alerts.map((alert) => {
                  const style = SEVERITY_STYLES[alert.severity];
                  return (
                    <tr key={alert.id} className={style.row}>
                      <td className="px-4 py-3">
                        <span
                          className={`inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium ${style.badge}`}
                        >
                          {alert.severity}
                        </span>
                      </td>
                      <td className="px-4 py-3 font-medium text-foreground">
                        {alert.vendor_name}
                      </td>
                      <td className="px-4 py-3 text-foreground">
                        {ALERT_TYPE_LABELS[alert.alert_type] ?? alert.alert_type}
                      </td>
                      <td className="px-4 py-3 text-muted-foreground">
                        {payloadPreview(alert)}
                      </td>
                      <td className="px-4 py-3 text-muted-foreground whitespace-nowrap">
                        {formatTimestamp(alert.created_at)}
                      </td>
                      <td className="px-4 py-3 text-right">
                        {alert.status === 'open' ? (
                          <button
                            onClick={() => acknowledgeMutation.mutate(alert.id)}
                            disabled={
                              acknowledgeMutation.isPending &&
                              acknowledgeMutation.variables === alert.id
                            }
                            className="btn btn-primary btn-sm"
                          >
                            {acknowledgeMutation.isPending &&
                            acknowledgeMutation.variables === alert.id
                              ? 'Acknowledging...'
                              : 'Acknowledge'}
                          </button>
                        ) : (
                          <span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
                            <CheckCircle2 className="w-3.5 h-3.5" />
                            {alert.acknowledged_at
                              ? formatTimestamp(alert.acknowledged_at)
                              : 'Acknowledged'}
                          </span>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </div>
  );
}
