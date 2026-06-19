import { describe, expect, it } from 'vitest';
import type {
  ImplementationModuleEntitlement,
  ImplementationReadinessScorecard,
  ImplementationStatus,
} from '@/lib/api';
import { computeGoLiveProjection } from '../go-live-projection';

const STARTED_AT = '2026-06-01T00:00:00Z';
const MS_PER_DAY = 24 * 60 * 60 * 1000;

function makeStatus(overrides: {
  entitlements?: ImplementationModuleEntitlement[];
  scorecard?: ImplementationReadinessScorecard | null;
  startedAt?: string;
  phasesOverride?: Partial<ImplementationStatus['phases']> | null;
} = {}): ImplementationStatus {
  const base: ImplementationStatus = {
    started_at: overrides.startedAt ?? STARTED_AT,
    day_number: 1,
    percent_complete: 0,
    phases: {
      erp: {
        status: 'not_started',
        provider: null,
        sub_items: { chart_of_accounts: false, vendors: false, open_pos: false },
        last_sync: null,
        last_error: null,
      },
      approvals: { status: 'not_started', template: null, template_id: null },
      ocr: {
        status: 'not_started',
        count: 0,
        sample_invoice_ids: [],
        measured_accuracy: null,
        accuracy_threshold: 0.9,
        total_extractions: 0,
        sufficient_sample: false,
      },
      configuration: {
        status: 'not_started',
        configuration: {
          privacy_mode: { enabled: false, scope: null, confirmed_at: null },
          capture_channels: {
            email_forwarding: { address: '', verified_at: null },
            manual_upload_enabled: false,
            erp_sync_enabled: false,
          },
          module_entitlements: overrides.entitlements ?? [{ module_key: 'core', enabled: true }],
          notification_approvals: {
            ap_team_distribution: [],
            escalation_distribution: [],
            approved_at: null,
          },
        },
      },
      go_live: {
        status: 'not_started',
        checks: {
          confirm_cutover_date: false,
          forwarding_email_verified: false,
          sample_invoice_routed: false,
          notifications_acknowledged: false,
          privacy_mode_confirmed: false,
        },
        backtest_scorecard: overrides.scorecard ?? null,
      },
    },
  };
  if (overrides.phasesOverride === null) {
    // Force missing phases for defensive-default test.
    return { ...base, phases: undefined as unknown as ImplementationStatus['phases'] };
  }
  return base;
}

function entitlements(n: number): ImplementationModuleEntitlement[] {
  return Array.from({ length: n }, (_, i) => ({ module_key: `mod_${i}`, enabled: true }));
}

function failingScorecard(): ImplementationReadinessScorecard {
  return {
    auto_route_coverage: 0.5,
    auto_approve_coverage: 0.5,
    vendor_map_coverage: 0.5,
    sample_size: 100,
    readiness_score: 0.5,
    passes_threshold: false,
    run_at: '2026-06-10T00:00:00Z',
  };
}

function passingScorecard(): ImplementationReadinessScorecard {
  return { ...failingScorecard(), readiness_score: 0.9, passes_threshold: true };
}

describe('computeGoLiveProjection', () => {
  it('returns baseline 14 days with one module and no backtest', () => {
    const result = computeGoLiveProjection(makeStatus({ entitlements: entitlements(1) }));
    expect(result.totalDays).toBe(14);
    expect(result.riskAdjusted).toBe(false);
    expect(result.moduleCount).toBe(1);
    expect(result.backtestFailed).toBe(false);
    expect(result.targetDate).toBe(new Date(STARTED_AT).getTime() + 14 * MS_PER_DAY);
  });

  it('adds 2 days per extra module', () => {
    const result = computeGoLiveProjection(makeStatus({ entitlements: entitlements(4) }));
    expect(result.totalDays).toBe(14 + 2 * 3);
    expect(result.riskAdjusted).toBe(true);
    expect(result.moduleCount).toBe(4);
  });

  it('caps module adjustment at +14 days', () => {
    const result = computeGoLiveProjection(makeStatus({ entitlements: entitlements(20) }));
    expect(result.totalDays).toBe(14 + 14);
    expect(result.moduleCount).toBe(20);
  });

  it('adds 7 days when the readiness backtest fails', () => {
    const result = computeGoLiveProjection(
      makeStatus({ entitlements: entitlements(1), scorecard: failingScorecard() }),
    );
    expect(result.totalDays).toBe(14 + 7);
    expect(result.backtestFailed).toBe(true);
    expect(result.riskAdjusted).toBe(true);
  });

  it('does not adjust when the backtest passes', () => {
    const result = computeGoLiveProjection(
      makeStatus({ entitlements: entitlements(1), scorecard: passingScorecard() }),
    );
    expect(result.totalDays).toBe(14);
    expect(result.backtestFailed).toBe(false);
  });

  it('combines module and backtest adjustments', () => {
    const result = computeGoLiveProjection(
      makeStatus({ entitlements: entitlements(6), scorecard: failingScorecard() }),
    );
    expect(result.totalDays).toBe(14 + 2 * 5 + 7);
    expect(result.moduleCount).toBe(6);
    expect(result.backtestFailed).toBe(true);
    expect(result.riskAdjusted).toBe(true);
  });

  it('ignores disabled module entitlements', () => {
    const result = computeGoLiveProjection(
      makeStatus({
        entitlements: [
          { module_key: 'core', enabled: true },
          { module_key: 'addon', enabled: false },
          { module_key: 'addon2', enabled: false },
        ],
      }),
    );
    expect(result.totalDays).toBe(14);
    expect(result.moduleCount).toBe(1);
  });

  it('falls back to 14 days when phases are missing', () => {
    const result = computeGoLiveProjection(makeStatus({ phasesOverride: null }));
    expect(result.totalDays).toBe(14);
    expect(result.riskAdjusted).toBe(false);
    expect(result.moduleCount).toBe(0);
    expect(result.backtestFailed).toBe(false);
  });
});
