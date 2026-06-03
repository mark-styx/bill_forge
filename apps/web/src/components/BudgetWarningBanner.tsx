'use client';

import { useQuery } from '@tanstack/react-query';
import { budgetsApi } from '@/lib/api';
import type { BudgetCheckResult } from '@/lib/api';
import { AlertTriangle, ShieldAlert, CheckCircle } from 'lucide-react';

interface BudgetWarningBannerProps {
  invoiceId: string;
  /** Show only when invoice is in a status that can be approved */
  visible: boolean;
}

export function BudgetWarningBanner({ invoiceId, visible }: BudgetWarningBannerProps) {
  const { data: check, isLoading } = useQuery({
    queryKey: ['budget-check', invoiceId],
    queryFn: () => budgetsApi.checkInvoice(invoiceId),
    enabled: visible,
    retry: false,
  });

  if (!visible || isLoading || !check) return null;

  if (check.blocked) {
    return (
      <div className="rounded-lg border border-destructive/50 bg-destructive/10 p-4 mb-4">
        <div className="flex items-start gap-3">
          <ShieldAlert className="w-5 h-5 text-destructive mt-0.5 shrink-0" />
          <div>
            <h4 className="font-semibold text-destructive text-sm">Budget Exceeded - Approval Blocked</h4>
            <p className="text-sm text-muted-foreground mt-1">
              This invoice would exceed one or more configured budgets. The approval has been blocked.
            </p>
            <ul className="mt-2 space-y-1">
              {check.violations.map((v: BudgetCheckResult, i: number) => (
                <li key={i} className="text-xs text-destructive">
                  <span className="font-medium capitalize">{v.scope_type.replace('_', ' ')}:</span>{' '}
                  {v.scope_value} - over by ${Math.abs(v.remaining_after_cents / 100).toLocaleString('en-US', { minimumFractionDigits: 2 })}
                </li>
              ))}
            </ul>
          </div>
        </div>
      </div>
    );
  }

  if (check.warnings.length > 0) {
    return (
      <div className="rounded-lg border border-warning/50 bg-warning/10 p-4 mb-4">
        <div className="flex items-start gap-3">
          <AlertTriangle className="w-5 h-5 text-warning mt-0.5 shrink-0" />
          <div>
            <h4 className="font-semibold text-warning text-sm">Budget Warning</h4>
            <p className="text-sm text-muted-foreground mt-1">
              This invoice is near or over budget thresholds. Approval will proceed with a warning.
            </p>
            <ul className="mt-2 space-y-1">
              {check.warnings.map((w: BudgetCheckResult, i: number) => (
                <li key={i} className="text-xs text-warning">
                  <span className="font-medium capitalize">{w.scope_type.replace('_', ' ')}:</span>{' '}
                  {w.scope_value} - remaining: ${Math.max(0, w.remaining_after_cents / 100).toLocaleString('en-US', { minimumFractionDigits: 2 })}
                </li>
              ))}
            </ul>
          </div>
        </div>
      </div>
    );
  }

  if (check.results.length > 0) {
    return (
      <div className="rounded-lg border border-border bg-secondary/30 p-3 mb-4">
        <div className="flex items-center gap-2">
          <CheckCircle className="w-4 h-4 text-success" />
          <span className="text-xs text-muted-foreground">
            Budget check passed for {check.results.length} dimension{check.results.length > 1 ? 's' : ''}
          </span>
        </div>
      </div>
    );
  }

  return null;
}
