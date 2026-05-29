'use client';

import { useState, useEffect, useCallback } from 'react';
import Link from 'next/link';
import { useAuthStore } from '@/stores/auth';
import {
  Rocket,
  CheckCircle2,
  Loader2,
  Plus,
  ExternalLink,
  Building2,
  GitBranch,
  ScanLine,
  Flag,
} from 'lucide-react';

/* ------------------------------------------------------------------ */
/*  Types                                                              */
/* ------------------------------------------------------------------ */

type PhaseStatus = 'not_started' | 'in_progress' | 'complete';

interface ErpSubItems {
  chartOfAccounts: boolean;
  vendors: boolean;
  openPOs: boolean;
}

interface ErpPhase {
  status: PhaseStatus;
  subItems: ErpSubItems;
}

interface ApprovalsPhase {
  status: PhaseStatus;
  template: string | null;
}

interface OcrPhase {
  status: PhaseStatus;
  count: number;
}

interface GoLivePhase {
  status: PhaseStatus;
  checks: {
    notifyApTeam: boolean;
    setEmailForwarding: boolean;
    enableApprovalRouting: boolean;
    scheduleFirstPaymentRun: boolean;
    confirmCutoverDate: boolean;
  };
}

interface WizardState {
  startedAt: string;
  phases: {
    erp: ErpPhase;
    approvals: ApprovalsPhase;
    ocr: OcrPhase;
    goLive: GoLivePhase;
  };
}

/* ------------------------------------------------------------------ */
/*  Helpers                                                            */
/* ------------------------------------------------------------------ */

const STORAGE_KEY_PREFIX = 'billforge.implementation.v1';

function getStorageKey(tenantId: string): string {
  return `${STORAGE_KEY_PREFIX}.${tenantId}`;
}

function getDefaultState(): WizardState {
  return {
    startedAt: new Date().toISOString(),
    phases: {
      erp: {
        status: 'not_started',
        subItems: { chartOfAccounts: false, vendors: false, openPOs: false },
      },
      approvals: { status: 'not_started', template: null },
      ocr: { status: 'not_started', count: 0 },
      goLive: {
        status: 'not_started',
        checks: {
          notifyApTeam: false,
          setEmailForwarding: false,
          enableApprovalRouting: false,
          scheduleFirstPaymentRun: false,
          confirmCutoverDate: false,
        },
      },
    },
  };
}

function loadState(tenantId: string): WizardState {
  if (typeof window === 'undefined') return getDefaultState();
  try {
    const raw = window.localStorage.getItem(getStorageKey(tenantId));
    if (raw) return JSON.parse(raw) as WizardState;
  } catch {
    // ignore corrupt data
  }
  const fresh = getDefaultState();
  window.localStorage.setItem(getStorageKey(tenantId), JSON.stringify(fresh));
  return fresh;
}

function saveState(tenantId: string, state: WizardState) {
  window.localStorage.setItem(getStorageKey(tenantId), JSON.stringify(state));
}

function computeDayNumber(startedAt: string): number {
  const start = new Date(startedAt);
  const now = new Date();
  const diffMs = now.getTime() - start.getTime();
  const days = Math.floor(diffMs / (1000 * 60 * 60 * 24)) + 1;
  return Math.max(1, Math.min(days, 14));
}

function computePercentComplete(state: WizardState): number {
  const phases = state.phases;
  let complete = 0;
  if (phases.erp.status === 'complete') complete++;
  if (phases.approvals.status === 'complete') complete++;
  if (phases.ocr.status === 'complete') complete++;
  if (phases.goLive.status === 'complete') complete++;
  return Math.round((complete / 4) * 100);
}

function goLiveDate(startedAt: string): string {
  const start = new Date(startedAt);
  const target = new Date(start.getTime() + 14 * 24 * 60 * 60 * 1000);
  return target.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' });
}

function statusLabel(status: PhaseStatus): string {
  switch (status) {
    case 'not_started': return 'Not started';
    case 'in_progress': return 'In progress';
    case 'complete': return 'Complete';
  }
}

function statusPillClasses(status: PhaseStatus): string {
  switch (status) {
    case 'not_started': return 'bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-400';
    case 'in_progress': return 'bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400';
    case 'complete': return 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400';
  }
}

/* ------------------------------------------------------------------ */
/*  Approval templates                                                 */
/* ------------------------------------------------------------------ */

const APPROVAL_TEMPLATES = [
  { id: 'amount', label: 'By amount threshold', description: 'Route invoices based on dollar value tiers.' },
  { id: 'department', label: 'By department', description: 'Each department has its own approver chain.' },
  { id: 'gl', label: 'By GL code', description: 'Assign approval paths by general ledger account.' },
];

/* ------------------------------------------------------------------ */
/*  Go-live checklist items                                            */
/* ------------------------------------------------------------------ */

const GO_LIVE_ITEMS: { key: keyof GoLivePhase['checks']; label: string }[] = [
  { key: 'notifyApTeam', label: 'Notify AP team of go-live date' },
  { key: 'setEmailForwarding', label: 'Set production email forwarding' },
  { key: 'enableApprovalRouting', label: 'Enable approval routing' },
  { key: 'scheduleFirstPaymentRun', label: 'Schedule first payment run' },
  { key: 'confirmCutoverDate', label: 'Confirm cutover date' },
];

/* ------------------------------------------------------------------ */
/*  Sub-item toggle                                                    */
/* ------------------------------------------------------------------ */

function SubItemToggle({ checked, label, onChange }: { checked: boolean; label: string; onChange: () => void }) {
  return (
    <label className="flex items-center gap-2 cursor-pointer text-sm text-foreground">
      <input
        type="checkbox"
        checked={checked}
        onChange={onChange}
        className="rounded border-border"
      />
      {label}
    </label>
  );
}

/* ------------------------------------------------------------------ */
/*  Phase card                                                         */
/* ------------------------------------------------------------------ */

function PhaseCard({
  title,
  dayRange,
  description,
  status,
  onMarkComplete,
  children,
}: {
  title: string;
  dayRange: string;
  description: string;
  status: PhaseStatus;
  onMarkComplete: () => void;
  children: React.ReactNode;
}) {
  return (
    <div className="border border-border rounded-xl bg-card p-5 space-y-4">
      <div className="flex items-start justify-between gap-3">
        <div className="space-y-1">
          <div className="flex items-center gap-2">
            <h3 className="text-base font-semibold text-foreground">{title}</h3>
            <span className="text-xs text-muted-foreground">{dayRange}</span>
          </div>
          <p className="text-sm text-muted-foreground">{description}</p>
        </div>
        <span className={`inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-medium whitespace-nowrap ${statusPillClasses(status)}`}>
          {status === 'in_progress' && <Loader2 className="w-3 h-3 mr-1 animate-spin" />}
          {status === 'complete' && <CheckCircle2 className="w-3 h-3 mr-1" />}
          {statusLabel(status)}
        </span>
      </div>
      {children}
      {status !== 'complete' && (
        <button
          type="button"
          onClick={onMarkComplete}
          className="text-xs text-muted-foreground hover:text-foreground underline underline-offset-2 transition-colors"
        >
          Mark phase complete
        </button>
      )}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Main page                                                          */
/* ------------------------------------------------------------------ */

export default function GettingStartedPage() {
  const { tenant } = useAuthStore();
  const tenantId = tenant?.id ?? 'anonymous';

  const [state, setState] = useState<WizardState>(() => getDefaultState());
  const [hydrated, setHydrated] = useState(false);

  useEffect(() => {
    const loaded = loadState(tenantId);
    setState(loaded);
    setHydrated(true);
  }, [tenantId]);

  const persist = useCallback(
    (next: WizardState) => {
      setState(next);
      if (hydrated) {
        saveState(tenantId, next);
      }
    },
    [tenantId, hydrated],
  );

  const dayNumber = computeDayNumber(state.startedAt);
  const percent = computePercentComplete(state);
  const onTrack = percent === 100
    || (dayNumber <= 3 && state.phases.erp.status !== 'not_started')
    || (dayNumber <= 6 && (state.phases.erp.status === 'complete' || state.phases.approvals.status !== 'not_started'))
    || percent >= 75;

  /* ---------- ERP phase handlers ---------- */

  const toggleErpSubItem = (key: keyof ErpSubItems) => {
    const subItems = { ...state.phases.erp.subItems, [key]: !state.phases.erp.subItems[key] };
    const allDone = subItems.chartOfAccounts && subItems.vendors && subItems.openPOs;
    persist({
      ...state,
      phases: {
        ...state.phases,
        erp: { status: allDone ? 'complete' : 'in_progress', subItems },
      },
    });
  };

  const markErpComplete = () => {
    persist({
      ...state,
      phases: {
        ...state.phases,
        erp: { status: 'complete', subItems: { chartOfAccounts: true, vendors: true, openPOs: true } },
      },
    });
  };

  /* ---------- Approvals phase handlers ---------- */

  const selectTemplate = (id: string) => {
    persist({
      ...state,
      phases: {
        ...state.phases,
        approvals: { status: 'complete', template: id },
      },
    });
  };

  const markApprovalsComplete = () => {
    persist({
      ...state,
      phases: {
        ...state.phases,
        approvals: { status: 'complete', template: state.phases.approvals.template ?? 'amount' },
      },
    });
  };

  /* ---------- OCR phase handlers ---------- */

  const incrementOcr = () => {
    const newCount = Math.min(state.phases.ocr.count + 1, 10);
    persist({
      ...state,
      phases: {
        ...state.phases,
        ocr: { status: newCount >= 10 ? 'complete' : 'in_progress', count: newCount },
      },
    });
  };

  const markOcrComplete = () => {
    persist({
      ...state,
      phases: {
        ...state.phases,
        ocr: { status: 'complete', count: 10 },
      },
    });
  };

  /* ---------- Go-live phase handlers ---------- */

  const toggleGoLiveCheck = (key: keyof GoLivePhase['checks']) => {
    const checks = { ...state.phases.goLive.checks, [key]: !state.phases.goLive.checks[key] };
    const allDone = Object.values(checks).every(Boolean);
    persist({
      ...state,
      phases: {
        ...state.phases,
        goLive: { status: allDone ? 'complete' : 'in_progress', checks },
      },
    });
  };

  const markGoLiveComplete = () => {
    persist({
      ...state,
      phases: {
        ...state.phases,
        goLive: {
          status: 'complete',
          checks: {
            notifyApTeam: true,
            setEmailForwarding: true,
            enableApprovalRouting: true,
            scheduleFirstPaymentRun: true,
            confirmCutoverDate: true,
          },
        },
      },
    });
  };

  return (
    <div className="max-w-3xl mx-auto space-y-6">
      {/* Header */}
      <div className="space-y-2">
        <div className="flex items-center gap-2">
          <Rocket className="w-6 h-6 text-primary" />
          <h1 className="text-2xl font-bold text-foreground">Implementation Wizard</h1>
        </div>
        <p className="text-muted-foreground">
          Day <span data-testid="day-number">{dayNumber}</span> of 14
        </p>
        {/* Progress bar */}
        <div className="w-full bg-secondary rounded-full h-2.5">
          <div
            data-testid="progress-bar"
            className="h-2.5 rounded-full bg-primary transition-all"
            style={{ width: `${percent}%` }}
          />
        </div>
        <p className="text-xs text-muted-foreground text-right" data-testid="progress-percent">
          {percent}% complete
        </p>
      </div>

      {/* Phase 1: ERP */}
      <PhaseCard
        title="Connect your accounting system"
        dayRange="Days 1-3"
        description="Connect QuickBooks or Xero to auto-import your chart of accounts, vendors, and open purchase orders."
        status={state.phases.erp.status}
        onMarkComplete={markErpComplete}
      >
        <div className="space-y-2">
          <SubItemToggle
            checked={state.phases.erp.subItems.chartOfAccounts}
            label="Chart of accounts imported"
            onChange={() => toggleErpSubItem('chartOfAccounts')}
          />
          <SubItemToggle
            checked={state.phases.erp.subItems.vendors}
            label="Vendors imported"
            onChange={() => toggleErpSubItem('vendors')}
          />
          <SubItemToggle
            checked={state.phases.erp.subItems.openPOs}
            label="Open POs imported"
            onChange={() => toggleErpSubItem('openPOs')}
          />
        </div>
        <Link
          href="/integrations"
          className="inline-flex items-center gap-1.5 text-sm font-medium text-primary hover:underline"
        >
          <Building2 className="w-4 h-4" />
          Go to Integrations
          <ExternalLink className="w-3 h-3" />
        </Link>
      </PhaseCard>

      {/* Phase 2: Approvals */}
      <PhaseCard
        title="Choose an approval-chain template"
        dayRange="Days 4-6"
        description="Select a preset approval routing strategy to get started quickly. You can customise it later."
        status={state.phases.approvals.status}
        onMarkComplete={markApprovalsComplete}
      >
        <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
          {APPROVAL_TEMPLATES.map((t) => {
            const selected = state.phases.approvals.template === t.id;
            return (
              <button
                key={t.id}
                type="button"
                onClick={() => selectTemplate(t.id)}
                className={`rounded-lg border p-3 text-left transition-colors ${
                  selected
                    ? 'border-primary bg-primary/5 ring-1 ring-primary'
                    : 'border-border hover:border-primary/50'
                }`}
              >
                <p className="text-sm font-medium text-foreground">{t.label}</p>
                <p className="text-xs text-muted-foreground mt-1">{t.description}</p>
              </button>
            );
          })}
        </div>
        <Link
          href="/processing/workflows"
          className="inline-flex items-center gap-1.5 text-sm font-medium text-primary hover:underline"
        >
          <GitBranch className="w-4 h-4" />
          Go to Workflows
          <ExternalLink className="w-3 h-3" />
        </Link>
      </PhaseCard>

      {/* Phase 3: OCR */}
      <PhaseCard
        title="Validate OCR with 10 sample invoices"
        dayRange="Days 7-10"
        description="Upload sample invoices to validate OCR accuracy and coding suggestions before going live."
        status={state.phases.ocr.status}
        onMarkComplete={markOcrComplete}
      >
        <div className="flex items-center gap-4">
          <span className="text-sm text-foreground font-medium" data-testid="ocr-count">
            {state.phases.ocr.count} / 10 uploaded
          </span>
          {state.phases.ocr.count < 10 && (
            <button
              type="button"
              onClick={incrementOcr}
              className="inline-flex items-center gap-1 rounded-lg bg-primary text-primary-foreground px-3 py-1.5 text-sm font-medium hover:bg-primary/90 transition-colors"
            >
              <Plus className="w-4 h-4" />
              +1 Sample
            </button>
          )}
        </div>
        <Link
          href="/invoices"
          className="inline-flex items-center gap-1.5 text-sm font-medium text-primary hover:underline"
        >
          <ScanLine className="w-4 h-4" />
          Go to Invoices
          <ExternalLink className="w-3 h-3" />
        </Link>
      </PhaseCard>

      {/* Phase 4: Go-live */}
      <PhaseCard
        title="Go-live checklist"
        dayRange="Days 11-14"
        description="Complete these final steps before your first production processing cycle."
        status={state.phases.goLive.status}
        onMarkComplete={markGoLiveComplete}
      >
        <div className="space-y-2">
          {GO_LIVE_ITEMS.map((item) => (
            <SubItemToggle
              key={item.key}
              checked={state.phases.goLive.checks[item.key]}
              label={item.label}
              onChange={() => toggleGoLiveCheck(item.key)}
            />
          ))}
        </div>
      </PhaseCard>

      {/* Footer */}
      <div
        className={`rounded-xl p-4 text-sm font-medium ${
          onTrack
            ? 'bg-green-50 text-green-800 dark:bg-green-900/20 dark:text-green-400'
            : 'bg-amber-50 text-amber-800 dark:bg-amber-900/20 dark:text-amber-400'
        }`}
      >
        <div className="flex items-center gap-2">
          <Flag className="w-4 h-4" />
          <span>
            {percent === 100
              ? 'All phases complete - you are live!'
              : `On track to go live by ${goLiveDate(state.startedAt)}`}
          </span>
        </div>
      </div>
    </div>
  );
}
