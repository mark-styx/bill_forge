'use client';

import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { discountsApi, type DiscountWorklistRow, type DiscountKpi } from '@/lib/api';
import { useAuthStore } from '@/stores/auth';
import {
  Percent,
  Clock,
  TrendingUp,
  AlertTriangle,
  CheckCircle2,
  XCircle,
  Zap,
  DollarSign,
} from 'lucide-react';

function formatCents(cents: number): string {
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
  }).format(cents / 100);
}

function formatApr(bps: number): string {
  return (bps / 100).toFixed(1) + '%';
}

function DaysBadge({ days }: { days: number }) {
  if (days <= 1) return <span className="inline-flex items-center rounded-full bg-red-100 text-red-700 px-2 py-0.5 text-xs font-medium">{days}d left</span>;
  if (days <= 3) return <span className="inline-flex items-center rounded-full bg-amber-100 text-amber-700 px-2 py-0.5 text-xs font-medium">{days}d left</span>;
  return <span className="inline-flex items-center rounded-full bg-green-100 text-green-700 px-2 py-0.5 text-xs font-medium">{days}d left</span>;
}

export default function DiscountsPage() {
  const { hasModule } = useAuthStore();
  const queryClient = useQueryClient();
  const [capturingId, setCapturingId] = useState<string | null>(null);

  const { data: worklist, isLoading: wlLoading, error: wlError } = useQuery({
    queryKey: ['discounts', 'worklist'],
    queryFn: () => discountsApi.worklist(),
  });

  const { data: kpi, isLoading: kpiLoading } = useQuery({
    queryKey: ['discounts', 'kpi'],
    queryFn: () => discountsApi.kpi(),
  });

  const captureMutation = useMutation({
    mutationFn: (invoiceId: string) => discountsApi.capture(invoiceId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['discounts'] });
      setCapturingId(null);
    },
    onError: () => setCapturingId(null),
  });

  const handleCapture = (invoiceId: string) => {
    setCapturingId(invoiceId);
    captureMutation.mutate(invoiceId);
  };

  if (wlLoading || kpiLoading) {
    return (
      <div className="flex items-center justify-center h-96">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary" />
      </div>
    );
  }

  if (wlError) {
    return (
      <div className="flex items-center justify-center h-96">
        <p className="text-destructive">Failed to load discount worklist. Please try again.</p>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Page header */}
      <div>
        <h1 className="text-2xl font-bold text-foreground flex items-center gap-2">
          <Percent className="w-6 h-6" />
          Early-Payment Discounts
        </h1>
        <p className="text-muted-foreground mt-1">
          Capture early-payment discounts to save on invoice costs.
        </p>
      </div>

      {/* KPI strip */}
      <KpiStrip kpi={kpi} totalSavings={worklist?.total_potential_savings_cents ?? 0} />

      {/* Worklist table */}
      <div className="bg-card rounded-lg border border-border shadow-sm">
        <div className="px-6 py-4 border-b border-border flex items-center justify-between">
          <h2 className="text-lg font-semibold text-foreground">Discount Opportunities</h2>
          {worklist && worklist.items.length > 0 && (
            <span className="text-sm text-muted-foreground">
              {worklist.count_recommended} recommended of {worklist.items.length} total
            </span>
          )}
        </div>

        {!worklist || worklist.items.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-16 text-muted-foreground">
            <CheckCircle2 className="w-12 h-12 mb-3 text-green-500" />
            <p className="text-lg font-medium">All caught up</p>
            <p className="text-sm">No invoices with active early-payment discounts right now.</p>
          </div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-border bg-muted/30">
                  <th className="text-left px-4 py-3 font-medium text-muted-foreground">Vendor</th>
                  <th className="text-left px-4 py-3 font-medium text-muted-foreground">Invoice</th>
                  <th className="text-right px-4 py-3 font-medium text-muted-foreground">Amount</th>
                  <th className="text-center px-4 py-3 font-medium text-muted-foreground">Terms</th>
                  <th className="text-center px-4 py-3 font-medium text-muted-foreground">Deadline</th>
                  <th className="text-right px-4 py-3 font-medium text-muted-foreground">Net Savings</th>
                  <th className="text-right px-4 py-3 font-medium text-muted-foreground">Effective APR</th>
                  <th className="text-center px-4 py-3 font-medium text-muted-foreground">Action</th>
                </tr>
              </thead>
              <tbody>
                {worklist.items.map((row) => (
                  <tr key={row.invoice_id} className="border-b border-border hover:bg-muted/20 transition-colors">
                    <td className="px-4 py-3 font-medium text-foreground">{row.vendor_name}</td>
                    <td className="px-4 py-3 text-muted-foreground">{row.invoice_number}</td>
                    <td className="px-4 py-3 text-right text-foreground">{formatCents(row.amount_cents)}</td>
                    <td className="px-4 py-3 text-center text-muted-foreground">
                      {row.discount_percent}/{row.discount_days} net {row.net_days}
                    </td>
                    <td className="px-4 py-3 text-center">
                      <DaysBadge days={row.days_remaining} />
                    </td>
                    <td className="px-4 py-3 text-right font-medium text-green-600">
                      {formatCents(row.net_savings_cents)}
                    </td>
                    <td className="px-4 py-3 text-right text-foreground">
                      {formatApr(row.effective_apr_bps)}
                      {row.recommended && (
                        <span className="ml-1 inline-flex items-center rounded-full bg-blue-100 text-blue-700 px-1.5 py-0.5 text-[10px] font-semibold">
                          <Zap className="w-3 h-3 mr-0.5" />REC
                        </span>
                      )}
                    </td>
                    <td className="px-4 py-3 text-center">
                      <button
                        onClick={() => handleCapture(row.invoice_id)}
                        disabled={capturingId === row.invoice_id || captureMutation.isPending}
                        className="inline-flex items-center gap-1.5 rounded-lg bg-primary text-primary-foreground px-3 py-1.5 text-xs font-medium hover:bg-primary/90 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                      >
                        {capturingId === row.invoice_id ? (
                          <>
                            <div className="animate-spin rounded-full h-3 w-3 border-b-2 border-white" />
                            Capturing...
                          </>
                        ) : (
                          <>
                            <DollarSign className="w-3.5 h-3.5" />
                            Capture
                          </>
                        )}
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
}

function KpiStrip({ kpi, totalSavings }: { kpi?: DiscountKpi; totalSavings: number }) {
  const cards = [
    {
      label: 'Captured (30d)',
      value: kpi ? `${kpi.captured_count_30d} invoices` : '--',
      sub: kpi ? formatCents(kpi.captured_savings_cents_30d) : '--',
      icon: CheckCircle2,
      color: 'text-green-600',
      bg: 'bg-green-50',
    },
    {
      label: 'Missed (30d)',
      value: kpi ? `${kpi.missed_count_30d} invoices` : '--',
      sub: kpi ? formatCents(kpi.missed_savings_cents_30d) : '--',
      icon: XCircle,
      color: 'text-red-600',
      bg: 'bg-red-50',
    },
    {
      label: 'Capture Rate',
      value: kpi ? `${kpi.capture_rate_pct}%` : '--',
      sub: 'Last 30 days',
      icon: TrendingUp,
      color: 'text-blue-600',
      bg: 'bg-blue-50',
    },
    {
      label: 'Potential Savings',
      value: formatCents(totalSavings),
      sub: 'Available now',
      icon: Clock,
      color: 'text-amber-600',
      bg: 'bg-amber-50',
    },
  ];

  return (
    <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
      {cards.map((card) => (
        <div key={card.label} className="bg-card rounded-lg border border-border p-4 flex items-start gap-3">
          <div className={`rounded-lg ${card.bg} p-2`}>
            <card.icon className={`w-5 h-5 ${card.color}`} />
          </div>
          <div>
            <p className="text-sm text-muted-foreground">{card.label}</p>
            <p className="text-lg font-semibold text-foreground">{card.value}</p>
            <p className="text-xs text-muted-foreground">{card.sub}</p>
          </div>
        </div>
      ))}
    </div>
  );
}
