'use client';

import { useCallback, useEffect, useState } from 'react';
import Link from 'next/link';
import {
  AlertCircle,
  Building2,
  CheckCircle2,
  ExternalLink,
  Flag,
  Gauge,
  GitBranch,
  Loader2,
  Mail,
  Rocket,
  ScanLine,
  Shield,
  Upload,
} from 'lucide-react';
import {
  implementationApi,
  type ImplementationErpProvider,
  type ImplementationErpSubItems,
  type ImplementationGoLiveChecks,
  type ImplementationModuleEntitlement,
  type ImplementationPhaseStatus,
  type ImplementationReadinessScorecard,
  type ImplementationStatus,
} from '@/lib/api';
import { useAuthStore } from '@/stores/auth';

const APPROVAL_TEMPLATES = [
  { id: 'amount', label: 'By amount threshold', description: 'Route invoices based on dollar value tiers.' },
  { id: 'department', label: 'By department', description: 'Each department has its own approver chain.' },
  { id: 'gl', label: 'By GL code', description: 'Assign approval paths by general ledger account.' },
];

const GO_LIVE_ITEMS: { key: keyof ImplementationGoLiveChecks; label: string; manual: boolean }[] = [
  { key: 'forwarding_email_verified', label: 'Email forwarding verified', manual: false },
  { key: 'sample_invoice_routed', label: 'Sample invoice routed end-to-end', manual: false },
  { key: 'notifications_acknowledged', label: 'Notifications acknowledged', manual: false },
  { key: 'privacy_mode_confirmed', label: 'Privacy mode confirmed', manual: false },
  { key: 'confirm_cutover_date', label: 'Confirm cutover date', manual: true },
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

const READINESS_THRESHOLD_PCT = 75;

function formatPercent(value: number): string {
  return `${Math.round(value * 100)}%`;
}

function ReadinessBacktest({
  scorecard,
  running,
  onRun,
  disabled,
}: {
  scorecard: ImplementationReadinessScorecard | null | undefined;
  running: boolean;
  onRun: () => void;
  disabled: boolean;
}) {
  const metrics = scorecard
    ? [
        { label: 'Auto-route coverage', value: scorecard.auto_route_coverage },
        { label: 'Auto-approve coverage', value: scorecard.auto_approve_coverage },
        { label: 'Vendor map coverage', value: scorecard.vendor_map_coverage },
        { label: 'Readiness score', value: scorecard.readiness_score },
      ]
    : [];

  return (
    <div className="space-y-3 rounded-lg border border-border bg-secondary/30 p-3">
      <div className="flex items-center justify-between gap-2">
        <h4 className="text-sm font-medium text-foreground flex items-center gap-1.5">
          <Gauge className="w-4 h-4" />
          Go-live readiness backtest
        </h4>
        <button
          type="button"
          onClick={onRun}
          disabled={disabled || running}
          className="inline-flex items-center gap-1.5 rounded-lg bg-primary text-primary-foreground px-3 py-1.5 text-sm font-medium hover:bg-primary/90 disabled:opacity-60"
        >
          {running ? <Loader2 className="w-4 h-4 animate-spin" /> : <Gauge className="w-4 h-4" />}
          {scorecard ? 'Re-run backtest' : 'Run readiness backtest'}
        </button>
      </div>
      <p className="text-xs text-muted-foreground">
        Replays configured workflow and categorization rules against up to 250 historical bills.
        Passes the gate at {READINESS_THRESHOLD_PCT}%.
      </p>
      {scorecard && (
        <div className="space-y-2">
          <div className="flex items-center gap-2">
            <span
              className={`inline-flex items-center gap-1 rounded-full px-2.5 py-0.5 text-xs font-medium ${
                scorecard.passes_threshold
                  ? 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400'
                  : 'bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400'
              }`}
              data-testid="readiness-pill"
            >
              {scorecard.passes_threshold
                ? <CheckCircle2 className="w-3 h-3" />
                : <AlertCircle className="w-3 h-3" />}
              {scorecard.passes_threshold
                ? `Ready (${formatPercent(scorecard.readiness_score)})`
                : `Not ready (${formatPercent(scorecard.readiness_score)})`}
            </span>
            <span className="text-xs text-muted-foreground" data-testid="readiness-sample-size">
              {scorecard.sample_size} bills sampled
            </span>
          </div>
          <dl className="grid grid-cols-2 gap-2 sm:grid-cols-4" data-testid="readiness-metrics">
            {metrics.map((metric) => (
              <div key={metric.label} className="rounded-md bg-background p-2">
                <dt className="text-xs text-muted-foreground">{metric.label}</dt>
                <dd className="text-sm font-semibold text-foreground">{formatPercent(metric.value)}</dd>
              </div>
            ))}
          </dl>
        </div>
      )}
    </div>
  );
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
  const [apTeamEmail, setApTeamEmail] = useState('');
  const [escalationEmail, setEscalationEmail] = useState('');

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

  const configurePrivacyMode = async (enabled: boolean, scope?: string) => {
    try {
      setPending('config-privacy');
      setError(null);
      setStatus(await implementationApi.updatePrivacyMode(enabled, scope));
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setPending(null);
    }
  };

  const configureCaptureChannels = async (params: {
    email_forwarding_address?: string;
    manual_upload_enabled?: boolean;
    erp_sync_enabled?: boolean;
  }) => {
    try {
      setPending('config-channels');
      setError(null);
      setStatus(await implementationApi.updateCaptureChannels(params));
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setPending(null);
    }
  };

  const verifyEmail = async () => {
    try {
      setPending('config-email-verify');
      setError(null);
      setStatus(await implementationApi.verifyEmailForwarding());
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setPending(null);
    }
  };

  const ackModules = async (entitlements: ImplementationModuleEntitlement[]) => {
    try {
      setPending('config-modules');
      setError(null);
      setStatus(await implementationApi.ackModuleEntitlements(entitlements));
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setPending(null);
    }
  };

  const configureNotifications = async (apTeam: string[], escalation: string[]) => {
    try {
      setPending('config-notifications');
      setError(null);
      setStatus(await implementationApi.updateNotificationApprovals(apTeam, escalation));
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setPending(null);
    }
  };

  const runBacktest = async () => {
    try {
      setPending('backtest');
      setError(null);
      // POST runs the backtest and persists the scorecard onto the wizard
      // state; refresh status so the rendered card reflects what landed.
      await implementationApi.runReadinessBacktest();
      setStatus(await implementationApi.status());
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
        title="Configuration"
        dayRange="Days 10-12"
        description="Configure privacy mode, capture channels, module entitlements, and notification approvals."
        status={phases.configuration.status}
      >
        <div className="space-y-4">
          <div className="space-y-2">
            <h4 className="text-sm font-medium text-foreground flex items-center gap-1.5">
              <Shield className="w-4 h-4" />
              Privacy mode
            </h4>
            <div className="flex items-center gap-3">
              <SubItemToggle
                checked={phases.configuration.configuration.privacy_mode.enabled}
                label="Require local OCR processing"
                disabled={pending !== null}
                onChange={() => configurePrivacyMode(!phases.configuration.configuration.privacy_mode.enabled)}
              />
              {phases.configuration.configuration.privacy_mode.confirmed_at && (
                <span className="text-xs text-green-600 dark:text-green-400">Confirmed</span>
              )}
            </div>
          </div>

          <div className="space-y-2">
            <h4 className="text-sm font-medium text-foreground flex items-center gap-1.5">
              <Mail className="w-4 h-4" />
              Capture channels
            </h4>
            <div className="space-y-1">
              <div className="flex items-center gap-3">
                <SubItemToggle
                  checked={phases.configuration.configuration.capture_channels.email_forwarding.verified_at !== null}
                  label="Email forwarding"
                  disabled={pending !== null}
                  onChange={() => {
                    // noop - verification is now server-driven
                  }}
                />
                {!phases.configuration.configuration.capture_channels.email_forwarding.verified_at && (
                  <div className="flex items-center gap-2">
                    {phases.configuration.configuration.capture_channels.email_forwarding.address ? (
                      <>
                        <span className="text-xs text-muted-foreground">
                          Send a test email to <strong>{phases.configuration.configuration.capture_channels.email_forwarding.address}</strong>, then
                        </span>
                        <button
                          type="button"
                          disabled={pending !== null}
                          onClick={() => verifyEmail()}
                          className="text-xs text-primary hover:underline disabled:opacity-60"
                        >
                          Verify
                        </button>
                      </>
                    ) : (
                      <span className="text-xs text-muted-foreground">No forwarding address configured</span>
                    )}
                  </div>
                )}
                {phases.configuration.configuration.capture_channels.email_forwarding.verified_at && (
                  <span className="text-xs text-green-600 dark:text-green-400">Verified</span>
                )}
              </div>
              <SubItemToggle
                checked={phases.configuration.configuration.capture_channels.manual_upload_enabled}
                label="Manual upload"
                disabled={pending !== null}
                onChange={() => configureCaptureChannels({ manual_upload_enabled: !phases.configuration.configuration.capture_channels.manual_upload_enabled })}
              />
              <SubItemToggle
                checked={phases.configuration.configuration.capture_channels.erp_sync_enabled}
                label="ERP sync"
                disabled={pending !== null}
                onChange={() => configureCaptureChannels({ erp_sync_enabled: !phases.configuration.configuration.capture_channels.erp_sync_enabled })}
              />
            </div>
          </div>

          <div className="space-y-2">
            <h4 className="text-sm font-medium text-foreground">Module entitlements</h4>
            {phases.configuration.configuration.module_entitlements.length === 0 ? (
              <button
                type="button"
                disabled={pending !== null}
                onClick={() => {
                  void ackModules([]);
                }}
                className="text-sm text-primary hover:underline disabled:opacity-60"
              >
                Acknowledge assigned modules
              </button>
            ) : (
              <div className="space-y-1">
                {phases.configuration.configuration.module_entitlements.map((ent) => (
                  <div key={ent.module_key} className="flex items-center gap-2 text-sm text-muted-foreground">
                    <CheckCircle2 className="w-3.5 h-3.5 text-green-600 dark:text-green-400" />
                    {ent.module_key}
                  </div>
                ))}
              </div>
            )}
          </div>

          <div className="space-y-2">
            <h4 className="text-sm font-medium text-foreground">Notification approvals</h4>
            {phases.configuration.configuration.notification_approvals.approved_at ? (
              <div className="flex items-center gap-2 text-sm text-green-600 dark:text-green-400">
                <CheckCircle2 className="w-3.5 h-3.5" />
                Approved
              </div>
            ) : (
              <div className="space-y-2">
                <div>
                  <label className="block text-xs text-muted-foreground mb-1">AP team distribution emails</label>
                  <input
                    type="text"
                    placeholder="ap-team@company.com"
                    onChange={(e) => setApTeamEmail(e.target.value)}
                    className="w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm"
                  />
                </div>
                <div>
                  <label className="block text-xs text-muted-foreground mb-1">Escalation distribution emails</label>
                  <input
                    type="text"
                    placeholder="manager@company.com"
                    onChange={(e) => setEscalationEmail(e.target.value)}
                    className="w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm"
                  />
                </div>
                <button
                  type="button"
                  disabled={pending !== null || !apTeamEmail.includes('@') || !escalationEmail.includes('@')}
                  onClick={() => configureNotifications(
                    apTeamEmail.split(',').map((s) => s.trim()).filter(Boolean),
                    escalationEmail.split(',').map((s) => s.trim()).filter(Boolean),
                  )}
                  className="text-sm text-primary hover:underline disabled:opacity-60"
                >
                  Save notification routing
                </button>
              </div>
            )}
          </div>
        </div>
      </PhaseCard>

      <PhaseCard
        title="Go-live checklist"
        dayRange="Days 11-14"
        description="Complete these final steps before your first production processing cycle."
        status={phases.go_live.status}
      >
        <div className="space-y-2">
          {GO_LIVE_ITEMS.map((item) => (
            item.manual ? (
              <SubItemToggle
                key={item.key}
                checked={phases.go_live.checks[item.key]}
                label={item.label}
                disabled={pending !== null}
                onChange={() => updateGoLiveCheck(item.key)}
              />
            ) : (
              <div key={item.key} className="flex items-center gap-2 text-sm">
                {phases.go_live.checks[item.key]
                  ? <CheckCircle2 className="w-4 h-4 text-green-600 dark:text-green-400" />
                  : <div className="w-4 h-4 rounded-full border border-border" />}
                <span className={phases.go_live.checks[item.key] ? 'text-foreground' : 'text-muted-foreground'}>
                  {item.label}
                </span>
              </div>
            )
          ))}
        </div>
        <ReadinessBacktest
          scorecard={phases.go_live.backtest_scorecard}
          running={pending === 'backtest'}
          onRun={() => runBacktest()}
          disabled={pending !== null}
        />
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
