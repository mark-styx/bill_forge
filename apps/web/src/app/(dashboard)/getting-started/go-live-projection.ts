import type { ImplementationStatus } from '@/lib/api';

const BASELINE_DAYS = 14;
const DAYS_PER_EXTRA_MODULE = 2;
const MAX_MODULE_DAYS = 14;
const BACKTEST_FAIL_DAYS = 7;
const MS_PER_DAY = 24 * 60 * 60 * 1000;

export interface GoLiveProjection {
  targetDate: number;
  totalDays: number;
  riskAdjusted: boolean;
  moduleCount: number;
  backtestFailed: boolean;
}

export function computeGoLiveProjection(status: ImplementationStatus): GoLiveProjection {
  const entitlements = status.phases?.configuration?.configuration?.module_entitlements;
  const moduleCount = Array.isArray(entitlements)
    ? entitlements.filter((ent) => ent?.enabled).length
    : 0;
  const moduleDays = Math.min(
    Math.max(moduleCount - 1, 0) * DAYS_PER_EXTRA_MODULE,
    MAX_MODULE_DAYS,
  );

  const scorecard = status.phases?.go_live?.backtest_scorecard;
  const backtestFailed = scorecard ? scorecard.passes_threshold === false : false;
  const backtestDays = backtestFailed ? BACKTEST_FAIL_DAYS : 0;

  const totalDays = BASELINE_DAYS + moduleDays + backtestDays;
  const startMs = new Date(status.started_at).getTime();
  const targetDate = startMs + totalDays * MS_PER_DAY;

  return {
    targetDate,
    totalDays,
    riskAdjusted: totalDays !== BASELINE_DAYS,
    moduleCount,
    backtestFailed,
  };
}

export function formatGoLiveDate(targetDate: number): string {
  return new Date(targetDate).toLocaleDateString('en-US', {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
  });
}
