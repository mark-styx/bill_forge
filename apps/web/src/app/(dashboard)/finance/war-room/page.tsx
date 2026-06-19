'use client';

import { useEffect, useMemo, useState } from 'react';
import { useQuery, useMutation } from '@tanstack/react-query';
import Link from 'next/link';
import { useRouter } from 'next/navigation';
import {
  reportsApi,
  type ApCashFlowSimulation,
} from '@/lib/api';
import {
  ChartContainer,
  BillForgeBarChart,
} from '@/components/ui/charts';
import {
  ArrowLeft,
  Radar,
  RotateCcw,
  DollarSign,
  AlertTriangle,
  Percent,
  Clock,
  Sparkles,
} from 'lucide-react';

const DEBOUNCE_MS = 300;

const formatCents = (value: number, currency = 'USD') =>
  new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency,
    maximumFractionDigits: 0,
  }).format(value / 100);

const formatCentsShort = (value: number) => {
  if (value >= 1_000_00) return `$${(value / 100).toLocaleString('en-US', { maximumFractionDigits: 0 })}`;
  return formatCents(value);
};

type WarRoomInputs = {
  pending_approval_delay_days: number;
  capture_all_epd: boolean;
  vendor_term_shift_days: number;
  override_funding_threshold_dollars: string;
};

const DEFAULT_INPUTS: WarRoomInputs = {
  pending_approval_delay_days: 0,
  capture_all_epd: false,
  vendor_term_shift_days: 0,
  override_funding_threshold_dollars: '',
};

export default function WarRoomPage() {
  const router = useRouter();
  const [inputs, setInputs] = useState<WarRoomInputs>({ ...DEFAULT_INPUTS });
  const [simResult, setSimResult] = useState<ApCashFlowSimulation | null>(null);
  const [simError, setSimError] = useState<string | null>(null);

  const baselineQuery = useQuery({
    queryKey: ['ap-cash-flow-forecast', 13],
    queryFn: () => reportsApi.apCashFlowForecast({ horizon_weeks: 13 }),
  });
  const forecast = baselineQuery.data;

  const simMutation = useMutation({
    mutationFn: () =>
      reportsApi.simulateApCashFlowForecast({
        horizon_weeks: 13,
        scenario: {
          pending_approval_delay_days: inputs.pending_approval_delay_days || null,
          capture_all_epd: inputs.capture_all_epd || null,
          vendor_term_shift_days: inputs.vendor_term_shift_days || null,
          override_funding_threshold_cents:
            inputs.override_funding_threshold_dollars === ''
              ? null
              : Math.max(0, Math.round(Number(inputs.override_funding_threshold_dollars) * 100)),
        },
      }),
    onSuccess: (data) => {
      setSimResult(data);
      setSimError(null);
    },
    onError: (err: Error) => {
      setSimError(err.message || 'Simulation failed');
    },
  });

  const scenarioActive =
    inputs.pending_approval_delay_days !== 0 ||
    inputs.capture_all_epd ||
    inputs.vendor_term_shift_days !== 0 ||
    inputs.override_funding_threshold_dollars !== '';

  // Re-run simulation on input change (debounced 300 ms).
  useEffect(() => {
    if (!scenarioActive) {
      setSimResult(null);
      setSimError(null);
      return;
    }
    const timer = setTimeout(() => {
      simMutation.mutate();
    }, DEBOUNCE_MS);
    return () => clearTimeout(timer);
    // We intentionally only re-fire on input changes; depending on the
    // mutation object would cause infinite re-runs because useMutation
    // returns a new handle every render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [inputs]);

  // KPI deltas (baseline vs scenario), same math as cash-flow-forecast/page.tsx.
  const deltas = useMemo(() => {
    if (!simResult) return null;
    const baselineTotal = simResult.baseline.weekly.reduce((s, w) => s + w.expected_amount, 0);
    const scenarioTotal = simResult.scenario.weekly.reduce((s, w) => s + w.expected_amount, 0);
    const baselineAlertDays = simResult.baseline.daily.filter((d) => d.funding_required).length;
    const scenarioAlertDays = simResult.scenario.daily.filter((d) => d.funding_required).length;
    let epdSavings = 0;
    let lateFeeExposure = 0;
    for (let i = 0; i < simResult.baseline.daily.length; i++) {
      const bd = simResult.baseline.daily[i];
      const sd = simResult.scenario.daily[i];
      if (!sd) continue;
      if (sd.expected_amount < bd.expected_amount) {
        epdSavings += bd.expected_amount - sd.expected_amount;
      } else if (sd.expected_amount > bd.expected_amount) {
        lateFeeExposure += sd.expected_amount - bd.expected_amount;
      }
    }
    return {
      baselineTotal,
      scenarioTotal,
      baselineAlertDays,
      scenarioAlertDays,
      epdSavings,
      lateFeeExposure,
    };
  }, [simResult]);

  const weeklyChartData = useMemo(
    () =>
      forecast?.weekly.map((w, i) => ({
        name: `W${i + 1}`,
        expected: Math.round(w.expected_amount / 100),
      })) ?? [],
    [forecast],
  );

  const scenarioWeeklyChartData = useMemo(
    () =>
      simResult?.scenario.weekly.map((w, i) => ({
        name: `W${i + 1}`,
        scenario: Math.round(w.expected_amount / 100),
      })) ?? [],
    [simResult],
  );

  const mergedWeeklyChartData = useMemo(() => {
    if (!simResult) return weeklyChartData;
    return weeklyChartData.map((w, i) => ({
      ...w,
      scenario: scenarioWeeklyChartData[i]?.scenario ?? 0,
    }));
  }, [weeklyChartData, scenarioWeeklyChartData, simResult]);

  const baselineTotalCents = useMemo(
    () => forecast?.weekly.reduce((s, w) => s + w.expected_amount, 0) ?? 0,
    [forecast],
  );
  const baselineAlertDays = useMemo(
    () => forecast?.daily.filter((d) => d.funding_required).length ?? 0,
    [forecast],
  );

  // Recommended actions, derived client-side from the simulation result.
  const recommendations = useMemo(() => {
    if (!simResult || !deltas) return [];
    const recs: Array<{
      id: string;
      title: string;
      detail: string;
      commitLabel: string;
      commitHref: string;
      tone: 'success' | 'warning' | 'vendor';
    }> = [];

    if (inputs.capture_all_epd && deltas.epdSavings > 0) {
      const epdDays = simResult.scenario.daily.reduce((acc, sd, i) => {
        const bd = simResult.baseline.daily[i];
        return acc + (bd && sd.expected_amount < bd.expected_amount ? 1 : 0);
      }, 0);
      recs.push({
        id: 'epd',
        title: `Capture EPD on ${epdDays} invoices (save ${formatCentsShort(deltas.epdSavings)})`,
        detail: 'Lock in early-payment discounts now in flight before their capture window closes.',
        commitLabel: 'Review in Discounts',
        commitHref: '/discounts',
        tone: 'success',
      });
    }

    if (
      inputs.pending_approval_delay_days > 0 &&
      deltas.scenarioAlertDays < deltas.baselineAlertDays
    ) {
      recs.push({
        id: 'delay',
        title: `Delay pending approvals by ${inputs.pending_approval_delay_days} days`,
        detail: `Reduces funding-alert days from ${deltas.baselineAlertDays} to ${deltas.scenarioAlertDays} by smoothing outflow.`,
        commitLabel: 'Review approval queue',
        commitHref: '/approvals',
        tone: 'warning',
      });
    }

    if (inputs.vendor_term_shift_days !== 0) {
      const sign = inputs.vendor_term_shift_days > 0 ? '+' : '';
      recs.push({
        id: 'vendor',
        title: `Renegotiate vendor terms (${sign}${inputs.vendor_term_shift_days} days)`,
        detail: 'Realign vendor net-term profiles to shift the modelled cash outflow curve.',
        commitLabel: 'Open vendor list',
        commitHref: '/vendors',
        tone: 'vendor',
      });
    }

    return recs;
  }, [simResult, deltas, inputs]);

  const handleReset = () => {
    setInputs({ ...DEFAULT_INPUTS });
    setSimResult(null);
    setSimError(null);
  };

  const toneClasses: Record<'success' | 'warning' | 'vendor', string> = {
    success: 'bg-success/10 text-success',
    warning: 'bg-warning/10 text-warning',
    vendor: 'bg-vendor/10 text-vendor',
  };

  if (baselineQuery.isLoading) {
    return (
      <div className="space-y-6 max-w-7xl mx-auto">
        <div className="flex items-center gap-2">
          <Link href="/dashboard" className="btn btn-secondary btn-sm">
            <ArrowLeft className="w-4 h-4 mr-1" /> Dashboard
          </Link>
        </div>
        <div className="flex items-center justify-center py-20 text-muted-foreground">
          Loading war room...
        </div>
      </div>
    );
  }

  if (baselineQuery.isError || !forecast) {
    return (
      <div className="space-y-6 max-w-7xl mx-auto">
        <div className="flex items-center gap-2">
          <Link href="/dashboard" className="btn btn-secondary btn-sm">
            <ArrowLeft className="w-4 h-4 mr-1" /> Dashboard
          </Link>
        </div>
        <div className="card p-6 text-center">
          <AlertTriangle className="w-8 h-8 text-error mx-auto mb-2" />
          <p className="font-medium text-foreground">Forecast unavailable</p>
          <p className="text-sm text-muted-foreground mt-1">
            Unable to load the cash position baseline.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6 max-w-7xl mx-auto">
      {/* Header strip */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div className="flex items-center gap-3">
          <Link href="/dashboard" className="btn btn-secondary btn-sm">
            <ArrowLeft className="w-4 h-4 mr-1" /> Dashboard
          </Link>
          <div className="flex items-center gap-2">
            <h1 className="text-2xl font-semibold text-foreground flex items-center gap-2">
              <Radar className="w-6 h-6 text-accent" />
              Cash Position War Room
            </h1>
            <span className="px-2 py-0.5 rounded-full text-xs font-semibold bg-accent/10 text-accent">
              Beta
            </span>
          </div>
        </div>
        <button className="btn btn-secondary btn-sm" onClick={handleReset}>
          <RotateCcw className="w-4 h-4 mr-1" />
          Reset Scenario
        </button>
      </div>
      <p className="text-muted-foreground -mt-2">
        Model what-if scenarios against your 13-week AP forecast and commit the actions that move cash.
      </p>

      {/* Three-column workspace */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6 items-start">
        {/* Left: scenario controls */}
        <section className="card overflow-hidden">
          <div className="h-1.5 bg-gradient-to-r from-accent via-accent/70 to-transparent" />
          <div className="p-6 space-y-5">
            <div className="flex items-center gap-3">
              <div className="p-2.5 rounded-xl bg-accent/10">
                <Sparkles className="w-5 h-5 text-accent" />
              </div>
              <div>
                <h2 className="font-semibold text-foreground">Scenario Controls</h2>
                <p className="text-sm text-muted-foreground">Auto-runs 300 ms after each change.</p>
              </div>
            </div>

            <div>
              <label className="block text-sm text-muted-foreground mb-1" htmlFor="delay-days">
                Delay pending approvals (days)
              </label>
              <div className="flex items-center gap-3">
                <input
                  id="delay-days"
                  type="range"
                  min={0}
                  max={30}
                  value={inputs.pending_approval_delay_days}
                  onChange={(e) =>
                    setInputs((prev) => ({
                      ...prev,
                      pending_approval_delay_days: Number(e.target.value),
                    }))
                  }
                  className="range range-sm flex-1"
                />
                <span className="text-sm font-medium text-foreground w-8 text-right">
                  {inputs.pending_approval_delay_days}
                </span>
              </div>
            </div>

            <div>
              <label className="block text-sm text-muted-foreground mb-1" htmlFor="vendor-shift">
                Shift vendor terms (days)
              </label>
              <div className="flex items-center gap-3">
                <input
                  id="vendor-shift"
                  type="range"
                  min={-15}
                  max={15}
                  value={inputs.vendor_term_shift_days}
                  onChange={(e) =>
                    setInputs((prev) => ({
                      ...prev,
                      vendor_term_shift_days: Number(e.target.value),
                    }))
                  }
                  className="range range-sm flex-1"
                />
                <span className="text-sm font-medium text-foreground w-8 text-right">
                  {inputs.vendor_term_shift_days > 0 ? '+' : ''}
                  {inputs.vendor_term_shift_days}
                </span>
              </div>
            </div>

            <label className="flex items-center gap-2 cursor-pointer">
              <input
                id="capture-epd"
                type="checkbox"
                checked={inputs.capture_all_epd}
                onChange={(e) =>
                  setInputs((prev) => ({ ...prev, capture_all_epd: e.target.checked }))
                }
                className="checkbox checkbox-sm"
              />
              <span className="text-sm text-foreground">Capture all EPD</span>
            </label>

            <div>
              <label className="block text-sm text-muted-foreground mb-1" htmlFor="threshold-override">
                Override funding threshold ($)
              </label>
              <input
                id="threshold-override"
                type="number"
                min={0}
                value={inputs.override_funding_threshold_dollars}
                onChange={(e) =>
                  setInputs((prev) => ({
                    ...prev,
                    override_funding_threshold_dollars: e.target.value,
                  }))
                }
                placeholder="No override"
                className="input input-bordered w-full"
              />
            </div>

            {simError && <p className="text-sm text-error">{simError}</p>}
            {simMutation.isPending && (
              <p className="text-sm text-muted-foreground">Running simulation...</p>
            )}
          </div>
        </section>

        {/* Center: KPI deltas + overlay chart */}
        <section className="space-y-6">
          <div className="grid grid-cols-2 gap-3">
            <div className="card p-4">
              <div className="flex items-center gap-2 mb-1">
                <DollarSign className="w-4 h-4 text-accent" />
                <p className="text-xs text-muted-foreground">Total AP</p>
              </div>
              <p className="text-lg font-semibold text-foreground">
                {formatCentsShort(deltas ? deltas.scenarioTotal : baselineTotalCents)}
              </p>
              {deltas && (
                <p className="text-xs text-muted-foreground">
                  baseline {formatCentsShort(deltas.baselineTotal)}
                </p>
              )}
            </div>
            <div className="card p-4">
              <div className="flex items-center gap-2 mb-1">
                <AlertTriangle className="w-4 h-4 text-error" />
                <p className="text-xs text-muted-foreground">Funding-Alert Days</p>
              </div>
              <p className="text-lg font-semibold text-foreground">
                {deltas ? deltas.scenarioAlertDays : baselineAlertDays}
              </p>
              {deltas && (
                <p className="text-xs text-muted-foreground">
                  baseline {deltas.baselineAlertDays}
                </p>
              )}
            </div>
            <div className="card p-4">
              <div className="flex items-center gap-2 mb-1">
                <Percent className="w-4 h-4 text-success" />
                <p className="text-xs text-muted-foreground">EPD Savings</p>
              </div>
              <p className="text-lg font-semibold text-success">
                {formatCentsShort(deltas?.epdSavings ?? 0)}
              </p>
            </div>
            <div className="card p-4">
              <div className="flex items-center gap-2 mb-1">
                <Clock className="w-4 h-4 text-warning" />
                <p className="text-xs text-muted-foreground">Late-Fee Exposure</p>
              </div>
              <p className="text-lg font-semibold text-warning">
                {formatCentsShort(deltas?.lateFeeExposure ?? 0)}
              </p>
            </div>
          </div>

          <ChartContainer
            title="Weekly Outflow Projection"
            description={simResult ? 'Baseline vs scenario (in dollars)' : 'Baseline (in dollars)'}
          >
            <BillForgeBarChart
              data={simResult ? mergedWeeklyChartData : weeklyChartData}
              dataKey={simResult ? ['expected', 'scenario'] : ['expected']}
              xAxisKey="name"
              height={280}
              colors={
                simResult
                  ? ['hsl(var(--primary))', 'hsl(var(--warning))']
                  : ['hsl(var(--primary))']
              }
              formatter={(v: number) => `$${v.toLocaleString()}`}
            />
          </ChartContainer>
        </section>

        {/* Right: recommended actions */}
        <section className="card overflow-hidden">
          <div className="h-1.5 bg-gradient-to-r from-reporting via-accent to-transparent" />
          <div className="p-6">
            <div className="flex items-center gap-3 mb-4">
              <div className="p-2.5 rounded-xl bg-reporting/10">
                <Sparkles className="w-5 h-5 text-reporting" />
              </div>
              <div>
                <h2 className="font-semibold text-foreground">Recommended Actions</h2>
                <p className="text-sm text-muted-foreground">Commit each action to the right review surface.</p>
              </div>
            </div>

            {recommendations.length === 0 ? (
              <p className="text-sm text-muted-foreground py-6 text-center">
                Adjust a scenario control to surface commitable actions.
              </p>
            ) : (
              <ul className="space-y-3" aria-label="Recommended actions list">
                {recommendations.map((rec) => (
                  <li
                    key={rec.id}
                    className={`rounded-xl border border-border p-4 ${toneClasses[rec.tone]}`}
                  >
                    <p className="font-medium text-foreground">{rec.title}</p>
                    <p className="text-sm text-muted-foreground mt-1">{rec.detail}</p>
                    <button
                      type="button"
                      className="btn btn-primary btn-sm mt-3"
                      onClick={() => router.push(rec.commitHref)}
                    >
                      {rec.commitLabel}
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </section>
      </div>
    </div>
  );
}
