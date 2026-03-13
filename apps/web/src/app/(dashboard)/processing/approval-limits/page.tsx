'use client';

import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { workflowsApi } from '@/lib/api';
import type { CreateApprovalLimitInput } from '@/lib/api';
import {
  Plus,
  Trash2,
  ArrowLeft,
  DollarSign,
  AlertCircle,
  Shield,
  X,
  Building,
  Briefcase,
} from 'lucide-react';
import { toast } from 'sonner';
import Link from 'next/link';

export default function ApprovalLimitsPage() {
  const queryClient = useQueryClient();
  const [showForm, setShowForm] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);

  const [formData, setFormData] = useState({
    user_id: '',
    max_amount_dollars: '',
    currency: 'USD',
    vendor_restrictions: '',
    department_restrictions: '',
  });

  const { data: limits, isLoading } = useQuery({
    queryKey: ['approval-limits'],
    queryFn: () => workflowsApi.listApprovalLimits(),
  });

  const createMutation = useMutation({
    mutationFn: (data: CreateApprovalLimitInput) =>
      workflowsApi.createApprovalLimit(data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['approval-limits'] });
      toast.success('Approval limit created');
      resetForm();
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to create approval limit');
    },
  });

  const updateMutation = useMutation({
    mutationFn: ({ id, data }: { id: string; data: CreateApprovalLimitInput }) =>
      workflowsApi.updateApprovalLimit(id, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['approval-limits'] });
      toast.success('Approval limit updated');
      resetForm();
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to update approval limit');
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => workflowsApi.deleteApprovalLimit(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['approval-limits'] });
      toast.success('Approval limit deleted');
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to delete approval limit');
    },
  });

  const resetForm = () => {
    setShowForm(false);
    setEditingId(null);
    setFormData({
      user_id: '',
      max_amount_dollars: '',
      currency: 'USD',
      vendor_restrictions: '',
      department_restrictions: '',
    });
  };

  const handleEdit = (limit: any) => {
    setEditingId(limit.id);
    setFormData({
      user_id: limit.user_id,
      max_amount_dollars: (limit.max_amount.amount / 100).toString(),
      currency: limit.max_amount.currency || 'USD',
      vendor_restrictions: limit.vendor_restrictions?.join(', ') || '',
      department_restrictions: limit.department_restrictions?.join(', ') || '',
    });
    setShowForm(true);
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const amountCents = Math.round(parseFloat(formData.max_amount_dollars) * 100);
    if (isNaN(amountCents) || amountCents <= 0) {
      toast.error('Please enter a valid amount');
      return;
    }

    const payload: CreateApprovalLimitInput = {
      user_id: formData.user_id,
      max_amount: { amount: amountCents, currency: formData.currency },
      vendor_restrictions: formData.vendor_restrictions
        ? formData.vendor_restrictions.split(',').map((s) => s.trim()).filter(Boolean)
        : undefined,
      department_restrictions: formData.department_restrictions
        ? formData.department_restrictions.split(',').map((s) => s.trim()).filter(Boolean)
        : undefined,
    };

    if (editingId) {
      updateMutation.mutate({ id: editingId, data: payload });
    } else {
      createMutation.mutate(payload);
    }
  };

  const formatAmount = (amount: { amount: number; currency: string }) => {
    return new Intl.NumberFormat('en-US', {
      style: 'currency',
      currency: amount.currency || 'USD',
    }).format(amount.amount / 100);
  };

  return (
    <div className="space-y-6 max-w-5xl mx-auto">
      {/* Header */}
      <div>
        <Link
          href="/processing"
          className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors mb-3"
        >
          <ArrowLeft className="w-4 h-4" />
          Back to Processing
        </Link>
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-semibold text-foreground">Approval Limits</h1>
            <p className="text-muted-foreground mt-0.5">
              Set maximum approval amounts per user, optionally restricted by vendor or department
            </p>
          </div>
          <button
            onClick={() => { resetForm(); setShowForm(true); }}
            className="btn btn-primary btn-sm"
          >
            <Plus className="w-4 h-4 mr-1.5" />
            New Limit
          </button>
        </div>
      </div>

      {/* Info Card */}
      <div className="card overflow-hidden">
        <div className="h-1 bg-gradient-to-r from-warning to-warning/50" />
        <div className="p-6">
          <div className="flex items-center gap-3 mb-3">
            <div className="p-2 rounded-lg bg-warning/10">
              <Shield className="w-5 h-5 text-warning" />
            </div>
            <div>
              <h3 className="font-semibold text-foreground">How Approval Limits Work</h3>
              <p className="text-sm text-muted-foreground">
                Each user can have a maximum amount they are authorized to approve. Invoices exceeding this limit require escalation to a higher authority.
              </p>
            </div>
          </div>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-3 mt-4">
            <div className="p-3 bg-secondary/50 rounded-xl">
              <div className="flex items-center gap-2 mb-1">
                <DollarSign className="w-4 h-4 text-warning" />
                <span className="font-medium text-foreground text-sm">Amount-based</span>
              </div>
              <p className="text-xs text-muted-foreground">Cap the maximum invoice value a user can approve</p>
            </div>
            <div className="p-3 bg-secondary/50 rounded-xl">
              <div className="flex items-center gap-2 mb-1">
                <Building className="w-4 h-4 text-vendor" />
                <span className="font-medium text-foreground text-sm">Vendor-scoped</span>
              </div>
              <p className="text-xs text-muted-foreground">Optionally restrict to specific vendors</p>
            </div>
            <div className="p-3 bg-secondary/50 rounded-xl">
              <div className="flex items-center gap-2 mb-1">
                <Briefcase className="w-4 h-4 text-processing" />
                <span className="font-medium text-foreground text-sm">Department-scoped</span>
              </div>
              <p className="text-xs text-muted-foreground">Optionally restrict to specific departments</p>
            </div>
          </div>
        </div>
      </div>

      {/* Form */}
      {showForm && (
        <div className="card p-6 animate-scale-in">
          <div className="flex items-center justify-between mb-4">
            <h3 className="font-semibold text-foreground">
              {editingId ? 'Edit Approval Limit' : 'Create Approval Limit'}
            </h3>
            <button onClick={resetForm} className="p-1.5 text-muted-foreground hover:text-foreground rounded-lg hover:bg-secondary transition-colors">
              <X className="w-4 h-4" />
            </button>
          </div>
          <form onSubmit={handleSubmit} className="space-y-4">
            <div>
              <label className="block text-sm font-medium text-foreground mb-1.5">
                User ID
              </label>
              <input
                type="text"
                value={formData.user_id}
                onChange={(e) => setFormData({ ...formData, user_id: e.target.value })}
                className="input"
                placeholder="UUID of the user"
                required
              />
            </div>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div>
                <label className="block text-sm font-medium text-foreground mb-1.5">
                  Maximum Amount
                </label>
                <div className="relative">
                  <span className="absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground">$</span>
                  <input
                    type="number"
                    step="0.01"
                    min="0"
                    value={formData.max_amount_dollars}
                    onChange={(e) => setFormData({ ...formData, max_amount_dollars: e.target.value })}
                    className="input pl-7"
                    placeholder="10000.00"
                    required
                  />
                </div>
              </div>
              <div>
                <label className="block text-sm font-medium text-foreground mb-1.5">
                  Currency
                </label>
                <select
                  value={formData.currency}
                  onChange={(e) => setFormData({ ...formData, currency: e.target.value })}
                  className="input"
                >
                  <option value="USD">USD</option>
                  <option value="EUR">EUR</option>
                  <option value="GBP">GBP</option>
                  <option value="CAD">CAD</option>
                </select>
              </div>
            </div>
            <div>
              <label className="block text-sm font-medium text-foreground mb-1.5">
                Vendor Restrictions (optional)
              </label>
              <input
                type="text"
                value={formData.vendor_restrictions}
                onChange={(e) => setFormData({ ...formData, vendor_restrictions: e.target.value })}
                className="input"
                placeholder="Comma-separated vendor IDs (leave empty for all vendors)"
              />
              <p className="text-xs text-muted-foreground mt-1">
                If specified, this limit only applies to invoices from these vendors
              </p>
            </div>
            <div>
              <label className="block text-sm font-medium text-foreground mb-1.5">
                Department Restrictions (optional)
              </label>
              <input
                type="text"
                value={formData.department_restrictions}
                onChange={(e) => setFormData({ ...formData, department_restrictions: e.target.value })}
                className="input"
                placeholder="Comma-separated departments (e.g., Engineering, Marketing)"
              />
              <p className="text-xs text-muted-foreground mt-1">
                If specified, this limit only applies to invoices from these departments
              </p>
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <button type="button" onClick={resetForm} className="btn btn-secondary btn-sm">
                Cancel
              </button>
              <button
                type="submit"
                className="btn btn-primary btn-sm"
                disabled={createMutation.isPending || updateMutation.isPending}
              >
                {createMutation.isPending || updateMutation.isPending ? 'Saving...' : editingId ? 'Update' : 'Create'}
              </button>
            </div>
          </form>
        </div>
      )}

      {/* Limits List */}
      {isLoading ? (
        <div className="space-y-3">
          {[1, 2, 3].map((i) => (
            <div key={i} className="card p-5 animate-pulse">
              <div className="h-5 bg-secondary rounded w-1/3 mb-3" />
              <div className="h-4 bg-secondary rounded w-2/3" />
            </div>
          ))}
        </div>
      ) : !limits || limits.length === 0 ? (
        <div className="card p-12 text-center">
          <div className="w-14 h-14 rounded-xl bg-warning/10 flex items-center justify-center mx-auto mb-4">
            <Shield className="w-7 h-7 text-warning" />
          </div>
          <h3 className="text-lg font-semibold text-foreground mb-2">No Approval Limits</h3>
          <p className="text-muted-foreground mb-4 max-w-sm mx-auto">
            Set approval limits to control the maximum invoice amount each user can approve.
          </p>
          <button
            onClick={() => { resetForm(); setShowForm(true); }}
            className="btn btn-primary btn-sm inline-flex"
          >
            <Plus className="w-4 h-4 mr-1.5" />
            Create your first limit
          </button>
        </div>
      ) : (
        <div className="space-y-3">
          {limits.map((limit) => (
            <div key={limit.id} className="card p-5 hover:bg-secondary/30 transition-colors">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-4">
                  <div className="p-2 rounded-lg bg-warning/10">
                    <DollarSign className="w-5 h-5 text-warning" />
                  </div>
                  <div>
                    <div className="flex items-center gap-2">
                      <span className="font-medium text-foreground font-mono text-sm">
                        {limit.user_id.slice(0, 8)}...
                      </span>
                      <span className="text-lg font-semibold text-foreground">
                        {formatAmount(limit.max_amount)}
                      </span>
                    </div>
                    <div className="flex items-center gap-3 mt-1">
                      {limit.vendor_restrictions && limit.vendor_restrictions.length > 0 && (
                        <span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
                          <Building className="w-3 h-3" />
                          {limit.vendor_restrictions.length} vendor{limit.vendor_restrictions.length !== 1 ? 's' : ''}
                        </span>
                      )}
                      {limit.department_restrictions && limit.department_restrictions.length > 0 && (
                        <span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
                          <Briefcase className="w-3 h-3" />
                          {limit.department_restrictions.join(', ')}
                        </span>
                      )}
                      {!limit.vendor_restrictions?.length && !limit.department_restrictions?.length && (
                        <span className="text-xs text-muted-foreground">No restrictions - applies to all invoices</span>
                      )}
                    </div>
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  <button
                    onClick={() => handleEdit(limit)}
                    className="p-2 text-muted-foreground hover:text-primary rounded-lg hover:bg-primary/10 transition-colors"
                  >
                    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                    </svg>
                  </button>
                  <button
                    onClick={() => {
                      if (confirm('Delete this approval limit?')) {
                        deleteMutation.mutate(limit.id);
                      }
                    }}
                    className="p-2 text-muted-foreground hover:text-error rounded-lg hover:bg-error/10 transition-colors"
                  >
                    <Trash2 className="w-4 h-4" />
                  </button>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Help Section */}
      <div className="p-4 bg-warning/5 border border-warning/20 rounded-xl">
        <h3 className="font-medium text-foreground mb-2">Tips</h3>
        <ul className="text-sm text-muted-foreground space-y-1">
          <li className="flex items-start gap-2">
            <AlertCircle className="w-4 h-4 text-warning mt-0.5 flex-shrink-0" />
            Invoices exceeding a user's limit will require approval from someone with a higher limit
          </li>
          <li className="flex items-start gap-2">
            <AlertCircle className="w-4 h-4 text-warning mt-0.5 flex-shrink-0" />
            Vendor and department restrictions narrow when the limit applies
          </li>
          <li className="flex items-start gap-2">
            <AlertCircle className="w-4 h-4 text-warning mt-0.5 flex-shrink-0" />
            Users without a configured limit have no spending cap on approvals
          </li>
        </ul>
      </div>
    </div>
  );
}
