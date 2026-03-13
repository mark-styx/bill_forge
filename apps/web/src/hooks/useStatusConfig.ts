'use client';

import { useQuery } from '@tanstack/react-query';
import { invoiceStatusApi, InvoiceStatusConfig } from '@/lib/api';

// Hardcoded fallback styles matching the current codebase defaults
const fallbackStyles: Record<string, { bg: string; text: string; label: string }> = {
  // Processing statuses
  draft: { bg: 'bg-secondary', text: 'text-muted-foreground', label: 'Draft' },
  submitted: { bg: 'bg-primary/10', text: 'text-primary', label: 'Submitted' },
  pending_approval: { bg: 'bg-warning/10', text: 'text-warning', label: 'Pending Approval' },
  approved: { bg: 'bg-success/10', text: 'text-success', label: 'Approved' },
  rejected: { bg: 'bg-error/10', text: 'text-error', label: 'Rejected' },
  on_hold: { bg: 'bg-warning/10', text: 'text-warning', label: 'On Hold' },
  ready_for_payment: { bg: 'bg-success/10', text: 'text-success', label: 'Ready for Payment' },
  paid: { bg: 'bg-success/10', text: 'text-success', label: 'Paid' },
  voided: { bg: 'bg-secondary', text: 'text-muted-foreground', label: 'Voided' },
  // Capture statuses
  pending: { bg: 'bg-warning/10', text: 'text-warning', label: 'Pending' },
  processing: { bg: 'bg-primary/10', text: 'text-primary', label: 'Processing' },
  ready_for_review: { bg: 'bg-warning/10', text: 'text-warning', label: 'Ready for Review' },
  reviewed: { bg: 'bg-success/10', text: 'text-success', label: 'Reviewed' },
  failed: { bg: 'bg-error/10', text: 'text-error', label: 'Failed' },
};

export interface StatusDisplay {
  key: string;
  label: string;
  bg: string;
  text: string;
  isTerminal: boolean;
  isActive: boolean;
}

export function useStatusConfig() {
  const { data: configs } = useQuery({
    queryKey: ['invoice-status-config'],
    queryFn: () => invoiceStatusApi.list(),
    staleTime: 5 * 60 * 1000, // Cache for 5 minutes
  });

  const configMap = new Map<string, InvoiceStatusConfig>();
  if (configs) {
    for (const c of configs) {
      configMap.set(c.status_key, c);
    }
  }

  const getStatusDisplay = (statusKey: string): StatusDisplay => {
    const config = configMap.get(statusKey);
    if (config) {
      return {
        key: config.status_key,
        label: config.display_label,
        bg: config.bg_color,
        text: config.text_color,
        isTerminal: config.is_terminal,
        isActive: config.is_active,
      };
    }

    const fallback = fallbackStyles[statusKey];
    if (fallback) {
      return {
        key: statusKey,
        label: fallback.label,
        bg: fallback.bg,
        text: fallback.text,
        isTerminal: ['paid', 'voided', 'rejected', 'reviewed', 'failed'].includes(statusKey),
        isActive: true,
      };
    }

    // Unknown status - generic display
    return {
      key: statusKey,
      label: statusKey.replace(/_/g, ' ').replace(/\b\w/g, l => l.toUpperCase()),
      bg: 'bg-secondary',
      text: 'text-muted-foreground',
      isTerminal: false,
      isActive: true,
    };
  };

  const getProcessingStatuses = (): StatusDisplay[] => {
    if (configs) {
      return configs
        .filter(c => c.category === 'processing' && c.is_active)
        .sort((a, b) => a.sort_order - b.sort_order)
        .map(c => getStatusDisplay(c.status_key));
    }
    // Fallback to hardcoded list
    return ['draft', 'submitted', 'pending_approval', 'approved', 'rejected', 'on_hold', 'ready_for_payment', 'paid', 'voided']
      .map(getStatusDisplay);
  };

  const getCaptureStatuses = (): StatusDisplay[] => {
    if (configs) {
      return configs
        .filter(c => c.category === 'capture' && c.is_active)
        .sort((a, b) => a.sort_order - b.sort_order)
        .map(c => getStatusDisplay(c.status_key));
    }
    return ['pending', 'processing', 'ready_for_review', 'reviewed', 'failed']
      .map(getStatusDisplay);
  };

  return {
    getStatusDisplay,
    getProcessingStatuses,
    getCaptureStatuses,
    isLoaded: !!configs,
  };
}
