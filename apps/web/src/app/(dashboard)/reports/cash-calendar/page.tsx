'use client';

import { useMemo, useState, useCallback, useEffect, useRef } from 'react';
import { useQuery } from '@tanstack/react-query';
import Link from 'next/link';
import {
  reportsApi,
  type ApCashFlowForecast,
  type ForecastDay,
} from '@/lib/api';
import { useAuthStore } from '@/stores/auth';
import {
  ArrowLeft,
  Calendar,
  AlertTriangle,
  RotateCcw,
  ChevronLeft,
  ChevronRight,
  FlaskConical,
  Info,
} from 'lucide-react';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const formatCents = (value: number, currency = 'USD') =>
  new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency,
    maximumFractionDigits: 0,
  }).format(value / 100);

const formatCentsShort = (value: number) => {
  if (Math.abs(value) >= 1_000_00)
    return `$${(value / 100).toLocaleString('en-US', { maximumFractionDigits: 0 })}`;
  return formatCents(value);
};

/** YYYY-MM-DD from a Date object. */
const toIsoDate = (d: Date) =>
  `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;

const DAYS_OF_WEEK = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];

// ---------------------------------------------------------------------------
// Scenario move record
// ---------------------------------------------------------------------------

interface ScenarioMove {
  fromDate: string; // YYYY-MM-DD
  toDate: string;   // YYYY-MM-DD
  amount: number;   // cents
}

// ---------------------------------------------------------------------------
// localStorage helpers for starting balance
// ---------------------------------------------------------------------------

function balanceStorageKey(tenantId: string) {
  return `billforge:cash-calendar:balance:${tenantId}`;
}

function loadBalance(tenantId: string): number {
  if (typeof window === 'undefined') return 0;
  const raw = localStorage.getItem(balanceStorageKey(tenantId));
  return raw ? Number(raw) : 0;
}

function saveBalance(tenantId: string, value: number) {
  localStorage.setItem(balanceStorageKey(tenantId), String(value));
}

// ---------------------------------------------------------------------------
// Calendar grid helpers
// ---------------------------------------------------------------------------

interface DayCell {
  date: string; // YYYY-MM-DD
  dayOfMonth: number;
  isCurrentMonth: boolean;
  isToday: boolean;
}

function buildCalendarGrid(year: number, month: number): DayCell[] {
  const firstOfMonth = new Date(year, month, 1);
  const startDow = firstOfMonth.getDay(); // 0=Sun
  const daysInMonth = new Date(year, month + 1, 0).getDate();
  const daysInPrevMonth = new Date(year, month, 0).getDate();

  const todayStr = toIsoDate(new Date());
  const cells: DayCell[] = [];

  // Leading days from previous month
  for (let i = startDow - 1; i >= 0; i--) {
    const d = daysInPrevMonth - i;
    const date = toIsoDate(new Date(year, month - 1, d));
    cells.push({ date, dayOfMonth: d, isCurrentMonth: false, isToday: date === todayStr });
  }

  // Current month
  for (let d = 1; d <= daysInMonth; d++) {
    const date = toIsoDate(new Date(year, month, d));
    cells.push({ date, dayOfMonth: d, isCurrentMonth: true, isToday: date === todayStr });
  }

  // Trailing days to fill 6 rows x 7 cols = 42 cells
  const remaining = 42 - cells.length;
  for (let d = 1; d <= remaining; d++) {
    const date = toIsoDate(new Date(year, month + 1, d));
    cells.push({ date, dayOfMonth: d, isCurrentMonth: false, isToday: date === todayStr });
  }

  return cells;
}

// ---------------------------------------------------------------------------
// Projected balance computation
// ---------------------------------------------------------------------------

interface DayProjection {
  date: string;
  outflow: number; // cents (after scenario moves)
  projectedBalance: number; // cents
}

function computeProjections(
  dailyMap: Map<string, number>, // date -> expected_amount from forecast
  moves: ScenarioMove[],
  startingBalance: number,
  sortedDates: string[],
): DayProjection[] {
  // Build net adjustment per date from moves
  const moveAdj = new Map<string, number>();
  for (const m of moves) {
    moveAdj.set(m.fromDate, (moveAdj.get(m.fromDate) ?? 0) - m.amount);
    moveAdj.set(m.toDate, (moveAdj.get(m.toDate) ?? 0) + m.amount);
  }

  let cumulative = 0;
  return sortedDates.map((date) => {
    const base = dailyMap.get(date) ?? 0;
    const adj = moveAdj.get(date) ?? 0;
    const outflow = base + adj;
    cumulative += outflow;
    return { date, outflow, projectedBalance: startingBalance - cumulative };
  });
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export default function CashCalendarPage() {
  const tenant = useAuthStore((s) => s.tenant);
  const tenantId = tenant?.id ?? 'unknown';

  // Month navigation
  const now = new Date();
  const [viewYear, setViewYear] = useState(now.getFullYear());
  const [viewMonth, setViewMonth] = useState(now.getMonth()); // 0-indexed

  // Starting balance
  const [startingBalance, setStartingBalance] = useState(() => loadBalance(tenantId));

  // Persist balance changes
  useEffect(() => {
    saveBalance(tenantId, startingBalance);
  }, [tenantId, startingBalance]);

  // Scenario moves
  const [moves, setMoves] = useState<ScenarioMove[]>([]);

  // Drag state
  const dragDateRef = useRef<string | null>(null);
  const dragAmountRef = useRef<number>(0);

  // Fetch forecast (13 weeks = ~91 days, covers 2-3 months)
  const forecastQuery = useQuery({
    queryKey: ['ap-cash-flow-forecast', 13],
    queryFn: () => reportsApi.apCashFlowForecast({ horizon_weeks: 13 }),
  });

  const forecast = forecastQuery.data;

  // Build daily map from forecast
  const { dailyMap, sortedDates } = useMemo(() => {
    const map = new Map<string, number>();
    if (!forecast) return { dailyMap: map, sortedDates: [] as string[] };
    const dates: string[] = [];
    for (const d of forecast.daily) {
      map.set(d.date, d.expected_amount);
      dates.push(d.date);
    }
    dates.sort();
    return { dailyMap: map, sortedDates: dates };
  }, [forecast]);

  // Projections
  const projections = useMemo(
    () => computeProjections(dailyMap, moves, startingBalance, sortedDates),
    [dailyMap, moves, startingBalance, sortedDates],
  );

  // Projection lookup
  const projMap = useMemo(() => {
    const m = new Map<string, DayProjection>();
    for (const p of projections) m.set(p.date, p);
    return m;
  }, [projections]);

  // Lowest projected balance
  const lowestBalance = useMemo(() => {
    if (projections.length === 0) return null;
    let lowest = projections[0];
    for (const p of projections) {
      if (p.projectedBalance < lowest.projectedBalance) lowest = p;
    }
    return lowest;
  }, [projections]);

  // Calendar grid
  const grid = useMemo(
    () => buildCalendarGrid(viewYear, viewMonth),
    [viewYear, viewMonth],
  );

  // Month nav
  const monthLabel = new Date(viewYear, viewMonth).toLocaleString('en-US', {
    month: 'long',
    year: 'numeric',
  });

  const goPrev = useCallback(() => {
    if (viewMonth === 0) {
      setViewMonth(11);
      setViewYear((y) => y - 1);
    } else {
      setViewMonth((m) => m - 1);
    }
  }, [viewMonth]);

  const goNext = useCallback(() => {
    if (viewMonth === 11) {
      setViewMonth(0);
      setViewYear((y) => y + 1);
    } else {
      setViewMonth((m) => m + 1);
    }
  }, [viewMonth]);

  // Drag handlers
  const handleDragStart = useCallback(
    (date: string, amount: number) => {
      dragDateRef.current = date;
      dragAmountRef.current = amount;
    },
    [],
  );

  const handleDrop = useCallback(
    (targetDate: string) => {
      const fromDate = dragDateRef.current;
      const amount = dragAmountRef.current;
      if (!fromDate || fromDate === targetDate || amount <= 0) return;
      setMoves((prev) => [...prev, { fromDate, toDate: targetDate, amount }]);
      dragDateRef.current = null;
      dragAmountRef.current = 0;
    },
    [],
  );

  const handleResetScenario = useCallback(() => {
    setMoves([]);
  }, []);

  // Bill count per day from vendor_breakdown length (approximation: each vendor entry = 1 bill)
  const billCountMap = useMemo(() => {
    const m = new Map<string, number>();
    if (!forecast) return m;
    for (const d of forecast.daily) {
      // Use vendor_breakdown entries as a proxy for bill count
      m.set(d.date, d.vendor_breakdown.length || (d.expected_amount > 0 ? 1 : 0));
    }
    return m;
  }, [forecast]);

  // Helpers for cell rendering
  const isScenarioActive = moves.length > 0;

  // Count moves for display
  const moveCount = moves.length;

  // Loading / error
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
              <Calendar className="w-6 h-6 text-accent" />
              Cash Calendar
            </h1>
            <p className="text-muted-foreground mt-0.5">
              Day-by-day AP outflows with projected bank balance
            </p>
          </div>
        </div>
        <Link
          href="/reports/cash-flow-forecast"
          className="btn btn-secondary btn-sm"
        >
          13-Week Forecast
        </Link>
      </div>

      {/* Toolbar: balance input, month nav, scenario controls */}
      <div className="card p-4">
        <div className="flex flex-col lg:flex-row lg:items-center gap-4">
          {/* Starting balance */}
          <div className="flex items-center gap-2 flex-shrink-0">
            <label htmlFor="starting-balance" className="text-sm font-medium text-foreground whitespace-nowrap">
              Current bank balance:
            </label>
            <div className="relative">
              <span className="absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground text-sm">$</span>
              <input
                id="starting-balance"
                type="number"
                min={0}
                step={1000}
                value={startingBalance || ''}
                onChange={(e) => setStartingBalance(Math.max(0, Number(e.target.value) || 0))}
                placeholder="0"
                className="input input-bordered w-40 pl-7"
              />
            </div>
          </div>

          {/* Month navigation */}
          <div className="flex items-center gap-2 flex-shrink-0">
            <button className="btn btn-secondary btn-sm" onClick={goPrev}>
              <ChevronLeft className="w-4 h-4" />
            </button>
            <span className="text-sm font-semibold text-foreground min-w-[140px] text-center">
              {monthLabel}
            </span>
            <button className="btn btn-secondary btn-sm" onClick={goNext}>
              <ChevronRight className="w-4 h-4" />
            </button>
          </div>

          {/* Scenario controls */}
          <div className="flex items-center gap-3 flex-shrink-0">
            {isScenarioActive && (
              <>
                <span className="badge badge-warning gap-1">
                  <FlaskConical className="w-3 h-3" />
                  {moveCount} local projection{moveCount !== 1 ? 's' : ''}
                </span>
                <button
                  className="btn btn-secondary btn-sm"
                  onClick={handleResetScenario}
                >
                  <RotateCcw className="w-3 h-3 mr-1" />
                  Reset scenario
                </button>
              </>
            )}
          </div>

          {/* Lowest balance badge */}
          {lowestBalance && startingBalance > 0 && (
            <div
              className={`ml-auto flex items-center gap-2 text-sm font-medium px-3 py-1.5 rounded-lg ${
                lowestBalance.projectedBalance < 0
                  ? 'bg-error/10 text-error'
                  : 'bg-success/10 text-success'
              }`}
            >
              {lowestBalance.projectedBalance < 0 ? (
                <AlertTriangle className="w-4 h-4" />
              ) : (
                <Info className="w-4 h-4" />
              )}
              <span data-testid="lowest-balance-badge">
                Lowest: {formatCents(lowestBalance.projectedBalance)} on {lowestBalance.date}
              </span>
            </div>
          )}
        </div>
      </div>

      {/* Local projection notice */}
      {isScenarioActive && (
        <div className="bg-warning/5 border border-warning/20 rounded-lg px-4 py-2 text-sm text-warning flex items-center gap-2">
          <FlaskConical className="w-4 h-4 flex-shrink-0" />
          <span>
            Showing <strong>local projection</strong> with {moveCount} rescheduled
            payment{moveCount !== 1 ? 's' : ''}. Drag day cells to reschedule AP outflows.
          </span>
        </div>
      )}

      {/* Calendar grid */}
      <div className="card overflow-hidden">
        {/* Day-of-week headers */}
        <div className="grid grid-cols-7 border-b border-border">
          {DAYS_OF_WEEK.map((d) => (
            <div
              key={d}
              className="p-2 text-center text-xs font-semibold text-muted-foreground"
            >
              {d}
            </div>
          ))}
        </div>

        {/* 6 rows of 7 day cells */}
        <div className="grid grid-cols-7">
          {grid.map((cell, idx) => {
            const proj = projMap.get(cell.date);
            const outflow = proj?.outflow ?? dailyMap.get(cell.date) ?? 0;
            const balance = proj?.projectedBalance ?? (startingBalance > 0 ? startingBalance : undefined);
            const billCount = billCountMap.get(cell.date) ?? 0;
            const hasOutflow = outflow > 0;
            const isNegative = balance !== undefined && balance < 0;

            return (
              <div
                key={cell.date + '-' + idx}
                data-testid={`day-cell-${cell.date}`}
                className={`border-b border-r border-border p-2 min-h-[90px] flex flex-col transition-colors
                  ${cell.isCurrentMonth ? 'bg-background' : 'bg-muted/30'}
                  ${cell.isToday ? 'ring-2 ring-accent ring-inset' : ''}
                  ${hasOutflow ? 'cursor-grab' : ''}
                `}
                draggable={hasOutflow}
                onDragStart={() => handleDragStart(cell.date, outflow)}
                onDragOver={(e) => e.preventDefault()}
                onDrop={() => handleDrop(cell.date)}
              >
                <div className="flex items-center justify-between mb-1">
                  <span
                    className={`text-xs font-medium ${
                      cell.isCurrentMonth ? 'text-foreground' : 'text-muted-foreground'
                    }`}
                  >
                    {cell.dayOfMonth}
                  </span>
                  {cell.isToday && (
                    <span className="text-[10px] font-bold text-accent uppercase">Today</span>
                  )}
                </div>

                {hasOutflow && (
                  <div className="text-xs space-y-0.5">
                    <p className="font-semibold text-error/80">
                      {formatCentsShort(outflow)}
                    </p>
                    {billCount > 0 && (
                      <p className="text-muted-foreground">
                        {billCount} bill{billCount !== 1 ? 's' : ''}
                      </p>
                    )}
                  </div>
                )}

                {balance !== undefined && startingBalance > 0 && (
                  <p
                    className={`text-[11px] mt-auto pt-1 font-medium ${
                      isNegative ? 'text-error' : 'text-muted-foreground'
                    }`}
                  >
                    Bal: {formatCentsShort(balance)}
                  </p>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
