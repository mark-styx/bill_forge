'use client';

import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { budgetsApi } from '@/lib/api';
import type { Budget, CreateBudgetInput, PatchBudgetInput } from '@/lib/api';
import {
  Plus,
  Trash2,
  ArrowLeft,
  DollarSign,
  AlertTriangle,
  Shield,
  X,
  Building2,
  CreditCard,
  FolderOpen,
  BarChart3,
} from 'lucide-react';
import { toast } from 'sonner';
import Link from 'next/link';

const SCOPE_TYPES = [
  { value: 'department', label: 'Department', icon: Building2 },
  { value: 'cost_center', label: 'Cost Center', icon: CreditCard },
  { value: 'gl_account', label: 'GL Account', icon: BarChart3 },
  { value: 'project', label: 'Project', icon: FolderOpen },
] as const;

const PERIOD_TYPES = [
  { value: 'monthly', label: 'Monthly' },
  { value: 'quarterly', label: 'Quarterly' },
  { value: 'annual', label: 'Annual' },
] as const;

const ENFORCEMENT_OPTIONS = [
  { value: 'warn', label: 'Warn (soft)', color: 'text-warning' },
  { value: 'block', label: 'Block (hard)', color: 'text-destructive' },
] as const;

function formatCents(cents: number) {
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
  }).format(cents / 100);
}

function formatDate(dateStr: string) {
  return new Date(dateStr + 'T00:00:00').toLocaleDateString('en-US', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  });
}

export default function BudgetsPage() {
  const queryClient = useQueryClient();
  const [showForm, setShowForm] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);

  const [formData, setFormData] = useState({
    scope_type: 'department',
    scope_value: '',
    period_type: 'monthly',
    period_start: '',
    period_end: '',
    amount_dollars: '',
    enforcement: 'warn' as 'warn' | 'block',
  });

  const { data: budgets, isLoading } = useQuery({
    queryKey: ['budgets'],
    queryFn: () => budgetsApi.list(),
  });

  const createMutation = useMutation({
    mutationFn: (data: CreateBudgetInput) => budgetsApi.create(data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['budgets'] });
      toast.success('Budget created');
      resetForm();
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to create budget');
    },
  });

  const updateMutation = useMutation({
    mutationFn: ({ id, data }: { id: string; data: PatchBudgetInput }) =>
      budgetsApi.update(id, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['budgets'] });
      toast.success('Budget updated');
      resetForm();
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to update budget');
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => budgetsApi.delete(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['budgets'] });
      toast.success('Budget deleted');
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to delete budget');
    },
  });

  const resetForm = () => {
    setShowForm(false);
    setEditingId(null);
    setFormData({
      scope_type: 'department',
      scope_value: '',
      period_type: 'monthly',
      period_start: '',
      period_end: '',
      amount_dollars: '',
      enforcement: 'warn',
    });
  };

  const handleEdit = (budget: Budget) => {
    setEditingId(budget.id);
    setFormData({
      scope_type: budget.scope_type as any,
      scope_value: budget.scope_value,
      period_type: budget.period_type as any,
      period_start: budget.period_start,
      period_end: budget.period_end,
      amount_dollars: (budget.amount_cents / 100).toString(),
      enforcement: budget.enforcement,
    });
    setShowForm(true);
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const amountCents = Math.round(parseFloat(formData.amount_dollars) * 100);
    if (isNaN(amountCents) || amountCents <= 0) {
      toast.error('Please enter a valid amount');
      return;
    }
    if (!formData.period_start || !formData.period_end) {
      toast.error('Please select a period');
      return;
    }
    if (!formData.scope_value.trim()) {
      toast.error('Please enter a scope value');
      return;
    }

    if (editingId) {
      updateMutation.mutate({
        id: editingId,
        data: {
          amount_cents: amountCents,
          enforcement: formData.enforcement,
        },
      });
    } else {
      createMutation.mutate({
        scope_type: formData.scope_type,
        scope_value: formData.scope_value.trim(),
        period_type: formData.period_type,
        period_start: formData.period_start,
        period_end: formData.period_end,
        amount_cents: amountCents,
        enforcement: formData.enforcement,
      });
    }
  };

  const getScopeLabel = (type: string) =>
    SCOPE_TYPES.find((s) => s.value === type)?.label ?? type;

  const getScopeIcon = (type: string) =>
    SCOPE_TYPES.find((s) => s.value === type)?.icon ?? Building2;

  return (
    <div className="space-y-6 max-w-5xl mx-auto">
      {/* Header */}
      <div>
        <Link
          href="/reports"
          className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors mb-3"
        >
          <ArrowLeft className="w-4 h-4" />
          Back to Reports
        </Link>
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-semibold text-foreground">Budget Guardrails</h1>
            <p className="text-muted-foreground mt-0.5">
              Configure per-department, cost-center, GL-account, and project budgets with approval-time enforcement
            </p>
          </div>
          <button
            onClick={() => { resetForm(); setShowForm(true); }}
            className="btn btn-primary btn-sm"
          >
            <Plus className="w-4 h-4 mr-1.5" />
            New Budget
          </button>
        </div>
      </div>

      {/* Info Card */}
      <div className="card overflow-hidden">
        <div className="h-1 bg-gradient-to-r from-blue-500 to-blue-500/50" />
        <div className="p-6">
          <div className="flex items-center gap-3 mb-3">
            <div className="p-2 rounded-lg bg-blue-500/10">
              <Shield className="w-5 h-5 text-blue-500" />
            </div>
            <div>
              <h3 className="font-semibold text-foreground">How Budget Guardrails Work</h3>
              <p className="text-sm text-muted-foreground">
                When an invoice is approved, the system checks whether it would exceed the configured budget
                for its department, cost center, GL account, or project. Budgets can be set to warn (allow with
                a recorded warning) or block (prevent the approval entirely).
              </p>
            </div>
          </div>
        </div>
      </div>

      {/* Form */}
      {showForm && (
        <div className="card p-6 animate-scale-in">
          <div className="flex items-center justify-between mb-4">
            <h3 className="font-semibold text-foreground">
              {editingId ? 'Edit Budget' : 'Create Budget'}
            </h3>
            <button
              onClick={resetForm}
              className="p-1.5 text-muted-foreground hover:text-foreground rounded-lg hover:bg-secondary transition-colors"
            >
              <X className="w-4 h-4" />
            </button>
          </div>
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div>
                <label className="block text-sm font-medium text-foreground mb-1.5">
                  Scope Type
                </label>
                <select
                  value={formData.scope_type}
                  onChange={(e) => setFormData({ ...formData, scope_type: e.target.value as any })}
                  className="input"
                  disabled={!!editingId}
                >
                  {SCOPE_TYPES.map((st) => (
                    <option key={st.value} value={st.value}>{st.label}</option>
                  ))}
                </select>
              </div>
              <div>
                <label className="block text-sm font-medium text-foreground mb-1.5">
                  Scope Value
                </label>
                <input
                  type="text"
                  value={formData.scope_value}
                  onChange={(e) => setFormData({ ...formData, scope_value: e.target.value })}
                  className="input"
                  placeholder={
                    formData.scope_type === 'department' ? 'e.g. Engineering' :
                    formData.scope_type === 'cost_center' ? 'e.g. CC-100' :
                    formData.scope_type === 'gl_account' ? 'e.g. 6000' :
                    'e.g. PROJ-2026-01'
                  }
                  disabled={!!editingId}
                  required
                />
              </div>
            </div>
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              <div>
                <label className="block text-sm font-medium text-foreground mb-1.5">
                  Period Type
                </label>
                <select
                  value={formData.period_type}
                  onChange={(e) => setFormData({ ...formData, period_type: e.target.value as any })}
                  className="input"
                  disabled={!!editingId}
                >
                  {PERIOD_TYPES.map((pt) => (
                    <option key={pt.value} value={pt.value}>{pt.label}</option>
                  ))}
                </select>
              </div>
              <div>
                <label className="block text-sm font-medium text-foreground mb-1.5">
                  Period Start
                </label>
                <input
                  type="date"
                  value={formData.period_start}
                  onChange={(e) => setFormData({ ...formData, period_start: e.target.value })}
                  className="input"
                  disabled={!!editingId}
                  required
                />
              </div>
              <div>
                <label className="block text-sm font-medium text-foreground mb-1.5">
                  Period End
                </label>
                <input
                  type="date"
                  value={formData.period_end}
                  onChange={(e) => setFormData({ ...formData, period_end: e.target.value })}
                  className="input"
                  disabled={!!editingId}
                  required
                />
              </div>
            </div>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div>
                <label className="block text-sm font-medium text-foreground mb-1.5">
                  Budget Amount
                </label>
                <div className="relative">
                  <span className="absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground">$</span>
                  <input
                    type="number"
                    step="0.01"
                    min="0"
                    value={formData.amount_dollars}
                    onChange={(e) => setFormData({ ...formData, amount_dollars: e.target.value })}
                    className="input pl-7"
                    placeholder="10000.00"
                    required
                  />
                </div>
              </div>
              <div>
                <label className="block text-sm font-medium text-foreground mb-1.5">
                  Enforcement
                </label>
                <select
                  value={formData.enforcement}
                  onChange={(e) => setFormData({ ...formData, enforcement: e.target.value as any })}
                  className="input"
                >
                  {ENFORCEMENT_OPTIONS.map((opt) => (
                    <option key={opt.value} value={opt.value}>{opt.label}</option>
                  ))}
                </select>
              </div>
            </div>
            <div className="flex justify-end gap-3 pt-2">
              <button type="button" onClick={resetForm} className="btn btn-ghost btn-sm">
                Cancel
              </button>
              <button
                type="submit"
                className="btn btn-primary btn-sm"
                disabled={createMutation.isPending || updateMutation.isPending}
              >
                {editingId ? 'Update Budget' : 'Create Budget'}
              </button>
            </div>
          </form>
        </div>
      )}

      {/* Budgets List */}
      <div className="card">
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-border">
                <th className="text-left p-3 font-medium text-muted-foreground">Scope</th>
                <th className="text-left p-3 font-medium text-muted-foreground">Value</th>
                <th className="text-left p-3 font-medium text-muted-foreground">Period</th>
                <th className="text-right p-3 font-medium text-muted-foreground">Amount</th>
                <th className="text-center p-3 font-medium text-muted-foreground">Enforcement</th>
                <th className="text-right p-3 font-medium text-muted-foreground">Actions</th>
              </tr>
            </thead>
            <tbody>
              {isLoading && (
                <tr>
                  <td colSpan={6} className="p-6 text-center text-muted-foreground">
                    Loading budgets...
                  </td>
                </tr>
              )}
              {budgets && budgets.length === 0 && (
                <tr>
                  <td colSpan={6} className="p-6 text-center text-muted-foreground">
                    No budgets configured. Create your first budget to start enforcing spending limits.
                  </td>
                </tr>
              )}
              {budgets?.map((budget) => {
                const Icon = getScopeIcon(budget.scope_type);
                return (
                  <tr key={budget.id} className="border-b border-border last:border-0 hover:bg-secondary/30 transition-colors">
                    <td className="p-3">
                      <div className="flex items-center gap-2">
                        <Icon className="w-4 h-4 text-muted-foreground" />
                        <span>{getScopeLabel(budget.scope_type)}</span>
                      </div>
                    </td>
                    <td className="p-3 font-mono text-xs">{budget.scope_value}</td>
                    <td className="p-3">
                      <div className="text-xs">
                        <span className="capitalize">{budget.period_type}</span>
                        <br />
                        <span className="text-muted-foreground">
                          {formatDate(budget.period_start)} - {formatDate(budget.period_end)}
                        </span>
                      </div>
                    </td>
                    <td className="p-3 text-right font-medium">
                      {formatCents(budget.amount_cents)}
                    </td>
                    <td className="p-3 text-center">
                      <span className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium ${
                        budget.enforcement === 'block'
                          ? 'bg-destructive/10 text-destructive'
                          : 'bg-warning/10 text-warning'
                      }`}>
                        {budget.enforcement === 'block' ? (
                          <Shield className="w-3 h-3" />
                        ) : (
                          <AlertTriangle className="w-3 h-3" />
                        )}
                        {budget.enforcement === 'block' ? 'Block' : 'Warn'}
                      </span>
                    </td>
                    <td className="p-3 text-right">
                      <div className="flex items-center justify-end gap-1">
                        <button
                          onClick={() => handleEdit(budget)}
                          className="p-1.5 text-muted-foreground hover:text-foreground rounded-lg hover:bg-secondary transition-colors"
                          title="Edit budget"
                        >
                          <DollarSign className="w-4 h-4" />
                        </button>
                        <button
                          onClick={() => {
                            if (confirm('Delete this budget?')) {
                              deleteMutation.mutate(budget.id);
                            }
                          }}
                          className="p-1.5 text-muted-foreground hover:text-destructive rounded-lg hover:bg-destructive/10 transition-colors"
                          title="Delete budget"
                        >
                          <Trash2 className="w-4 h-4" />
                        </button>
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}
