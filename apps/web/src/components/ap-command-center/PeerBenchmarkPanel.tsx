'use client';

import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  benchmarkApi,
  type BenchmarkResponse,
  type BenchmarkOptInRequest,
} from '@/lib/api';
import { BarChart3, Info } from 'lucide-react';

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const INDUSTRIES = [
  'manufacturing',
  'retail',
  'healthcare',
  'financial_services',
  'technology',
  'construction',
  'education',
  'government',
  'other',
] as const;

const HEADCOUNT_BANDS = ['1-49', '50-200', '201-1000', '1000+'] as const;
const VOLUME_BANDS = ['0-499', '500-2000', '2001-10000', '10000+'] as const;

const METRIC_LABELS: Record<string, { label: string; unit: string; lowerIsBetter: boolean }> = {
  dpo_days: { label: 'Days Payable Outstanding', unit: 'days', lowerIsBetter: false },
  avg_approval_cycle_hours: { label: 'Avg Approval Cycle', unit: 'hrs', lowerIsBetter: true },
  ocr_straight_through_rate: { label: 'OCR Straight-Through', unit: '%', lowerIsBetter: false },
  exception_rate: { label: 'Exception Rate', unit: '%', lowerIsBetter: true },
  discount_capture_rate: { label: 'Discount Capture', unit: '%', lowerIsBetter: false },
  cost_per_invoice: { label: 'Cost per Invoice', unit: '$', lowerIsBetter: true },
};

const METRIC_KEYS = [
  'dpo_days',
  'avg_approval_cycle_hours',
  'ocr_straight_through_rate',
  'exception_rate',
  'discount_capture_rate',
  'cost_per_invoice',
] as const;

// ---------------------------------------------------------------------------
// Opt-in form
// ---------------------------------------------------------------------------

function OptInForm({ onSubmit, isPending }: { onSubmit: (body: BenchmarkOptInRequest) => void; isPending: boolean }) {
  const [industry, setIndustry] = useState('');
  const [headcount, setHeadcount] = useState('');
  const [volume, setVolume] = useState('');

  const canSubmit = industry && headcount && volume;

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2 text-muted-foreground">
        <Info className="w-4 h-4" />
        <p className="text-sm">
          Opt in to compare your AP performance against anonymized peers. Your data
          is aggregated with at least 4 other tenants before any insights are shown.
        </p>
      </div>
      <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
        <select
          value={industry}
          onChange={(e) => setIndustry(e.target.value)}
          className="rounded-md border border-border bg-background px-3 py-2 text-sm"
          aria-label="Industry"
        >
          <option value="">Select industry</option>
          {INDUSTRIES.map((i) => (
            <option key={i} value={i}>
              {i.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase())}
            </option>
          ))}
        </select>
        <select
          value={headcount}
          onChange={(e) => setHeadcount(e.target.value)}
          className="rounded-md border border-border bg-background px-3 py-2 text-sm"
          aria-label="Headcount band"
        >
          <option value="">Headcount</option>
          {HEADCOUNT_BANDS.map((h) => (
            <option key={h} value={h}>
              {h} employees
            </option>
          ))}
        </select>
        <select
          value={volume}
          onChange={(e) => setVolume(e.target.value)}
          className="rounded-md border border-border bg-background px-3 py-2 text-sm"
          aria-label="Monthly invoice volume band"
        >
          <option value="">Invoice volume / mo</option>
          {VOLUME_BANDS.map((v) => (
            <option key={v} value={v}>
              {v} invoices
            </option>
          ))}
        </select>
      </div>
      <button
        onClick={() => onSubmit({ industry, headcount_band: headcount, volume_band: volume })}
        disabled={!canSubmit || isPending}
        className="rounded-lg bg-primary text-primary-foreground px-4 py-2 text-sm font-medium hover:bg-primary/90 disabled:opacity-50 transition-colors"
      >
        {isPending ? 'Opting in...' : 'Opt In'}
      </button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Metric row with horizontal bar
// ---------------------------------------------------------------------------

function formatValue(key: string, value: number): string {
  if (key === 'ocr_straight_through_rate' || key === 'exception_rate' || key === 'discount_capture_rate') {
    return `${(value * 100).toFixed(1)}%`;
  }
  if (key === 'cost_per_invoice') {
    return `$${value.toFixed(2)}`;
  }
  return value.toFixed(1);
}

function MetricRow({ kpiKey, tenant, p25, p50, p75 }: { kpiKey: string; tenant: number; p25: number; p50: number; p75: number }) {
  const meta = METRIC_LABELS[kpiKey] ?? { label: kpiKey, unit: '', lowerIsBetter: true };

  // Compute bar position (0-100 range)
  const range = p75 - p25 || 1;
  const clamped = Math.min(Math.max(tenant, p25), p75);
  const markerPct = ((clamped - p25) / range) * 100;

  return (
    <div className="py-2 border-b border-border last:border-b-0">
      <div className="flex items-center justify-between mb-1">
        <span className="text-sm font-medium text-foreground">{meta.label}</span>
        <div className="flex items-center gap-3 text-xs text-muted-foreground">
          <span>p50: <strong className="text-foreground">{formatValue(kpiKey, p50)}</strong></span>
          <span>You: <strong className="text-foreground">{formatValue(kpiKey, tenant)}</strong></span>
        </div>
      </div>
      {/* Horizontal bar: p25-p75 band with tenant marker */}
      <div className="relative h-2 bg-muted rounded-full">
        <div className="absolute inset-y-0 bg-primary/20 rounded-full" style={{ left: 0, right: 0 }} />
        <div
          className="absolute top-1/2 -translate-y-1/2 w-2.5 h-2.5 rounded-full bg-primary border-2 border-background"
          style={{ left: `calc(${markerPct}% - 5px)` }}
        />
      </div>
      <div className="flex justify-between text-[10px] text-muted-foreground mt-0.5">
        <span>p25: {formatValue(kpiKey, p25)}</span>
        <span>p75: {formatValue(kpiKey, p75)}</span>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Data display (opted in with cohort data)
// ---------------------------------------------------------------------------

function BenchmarkData({ data }: { data: BenchmarkResponse }) {
  if (!data.tenant_kpis || !data.cohort_kpis) {
    return (
      <div className="flex items-center gap-2 text-sm text-amber-600">
        <Info className="w-4 h-4" />
        <span>
          Your peer cohort does not yet have enough members (minimum 5) to show
          anonymized benchmarks. Check back after the next quarterly refresh.
        </span>
      </div>
    );
  }

  return (
    <div>
      {METRIC_KEYS.map((key) => (
        <MetricRow
          key={key}
          kpiKey={key}
          tenant={data.tenant_kpis![key as keyof typeof data.tenant_kpis] as number}
          p25={data.cohort_kpis!.p25[key as keyof typeof data.cohort_kpis.p25] as number}
          p50={data.cohort_kpis!.p50[key as keyof typeof data.cohort_kpis.p50] as number}
          p75={data.cohort_kpis!.p75[key as keyof typeof data.cohort_kpis.p75] as number}
        />
      ))}
      {data.cohort_size != null && (
        <p className="text-xs text-muted-foreground mt-2">
          Compared against {data.cohort_size} anonymized peers
          {data.cohort
            ? ` in ${data.cohort.industry.replace(/_/g, ' ')} (${data.cohort.headcount_band} employees, ${data.cohort.volume_band} invoices/mo)`
            : ''}.
        </p>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main panel
// ---------------------------------------------------------------------------

export default function PeerBenchmarkPanel() {
  const queryClient = useQueryClient();

  const { data, isLoading } = useQuery({
    queryKey: ['benchmark'],
    queryFn: () => benchmarkApi.get(),
  });

  const optInMut = useMutation({
    mutationFn: benchmarkApi.optIn,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['benchmark'] });
    },
  });

  return (
    <div className="bg-card rounded-lg border border-border shadow-sm">
      <div className="px-6 py-4 border-b border-border flex items-center gap-2">
        <BarChart3 className="w-5 h-5 text-primary" />
        <h2 className="text-lg font-semibold text-foreground">Peer Benchmark</h2>
      </div>
      <div className="px-6 py-4">
        {isLoading ? (
          <div className="flex items-center justify-center h-24">
            <div className="animate-spin rounded-full h-6 w-6 border-b-2 border-primary" />
          </div>
        ) : !data?.opted_in ? (
          <OptInForm
            onSubmit={(body) => optInMut.mutate(body)}
            isPending={optInMut.isPending}
          />
        ) : (
          <BenchmarkData data={data} />
        )}
      </div>
    </div>
  );
}
