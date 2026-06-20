'use client';

import { useState, useEffect, type FormEvent } from 'react';
import {
  Cable,
  CheckCircle2,
  XCircle,
  ArrowRight,
  ExternalLink,
  Shield,
  Zap,
  Users,
  Globe,
  Database,
  BarChart3,
  Loader2,
  AlertTriangle,
  Bell,
  MessageSquare,
  X,
  Lock,
} from 'lucide-react';
import {
  api,
  billingApi,
  BillingModule,
  BillingSubscription,
  billComApi,
  getIntegrationStatus,
  IntegrationStatusResponse,
  notificationsApi,
  sageIntacctApi,
} from '@/lib/api';
import { useAuthStore } from '@/stores/auth';

type IntegrationStatus =
  | 'connected'
  | 'disconnected'
  | 'available'
  | 'loading'
  | 'error'
  | 'locked'
  | 'unsupported';
type IntegrationCategory = 'erp' | 'crm' | 'payments' | 'notifications' | 'all';

interface IntegrationStatusState {
  status: IntegrationStatus;
  liveStatus?: IntegrationStatusResponse;
  detail?: string;
  error?: string;
}

interface Integration {
  id: string;
  name: string;
  description: string;
  longDescription: string;
  category: Exclude<IntegrationCategory, 'all'>;
  status: IntegrationStatus;
  logo: string;
  authType: 'oauth' | 'credentials' | 'slack' | 'teams';
  capabilities: string[];
  endpoints: {
    connect?: string;
    status?: string;
    disconnect?: string;
  };
  docsUrl?: string;
  requiredModule?: string;
}

interface SageCredentials {
  sender_id: string;
  sender_password: string;
  company_id: string;
  entity_id: string;
  user_id: string;
  user_password: string;
}

interface BillComCredentials {
  dev_key: string;
  org_id: string;
  user_name: string;
  password: string;
  environment: 'sandbox' | 'production';
}

const integrations: Integration[] = [
  {
    id: 'quickbooks',
    name: 'QuickBooks Online',
    description: 'Sync vendors, GL accounts, and export approved invoices as bills.',
    longDescription: 'Connect your QuickBooks Online account to automatically sync vendor master data, map GL accounts, and push approved invoices directly to QuickBooks as AP bills. Supports both sandbox and production environments.',
    category: 'erp',
    status: 'disconnected',
    logo: '/integrations/quickbooks.svg',
    authType: 'oauth',
    capabilities: [
      'Vendor master data sync',
      'GL account mapping',
      'AP bill export',
      'Two-way sync status',
    ],
    endpoints: {
      connect: '/api/v1/quickbooks/connect',
      status: '/api/v1/quickbooks/status',
      disconnect: '/api/v1/quickbooks/disconnect',
    },
    docsUrl: 'https://developer.intuit.com/app/developer/qbo/docs',
    requiredModule: 'quickbooks',
  },
  {
    id: 'xero',
    name: 'Xero',
    description: 'Sync contacts, chart of accounts, and export bills to Xero.',
    longDescription: 'Connect to Xero to synchronize your contact list as vendors, map your chart of accounts, and export approved invoices as Xero bills. Full OAuth 2.0 integration with automatic token refresh.',
    category: 'erp',
    status: 'disconnected',
    logo: '/integrations/xero.svg',
    authType: 'oauth',
    capabilities: [
      'Contact / vendor sync',
      'Chart of accounts mapping',
      'Bill export',
      'Multi-org support',
    ],
    endpoints: {
      connect: '/api/v1/xero/connect',
      status: '/api/v1/xero/status',
      disconnect: '/api/v1/xero/disconnect',
    },
    docsUrl: 'https://developer.xero.com/documentation',
    requiredModule: 'xero',
  },
  {
    id: 'sage-intacct',
    name: 'Sage Intacct',
    description: 'Enterprise ERP with multi-entity support, GL sync, and AP bill automation.',
    longDescription: 'Connect to Sage Intacct for mid-market and enterprise AP automation. Supports multi-entity/subsidiary structures, dimensional GL account mapping, vendor master sync, and direct AP bill creation via the XML Web Services API. Ideal for organizations with complex chart-of-accounts and multi-location operations.',
    category: 'erp',
    status: 'disconnected',
    logo: '/integrations/sage-intacct.svg',
    authType: 'credentials',
    capabilities: [
      'Multi-entity / subsidiary support',
      'Vendor master data sync',
      'Dimensional GL account mapping',
      'AP bill export (create_potransaction)',
      'Session-based authentication',
    ],
    endpoints: {
      connect: '/api/v1/sage-intacct/connect',
      status: '/api/v1/sage-intacct/status',
      disconnect: '/api/v1/sage-intacct/disconnect',
    },
    docsUrl: 'https://developer.intacct.com/api/',
    requiredModule: 'sage_intacct',
  },
  {
    id: 'netsuite',
    name: 'NetSuite',
    description: 'Oracle NetSuite vendor bill and payment automation.',
    longDescription: 'NetSuite is included as a paid add-on. Connecting is currently disabled because real NetSuite OAuth 2.0 M2M requires signed JWT client_assertion authentication, which is still being implemented. The card is shown so the entitlement is visible; reach out to support for sandbox onboarding once JWT signing ships.',
    category: 'erp',
    status: 'disconnected',
    logo: '/integrations/netsuite.svg',
    authType: 'credentials',
    capabilities: [
      'Vendor master data sync',
      'Vendor bill export',
      'Sandbox + production environments',
    ],
    endpoints: {
      connect: '/api/v1/netsuite/connect',
      status: '/api/v1/netsuite/status',
      disconnect: '/api/v1/netsuite/disconnect',
    },
    docsUrl: 'https://docs.oracle.com/en/cloud/saas/netsuite/index.html',
    requiredModule: 'net_suite',
  },
  {
    id: 'salesforce',
    name: 'Salesforce',
    description: 'CRM integration for vendor enrichment, contact sync, and PO linkage.',
    longDescription: 'Connect Salesforce to enrich your AP workflow with CRM data. Sync Salesforce Accounts as vendors, pull contact details for vendor communication, and link PO numbers from Opportunities for automated 3-way matching. Push payment status back to Salesforce for complete vendor lifecycle visibility.',
    category: 'crm',
    status: 'disconnected',
    logo: '/integrations/salesforce.svg',
    authType: 'oauth',
    capabilities: [
      'Account → Vendor sync',
      'Contact enrichment',
      'PO number linkage via Opportunities',
      'Payment status push-back',
      'Custom object support',
    ],
    endpoints: {
      connect: '/api/v1/salesforce/connect',
      status: '/api/v1/salesforce/status',
      disconnect: '/api/v1/salesforce/disconnect',
    },
    docsUrl: 'https://developer.salesforce.com/docs',
    requiredModule: 'salesforce',
  },
  {
    id: 'workday',
    name: 'Workday',
    description: 'Enterprise ERP with supplier sync, invoice export, and multi-company support.',
    longDescription: 'Connect Workday Financial Management for enterprise-grade AP automation. Sync suppliers as vendors, map ledger accounts and spend categories, and export approved invoices as Workday supplier invoices. Supports multi-company environments with OAuth 2.0 (API Client registration) and automatic token refresh.',
    category: 'erp',
    status: 'disconnected' as IntegrationStatus,
    logo: '/integrations/workday.svg',
    authType: 'oauth' as const,
    capabilities: [
      'Supplier → Vendor sync',
      'Ledger account mapping',
      'Supplier invoice export',
      'Multi-company support',
      'Spend category sync',
    ],
    endpoints: {
      connect: '/api/v1/workday/connect',
      status: '/api/v1/workday/status',
      disconnect: '/api/v1/workday/disconnect',
    },
    docsUrl: 'https://developer.workday.com/documentation',
    requiredModule: 'workday',
  },
  {
    id: 'bill-com',
    name: 'Bill.com',
    description: 'AP payment execution - ACH, check, and virtual card payments from approved invoices.',
    longDescription: 'Connect Bill.com to execute payments directly from BillForge. After an invoice is approved and pushed to Bill.com as a bill, pay vendors via ACH, check, or virtual card - all without leaving BillForge. Supports bulk payments, funding account selection, and real-time payment status tracking. Uses session-based API authentication.',
    category: 'payments',
    status: 'disconnected' as IntegrationStatus,
    logo: '/integrations/bill-com.svg',
    authType: 'credentials' as const,
    capabilities: [
      'Vendor sync',
      'Push approved invoices as bills',
      'ACH / Check / Virtual Card payments',
      'Bulk payment support',
      'Payment status tracking',
    ],
    endpoints: {
      connect: '/api/v1/bill-com/connect',
      status: '/api/v1/bill-com/status',
      disconnect: '/api/v1/bill-com/disconnect',
    },
    docsUrl: 'https://developer.bill.com/docs',
    requiredModule: 'bill_com',
  },
  {
    id: 'slack',
    name: 'Slack',
    description: 'Send approval requests, invoice exceptions, and AP updates into Slack.',
    longDescription: 'Install the Slack app so BillForge can notify finance teams and approvers in the channels where invoice work already happens. The existing notification preferences control which events are delivered after installation.',
    category: 'notifications',
    status: 'disconnected' as IntegrationStatus,
    logo: '/integrations/slack.svg',
    authType: 'slack' as const,
    capabilities: [
      'Approval request notifications',
      'Invoice exception alerts',
      'Direct approver messages',
      'Preference-based routing',
    ],
    endpoints: {},
    docsUrl: 'https://api.slack.com/start',
  },
  {
    id: 'teams',
    name: 'Microsoft Teams',
    description: 'Post AP notifications to a Teams channel through an incoming webhook.',
    longDescription: 'Configure a Microsoft Teams incoming webhook for invoice and approval notifications. BillForge stores the webhook configuration and uses notification preferences to determine which events are sent.',
    category: 'notifications',
    status: 'disconnected' as IntegrationStatus,
    logo: '/integrations/teams.svg',
    authType: 'teams' as const,
    capabilities: [
      'Teams channel notifications',
      'Approval and exception updates',
      'Webhook-based setup',
      'Preference-based routing',
    ],
    endpoints: {},
    docsUrl: 'https://learn.microsoft.com/microsoftteams/platform/webhooks-and-connectors/how-to/add-incoming-webhook',
  },
];

const categoryIcons = {
  erp: Database,
  crm: Users,
  payments: BarChart3,
  notifications: Bell,
};

const categoryLabels = {
  erp: 'ERP / Accounting',
  crm: 'CRM',
  payments: 'Payments',
  notifications: 'Notifications',
};

function IntegrationLogo({ integration }: { integration: Integration }) {
  // Use styled fallback icons since we may not have actual logo SVGs
  const logoMap: Record<string, { bg: string; text: string; label: string }> = {
    'quickbooks': { bg: 'bg-emerald-100 dark:bg-emerald-900/40', text: 'text-emerald-700 dark:text-emerald-300', label: 'QB' },
    'xero': { bg: 'bg-sky-100 dark:bg-sky-900/40', text: 'text-sky-700 dark:text-sky-300', label: 'XR' },
    'sage-intacct': { bg: 'bg-green-100 dark:bg-green-900/40', text: 'text-green-700 dark:text-green-300', label: 'SI' },
    'netsuite': { bg: 'bg-red-100 dark:bg-red-900/40', text: 'text-red-700 dark:text-red-300', label: 'NS' },
    'salesforce': { bg: 'bg-blue-100 dark:bg-blue-900/40', text: 'text-blue-700 dark:text-blue-300', label: 'SF' },
    'workday': { bg: 'bg-orange-100 dark:bg-orange-900/40', text: 'text-orange-700 dark:text-orange-300', label: 'WD' },
    'bill-com': { bg: 'bg-violet-100 dark:bg-violet-900/40', text: 'text-violet-700 dark:text-violet-300', label: 'BC' },
    'slack': { bg: 'bg-fuchsia-100 dark:bg-fuchsia-900/40', text: 'text-fuchsia-700 dark:text-fuchsia-300', label: 'SL' },
    'teams': { bg: 'bg-indigo-100 dark:bg-indigo-900/40', text: 'text-indigo-700 dark:text-indigo-300', label: 'TM' },
  };

  const style = logoMap[integration.id] || { bg: 'bg-gray-100 dark:bg-gray-800', text: 'text-gray-700 dark:text-gray-300', label: '??' };

  return (
    <div className={`w-12 h-12 rounded-xl ${style.bg} flex items-center justify-center`}>
      <span className={`text-lg font-bold ${style.text}`}>{style.label}</span>
    </div>
  );
}

function StatusBadge({ status }: { status: IntegrationStatus }) {
  if (status === 'locked') {
    return (
      <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-amber-100 text-amber-700 dark:bg-amber-900/40 dark:text-amber-300">
        <Lock className="h-3.5 w-3.5" />
        Upgrade required
      </span>
    );
  }
  if (status === 'unsupported') {
    return (
      <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-amber-100 text-amber-700 dark:bg-amber-900/40 dark:text-amber-300">
        <AlertTriangle className="h-3.5 w-3.5" />
        Auth setup pending
      </span>
    );
  }
  if (status === 'connected') {
    return (
      <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-emerald-100 text-emerald-700 dark:bg-emerald-900/40 dark:text-emerald-300">
        <CheckCircle2 className="h-3.5 w-3.5" />
        Connected
      </span>
    );
  }
  if (status === 'loading') {
    return (
      <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-zinc-100 text-zinc-500 dark:bg-zinc-800 dark:text-zinc-400">
        <Loader2 className="h-3.5 w-3.5 animate-spin" />
        Checking
      </span>
    );
  }
  if (status === 'error') {
    return (
      <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-amber-100 text-amber-700 dark:bg-amber-900/40 dark:text-amber-300">
        <AlertTriangle className="h-3.5 w-3.5" />
        Status unavailable
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-zinc-100 text-zinc-500 dark:bg-zinc-800 dark:text-zinc-400">
      <XCircle className="h-3.5 w-3.5" />
      Not connected
    </span>
  );
}

function IntegrationCard({ integration, liveStatus, subscriptionLoading, onConnect, onDisconnect, onRefresh, onUpgrade }: {
  integration: Integration;
  liveStatus?: IntegrationStatusState;
  subscriptionLoading: boolean;
  onConnect: (id: string) => void;
  onDisconnect: (id: string) => void;
  onRefresh: (id: string) => void;
  onUpgrade: (id: string) => void;
}) {
  const [expanded, setExpanded] = useState(false);

  const displayStatus = liveStatus?.status ?? 'loading';
  const isLocked = displayStatus === 'locked';
  const lastSync = liveStatus?.liveStatus?.last_sync_at;

  return (
    <div className={`bg-white dark:bg-zinc-900 border border-zinc-200 dark:border-zinc-800 rounded-xl overflow-hidden hover:border-zinc-300 dark:hover:border-zinc-700 transition-all ${isLocked ? 'opacity-75' : ''}`}>
      <div className="p-5">
        <div className="flex items-start gap-4">
          <IntegrationLogo integration={integration} />
          <div className="flex-1 min-w-0">
            <div className="flex items-center justify-between gap-3">
              <h3 className="text-base font-semibold text-zinc-900 dark:text-zinc-100">
                {integration.name}
              </h3>
              <StatusBadge status={displayStatus} />
            </div>
            <p className="mt-1 text-sm text-zinc-500 dark:text-zinc-400">
              {integration.description}
            </p>
            {isLocked && integration.requiredModule && (
              <p className="mt-0.5 text-xs text-amber-600 dark:text-amber-400">
                Requires {integration.name} add-on
              </p>
            )}
            {displayStatus === 'unsupported' && (
              <p className="mt-0.5 text-xs text-amber-600 dark:text-amber-400">
                NetSuite JWT authentication is not yet available. Contact support for sandbox onboarding.
              </p>
            )}
            {lastSync && (
              <p className="mt-0.5 text-xs text-zinc-400 dark:text-zinc-500">
                Last synced: {new Date(lastSync).toLocaleString()}
              </p>
            )}
            {liveStatus?.detail && (
              <p className="mt-0.5 text-xs text-zinc-400 dark:text-zinc-500">
                {liveStatus.detail}
              </p>
            )}
            {displayStatus === 'error' && (
              <p className="mt-1 text-xs text-amber-600 dark:text-amber-400">
                {liveStatus?.error ?? 'Status could not be refreshed.'} Last known state is preserved when available.
              </p>
            )}
          </div>
        </div>

        {/* Capabilities */}
        <div className="mt-4 flex flex-wrap gap-2">
          {integration.capabilities.slice(0, 3).map((cap) => (
            <span
              key={cap}
              className="inline-flex items-center px-2 py-0.5 rounded-md text-xs bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400"
            >
              {cap}
            </span>
          ))}
          {integration.capabilities.length > 3 && (
            <span className="inline-flex items-center px-2 py-0.5 rounded-md text-xs bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400">
              +{integration.capabilities.length - 3} more
            </span>
          )}
        </div>

        {/* Expanded details */}
        {expanded && (
          <div className="mt-4 pt-4 border-t border-zinc-100 dark:border-zinc-800">
            <p className="text-sm text-zinc-600 dark:text-zinc-400 leading-relaxed">
              {integration.longDescription}
            </p>
            <div className="mt-3 space-y-2">
              <h4 className="text-xs font-semibold uppercase tracking-wider text-zinc-400 dark:text-zinc-500">
                Capabilities
              </h4>
              <ul className="space-y-1.5">
                {integration.capabilities.map((cap) => (
                  <li key={cap} className="flex items-center gap-2 text-sm text-zinc-600 dark:text-zinc-400">
                    <Zap className="h-3.5 w-3.5 text-zinc-400" />
                    {cap}
                  </li>
                ))}
              </ul>
            </div>
            {integration.authType === 'credentials' && displayStatus !== 'unsupported' && (
              <div className="mt-3 flex items-center gap-2 text-xs text-amber-600 dark:text-amber-400 bg-amber-50 dark:bg-amber-900/20 px-3 py-2 rounded-lg">
                <Shield className="h-3.5 w-3.5" />
                This integration uses credential-based authentication. Your credentials are encrypted at rest.
              </div>
            )}
            {(integration.authType === 'slack' || integration.authType === 'teams') && (
              <div className="mt-3 flex items-center gap-2 text-xs text-sky-600 dark:text-sky-400 bg-sky-50 dark:bg-sky-900/20 px-3 py-2 rounded-lg">
                <MessageSquare className="h-3.5 w-3.5" />
                Notification delivery is controlled by your notification preferences after setup.
              </div>
            )}
          </div>
        )}
      </div>

      {/* Actions */}
      <div className="px-5 py-3 bg-zinc-50 dark:bg-zinc-900/50 border-t border-zinc-100 dark:border-zinc-800 flex items-center justify-between">
        <button
          onClick={() => setExpanded(!expanded)}
          className="text-sm text-zinc-500 hover:text-zinc-700 dark:text-zinc-400 dark:hover:text-zinc-200 transition-colors"
        >
          {expanded ? 'Show less' : 'Learn more'}
        </button>
        <div className="flex items-center gap-2">
          {integration.docsUrl && (
            <a
              href={integration.docsUrl}
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-1.5 px-3 py-1.5 text-sm text-zinc-500 hover:text-zinc-700 dark:text-zinc-400 dark:hover:text-zinc-200 transition-colors"
            >
              <ExternalLink className="h-3.5 w-3.5" />
              Docs
            </a>
          )}
          {displayStatus === 'locked' ? (
            <button
              onClick={() => onUpgrade(integration.id)}
              aria-label={`Upgrade required for ${integration.name}`}
              className="inline-flex items-center gap-1.5 px-4 py-1.5 text-sm font-medium text-amber-700 hover:text-amber-800 dark:text-amber-300 dark:hover:text-amber-200 bg-amber-50 dark:bg-amber-900/20 hover:bg-amber-100 dark:hover:bg-amber-900/30 rounded-lg transition-colors"
            >
              <Lock className="h-3.5 w-3.5" />
              Upgrade required
            </button>
          ) : displayStatus === 'unsupported' ? (
            <button
              type="button"
              disabled
              aria-label={`${integration.name} connect unavailable`}
              title="NetSuite JWT authentication is not yet available. Contact support for sandbox onboarding."
              className="inline-flex items-center gap-1.5 px-4 py-1.5 text-sm font-medium text-zinc-500 dark:text-zinc-400 bg-zinc-100 dark:bg-zinc-800 rounded-lg cursor-not-allowed"
            >
              <Lock className="h-3.5 w-3.5" />
              Connect unavailable
            </button>
          ) : displayStatus === 'connected' ? (
            <button
              onClick={() => onDisconnect(integration.id)}
              aria-label={`Disconnect ${integration.name}`}
              className="inline-flex items-center gap-1.5 px-4 py-1.5 text-sm font-medium text-red-600 hover:text-red-700 dark:text-red-400 dark:hover:text-red-300 bg-red-50 dark:bg-red-900/20 hover:bg-red-100 dark:hover:bg-red-900/30 rounded-lg transition-colors"
            >
              Disconnect
            </button>
          ) : displayStatus === 'error' ? (
            <button
              onClick={() => onRefresh(integration.id)}
              aria-label={`Retry ${integration.name} status`}
              className="inline-flex items-center gap-1.5 px-4 py-1.5 text-sm font-medium text-amber-700 hover:text-amber-800 dark:text-amber-300 dark:hover:text-amber-200 bg-amber-50 dark:bg-amber-900/20 hover:bg-amber-100 dark:hover:bg-amber-900/30 rounded-lg transition-colors"
            >
              Retry status
            </button>
          ) : displayStatus === 'loading' ? (
            <button
              disabled
              className="inline-flex items-center gap-1.5 px-4 py-1.5 text-sm font-medium text-zinc-500 bg-zinc-100 dark:bg-zinc-800 rounded-lg cursor-not-allowed"
            >
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              Checking
            </button>
          ) : (
            <button
              onClick={() => onConnect(integration.id)}
              aria-label={`Connect ${integration.name}`}
              className="inline-flex items-center gap-1.5 px-4 py-1.5 text-sm font-medium text-white bg-zinc-900 dark:bg-zinc-100 dark:text-zinc-900 hover:bg-zinc-800 dark:hover:bg-zinc-200 rounded-lg transition-colors"
            >
              Connect
              <ArrowRight className="h-3.5 w-3.5" />
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

const emptySageCredentials: SageCredentials = {
  sender_id: '',
  sender_password: '',
  company_id: '',
  entity_id: '',
  user_id: '',
  user_password: '',
};

const emptyBillComCredentials: BillComCredentials = {
  dev_key: '',
  org_id: '',
  user_name: '',
  password: '',
  environment: 'sandbox',
};

function TextField({
  label,
  value,
  onChange,
  type = 'text',
  required = true,
  autoComplete,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  type?: string;
  required?: boolean;
  autoComplete?: string;
}) {
  return (
    <label className="block">
      <span className="text-sm font-medium text-zinc-700 dark:text-zinc-300">{label}</span>
      <input
        type={type}
        required={required}
        value={value}
        autoComplete={autoComplete}
        onChange={(event) => onChange(event.target.value)}
        className="mt-1 w-full px-3 py-2 text-sm border border-zinc-200 dark:border-zinc-700 rounded-lg bg-white dark:bg-zinc-950 text-zinc-900 dark:text-zinc-100 focus:outline-none focus:ring-2 focus:ring-zinc-400 dark:focus:ring-zinc-600"
      />
    </label>
  );
}

function CredentialModal({
  integration,
  sageCredentials,
  billComCredentials,
  error,
  isSubmitting,
  onClose,
  onSubmit,
  onSageChange,
  onBillComChange,
}: {
  integration: Integration;
  sageCredentials: SageCredentials;
  billComCredentials: BillComCredentials;
  error: string | null;
  isSubmitting: boolean;
  onClose: () => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  onSageChange: (credentials: SageCredentials) => void;
  onBillComChange: (credentials: BillComCredentials) => void;
}) {
  const isSage = integration.id === 'sage-intacct';

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 px-4" role="dialog" aria-modal="true" aria-labelledby="credential-modal-title">
      <div className="w-full max-w-2xl bg-white dark:bg-zinc-950 border border-zinc-200 dark:border-zinc-800 rounded-xl shadow-xl">
        <div className="flex items-start justify-between gap-4 px-5 py-4 border-b border-zinc-200 dark:border-zinc-800">
          <div>
            <h2 id="credential-modal-title" className="text-lg font-semibold text-zinc-900 dark:text-zinc-100">
              Connect {integration.name}
            </h2>
            <p className="mt-1 text-sm text-zinc-500 dark:text-zinc-400">
              Enter the credentials BillForge will use to verify and sync this integration.
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="p-1.5 rounded-lg text-zinc-500 hover:text-zinc-900 hover:bg-zinc-100 dark:text-zinc-400 dark:hover:text-zinc-100 dark:hover:bg-zinc-800"
            aria-label="Close credential form"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <form onSubmit={onSubmit} className="p-5 space-y-5">
          {isSage ? (
            <div className="grid sm:grid-cols-2 gap-4">
              <TextField
                label="Sender ID"
                value={sageCredentials.sender_id}
                onChange={(value) => onSageChange({ ...sageCredentials, sender_id: value })}
                autoComplete="off"
              />
              <TextField
                label="Sender Password"
                type="password"
                value={sageCredentials.sender_password}
                onChange={(value) => onSageChange({ ...sageCredentials, sender_password: value })}
                autoComplete="new-password"
              />
              <TextField
                label="Company ID"
                value={sageCredentials.company_id}
                onChange={(value) => onSageChange({ ...sageCredentials, company_id: value })}
                autoComplete="organization"
              />
              <TextField
                label="Entity ID"
                value={sageCredentials.entity_id}
                onChange={(value) => onSageChange({ ...sageCredentials, entity_id: value })}
                required={false}
                autoComplete="off"
              />
              <TextField
                label="User ID"
                value={sageCredentials.user_id}
                onChange={(value) => onSageChange({ ...sageCredentials, user_id: value })}
                autoComplete="username"
              />
              <TextField
                label="User Password"
                type="password"
                value={sageCredentials.user_password}
                onChange={(value) => onSageChange({ ...sageCredentials, user_password: value })}
                autoComplete="new-password"
              />
            </div>
          ) : (
            <div className="grid sm:grid-cols-2 gap-4">
              <TextField
                label="Developer Key"
                value={billComCredentials.dev_key}
                onChange={(value) => onBillComChange({ ...billComCredentials, dev_key: value })}
                autoComplete="off"
              />
              <TextField
                label="Organization ID"
                value={billComCredentials.org_id}
                onChange={(value) => onBillComChange({ ...billComCredentials, org_id: value })}
                autoComplete="organization"
              />
              <TextField
                label="User Name"
                value={billComCredentials.user_name}
                onChange={(value) => onBillComChange({ ...billComCredentials, user_name: value })}
                autoComplete="username"
              />
              <TextField
                label="Password"
                type="password"
                value={billComCredentials.password}
                onChange={(value) => onBillComChange({ ...billComCredentials, password: value })}
                autoComplete="new-password"
              />
              <label className="block sm:col-span-2">
                <span className="text-sm font-medium text-zinc-700 dark:text-zinc-300">Environment</span>
                <select
                  value={billComCredentials.environment}
                  onChange={(event) =>
                    onBillComChange({
                      ...billComCredentials,
                      environment: event.target.value as BillComCredentials['environment'],
                    })
                  }
                  className="mt-1 w-full px-3 py-2 text-sm border border-zinc-200 dark:border-zinc-700 rounded-lg bg-white dark:bg-zinc-950 text-zinc-900 dark:text-zinc-100 focus:outline-none focus:ring-2 focus:ring-zinc-400 dark:focus:ring-zinc-600"
                >
                  <option value="sandbox">Sandbox</option>
                  <option value="production">Production</option>
                </select>
              </label>
            </div>
          )}

          {error && (
            <div className="rounded-lg bg-red-50 dark:bg-red-900/20 px-3 py-2 text-sm text-red-700 dark:text-red-300">
              {error}
            </div>
          )}

          <div className="flex items-center justify-end gap-3">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 text-sm font-medium text-zinc-600 hover:text-zinc-900 dark:text-zinc-400 dark:hover:text-zinc-100"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={isSubmitting}
              className="inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-white bg-zinc-900 hover:bg-zinc-800 disabled:bg-zinc-400 dark:bg-zinc-100 dark:text-zinc-900 dark:hover:bg-zinc-200 rounded-lg transition-colors"
            >
              {isSubmitting && <Loader2 className="h-4 w-4 animate-spin" />}
              Connect
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

function TeamsModal({
  webhookUrl,
  channelName,
  error,
  isSubmitting,
  onClose,
  onSubmit,
  onWebhookUrlChange,
  onChannelNameChange,
}: {
  webhookUrl: string;
  channelName: string;
  error: string | null;
  isSubmitting: boolean;
  onClose: () => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  onWebhookUrlChange: (value: string) => void;
  onChannelNameChange: (value: string) => void;
}) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 px-4" role="dialog" aria-modal="true" aria-labelledby="teams-modal-title">
      <div className="w-full max-w-xl bg-white dark:bg-zinc-950 border border-zinc-200 dark:border-zinc-800 rounded-xl shadow-xl">
        <div className="flex items-start justify-between gap-4 px-5 py-4 border-b border-zinc-200 dark:border-zinc-800">
          <div>
            <h2 id="teams-modal-title" className="text-lg font-semibold text-zinc-900 dark:text-zinc-100">
              Configure Microsoft Teams
            </h2>
            <p className="mt-1 text-sm text-zinc-500 dark:text-zinc-400">
              Paste the incoming webhook URL for the Teams channel that should receive AP notifications.
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="p-1.5 rounded-lg text-zinc-500 hover:text-zinc-900 hover:bg-zinc-100 dark:text-zinc-400 dark:hover:text-zinc-100 dark:hover:bg-zinc-800"
            aria-label="Close Teams form"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <form onSubmit={onSubmit} className="p-5 space-y-4">
          <TextField
            label="Webhook URL"
            type="url"
            value={webhookUrl}
            onChange={onWebhookUrlChange}
            autoComplete="url"
          />
          <TextField
            label="Channel Name"
            value={channelName}
            onChange={onChannelNameChange}
            required={false}
            autoComplete="off"
          />

          {error && (
            <div className="rounded-lg bg-red-50 dark:bg-red-900/20 px-3 py-2 text-sm text-red-700 dark:text-red-300">
              {error}
            </div>
          )}

          <div className="flex items-center justify-end gap-3">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 text-sm font-medium text-zinc-600 hover:text-zinc-900 dark:text-zinc-400 dark:hover:text-zinc-100"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={isSubmitting}
              className="inline-flex items-center gap-2 px-4 py-2 text-sm font-medium text-white bg-zinc-900 hover:bg-zinc-800 disabled:bg-zinc-400 dark:bg-zinc-100 dark:text-zinc-900 dark:hover:bg-zinc-200 rounded-lg transition-colors"
            >
              {isSubmitting && <Loader2 className="h-4 w-4 animate-spin" />}
              Save
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

function getInitialCategory(): IntegrationCategory {
  if (typeof window === 'undefined') return 'all';
  const category = new URLSearchParams(window.location.search).get('category');
  return category === 'erp' || category === 'crm' || category === 'payments' || category === 'notifications'
    ? category
    : 'all';
}

export default function IntegrationsPage() {
  const [filter, setFilter] = useState<IntegrationCategory>(getInitialCategory);
  const [searchQuery, setSearchQuery] = useState('');
  const [statuses, setStatuses] = useState<Record<string, IntegrationStatusState>>({});
  const [loading, setLoading] = useState(true);
  const [credentialIntegration, setCredentialIntegration] = useState<Integration | null>(null);
  const [sageCredentials, setSageCredentials] = useState<SageCredentials>(emptySageCredentials);
  const [billComCredentials, setBillComCredentials] = useState<BillComCredentials>(emptyBillComCredentials);
  const [teamsModalOpen, setTeamsModalOpen] = useState(false);
  const [teamsWebhookUrl, setTeamsWebhookUrl] = useState('');
  const [teamsChannelName, setTeamsChannelName] = useState('');
  const [modalError, setModalError] = useState<string | null>(null);
  const [submittingModal, setSubmittingModal] = useState(false);
  const [subscription, setSubscription] = useState<BillingSubscription | null>(null);
  const [subscriptionLoading, setSubscriptionLoading] = useState(true);
  const hasModule = useAuthStore((s) => s.hasModule);

  const isIntegrationLocked = (integration: Integration): boolean => {
    if (!integration.requiredModule) return false;
    const subIncludes = subscription?.add_on_modules?.includes(integration.requiredModule as BillingModule) ?? false;
    if (subIncludes) return false;
    return !hasModule(integration.requiredModule);
  };

  // Fetch billing subscription once on mount
  useEffect(() => {
    let cancelled = false;
    billingApi.getSubscription()
      .then((data) => {
        if (!cancelled) setSubscription(data.subscription);
      })
      .catch(() => {
        // Subscription fetch failed - treat as no add-ons
      })
      .finally(() => {
        if (!cancelled) setSubscriptionLoading(false);
      });
    return () => { cancelled = true; };
  }, []);

  const getStatusForIntegration = async (integration: Integration): Promise<IntegrationStatusState> => {
    // Short-circuit: locked integrations skip status API calls
    if (isIntegrationLocked(integration)) {
      return { status: 'locked' };
    }

    if (integration.authType === 'slack') {
      const status = await notificationsApi.getSlackStatus();
      return status
        ? {
            status: status.is_active ? 'connected' : 'disconnected',
            detail: status.slack_team_name ? `Workspace: ${status.slack_team_name}` : undefined,
          }
        : { status: 'disconnected' };
    }

    if (integration.authType === 'teams') {
      const status = await notificationsApi.getTeamsStatus();
      return status
        ? {
            status: status.is_active ? 'connected' : 'disconnected',
            detail: status.channel_name ? `Channel: ${status.channel_name}` : undefined,
          }
        : { status: 'disconnected' };
    }

    if (!integration.endpoints.status) {
      return { status: 'disconnected' };
    }

    const result = await getIntegrationStatus(integration.endpoints.status);
    if (!result.connected && result.reason === 'jwt_not_implemented') {
      return {
        status: 'unsupported',
        liveStatus: result,
      };
    }
    return {
      status: result.connected ? 'connected' : 'disconnected',
      liveStatus: result,
    };
  };

  useEffect(() => {
    async function fetchStatuses() {
      setStatuses(Object.fromEntries(
        integrations.map((integration) => [integration.id, { status: 'loading' as IntegrationStatus }]),
      ));
      const results = await Promise.allSettled(
        integrations.map(i =>
          getStatusForIntegration(i).then(r => [i.id, r] as const)
        )
      );
      const map: Record<string, IntegrationStatusState> = {};
      for (const r of results) {
        if (r.status === 'fulfilled') {
          const [id, result] = r.value;
          map[id] = result;
        }
      }
      for (const integration of integrations) {
        if (!map[integration.id]) {
          map[integration.id] = {
            status: 'error',
            error: 'Status request failed',
          };
        }
      }
      setStatuses(map);
      setLoading(false);
    }
    // Wait for subscription to load before fetching statuses so locked state is accurate
    if (subscriptionLoading) return;
    fetchStatuses();
  }, [subscriptionLoading]);

  const fetchSingleStatus = async (integrationId: string) => {
    const integration = integrations.find(i => i.id === integrationId);
    if (!integration) return;
    setStatuses(prev => ({
      ...prev,
      [integrationId]: {
        ...prev[integrationId],
        status: 'loading',
      },
    }));
    try {
      const result = await getStatusForIntegration(integration);
      setStatuses(prev => ({
        ...prev,
        [integrationId]: result,
      }));
    } catch (error) {
      setStatuses(prev => ({
        ...prev,
        [integrationId]: {
          ...prev[integrationId],
          status: 'error',
          error: error instanceof Error ? error.message : 'Status request failed',
        },
      }));
    }
  };

  const filteredIntegrations = integrations.filter((integration) => {
    const matchesCategory = filter === 'all' || integration.category === filter;
    const matchesSearch = searchQuery === '' ||
      integration.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      integration.description.toLowerCase().includes(searchQuery.toLowerCase());
    return matchesCategory && matchesSearch;
  });

  const connectedCount = integrations.filter((i) => statuses[i.id]?.status === 'connected').length;

  const resetCredentialModal = () => {
    setCredentialIntegration(null);
    setSageCredentials(emptySageCredentials);
    setBillComCredentials(emptyBillComCredentials);
    setModalError(null);
    setSubmittingModal(false);
  };

  const resetTeamsModal = () => {
    setTeamsModalOpen(false);
    setTeamsWebhookUrl('');
    setTeamsChannelName('');
    setModalError(null);
    setSubmittingModal(false);
  };

  const handleConnect = async (integrationId: string) => {
    const integration = integrations.find((i) => i.id === integrationId);
    if (!integration) return;

    // Block connect for locked integrations - redirect to billing upgrade
    if (isIntegrationLocked(integration)) {
      window.location.href = '/settings?tab=billing';
      return;
    }

    // NetSuite connect is intentionally disabled until JWT signing ships.
    if (statuses[integrationId]?.status === 'unsupported') {
      return;
    }

    if (integration.authType === 'oauth' && integration.endpoints.connect) {
      // Redirect to OAuth flow
      window.location.href = integration.endpoints.connect;
      return;
    }

    if (integration.authType === 'credentials') {
      setCredentialIntegration(integration);
      setModalError(null);
      return;
    }

    if (integration.authType === 'slack') {
      try {
        const redirectUrl = `${window.location.origin}/integrations?category=notifications`;
        const response = await notificationsApi.installSlack(redirectUrl);
        window.location.href = response.authorize_url;
      } catch (error) {
        setStatuses(prev => ({
          ...prev,
          [integration.id]: {
            ...prev[integration.id],
            status: 'error',
            error: error instanceof Error ? error.message : 'Failed to start Slack install',
          },
        }));
      }
      return;
    }

    if (integration.authType === 'teams') {
      setTeamsModalOpen(true);
      setModalError(null);
    }
  };

  const handleDisconnect = async (integrationId: string) => {
    const integration = integrations.find((i) => i.id === integrationId);
    if (!integration) return;

    if (confirm(`Disconnect ${integration.name}? Sync will stop and mappings will be preserved.`)) {
      try {
        if (integration.authType === 'slack') {
          await notificationsApi.disconnectSlack();
        } else if (integration.authType === 'teams') {
          await notificationsApi.disconnectTeams();
        } else if (integration.endpoints.disconnect) {
          await api.post(integration.endpoints.disconnect);
        }
        await fetchSingleStatus(integrationId);
      } catch {
        alert('Failed to disconnect. Please try again.');
      }
    }
  };

  const handleCredentialSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!credentialIntegration) return;

    setSubmittingModal(true);
    setModalError(null);
    try {
      if (credentialIntegration.id === 'sage-intacct') {
        const payload = {
          ...sageCredentials,
          entity_id: sageCredentials.entity_id.trim() || undefined,
        };
        await sageIntacctApi.connect(payload);
      } else if (credentialIntegration.id === 'bill-com') {
        await billComApi.connect(billComCredentials);
      }
      const integrationId = credentialIntegration.id;
      resetCredentialModal();
      await fetchSingleStatus(integrationId);
    } catch (error) {
      setModalError(error instanceof Error ? error.message : 'Failed to connect integration');
      setSubmittingModal(false);
    }
  };

  const handleTeamsSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setSubmittingModal(true);
    setModalError(null);
    try {
      await notificationsApi.configureTeams({
        webhook_url: teamsWebhookUrl,
        channel_name: teamsChannelName.trim() || undefined,
      });
      resetTeamsModal();
      await fetchSingleStatus('teams');
    } catch (error) {
      setModalError(error instanceof Error ? error.message : 'Failed to configure Microsoft Teams');
      setSubmittingModal(false);
    }
  };

  return (
    <div className="max-w-5xl mx-auto px-4 sm:px-6 py-8">
      {/* Header */}
      <div className="flex items-start justify-between mb-8">
        <div>
          <h1 className="text-2xl font-bold text-zinc-900 dark:text-zinc-100">
            Integrations
          </h1>
          <p className="mt-1 text-sm text-zinc-500 dark:text-zinc-400">
            Connect ERP, accounting, payment, and notification systems to automate the AP workflow.
          </p>
        </div>
        <div className="flex items-center gap-2 text-sm text-zinc-500 dark:text-zinc-400">
          <Cable className="h-4 w-4" />
          {connectedCount} of {integrations.length} connected
        </div>
      </div>

      {/* Filters */}
      <div className="flex items-center gap-3 mb-6">
        {(['all', 'erp', 'crm', 'payments', 'notifications'] as IntegrationCategory[]).map((cat) => (
          <button
            key={cat}
            onClick={() => setFilter(cat)}
            className={`inline-flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium rounded-lg transition-colors ${
              filter === cat
                ? 'bg-zinc-900 text-white dark:bg-zinc-100 dark:text-zinc-900'
                : 'text-zinc-500 hover:text-zinc-700 dark:text-zinc-400 dark:hover:text-zinc-200 bg-zinc-100 dark:bg-zinc-800'
            }`}
          >
            {cat === 'all' ? (
              <Globe className="h-3.5 w-3.5" />
            ) : (
              (() => {
                const Icon = categoryIcons[cat];
                return <Icon className="h-3.5 w-3.5" />;
              })()
            )}
            {cat === 'all' ? 'All' : categoryLabels[cat]}
          </button>
        ))}
        <div className="flex-1" />
        <input
          type="text"
          placeholder="Search integrations..."
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          className="w-64 px-3 py-1.5 text-sm border border-zinc-200 dark:border-zinc-700 rounded-lg bg-white dark:bg-zinc-900 text-zinc-900 dark:text-zinc-100 placeholder-zinc-400 focus:outline-none focus:ring-2 focus:ring-zinc-400 dark:focus:ring-zinc-600"
        />
      </div>

      {/* Integration Cards */}
      {loading ? (
        <div className="flex items-center justify-center py-16">
          <Loader2 className="h-8 w-8 text-zinc-400 animate-spin" />
        </div>
      ) : (
      <div className="grid gap-4">
        {filteredIntegrations.map((integration) => (
          <IntegrationCard
            key={integration.id}
            integration={integration}
            liveStatus={statuses[integration.id]}
            subscriptionLoading={subscriptionLoading}
            onConnect={handleConnect}
            onDisconnect={handleDisconnect}
            onRefresh={fetchSingleStatus}
            onUpgrade={handleConnect}
          />
        ))}
      </div>
      )}

      {filteredIntegrations.length === 0 && (
        <div className="text-center py-16">
          <Cable className="h-12 w-12 text-zinc-300 dark:text-zinc-600 mx-auto mb-3" />
          <p className="text-sm text-zinc-500 dark:text-zinc-400">
            No integrations match your search.
          </p>
        </div>
      )}

      {/* Coming Soon */}
      <div className="mt-8 p-6 bg-zinc-50 dark:bg-zinc-900/50 border border-zinc-200 dark:border-zinc-800 rounded-xl">
        <h3 className="text-sm font-semibold text-zinc-700 dark:text-zinc-300 mb-2">
          Coming soon
        </h3>
        <div className="flex flex-wrap gap-3">
          {['SAP', 'Microsoft Dynamics 365', 'FreshBooks'].map((name) => (
            <span
              key={name}
              className="inline-flex items-center px-3 py-1.5 text-sm text-zinc-400 dark:text-zinc-500 bg-white dark:bg-zinc-800 border border-zinc-200 dark:border-zinc-700 rounded-lg"
            >
              {name}
            </span>
          ))}
        </div>
      </div>

      {credentialIntegration && (
        <CredentialModal
          integration={credentialIntegration}
          sageCredentials={sageCredentials}
          billComCredentials={billComCredentials}
          error={modalError}
          isSubmitting={submittingModal}
          onClose={resetCredentialModal}
          onSubmit={handleCredentialSubmit}
          onSageChange={setSageCredentials}
          onBillComChange={setBillComCredentials}
        />
      )}

      {teamsModalOpen && (
        <TeamsModal
          webhookUrl={teamsWebhookUrl}
          channelName={teamsChannelName}
          error={modalError}
          isSubmitting={submittingModal}
          onClose={resetTeamsModal}
          onSubmit={handleTeamsSubmit}
          onWebhookUrlChange={setTeamsWebhookUrl}
          onChannelNameChange={setTeamsChannelName}
        />
      )}
    </div>
  );
}
