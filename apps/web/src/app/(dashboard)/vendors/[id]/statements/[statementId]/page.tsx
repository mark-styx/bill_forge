'use client';

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useParams, useRouter } from 'next/navigation';
import Link from 'next/link';
import { vendorStatementsApi, StatementLineItem, LineMatchStatus } from '@/lib/api';
import { toast } from 'sonner';
import {
  ArrowLeft,
  RefreshCw,
  CheckCircle,
  AlertTriangle,
  AlertCircle,
  Eye,
  Link2,
  Unlink,
  Loader2,
  DollarSign,
} from 'lucide-react';

const matchStatusConfig: Record<string, { bg: string; text: string; icon: typeof CheckCircle; label: string }> = {
  matched: { bg: 'bg-success/10', text: 'text-success', icon: CheckCircle, label: 'Matched' },
  unmatched: { bg: 'bg-error/10', text: 'text-error', icon: AlertCircle, label: 'Unmatched' },
  discrepancy: { bg: 'bg-warning/10', text: 'text-warning', icon: AlertTriangle, label: 'Discrepancy' },
  ignored: { bg: 'bg-secondary', text: 'text-muted-foreground', icon: Eye, label: 'Ignored' },
};

const stmtStatusConfig: Record<string, { bg: string; text: string; label: string }> = {
  pending: { bg: 'bg-warning/10', text: 'text-warning', label: 'Pending' },
  in_review: { bg: 'bg-accent/10', text: 'text-accent', label: 'In Review' },
  reconciled: { bg: 'bg-success/10', text: 'text-success', label: 'Reconciled' },
  disputed: { bg: 'bg-error/10', text: 'text-error', label: 'Disputed' },
};

export default function StatementDetailPage() {
  const params = useParams();
  const router = useRouter();
  const queryClient = useQueryClient();
  const vendorId = params.id as string;
  const statementId = params.statementId as string;

  const { data, isLoading } = useQuery({
    queryKey: ['vendor-statement', vendorId, statementId],
    queryFn: () => vendorStatementsApi.get(vendorId, statementId),
  });

  const matchMutation = useMutation({
    mutationFn: () => vendorStatementsApi.runMatch(vendorId, statementId),
    onSuccess: (resp) => {
      toast.success(`Auto-matched ${resp.results.filter((r) => r.confidence !== 'no_match').length} lines`);
      queryClient.invalidateQueries({ queryKey: ['vendor-statement', vendorId, statementId] });
      queryClient.invalidateQueries({ queryKey: ['vendor-statements', vendorId] });
    },
    onError: (err: any) => toast.error(err.message || 'Auto-match failed'),
  });

  const ignoreMutation = useMutation({
    mutationFn: (lineId: string) =>
      vendorStatementsApi.updateLine(vendorId, statementId, lineId, {
        match_status: 'ignored',
        matched_invoice_id: null,
      }),
    onSuccess: () => {
      toast.success('Line ignored');
      queryClient.invalidateQueries({ queryKey: ['vendor-statement', vendorId, statementId] });
    },
    onError: (err: any) => toast.error(err.message || 'Failed to ignore line'),
  });

  const unmatchMutation = useMutation({
    mutationFn: (lineId: string) =>
      vendorStatementsApi.updateLine(vendorId, statementId, lineId, {
        match_status: 'unmatched',
        matched_invoice_id: null,
      }),
    onSuccess: () => {
      toast.success('Line unmatched');
      queryClient.invalidateQueries({ queryKey: ['vendor-statement', vendorId, statementId] });
    },
    onError: (err: any) => toast.error(err.message || 'Failed to unmatch line'),
  });

  const reconcileMutation = useMutation({
    mutationFn: () => vendorStatementsApi.reconcile(vendorId, statementId),
    onSuccess: () => {
      toast.success('Statement reconciled');
      queryClient.invalidateQueries({ queryKey: ['vendor-statement', vendorId, statementId] });
      queryClient.invalidateQueries({ queryKey: ['vendor-statements', vendorId] });
    },
    onError: (err: any) => toast.error(err.message || 'Reconciliation failed'),
  });

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-24">
        <Loader2 className="w-5 h-5 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (!data) {
    return (
      <div className="text-center py-24">
        <p className="text-muted-foreground">Statement not found</p>
        <Link href={`/vendors/${vendorId}/statements`} className="text-primary hover:underline text-sm mt-2 inline-block">
          Back to Statements
        </Link>
      </div>
    );
  }

  const { statement, lines, summary } = data;
  const stmtCfg = stmtStatusConfig[statement.status] || stmtStatusConfig.pending;
  const canReconcile =
    statement.status !== 'reconciled' &&
    summary.unmatched === 0;
  const isReconciled = statement.status === 'reconciled';

  return (
    <div className="space-y-6 max-w-6xl mx-auto">
      {/* Header */}
      <div>
        <Link
          href={`/vendors/${vendorId}/statements`}
          className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors mb-3"
        >
          <ArrowLeft className="w-4 h-4" />
          Back to Statements
        </Link>

        <div className="flex flex-col sm:flex-row sm:items-start sm:justify-between gap-4">
          <div>
            <div className="flex items-center gap-3">
              <h1 className="text-2xl font-semibold text-foreground">
                Statement {statement.statement_number || statement.id.slice(0, 8)}
              </h1>
              <span className={`px-2.5 py-0.5 rounded-full text-xs font-medium ${stmtCfg.bg} ${stmtCfg.text}`}>
                {stmtCfg.label}
              </span>
            </div>
            <p className="text-muted-foreground mt-0.5">
              {statement.period_start} to {statement.period_end} &middot; {statement.currency}
            </p>
          </div>

          <div className="flex items-center gap-2">
            <button
              onClick={() => matchMutation.mutate()}
              disabled={matchMutation.isPending || isReconciled}
              className="btn btn-secondary btn-sm"
            >
              {matchMutation.isPending ? (
                <Loader2 className="w-4 h-4 mr-1.5 animate-spin" />
              ) : (
                <RefreshCw className="w-4 h-4 mr-1.5" />
              )}
              Re-run Auto Match
            </button>
            <button
              onClick={() => reconcileMutation.mutate()}
              disabled={reconcileMutation.isPending || !canReconcile}
              className="btn btn-primary btn-sm"
            >
              {reconcileMutation.isPending ? (
                <Loader2 className="w-4 h-4 mr-1.5 animate-spin" />
              ) : (
                <CheckCircle className="w-4 h-4 mr-1.5" />
              )}
              Mark Reconciled
            </button>
          </div>
        </div>
      </div>

      {/* Balance Info */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
        <div className="card p-4">
          <p className="text-xs text-muted-foreground mb-1">Opening Balance</p>
          <p className="text-xl font-semibold text-foreground">
            ${(statement.opening_balance_cents / 100).toLocaleString()}
          </p>
        </div>
        <div className="card p-4">
          <p className="text-xs text-muted-foreground mb-1">Closing Balance</p>
          <p className="text-xl font-semibold text-foreground">
            ${(statement.closing_balance_cents / 100).toLocaleString()}
          </p>
        </div>
        <div className="card p-4">
          <p className="text-xs text-muted-foreground mb-1">Total Variance</p>
          <p className={`text-xl font-semibold ${summary.total_variance_cents > 0 ? 'text-warning' : 'text-success'}`}>
            ${(summary.total_variance_cents / 100).toLocaleString()}
          </p>
        </div>
        <div className="card p-4">
          <p className="text-xs text-muted-foreground mb-1">Reconciled By</p>
          <p className="text-sm font-medium text-foreground">
            {statement.reconciled_by ? new Date(statement.reconciled_at!).toLocaleDateString() : '—'}
          </p>
        </div>
      </div>

      {/* Summary Bar */}
      <div className="grid grid-cols-2 sm:grid-cols-4 gap-4">
        <div className="card p-4 border-l-4 border-l-success">
          <p className="text-2xl font-bold text-success">{summary.matched}</p>
          <p className="text-xs text-muted-foreground">Matched</p>
        </div>
        <div className="card p-4 border-l-4 border-l-error">
          <p className="text-2xl font-bold text-error">{summary.unmatched}</p>
          <p className="text-xs text-muted-foreground">Unmatched</p>
        </div>
        <div className="card p-4 border-l-4 border-l-warning">
          <p className="text-2xl font-bold text-warning">{summary.discrepancies}</p>
          <p className="text-xs text-muted-foreground">Discrepancies</p>
        </div>
        <div className="card p-4 border-l-4 border-l-muted-foreground">
          <p className="text-2xl font-bold text-muted-foreground">{summary.ignored}</p>
          <p className="text-xs text-muted-foreground">Ignored</p>
        </div>
      </div>

      {/* Line Items Table */}
      <div className="card overflow-hidden">
        <div className="p-4 border-b border-border">
          <h2 className="text-lg font-semibold text-foreground">
            Line Items ({summary.total_lines})
          </h2>
        </div>

        {lines.length > 0 ? (
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead>
                <tr className="border-b border-border">
                  <th className="text-left px-4 py-3 text-xs font-medium text-muted-foreground uppercase">Date</th>
                  <th className="text-left px-4 py-3 text-xs font-medium text-muted-foreground uppercase">Description</th>
                  <th className="text-left px-4 py-3 text-xs font-medium text-muted-foreground uppercase">Reference</th>
                  <th className="text-right px-4 py-3 text-xs font-medium text-muted-foreground uppercase">Amount</th>
                  <th className="text-center px-4 py-3 text-xs font-medium text-muted-foreground uppercase">Status</th>
                  <th className="text-left px-4 py-3 text-xs font-medium text-muted-foreground uppercase">Matched Invoice</th>
                  <th className="text-right px-4 py-3 text-xs font-medium text-muted-foreground uppercase">Variance</th>
                  <th className="text-center px-4 py-3 text-xs font-medium text-muted-foreground uppercase">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border">
                {lines.map((line) => {
                  const cfg = matchStatusConfig[line.match_status] || matchStatusConfig.unmatched;
                  const Icon = cfg.icon;
                  const rowBg =
                    line.match_status === 'matched'
                      ? 'bg-success/[0.02]'
                      : line.match_status === 'discrepancy'
                      ? 'bg-warning/[0.02]'
                      : line.match_status === 'unmatched'
                      ? 'bg-error/[0.02]'
                      : '';

                  return (
                    <tr key={line.id} className={`${rowBg} hover:bg-secondary/30 transition-colors`}>
                      <td className="px-4 py-3 text-sm text-foreground">{line.line_date}</td>
                      <td className="px-4 py-3 text-sm text-foreground">{line.description}</td>
                      <td className="px-4 py-3 text-sm text-muted-foreground font-mono">
                        {line.reference_number || '—'}
                      </td>
                      <td className="px-4 py-3 text-sm text-right font-medium text-foreground">
                        ${(line.amount_cents / 100).toLocaleString()}
                      </td>
                      <td className="px-4 py-3 text-center">
                        <span className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium ${cfg.bg} ${cfg.text}`}>
                          <Icon className="w-3 h-3" />
                          {cfg.label}
                        </span>
                      </td>
                      <td className="px-4 py-3 text-sm">
                        {line.matched_invoice_id ? (
                          <Link
                            href={`/invoices/${line.matched_invoice_id}`}
                            className="text-primary hover:underline inline-flex items-center gap-1"
                          >
                            <Link2 className="w-3 h-3" />
                            {line.matched_invoice_id.slice(0, 8)}...
                          </Link>
                        ) : (
                          <span className="text-muted-foreground">—</span>
                        )}
                      </td>
                      <td className="px-4 py-3 text-sm text-right">
                        {line.variance_cents !== 0 ? (
                          <span className="text-warning font-medium">
                            ${(Math.abs(line.variance_cents) / 100).toLocaleString()}
                            {line.variance_cents > 0 ? ' over' : ' under'}
                          </span>
                        ) : (
                          <span className="text-muted-foreground">—</span>
                        )}
                      </td>
                      <td className="px-4 py-3 text-center">
                        <div className="flex items-center justify-center gap-1">
                          {line.match_status === 'unmatched' && !isReconciled && (
                            <button
                              onClick={() => ignoreMutation.mutate(line.id)}
                              disabled={ignoreMutation.isPending}
                              className="btn btn-ghost btn-sm text-xs"
                              title="Ignore this line"
                            >
                              <Eye className="w-3.5 h-3.5" />
                            </button>
                          )}
                          {(line.match_status === 'matched' || line.match_status === 'discrepancy') && !isReconciled && (
                            <button
                              onClick={() => unmatchMutation.mutate(line.id)}
                              disabled={unmatchMutation.isPending}
                              className="btn btn-ghost btn-sm text-xs"
                              title="Unmatch this line"
                            >
                              <Unlink className="w-3.5 h-3.5" />
                            </button>
                          )}
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        ) : (
          <div className="p-8 text-center">
            <p className="text-sm text-muted-foreground">No line items</p>
          </div>
        )}
      </div>

      {/* Notes */}
      {statement.notes && (
        <div className="card p-4">
          <h3 className="text-sm font-medium text-muted-foreground mb-2">Notes</h3>
          <p className="text-sm text-foreground">{statement.notes}</p>
        </div>
      )}
    </div>
  );
}
