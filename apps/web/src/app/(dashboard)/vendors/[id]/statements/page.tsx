'use client';

import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useParams, useRouter } from 'next/navigation';
import Link from 'next/link';
import { vendorStatementsApi, CreateStatementLineInput } from '@/lib/api';
import { toast } from 'sonner';
import {
  ArrowLeft,
  FileText,
  Plus,
  RefreshCw,
  CheckCircle,
  AlertTriangle,
  Clock,
  Loader2,
  X,
} from 'lucide-react';

const statusConfig: Record<string, { bg: string; text: string; label: string }> = {
  pending: { bg: 'bg-warning/10', text: 'text-warning', label: 'Pending' },
  in_review: { bg: 'bg-accent/10', text: 'text-accent', label: 'In Review' },
  reconciled: { bg: 'bg-success/10', text: 'text-success', label: 'Reconciled' },
  disputed: { bg: 'bg-error/10', text: 'text-error', label: 'Disputed' },
};

export default function VendorStatementsPage() {
  const params = useParams();
  const router = useRouter();
  const queryClient = useQueryClient();
  const vendorId = params.id as string;

  const [showCreate, setShowCreate] = useState(false);
  const [statusFilter, setStatusFilter] = useState<string>('');
  const [form, setForm] = useState({
    statement_number: '',
    statement_date: '',
    period_start: '',
    period_end: '',
    opening_balance_cents: 0,
    closing_balance_cents: 0,
    notes: '',
  });
  const [lines, setLines] = useState<CreateStatementLineInput[]>([]);

  const { data, isLoading } = useQuery({
    queryKey: ['vendor-statements', vendorId, statusFilter],
    queryFn: () => vendorStatementsApi.list(vendorId, { status: statusFilter || undefined }),
  });

  const createMutation = useMutation({
    mutationFn: () =>
      vendorStatementsApi.create(vendorId, {
        statement_number: form.statement_number || undefined,
        statement_date: form.statement_date || undefined,
        period_start: form.period_start,
        period_end: form.period_end,
        opening_balance_cents: form.opening_balance_cents,
        closing_balance_cents: form.closing_balance_cents,
        notes: form.notes || undefined,
        lines,
      }),
    onSuccess: (resp) => {
      toast.success('Statement created');
      setShowCreate(false);
      setLines([]);
      setForm({ statement_number: '', statement_date: '', period_start: '', period_end: '', opening_balance_cents: 0, closing_balance_cents: 0, notes: '' });
      queryClient.invalidateQueries({ queryKey: ['vendor-statements', vendorId] });
      if (resp.statement?.id) {
        router.push(`/vendors/${vendorId}/statements/${resp.statement.id}`);
      }
    },
    onError: (err: any) => toast.error(err.message || 'Failed to create statement'),
  });

  const addLine = () => {
    setLines([...lines, { line_date: '', description: '', reference_number: '', amount_cents: 0, line_type: 'invoice' }]);
  };

  const updateLine = (idx: number, field: string, value: string | number) => {
    const updated = [...lines];
    (updated[idx] as any)[field] = value;
    setLines(updated);
  };

  const removeLine = (idx: number) => {
    setLines(lines.filter((_, i) => i !== idx));
  };

  const statements = data?.data || [];

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-24">
        <Loader2 className="w-5 h-5 animate-spin text-muted-foreground" />
      </div>
    );
  }

  return (
    <div className="space-y-6 max-w-6xl mx-auto">
      {/* Header */}
      <div>
        <Link
          href={`/vendors/${vendorId}`}
          className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors mb-3"
        >
          <ArrowLeft className="w-4 h-4" />
          Back to Vendor
        </Link>
        <div className="flex items-center justify-between">
          <h1 className="text-2xl font-semibold text-foreground">Vendor Statements</h1>
          <button onClick={() => setShowCreate(true)} className="btn btn-primary btn-sm">
            <Plus className="w-4 h-4 mr-1.5" />
            New Statement
          </button>
        </div>
      </div>

      {/* Status Filter */}
      <div className="flex gap-2">
        {['', 'pending', 'in_review', 'reconciled', 'disputed'].map((s) => (
          <button
            key={s}
            onClick={() => setStatusFilter(s)}
            className={`px-3 py-1.5 rounded-lg text-sm font-medium transition-colors ${
              statusFilter === s ? 'bg-primary text-primary-foreground' : 'bg-secondary text-muted-foreground hover:bg-secondary/80'
            }`}
          >
            {s === '' ? 'All' : s.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase())}
          </button>
        ))}
      </div>

      {/* Statements Table */}
      {statements.length > 0 ? (
        <div className="card overflow-hidden">
          <table className="w-full">
            <thead>
              <tr className="border-b border-border">
                <th className="text-left px-4 py-3 text-xs font-medium text-muted-foreground uppercase">Statement #</th>
                <th className="text-left px-4 py-3 text-xs font-medium text-muted-foreground uppercase">Period</th>
                <th className="text-left px-4 py-3 text-xs font-medium text-muted-foreground uppercase">Status</th>
                <th className="text-right px-4 py-3 text-xs font-medium text-muted-foreground uppercase">Opening</th>
                <th className="text-right px-4 py-3 text-xs font-medium text-muted-foreground uppercase">Closing</th>
                <th className="text-right px-4 py-3 text-xs font-medium text-muted-foreground uppercase">Created</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {statements.map((stmt) => {
                const cfg = statusConfig[stmt.status] || statusConfig.pending;
                return (
                  <tr
                    key={stmt.id}
                    onClick={() => router.push(`/vendors/${vendorId}/statements/${stmt.id}`)}
                    className="hover:bg-secondary/50 cursor-pointer transition-colors"
                  >
                    <td className="px-4 py-3 text-sm font-medium text-foreground">
                      {stmt.statement_number || '—'}
                    </td>
                    <td className="px-4 py-3 text-sm text-muted-foreground">
                      {stmt.period_start} to {stmt.period_end}
                    </td>
                    <td className="px-4 py-3">
                      <span className={`px-2.5 py-0.5 rounded-full text-xs font-medium ${cfg.bg} ${cfg.text}`}>
                        {cfg.label}
                      </span>
                    </td>
                    <td className="px-4 py-3 text-sm text-right text-muted-foreground">
                      ${(stmt.opening_balance_cents / 100).toLocaleString()}
                    </td>
                    <td className="px-4 py-3 text-sm text-right font-medium text-foreground">
                      ${(stmt.closing_balance_cents / 100).toLocaleString()}
                    </td>
                    <td className="px-4 py-3 text-sm text-right text-muted-foreground">
                      {new Date(stmt.created_at).toLocaleDateString()}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      ) : (
        <div className="card p-12 text-center">
          <div className="w-12 h-12 rounded-xl bg-secondary flex items-center justify-center mx-auto mb-3">
            <FileText className="w-6 h-6 text-muted-foreground" />
          </div>
          <p className="text-sm text-muted-foreground">No statements found</p>
        </div>
      )}

      {/* Create Statement Modal */}
      {showCreate && (
        <>
          <div className="fixed inset-0 bg-black/50 backdrop-blur-sm z-50" onClick={() => setShowCreate(false)} />
          <div className="fixed inset-0 flex items-start justify-center z-50 p-4 pt-16 overflow-y-auto">
            <div className="bg-card border border-border rounded-xl shadow-xl max-w-2xl w-full p-6 animate-scale-in">
              <div className="flex items-center justify-between mb-6">
                <h2 className="text-lg font-semibold text-foreground">New Vendor Statement</h2>
                <button onClick={() => setShowCreate(false)} className="btn btn-ghost btn-sm">
                  <X className="w-4 h-4" />
                </button>
              </div>

              <div className="grid grid-cols-2 gap-4 mb-4">
                <div>
                  <label className="text-xs text-muted-foreground mb-1 block">Statement Number</label>
                  <input
                    type="text"
                    value={form.statement_number}
                    onChange={(e) => setForm({ ...form, statement_number: e.target.value })}
                    className="input"
                    placeholder="Optional"
                  />
                </div>
                <div>
                  <label className="text-xs text-muted-foreground mb-1 block">Statement Date</label>
                  <input
                    type="date"
                    value={form.statement_date}
                    onChange={(e) => setForm({ ...form, statement_date: e.target.value })}
                    className="input"
                  />
                </div>
                <div>
                  <label className="text-xs text-muted-foreground mb-1 block">Period Start *</label>
                  <input
                    type="date"
                    value={form.period_start}
                    onChange={(e) => setForm({ ...form, period_start: e.target.value })}
                    className="input"
                  />
                </div>
                <div>
                  <label className="text-xs text-muted-foreground mb-1 block">Period End *</label>
                  <input
                    type="date"
                    value={form.period_end}
                    onChange={(e) => setForm({ ...form, period_end: e.target.value })}
                    className="input"
                  />
                </div>
                <div>
                  <label className="text-xs text-muted-foreground mb-1 block">Opening Balance ($)</label>
                  <input
                    type="number"
                    step="0.01"
                    value={form.opening_balance_cents / 100}
                    onChange={(e) => setForm({ ...form, opening_balance_cents: Math.round(parseFloat(e.target.value || '0') * 100) })}
                    className="input"
                  />
                </div>
                <div>
                  <label className="text-xs text-muted-foreground mb-1 block">Closing Balance ($)</label>
                  <input
                    type="number"
                    step="0.01"
                    value={form.closing_balance_cents / 100}
                    onChange={(e) => setForm({ ...form, closing_balance_cents: Math.round(parseFloat(e.target.value || '0') * 100) })}
                    className="input"
                  />
                </div>
              </div>

              <div className="mb-4">
                <label className="text-xs text-muted-foreground mb-1 block">Notes</label>
                <textarea
                  value={form.notes}
                  onChange={(e) => setForm({ ...form, notes: e.target.value })}
                  className="input min-h-[60px] resize-none"
                  placeholder="Optional notes"
                />
              </div>

              {/* Line Items */}
              <div className="mb-4">
                <div className="flex items-center justify-between mb-2">
                  <label className="text-xs font-medium text-muted-foreground uppercase">Line Items</label>
                  <button onClick={addLine} className="text-sm text-primary hover:underline">
                    + Add Line
                  </button>
                </div>
                {lines.length > 0 && (
                  <div className="space-y-2">
                    {lines.map((line, idx) => (
                      <div key={idx} className="grid grid-cols-12 gap-2 items-end">
                        <div className="col-span-2">
                          <input
                            type="date"
                            value={line.line_date}
                            onChange={(e) => updateLine(idx, 'line_date', e.target.value)}
                            className="input text-xs"
                            placeholder="Date"
                          />
                        </div>
                        <div className="col-span-3">
                          <input
                            type="text"
                            value={line.description}
                            onChange={(e) => updateLine(idx, 'description', e.target.value)}
                            className="input text-xs"
                            placeholder="Description"
                          />
                        </div>
                        <div className="col-span-2">
                          <input
                            type="text"
                            value={line.reference_number || ''}
                            onChange={(e) => updateLine(idx, 'reference_number', e.target.value)}
                            className="input text-xs"
                            placeholder="Ref #"
                          />
                        </div>
                        <div className="col-span-2">
                          <input
                            type="number"
                            step="0.01"
                            value={line.amount_cents / 100}
                            onChange={(e) => updateLine(idx, 'amount_cents', Math.round(parseFloat(e.target.value || '0') * 100))}
                            className="input text-xs"
                            placeholder="Amount"
                          />
                        </div>
                        <div className="col-span-2">
                          <select
                            value={line.line_type || 'invoice'}
                            onChange={(e) => updateLine(idx, 'line_type', e.target.value)}
                            className="input text-xs"
                          >
                            <option value="invoice">Invoice</option>
                            <option value="credit">Credit</option>
                            <option value="payment">Payment</option>
                            <option value="adjustment">Adjustment</option>
                          </select>
                        </div>
                        <div className="col-span-1">
                          <button onClick={() => removeLine(idx)} className="btn btn-ghost btn-sm text-error">
                            <X className="w-3 h-3" />
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>

              <div className="flex gap-3 justify-end">
                <button onClick={() => setShowCreate(false)} className="btn btn-secondary">
                  Cancel
                </button>
                <button
                  onClick={() => createMutation.mutate()}
                  disabled={createMutation.isPending || !form.period_start || !form.period_end || lines.length === 0}
                  className="btn btn-primary"
                >
                  {createMutation.isPending ? <Loader2 className="w-4 h-4 mr-2 animate-spin" /> : null}
                  Create Statement
                </button>
              </div>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
