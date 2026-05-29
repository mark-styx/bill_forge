'use client';

import { useCallback, useEffect, useState } from 'react';
import Link from 'next/link';
import {
  AlertCircle,
  Building2,
  CheckCircle2,
  ExternalLink,
  Flag,
  GitBranch,
  Loader2,
  Rocket,
  ScanLine,
  Upload,
} from 'lucide-react';
import {
  implementationApi,
  type ImplementationErpProvider,
  type ImplementationErpSubItems,
  type ImplementationGoLiveChecks,
  type ImplementationPhaseStatus,
  type ImplementationStatus,
} from '@/lib/api';
import { useAuthStore } from '@/stores/auth';

const APPROVAL_TEMPLATES = [
  { id: 'amount', label: 'By amount threshold', description: 'Route invoices based on dollar value tiers.' },
  { id: 'department', label: 'By department', description: 'Each department has its own approver chain.' },
  { id: 'gl', label: 'By GL code', description: 'Assign approval paths by general ledger account.' },
];

const GO_LIVE_ITEMS: { key: keyof ImplementationGoLiveChecks; label: string }[] = [
  { key: 'notify_ap_team', label: 'Notify AP team of go-live date' },
  { key: 'set_email_forwarding', label: 'Set production email forwarding' },
  { key: 'enable_approval_routing', label: 'Enable approval routing' },
  { key: 'schedule_first_payment_run', label: 'Schedule first payment run' },
  { key: 'confirm_cutover_date', label: 'Confirm cutover date' },
];

function statusLabel(status: ImplementationPhaseStatus): string {
  switch (status) {
    case 'not_started': return 'Not started';
    case 'in_progress': return 'In progress';
    case 'complete': return 'Complete';
  }
}

function statusPillClasses(status: ImplementationPhaseStatus): string {
  switch (status) {
    case 'not_started': return 'bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-400';
    case 'in_progress': return 'bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400';
    case 'complete': return 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400';
  }
}

function goLiveDate(startedAt: string): string {
  const start = new Date(startedAt);
  const target = new Date(start.getTime() + 14 * 24 * 60 * 60 * 1000);
  return target.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' });
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : 'Request failed';
}

function SubItemToggle({ checked, label, onChange, disabled }: {
  checked: boolean;
  label: string;
  onChange: () => void;
  disabled?: boolean;
}) {
  return (
    <label className="flex items-center gap-2 text-sm text-foreground">
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={onChange}
        className="rounded border-border"
      />
      {label}
    </label>
  );
}

function PhaseCard({
  title,
  dayRange,
  description,
  status,
  children,
}: {
  title: string;
  dayRange: string;
  description: string;
  status: ImplementationPhaseStatus;
  children: React.ReactNode;
}) {
  return (
    <div className="border border-border rounded-lg bg-card p-5 space-y-4">
      <div className="flex items-start justify-between gap-3">
        <div className="space-y-1">
          <div className="flex flex-wrap items-center gap-2">
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
    </div>
  );
}

export default function GettingStartedPage() {
  const { tenant } = useAuthStore();
  const [status, setStatus] = useState<ImplementationStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState<string | null>(null);

  const loadStatus = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      setStatus(await implementationApi.status());
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadStatus();
  }, [loadStatus, tenant?.id]);

  const runErpSync = async (provider: ImplementationErpProvider) => {
    try {
      setPending(`erp-${provider}`);
      setError(null);
      setStatus(await implementationApi.syncErp(provider));
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setPending(null);
    }
  };

  const updateErpSubItem = async (key: keyof ImplementationErpSubItems) => {
    if (!status) return;
    const subItems = {
      ...status.phases.erp.sub_items,
      [key]: !status.phases.erp.sub_items[key],
    };
    try {
      setPending(`erp-${key}`);
      setError(null);
      setStatus(await implementationApi.updateErpSubItems(subItems));
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setPending(null);
    }
  };

  const selectTemplate = async (template: string) => {
    try {
      setPending(`template-${template}`);
      setError(null);
      setStatus(await implementationApi.selectApprovalTemplate(template));
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setPending(null);
    }
  };

  const uploadSamples = async (files: FileList | null) => {
    if (!files?.length) return;
    try {
      setPending('samples');
      setError(null);
      const response = await implementationApi.uploadSampleInvoices(Array.from(files));
      setStatus(response.status);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setPending(null);
    }
  };

  const updateGoLiveCheck = async (key: keyof ImplementationGoLiveChecks) => {
    if (!status) return;
    const checks = {
      ...status.phases.go_live.checks,
      [key]: !status.phases.go_live.checks[key],
    };
    try {
      setPending(`go-live-${key}`);
      setError(null);
      setStatus(await implementationApi.updateChecklist(checks));
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setPending(null);
    }
  };

  if (loading && !status) {
    return (
      <div className="max-w-3xl mx-auto flex items-center gap-2 text-sm text-muted-foreground">
        <Loader2 className="w-4 h-4 animate-spin" />
        Loading implementation status
      </div>
    );
  }

  if (!status) {
    return (
      <div className="max-w-3xl mx-auto border border-destructive/40 rounded-lg p-5 text-sm text-destructive">
        <div className="flex items-center gap-2">
          <AlertCircle className="w-4 h-4" />
          {error ?? 'Unable to load implementation status'}
        </div>
      </div>
    );
  }

  const { phases } = status;
  const onTrack = status.percent_complete === 100
    || (status.day_number <= 3 && phases.erp.status !== 'not_started')
    || (status.day_number <= 6 && (phases.erp.status === 'complete' || phases.approvals.status !== 'not_started'))
    || status.percent_complete >= 75;

  return (
    <div className="max-w-3xl mx-auto space-y-6">
      <div className="space-y-2">
        <div className="flex items-center gap-2">
          <Rocket className="w-6 h-6 text-primary" />
          <h1 className="text-2xl font-bold text-foreground">Implementation Wizard</h1>
        </div>
        <p className="text-muted-foreground">
          Day <span data-testid="day-number">{status.day_number}</span> of 14
        </p>
        <div className="w-full bg-secondary rounded-full h-2.5">
          <div
            data-testid="progress-bar"
            className="h-2.5 rounded-full bg-primary transition-all"
            style={{ width: `${status.percent_complete}%` }}
          />
        </div>
        <p className="text-xs text-muted-foreground text-right" data-testid="progress-percent">
          {status.percent_complete}% complete
        </p>
      </div>

      {error && (
        <div className="border border-destructive/40 rounded-lg p-3 text-sm text-destructive flex items-center gap-2">
          <AlertCircle className="w-4 h-4" />
          {error}
        </div>
      )}

      <PhaseCard
        title="Connect your accounting system"
        dayRange="Days 1-3"
        description="Connect QuickBooks or Xero to auto-import your chart of accounts, vendors, and open purchase orders."
        status={phases.erp.status}
      >
        <div className="flex flex-wrap gap-2">
          <button
            type="button"
            onClick={() => runErpSync('quickbooks')}
            disabled={pending !== null}
            className="inline-flex items-center gap-1.5 rounded-lg bg-primary text-primary-foreground px-3 py-1.5 text-sm font-medium hover:bg-primary/90 disabled:opacity-60"
          >
            {pending === 'erp-quickbooks' ? <Loader2 className="w-4 h-4 animate-spin" /> : <Building2 className="w-4 h-4" />}
            Sync QuickBooks
          </button>
          <button
            type="button"
            onClick={() => runErpSync('xero')}
            disabled={pending !== null}
            className="inline-flex items-center gap-1.5 rounded-lg border border-border px-3 py-1.5 text-sm font-medium hover:border-primary/50 disabled:opacity-60"
          >
            {pending === 'erp-xero' ? <Loader2 className="w-4 h-4 animate-spin" /> : <Building2 className="w-4 h-4" />}
            Sync Xero
          </button>
        </div>
        <div className="space-y-2">
          <SubItemToggle
            checked={phases.erp.sub_items.chart_of_accounts}
            label="Chart of accounts imported"
            disabled={pending !== null}
            onChange={() => updateErpSubItem('chart_of_accounts')}
          />
          <SubItemToggle
            checked={phases.erp.sub_items.vendors}
            label="Vendors imported"
            disabled={pending !== null}
            onChange={() => updateErpSubItem('vendors')}
          />
          <SubItemToggle
            checked={phases.erp.sub_items.open_pos}
            label="Open POs imported"
            disabled={pending !== null}
            onChange={() => updateErpSubItem('open_pos')}
          />
        </div>
        {phases.erp.last_sync && (
          <p className="text-xs text-muted-foreground">
            {phases.erp.last_sync.message}
          </p>
        )}
        <Link href="/integrations" className="inline-flex items-center gap-1.5 text-sm font-medium text-primary hover:underline">
          <Building2 className="w-4 h-4" />
          Go to Integrations
          <ExternalLink className="w-3 h-3" />
        </Link>
      </PhaseCard>

      <PhaseCard
        title="Choose an approval-chain template"
        dayRange="Days 4-6"
        description="Select a preset approval routing strategy to get started quickly. You can customise it later."
        status={phases.approvals.status}
      >
        <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
          {APPROVAL_TEMPLATES.map((template) => {
            const selected = phases.approvals.template === template.id;
            return (
              <button
                key={template.id}
                type="button"
                disabled={pending !== null}
                onClick={() => selectTemplate(template.id)}
                className={`rounded-lg border p-3 text-left transition-colors disabled:opacity-60 ${
                  selected ? 'border-primary bg-primary/5 ring-1 ring-primary' : 'border-border hover:border-primary/50'
                }`}
              >
                <p className="text-sm font-medium text-foreground">{template.label}</p>
                <p className="text-xs text-muted-foreground mt-1">{template.description}</p>
              </button>
            );
          })}
        </div>
        <Link href="/processing/workflows" className="inline-flex items-center gap-1.5 text-sm font-medium text-primary hover:underline">
          <GitBranch className="w-4 h-4" />
          Go to Workflows
          <ExternalLink className="w-3 h-3" />
        </Link>
      </PhaseCard>

      <PhaseCard
        title="Validate OCR with 10 sample invoices"
        dayRange="Days 7-10"
        description="Upload sample invoices to validate OCR accuracy and coding suggestions before going live."
        status={phases.ocr.status}
      >
        <div className="flex flex-wrap items-center gap-4">
          <span className="text-sm text-foreground font-medium" data-testid="ocr-count">
            {phases.ocr.count} / 10 uploaded
          </span>
          {phases.ocr.count < 10 && (
            <label className="inline-flex items-center gap-1 rounded-lg bg-primary text-primary-foreground px-3 py-1.5 text-sm font-medium hover:bg-primary/90 transition-colors cursor-pointer">
              {pending === 'samples' ? <Loader2 className="w-4 h-4 animate-spin" /> : <Upload className="w-4 h-4" />}
              Upload Samples
              <input
                type="file"
                multiple
                className="sr-only"
                disabled={pending !== null}
                onChange={(event) => uploadSamples(event.currentTarget.files)}
              />
            </label>
          )}
        </div>
        <Link href="/invoices" className="inline-flex items-center gap-1.5 text-sm font-medium text-primary hover:underline">
          <ScanLine className="w-4 h-4" />
          Go to Invoices
          <ExternalLink className="w-3 h-3" />
        </Link>
      </PhaseCard>

      <PhaseCard
        title="Go-live checklist"
        dayRange="Days 11-14"
        description="Complete these final steps before your first production processing cycle."
        status={phases.go_live.status}
      >
        <div className="space-y-2">
          {GO_LIVE_ITEMS.map((item) => (
            <SubItemToggle
              key={item.key}
              checked={phases.go_live.checks[item.key]}
              label={item.label}
              disabled={pending !== null}
              onChange={() => updateGoLiveCheck(item.key)}
            />
          ))}
        </div>
      </PhaseCard>

      <div
        className={`rounded-lg p-4 text-sm font-medium ${
          onTrack
            ? 'bg-green-50 text-green-800 dark:bg-green-900/20 dark:text-green-400'
            : 'bg-amber-50 text-amber-800 dark:bg-amber-900/20 dark:text-amber-400'
        }`}
      >
        <div className="flex items-center gap-2">
          <Flag className="w-4 h-4" />
          <span>
            {status.percent_complete === 100
              ? 'All phases complete - you are live!'
              : `On track to go live by ${goLiveDate(status.started_at)}`}
          </span>
        </div>
      </div>
    </div>
  );
}
