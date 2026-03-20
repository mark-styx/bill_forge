'use client';

import { useState } from 'react';
import {
  Cable,
  CheckCircle2,
  XCircle,
  ArrowRight,
  RefreshCw,
  ExternalLink,
  Shield,
  Zap,
  Building2,
  Users,
  FileText,
  Globe,
  Database,
  BarChart3,
} from 'lucide-react';

type IntegrationStatus = 'connected' | 'disconnected' | 'available';
type IntegrationCategory = 'erp' | 'crm' | 'payments' | 'all';

interface Integration {
  id: string;
  name: string;
  description: string;
  longDescription: string;
  category: 'erp' | 'crm' | 'payments';
  status: IntegrationStatus;
  logo: string;
  authType: 'oauth' | 'credentials';
  capabilities: string[];
  endpoints: {
    connect: string;
    status: string;
    disconnect: string;
  };
  docsUrl?: string;
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
  },
  {
    id: 'bill-com',
    name: 'Bill.com',
    description: 'AP payment execution — ACH, check, and virtual card payments from approved invoices.',
    longDescription: 'Connect Bill.com to execute payments directly from BillForge. After an invoice is approved and pushed to Bill.com as a bill, pay vendors via ACH, check, or virtual card — all without leaving BillForge. Supports bulk payments, funding account selection, and real-time payment status tracking. Uses session-based API authentication.',
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
  },
];

const categoryIcons = {
  erp: Database,
  crm: Users,
  payments: BarChart3,
};

const categoryLabels = {
  erp: 'ERP / Accounting',
  crm: 'CRM',
  payments: 'Payments',
};

function IntegrationLogo({ integration }: { integration: Integration }) {
  // Use styled fallback icons since we may not have actual logo SVGs
  const logoMap: Record<string, { bg: string; text: string; label: string }> = {
    'quickbooks': { bg: 'bg-emerald-100 dark:bg-emerald-900/40', text: 'text-emerald-700 dark:text-emerald-300', label: 'QB' },
    'xero': { bg: 'bg-sky-100 dark:bg-sky-900/40', text: 'text-sky-700 dark:text-sky-300', label: 'XR' },
    'sage-intacct': { bg: 'bg-green-100 dark:bg-green-900/40', text: 'text-green-700 dark:text-green-300', label: 'SI' },
    'salesforce': { bg: 'bg-blue-100 dark:bg-blue-900/40', text: 'text-blue-700 dark:text-blue-300', label: 'SF' },
    'workday': { bg: 'bg-orange-100 dark:bg-orange-900/40', text: 'text-orange-700 dark:text-orange-300', label: 'WD' },
    'bill-com': { bg: 'bg-violet-100 dark:bg-violet-900/40', text: 'text-violet-700 dark:text-violet-300', label: 'BC' },
  };

  const style = logoMap[integration.id] || { bg: 'bg-gray-100 dark:bg-gray-800', text: 'text-gray-700 dark:text-gray-300', label: '??' };

  return (
    <div className={`w-12 h-12 rounded-xl ${style.bg} flex items-center justify-center`}>
      <span className={`text-lg font-bold ${style.text}`}>{style.label}</span>
    </div>
  );
}

function StatusBadge({ status }: { status: IntegrationStatus }) {
  if (status === 'connected') {
    return (
      <span className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-emerald-100 text-emerald-700 dark:bg-emerald-900/40 dark:text-emerald-300">
        <CheckCircle2 className="h-3.5 w-3.5" />
        Connected
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

function IntegrationCard({ integration, onConnect, onDisconnect }: {
  integration: Integration;
  onConnect: (id: string) => void;
  onDisconnect: (id: string) => void;
}) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="bg-white dark:bg-zinc-900 border border-zinc-200 dark:border-zinc-800 rounded-xl overflow-hidden hover:border-zinc-300 dark:hover:border-zinc-700 transition-all">
      <div className="p-5">
        <div className="flex items-start gap-4">
          <IntegrationLogo integration={integration} />
          <div className="flex-1 min-w-0">
            <div className="flex items-center justify-between gap-3">
              <h3 className="text-base font-semibold text-zinc-900 dark:text-zinc-100">
                {integration.name}
              </h3>
              <StatusBadge status={integration.status} />
            </div>
            <p className="mt-1 text-sm text-zinc-500 dark:text-zinc-400">
              {integration.description}
            </p>
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
            {integration.authType === 'credentials' && (
              <div className="mt-3 flex items-center gap-2 text-xs text-amber-600 dark:text-amber-400 bg-amber-50 dark:bg-amber-900/20 px-3 py-2 rounded-lg">
                <Shield className="h-3.5 w-3.5" />
                This integration uses credential-based authentication. Your credentials are encrypted at rest.
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
          {integration.status === 'connected' ? (
            <button
              onClick={() => onDisconnect(integration.id)}
              className="inline-flex items-center gap-1.5 px-4 py-1.5 text-sm font-medium text-red-600 hover:text-red-700 dark:text-red-400 dark:hover:text-red-300 bg-red-50 dark:bg-red-900/20 hover:bg-red-100 dark:hover:bg-red-900/30 rounded-lg transition-colors"
            >
              Disconnect
            </button>
          ) : (
            <button
              onClick={() => onConnect(integration.id)}
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

export default function IntegrationsPage() {
  const [filter, setFilter] = useState<IntegrationCategory>('all');
  const [searchQuery, setSearchQuery] = useState('');

  const filteredIntegrations = integrations.filter((integration) => {
    const matchesCategory = filter === 'all' || integration.category === filter;
    const matchesSearch = searchQuery === '' ||
      integration.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      integration.description.toLowerCase().includes(searchQuery.toLowerCase());
    return matchesCategory && matchesSearch;
  });

  const connectedCount = integrations.filter((i) => i.status === 'connected').length;

  const handleConnect = (integrationId: string) => {
    const integration = integrations.find((i) => i.id === integrationId);
    if (!integration) return;

    if (integration.authType === 'oauth') {
      // Redirect to OAuth flow
      window.location.href = integration.endpoints.connect;
    } else {
      // Show credentials modal (for Sage Intacct, Bill.com)
      // In production this would open a modal form
      alert(`Configure ${integration.name} credentials in Settings → Integrations`);
    }
  };

  const handleDisconnect = async (integrationId: string) => {
    const integration = integrations.find((i) => i.id === integrationId);
    if (!integration) return;

    if (confirm(`Disconnect ${integration.name}? Sync will stop and mappings will be preserved.`)) {
      try {
        await fetch(integration.endpoints.disconnect, { method: 'POST' });
        // Refresh status
        window.location.reload();
      } catch {
        alert('Failed to disconnect. Please try again.');
      }
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
            Connect your ERP, accounting, and CRM systems to automate the AP workflow.
          </p>
        </div>
        <div className="flex items-center gap-2 text-sm text-zinc-500 dark:text-zinc-400">
          <Cable className="h-4 w-4" />
          {connectedCount} of {integrations.length} connected
        </div>
      </div>

      {/* Filters */}
      <div className="flex items-center gap-3 mb-6">
        {(['all', 'erp', 'crm', 'payments'] as IntegrationCategory[]).map((cat) => (
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
      <div className="grid gap-4">
        {filteredIntegrations.map((integration) => (
          <IntegrationCard
            key={integration.id}
            integration={integration}
            onConnect={handleConnect}
            onDisconnect={handleDisconnect}
          />
        ))}
      </div>

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
          {['NetSuite', 'SAP', 'Microsoft Dynamics 365', 'FreshBooks'].map((name) => (
            <span
              key={name}
              className="inline-flex items-center px-3 py-1.5 text-sm text-zinc-400 dark:text-zinc-500 bg-white dark:bg-zinc-800 border border-zinc-200 dark:border-zinc-700 rounded-lg"
            >
              {name}
            </span>
          ))}
        </div>
      </div>
    </div>
  );
}
