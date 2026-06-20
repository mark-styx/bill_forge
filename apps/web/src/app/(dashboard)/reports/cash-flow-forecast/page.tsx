'use client';

import { useMemo, useState, useCallback } from 'react';
import { useQuery, useMutation } from '@tanstack/react-query';
import Link from 'next/link';
import { toast } from 'sonner';
import {
  reportsApi,
  type ApCashFlowSimulation,
  type SolverResult,
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
  Target,
  CheckCircle2,
  XCircle,
  Download,
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
  const [exportMenuOpen, setExportMenuOpen] = useState(false);
  const [exportBusy, setExportBusy] = useState<'csv' | 'slack' | 'email' | null>(null);

  // What-If Simulator state
  const [simDelayDays, setSimDelayDays] = useState(0);
  const [simCaptureEpd, setSimCaptureEpd] = useState(false);
  const [simVendorShift, setSimVendorShift] = useState(0);
  const [simResult, setSimResult] = useState<ApCashFlowSimulation | null>(null);
  const [simError, setSimError] = useState<string | null>(null);

  // Find Cash solver state
  const defaultTargetDate = useMemo(() => {
    const d = new Date();
    d.setDate(d.getDate() + 14);
    return d.toISOString().slice(0, 10);
  }, []);
  const [solveTargetDollars, setSolveTargetDollars] = useState<number>(0);
  const [solveStartingDollars, setSolveStartingDollars] = useState<number>(0);
  const [solveTargetDate, setSolveTargetDate] = useState<string>(defaultTargetDate);
  const [solveMaxDelayDays, setSolveMaxDelayDays] = useState<number>(14);
  const [solveAllowEpd, setSolveAllowEpd] = useState<boolean>(true);
  const [solveResult, setSolveResult] = useState<SolverResult | null>(null);
  const [solveError, setSolveError] = useState<string | null>(null);

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

  // Find Cash solver mutation
  const solveMutation = useMutation({
    mutationFn: () =>
      reportsApi.solveApCashFlowForecast({
        target_balance_cents: Math.round(solveTargetDollars * 100),
        target_date: solveTargetDate,
        starting_balance_cents: Math.round(solveStartingDollars * 100),
        max_delay_days: solveMaxDelayDays,
        allow_epd_capture: solveAllowEpd,
        respect_vendor_terms: true,
        horizon_weeks: 13,
      }),
    onSuccess: (data) => {
      setSolveResult(data);
      setSolveError(null);
    },
    onError: (err: Error) => {
      setSolveError(err.message || 'Solver failed');
    },
  });

  const buildScenarioForExport = useCallback(() => {
    if (simDelayDays === 0 && !simCaptureEpd && simVendorShift === 0) {
      return undefined;
    }
    return {
      pending_approval_delay_days: simDelayDays || null,
      capture_all_epd: simCaptureEpd || null,
      vendor_term_shift_days: simVendorShift || null,
    };
  }, [simDelayDays, simCaptureEpd, simVendorShift]);

  const handleExportCsv = useCallback(async () => {
    setExportBusy('csv');
    setExportMenuOpen(false);
    try {
      const result = await reportsApi.exportApCashFlowForecast({
        format: 'csv',
        horizon_weeks: 13,
        scenario: buildScenarioForExport(),
      });
      if (result.kind !== 'csv') throw new Error('Unexpected response');
      const url = URL.createObjectURL(result.blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `cash-flow-forecast-${new Date().toISOString().slice(0, 10)}.csv`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      toast.success('CSV downloaded');
    } catch (err) {
      toast.error((err as Error).message || 'CSV export failed');
    } finally {
      setExportBusy(null);
    }
  }, [buildScenarioForExport]);

  const handleExportSlack = useCallback(async () => {
    setExportMenuOpen(false);
    const input = window.prompt('Slack channel ID (or comma-separated channel IDs)');
    if (!input) return;
    const recipients = input
      .split(',')
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
    if (recipients.length === 0) return;
    setExportBusy('slack');
    try {
      await reportsApi.exportApCashFlowForecast({
        format: 'slack',
        horizon_weeks: 13,
        recipients,
        scenario: buildScenarioForExport(),
      });
      toast.success(`Posted forecast to ${recipients.length} Slack channel(s)`);
    } catch (err) {
      toast.error((err as Error).message || 'Slack export failed');
    } finally {
      setExportBusy(null);
    }
  }, [buildScenarioForExport]);

  const handleExportEmail = useCallback(async () => {
    setExportMenuOpen(false);
    const input = window.prompt('Email recipients (comma-separated)');
    if (!input) return;
    const recipients = input
      .split(',')
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
    if (recipients.length === 0) return;
    setExportBusy('email');
    try {
      await reportsApi.exportApCashFlowForecast({
        format: 'email',
        horizon_weeks: 13,
        recipients,
        scenario: buildScenarioForExport(),
      });
      toast.success(`Forecast emailed to ${recipients.length} recipient(s)`);
    } catch (err) {
      toast.error((err as Error).message || 'Email export failed');
    } finally {
      setExportBusy(null);
    }
  }, [buildScenarioForExport]);

  const handleApplySolution = useCallback(() => {
    if (!solveResult) return;
    const inputs = solveResult.recommended_inputs;
    const delay = inputs.pending_approval_delay_days ?? 0;
    const capture = Boolean(inputs.capture_all_epd);
    const shift = inputs.vendor_term_shift_days ?? 0;
    setSimDelayDays(delay);
    setSimCaptureEpd(capture);
    setSimVendorShift(shift);
    // Trigger the existing simulator with the recommended scenario inline
    // (state updates are async, so build the body from the solver result).
    reportsApi
      .simulateApCashFlowForecast({
        horizon_weeks: 13,
        scenario: {
          pending_approval_delay_days: delay || null,
          capture_all_epd: capture || null,
          vendor_term_shift_days: shift || null,
        },
      })
      .then((data) => {
        setSimResult(data);
        setSimError(null);
      })
      .catch((err: Error) => {
        setSimError(err.message || 'Simulation failed');
      });
  }, [solveResult]);

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
        <div className="relative">
          <button
            type="button"
            className="btn btn-secondary btn-sm"
            onClick={() => setExportMenuOpen((open) => !open)}
            disabled={exportBusy !== null}
            aria-haspopup="menu"
            aria-expanded={exportMenuOpen}
          >
            <Download className="w-4 h-4 mr-1" />
            {exportBusy ? `Exporting ${exportBusy}...` : 'Export'}
          </button>
          {exportMenuOpen && (
            <div
              role="menu"
              className="absolute right-0 mt-2 w-48 rounded-xl border border-border bg-card shadow-lg z-10 overflow-hidden"
            >
              <button
                role="menuitem"
                type="button"
                className="w-full text-left px-3 py-2 text-sm hover:bg-secondary/40"
                onClick={handleExportCsv}
              >
                Download CSV
              </button>
              <button
                role="menuitem"
                type="button"
                className="w-full text-left px-3 py-2 text-sm hover:bg-secondary/40"
                onClick={handleExportSlack}
              >
                Send to Slack
              </button>
              <button
                role="menuitem"
                type="button"
                className="w-full text-left px-3 py-2 text-sm hover:bg-secondary/40"
                onClick={handleExportEmail}
              >
                Email
              </button>
            </div>
          )}
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

      {/* Find Cash solver */}
      <div className="card overflow-hidden">
        <div className="h-1.5 bg-gradient-to-r from-success via-success/70 to-transparent" />
        <div className="p-6">
          <div className="flex items-center gap-3 mb-4">
            <div className="p-2.5 rounded-xl bg-success/10">
              <Target className="w-5 h-5 text-success" />
            </div>
            <div>
              <h2 className="font-semibold text-foreground">Find Cash</h2>
              <p className="text-sm text-muted-foreground">
                Recommend a payment schedule that meets a cash target by a date
              </p>
            </div>
          </div>
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-4">
            <div>
              <label className="block text-sm text-muted-foreground mb-1" htmlFor="solve-target-balance">
                Target balance ($)
              </label>
              <input
                id="solve-target-balance"
                type="number"
                min={0}
                step={100}
                value={solveTargetDollars}
                onChange={(e) => setSolveTargetDollars(Math.max(0, Number(e.target.value) || 0))}
                className="input input-bordered w-full"
              />
            </div>
            <div>
              <label className="block text-sm text-muted-foreground mb-1" htmlFor="solve-starting-balance">
                Starting balance ($)
              </label>
              <input
                id="solve-starting-balance"
                type="number"
                min={0}
                step={100}
                value={solveStartingDollars}
                onChange={(e) => setSolveStartingDollars(Math.max(0, Number(e.target.value) || 0))}
                className="input input-bordered w-full"
              />
            </div>
            <div>
              <label className="block text-sm text-muted-foreground mb-1" htmlFor="solve-target-date">
                Target date
              </label>
              <input
                id="solve-target-date"
                type="date"
                value={solveTargetDate}
                onChange={(e) => setSolveTargetDate(e.target.value)}
                className="input input-bordered w-full"
              />
            </div>
            <div>
              <label className="block text-sm text-muted-foreground mb-1" htmlFor="solve-max-delay">
                Max delay (days): {solveMaxDelayDays}
              </label>
              <input
                id="solve-max-delay"
                type="range"
                min={0}
                max={30}
                step={1}
                value={solveMaxDelayDays}
                onChange={(e) => setSolveMaxDelayDays(Number(e.target.value))}
                className="range range-sm w-full"
              />
            </div>
          </div>
          <div className="flex items-center gap-4 mb-4">
            <label className="flex items-center gap-2 cursor-pointer">
              <input
                type="checkbox"
                checked={solveAllowEpd}
                onChange={(e) => setSolveAllowEpd(e.target.checked)}
                className="checkbox checkbox-sm"
              />
              <span className="text-sm text-foreground">Allow early-payment discount capture</span>
            </label>
          </div>
          <div className="flex gap-2">
            <button
              className="btn btn-primary btn-sm"
              onClick={() => solveMutation.mutate()}
              disabled={solveMutation.isPending || solveTargetDollars <= 0}
            >
              {solveMutation.isPending ? 'Solving...' : 'Find cash'}
            </button>
            <button
              className="btn btn-secondary btn-sm"
              onClick={() => {
                setSolveResult(null);
                setSolveError(null);
              }}
            >
              Reset
            </button>
          </div>
          {solveError && <p className="text-sm text-error mt-2">{solveError}</p>}

          {solveResult && (
            <div className="mt-4 space-y-4">
              <div className="flex flex-wrap items-center gap-3">
                {solveResult.feasible ? (
                  <span className="inline-flex items-center gap-1.5 rounded-full bg-success/10 text-success px-3 py-1 text-sm font-medium">
                    <CheckCircle2 className="w-4 h-4" /> Feasible
                  </span>
                ) : (
                  <span className="inline-flex items-center gap-1.5 rounded-full bg-error/10 text-error px-3 py-1 text-sm font-medium">
                    <XCircle className="w-4 h-4" /> Infeasible under current constraints
                  </span>
                )}
                <span className="text-sm text-muted-foreground">
                  Target: {formatCents(solveResult.target_balance_cents)} by {solveResult.target_date}
                </span>
                <span className="text-sm text-muted-foreground">
                  Achieved: {formatCents(solveResult.achieved_balance_cents)} (
                  {solveResult.achieved_balance_cents >= solveResult.target_balance_cents ? '+' : ''}
                  {formatCents(
                    solveResult.achieved_balance_cents - solveResult.target_balance_cents,
                  )}{' '}
                  vs target)
                </span>
              </div>

              {solveResult.recommended_actions.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                  No actions needed - the baseline forecast already meets the target.
                </p>
              ) : (
                <div className="overflow-x-auto rounded-xl border border-border">
                  <table className="w-full text-sm">
                    <thead className="bg-secondary/40 text-muted-foreground">
                      <tr>
                        <th className="text-left px-3 py-2 font-medium">Vendor</th>
                        <th className="text-right px-3 py-2 font-medium">Amount</th>
                        <th className="text-left px-3 py-2 font-medium">Original due</th>
                        <th className="text-left px-3 py-2 font-medium">Recommended pay</th>
                        <th className="text-left px-3 py-2 font-medium">Action</th>
                      </tr>
                    </thead>
                    <tbody>
                      {solveResult.recommended_actions.slice(0, 25).map((a) => (
                        <tr key={a.invoice_id} className="border-t border-border">
                          <td className="px-3 py-2 text-foreground truncate max-w-[180px]">
                            {a.vendor_name}
                          </td>
                          <td className="px-3 py-2 text-right font-medium text-foreground">
                            {formatCents(a.amount_cents)}
                          </td>
                          <td className="px-3 py-2 text-muted-foreground">{a.original_due_date}</td>
                          <td className="px-3 py-2 text-muted-foreground">{a.recommended_pay_date}</td>
                          <td className="px-3 py-2">
                            <span
                              className={`inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium ${
                                a.action_kind === 'delay'
                                  ? 'bg-warning/10 text-warning'
                                  : a.action_kind === 'accelerate_epd'
                                    ? 'bg-success/10 text-success'
                                    : 'bg-secondary/40 text-muted-foreground'
                              }`}
                            >
                              {a.action_kind === 'delay'
                                ? 'Delay'
                                : a.action_kind === 'accelerate_epd'
                                  ? 'Capture EPD'
                                  : 'Hold'}
                            </span>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                  {solveResult.recommended_actions.length > 25 && (
                    <p className="text-xs text-muted-foreground px-3 py-2">
                      Showing 25 of {solveResult.recommended_actions.length} actions.
                    </p>
                  )}
                </div>
              )}

              <div>
                <button
                  className="btn btn-primary btn-sm"
                  onClick={handleApplySolution}
                  disabled={solveResult.recommended_actions.length === 0}
                >
                  Apply to scenario
                </button>
              </div>
            </div>
          )}
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
