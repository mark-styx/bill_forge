'use client';

import { useMemo, useState, useCallback } from 'react';
import { useQuery, useMutation } from '@tanstack/react-query';
import Link from 'next/link';
import {
  reportsApi,
  type ApCashFlowSimulation,
} from '@/lib/api';
import {
  ChartContainer,
  BillForgeBarChart,
} from '@/components/ui/charts';
import {
  DollarSign,
  AlertTriangle,
  Calendar,
  TrendingUp,
  ArrowLeft,
  FlaskConical,
} from 'lucide-react';

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

type BreakdownTab = 'vendor' | 'gl';

export default function CashFlowForecastPage() {
  const [breakdownTab, setBreakdownTab] = useState<BreakdownTab>('vendor');

  // What-If Simulator state
  const [simDelayDays, setSimDelayDays] = useState(0);
  const [simCaptureEpd, setSimCaptureEpd] = useState(false);
  const [simVendorShift, setSimVendorShift] = useState(0);
  const [simResult, setSimResult] = useState<ApCashFlowSimulation | null>(null);
  const [simError, setSimError] = useState<string | null>(null);

  const forecastQuery = useQuery({
    queryKey: ['ap-cash-flow-forecast', 13],
    queryFn: () => reportsApi.apCashFlowForecast({ horizon_weeks: 13 }),
  });

  const forecast = forecastQuery.data;

  // Simulation mutation
  const simMutation = useMutation({
    mutationFn: () =>
      reportsApi.simulateApCashFlowForecast({
        horizon_weeks: 13,
        scenario: {
          pending_approval_delay_days: simDelayDays || null,
          capture_all_epd: simCaptureEpd || null,
          vendor_term_shift_days: simVendorShift || null,
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

  const handleResetSim = useCallback(() => {
    setSimDelayDays(0);
    setSimCaptureEpd(false);
    setSimVendorShift(0);
    setSimResult(null);
    setSimError(null);
  }, []);

  // Top KPIs
  const totalExpected = useMemo(
    () => forecast?.weekly.reduce((sum, w) => sum + w.expected_amount, 0) ?? 0,
    [forecast],
  );
  const peakWeek = useMemo(
    () => {
      const emptyWeek = { expected_amount: 0, week_start: '', week_end: '', low_band: 0, high_band: 0 };
      if (!forecast) return emptyWeek;
      return forecast.weekly.reduce(
        (max, w) => (w.expected_amount > max.expected_amount ? w : max),
        emptyWeek,
      );
    },
    [forecast],
  );
  const fundingAlertDays = useMemo(
    () => forecast?.daily.filter((d) => d.funding_required) ?? [],
    [forecast],
  );

  // Weekly chart data
  const weeklyChartData = useMemo(
    () =>
      forecast?.weekly.map((w, i) => ({
        name: `W${i + 1}`,
        expected: Math.round(w.expected_amount / 100),
        low: Math.round(w.low_band / 100),
        high: Math.round(w.high_band / 100),
      })) ?? [],
    [forecast],
  );

  // Scenario weekly chart data (overlay)
  const scenarioWeeklyChartData = useMemo(
    () =>
      simResult?.scenario.weekly.map((w, i) => ({
        name: `W${i + 1}`,
        scenario: Math.round(w.expected_amount / 100),
      })) ?? [],
    [simResult],
  );

  // Merged chart data when simulation is active
  const mergedWeeklyChartData = useMemo(() => {
    if (!simResult) return weeklyChartData;
    return weeklyChartData.map((w, i) => ({
      ...w,
      scenario: scenarioWeeklyChartData[i]?.scenario ?? 0,
    }));
  }, [weeklyChartData, scenarioWeeklyChartData, simResult]);

  // Scenario KPI deltas
  const scenarioDeltas = useMemo(() => {
    if (!simResult) return null;
    const baselineTotal = simResult.baseline.weekly.reduce((s, w) => s + w.expected_amount, 0);
    const scenarioTotal = simResult.scenario.weekly.reduce((s, w) => s + w.expected_amount, 0);
    const baselineAlertDays = simResult.baseline.daily.filter((d) => d.funding_required).length;
    const scenarioAlertDays = simResult.scenario.daily.filter((d) => d.funding_required).length;
    // EPD discount captured = sum of (baseline - scenario) for dates where scenario < baseline
    let epdSavings = 0;
    for (let i = 0; i < simResult.baseline.daily.length; i++) {
      const bd = simResult.baseline.daily[i];
      const sd = simResult.scenario.daily[i];
      if (sd && sd.expected_amount < bd.expected_amount) {
        epdSavings += bd.expected_amount - sd.expected_amount;
      }
    }
    return { baselineTotal, scenarioTotal, baselineAlertDays, scenarioAlertDays, epdSavings };
  }, [simResult]);

  // Aggregated vendor/GL breakdown across the horizon
  const vendorBreakdown = useMemo(() => {
    if (!forecast) return [];
    const map = new Map<string, number>();
    for (const day of forecast.daily) {
      for (const v of day.vendor_breakdown) {
        map.set(v.name, (map.get(v.name) ?? 0) + v.amount_cents);
      }
    }
    return Array.from(map.entries())
      .map(([name, amount_cents]) => ({ name, amount_cents }))
      .sort((a, b) => b.amount_cents - a.amount_cents);
  }, [forecast]);

  const glBreakdown = useMemo(() => {
    if (!forecast) return [];
    const map = new Map<string, number>();
    for (const day of forecast.daily) {
      for (const g of day.gl_breakdown) {
        map.set(g.name, (map.get(g.name) ?? 0) + g.amount_cents);
      }
    }
    return Array.from(map.entries())
      .map(([name, amount_cents]) => ({ name, amount_cents }))
      .sort((a, b) => b.amount_cents - a.amount_cents);
  }, [forecast]);

  // EPD windows in next 14 days
  const epdWindows = useMemo(() => {
    if (!forecast) return [];
    const today = new Date();
    const fourteen = new Date(today);
    fourteen.setDate(fourteen.getDate() + 14);
    // Days with vendor breakdowns that shifted earlier than due_date are EPD-impacted
    // For simplicity, highlight days with non-zero funding_required in the next 14 days
    return forecast.daily
      .filter((d) => {
        const date = new Date(d.date);
        return date <= fourteen && d.expected_amount > 0;
      })
      .slice(0, 14);
  }, [forecast]);

  const breakdownData = breakdownTab === 'vendor' ? vendorBreakdown : glBreakdown;

  if (forecastQuery.isLoading) {
    return (
      <div className="space-y-6 max-w-7xl mx-auto">
        <div className="flex items-center gap-2">
          <Link href="/reports" className="btn btn-secondary btn-sm">
            <ArrowLeft className="w-4 h-4 mr-1" /> Reports
          </Link>
        </div>
        <div className="flex items-center justify-center py-20 text-muted-foreground">
          Loading forecast data...
        </div>
      </div>
    );
  }

  if (forecastQuery.isError || !forecast) {
    return (
      <div className="space-y-6 max-w-7xl mx-auto">
        <div className="flex items-center gap-2">
          <Link href="/reports" className="btn btn-secondary btn-sm">
            <ArrowLeft className="w-4 h-4 mr-1" /> Reports
          </Link>
        </div>
        <div className="card p-6 text-center">
          <AlertTriangle className="w-8 h-8 text-error mx-auto mb-2" />
          <p className="font-medium text-foreground">Forecast unavailable</p>
          <p className="text-sm text-muted-foreground mt-1">
            Unable to load cash flow forecast data.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6 max-w-7xl mx-auto">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div className="flex items-center gap-3">
          <Link href="/reports" className="btn btn-secondary btn-sm">
            <ArrowLeft className="w-4 h-4 mr-1" /> Reports
          </Link>
          <div>
            <h1 className="text-2xl font-semibold text-foreground flex items-center gap-2">
              <TrendingUp className="w-6 h-6 text-accent" />
              13-Week Cash Flow Forecast
            </h1>
            <p className="text-muted-foreground mt-0.5">
              AP-driven funding projection as of {forecast.as_of_date}
            </p>
          </div>
        </div>
      </div>

      {/* KPI cards */}
      <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
        <div className="card p-5">
          <div className="flex items-center gap-3 mb-2">
            <div className="p-2.5 rounded-xl bg-accent/10">
              <DollarSign className="w-5 h-5 text-accent" />
            </div>
            <p className="text-sm text-muted-foreground">Total expected outflow</p>
          </div>
          <p className="text-2xl font-semibold text-foreground">{formatCents(totalExpected)}</p>
          <p className="text-xs text-muted-foreground mt-1">Next {forecast.horizon_weeks} weeks</p>
        </div>
        <div className="card p-5">
          <div className="flex items-center gap-3 mb-2">
            <div className="p-2.5 rounded-xl bg-warning/10">
              <TrendingUp className="w-5 h-5 text-warning" />
            </div>
            <p className="text-sm text-muted-foreground">Peak week</p>
          </div>
          <p className="text-2xl font-semibold text-foreground">{formatCents(peakWeek.expected_amount)}</p>
          <p className="text-xs text-muted-foreground mt-1">
            {peakWeek.week_start} to {peakWeek.week_end}
          </p>
        </div>
        <div className="card p-5">
          <div className="flex items-center gap-3 mb-2">
            <div className="p-2.5 rounded-xl bg-error/10">
              <AlertTriangle className="w-5 h-5 text-error" />
            </div>
            <p className="text-sm text-muted-foreground">Funding alert days</p>
          </div>
          <p className="text-2xl font-semibold text-foreground">{fundingAlertDays.length}</p>
          <p className="text-xs text-muted-foreground mt-1">
            Days exceeding funding threshold
          </p>
        </div>
      </div>

      {/* What-If Simulator */}
      <div className="card overflow-hidden">
        <div className="h-1.5 bg-gradient-to-r from-accent via-accent/70 to-transparent" />
        <div className="p-6">
          <div className="flex items-center gap-3 mb-4">
            <div className="p-2.5 rounded-xl bg-accent/10">
              <FlaskConical className="w-5 h-5 text-accent" />
            </div>
            <div>
              <h2 className="font-semibold text-foreground">What-If Simulator</h2>
              <p className="text-sm text-muted-foreground">
                Explore scenarios against the baseline forecast
              </p>
            </div>
          </div>
          <div className="grid grid-cols-1 sm:grid-cols-3 gap-4 mb-4">
            <div>
              <label className="block text-sm text-muted-foreground mb-1" htmlFor="sim-delay">
                Delay pending approvals by (days)
              </label>
              <input
                id="sim-delay"
                type="number"
                min={0}
                max={30}
                value={simDelayDays}
                onChange={(e) => setSimDelayDays(Math.max(0, Math.min(30, Number(e.target.value) || 0)))}
                className="input input-bordered w-full"
              />
            </div>
            <div className="flex items-end">
              <label className="flex items-center gap-2 cursor-pointer">
                <input
                  type="checkbox"
                  checked={simCaptureEpd}
                  onChange={(e) => setSimCaptureEpd(e.target.checked)}
                  className="checkbox checkbox-sm"
                />
                <span className="text-sm text-foreground">Capture every early-payment discount</span>
              </label>
            </div>
            <div>
              <label className="block text-sm text-muted-foreground mb-1" htmlFor="sim-vendor-shift">
                Shift vendor terms by (days)
              </label>
              <input
                id="sim-vendor-shift"
                type="number"
                min={-30}
                max={30}
                value={simVendorShift}
                onChange={(e) => setSimVendorShift(Math.max(-30, Math.min(30, Number(e.target.value) || 0)))}
                className="input input-bordered w-full"
              />
            </div>
          </div>
          <div className="flex gap-2">
            <button
              className="btn btn-primary btn-sm"
              onClick={() => simMutation.mutate()}
              disabled={simMutation.isPending}
            >
              {simMutation.isPending ? 'Running...' : 'Run simulation'}
            </button>
            <button
              className="btn btn-secondary btn-sm"
              onClick={handleResetSim}
            >
              Reset
            </button>
          </div>
          {simError && (
            <p className="text-sm text-error mt-2">{simError}</p>
          )}
        </div>
      </div>

      {/* Scenario KPI deltas */}
      {scenarioDeltas && (
        <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
          <div className="card p-4">
            <p className="text-sm text-muted-foreground">Total expected (baseline vs scenario)</p>
            <p className="text-lg font-semibold text-foreground">
              {formatCents(scenarioDeltas.baselineTotal)} / {formatCents(scenarioDeltas.scenarioTotal)}
            </p>
          </div>
          <div className="card p-4">
            <p className="text-sm text-muted-foreground">Funding-alert days (baseline vs scenario)</p>
            <p className="text-lg font-semibold text-foreground">
              {scenarioDeltas.baselineAlertDays} / {scenarioDeltas.scenarioAlertDays}
            </p>
          </div>
          <div className="card p-4">
            <p className="text-sm text-muted-foreground">EPD discount captured</p>
            <p className="text-lg font-semibold text-success">
              {formatCents(scenarioDeltas.epdSavings)}
            </p>
          </div>
        </div>
      )}

      {/* Weekly chart */}
      <ChartContainer title="Weekly Outflow Projection" description="Expected amount with confidence bands (in dollars)">
        <BillForgeBarChart
          data={simResult ? mergedWeeklyChartData : weeklyChartData}
          dataKey={simResult ? ['expected', 'scenario'] : ['expected']}
          xAxisKey="name"
          height={320}
          colors={simResult ? ['hsl(var(--primary))', 'hsl(var(--warning))'] : ['hsl(var(--primary))']}
          formatter={(v: number) => `$${v.toLocaleString()}`}
        />
      </ChartContainer>

      {/* Funding alerts */}
      {fundingAlertDays.length > 0 && (
        <div className="card overflow-hidden">
          <div className="h-1.5 bg-gradient-to-r from-error via-error/70 to-transparent" />
          <div className="p-6">
            <div className="flex items-center gap-3 mb-4">
              <div className="p-2.5 rounded-xl bg-error/10">
                <AlertTriangle className="w-5 h-5 text-error" />
              </div>
              <div>
                <h2 className="font-semibold text-foreground">Daily Funding Required</h2>
                <p className="text-sm text-muted-foreground">
                  Days where expected outflow exceeds threshold
                </p>
              </div>
            </div>
            <div className="divide-y divide-border rounded-xl border border-border overflow-hidden">
              {fundingAlertDays.slice(0, 20).map((day, index) => (
                <div
                  key={`${day.date}-${index}`}
                  className="flex items-center justify-between gap-3 p-3 hover:bg-secondary/40 transition-colors"
                >
                  <div className="min-w-0">
                    <p className="font-medium text-foreground">{day.date}</p>
                    <p className="text-sm text-muted-foreground">
                      {day.vendor_breakdown.slice(0, 3).map((v) => v.name).join(', ')}
                      {day.vendor_breakdown.length > 3 ? ` +${day.vendor_breakdown.length - 3} more` : ''}
                    </p>
                  </div>
                  <div className="text-right flex-shrink-0">
                    <p className="font-semibold text-error">{formatCentsShort(day.expected_amount)}</p>
                    <p className="text-xs text-muted-foreground">
                      Range: {formatCentsShort(day.low_band)} - {formatCentsShort(day.high_band)}
                    </p>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}

      {/* Breakdown tabs */}
      <div className="card overflow-hidden">
        <div className="p-6">
          <div className="flex items-center gap-3 mb-4">
            <div className="p-2.5 rounded-xl bg-reporting/10">
              <Calendar className="w-5 h-5 text-reporting" />
            </div>
            <div>
              <h2 className="font-semibold text-foreground">Breakdown</h2>
              <p className="text-sm text-muted-foreground">
                Aggregated across {forecast.horizon_weeks}-week horizon
              </p>
            </div>
          </div>

          <div className="flex gap-2 mb-4">
            <button
              className={`btn btn-sm ${breakdownTab === 'vendor' ? 'btn-primary' : 'btn-secondary'}`}
              onClick={() => setBreakdownTab('vendor')}
            >
              By Vendor
            </button>
            <button
              className={`btn btn-sm ${breakdownTab === 'gl' ? 'btn-primary' : 'btn-secondary'}`}
              onClick={() => setBreakdownTab('gl')}
            >
              By GL Code
            </button>
          </div>

          {breakdownData.length === 0 ? (
            <p className="text-sm text-muted-foreground py-4">No breakdown data available.</p>
          ) : (
            <div className="divide-y divide-border rounded-xl border border-border overflow-hidden">
              {breakdownData.map((entry) => (
                <div
                  key={entry.name}
                  className="flex items-center justify-between gap-3 p-3 hover:bg-secondary/40 transition-colors"
                >
                  <p className="font-medium text-foreground truncate">{entry.name}</p>
                  <p className="font-semibold text-foreground flex-shrink-0">
                    {formatCents(entry.amount_cents)}
                  </p>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      {/* EPD deadline callout */}
      {epdWindows.length > 0 && (
        <div className="card overflow-hidden">
          <div className="h-1.5 bg-gradient-to-r from-success via-success/70 to-transparent" />
          <div className="p-6">
            <div className="flex items-center gap-3 mb-4">
              <div className="p-2.5 rounded-xl bg-success/10">
                <Calendar className="w-5 h-5 text-success" />
              </div>
              <div>
                <h2 className="font-semibold text-foreground">Upcoming 14-Day Outlook</h2>
                <p className="text-sm text-muted-foreground">
                  Cash outflows in the next 14 days (includes EPD-impacted payments)
                </p>
              </div>
            </div>
            <div className="space-y-2">
              {epdWindows
                .filter((d) => d.expected_amount > 0)
                .map((day, index) => (
                  <div
                    key={`${day.date}-${index}`}
                    className="flex items-center justify-between text-sm"
                  >
                    <span className="text-muted-foreground">{day.date}</span>
                    <span className="font-medium text-foreground">
                      {formatCents(day.expected_amount)}
                    </span>
                  </div>
                ))}
            </div>
          </div>
        </div>
      )}

      {/* Link to Cash Calendar */}
      <div className="card p-4">
        <Link
          href="/reports/cash-calendar"
          className="flex items-center gap-3 hover:bg-secondary/40 rounded-lg p-2 -m-2 transition-colors"
        >
          <div className="p-2.5 rounded-xl bg-accent/10">
            <Calendar className="w-5 h-5 text-accent" />
          </div>
          <div>
            <p className="font-semibold text-foreground">Cash Calendar</p>
            <p className="text-sm text-muted-foreground">
              Day-by-day calendar with projected bank balance and drag-to-reschedule
            </p>
          </div>
        </Link>
      </div>
    </div>
  );
}
