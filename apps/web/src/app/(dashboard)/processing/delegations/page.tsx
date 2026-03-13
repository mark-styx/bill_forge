'use client';

import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { workflowsApi } from '@/lib/api';
import type { CreateApprovalDelegationInput } from '@/lib/api';
import {
  Plus,
  Trash2,
  ArrowLeft,
  UserCheck,
  ArrowRight,
  Calendar,
  AlertCircle,
  X,
} from 'lucide-react';
import { toast } from 'sonner';
import Link from 'next/link';

export default function DelegationsPage() {
  const queryClient = useQueryClient();
  const [showForm, setShowForm] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);

  const [formData, setFormData] = useState<CreateApprovalDelegationInput>({
    delegator_id: '',
    delegate_id: '',
    start_date: new Date().toISOString().slice(0, 16),
    end_date: new Date(Date.now() + 30 * 24 * 60 * 60 * 1000).toISOString().slice(0, 16),
  });

  const { data: delegations, isLoading } = useQuery({
    queryKey: ['delegations'],
    queryFn: () => workflowsApi.listDelegations(),
  });

  const createMutation = useMutation({
    mutationFn: (data: CreateApprovalDelegationInput) =>
      workflowsApi.createDelegation(data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['delegations'] });
      toast.success('Delegation created');
      resetForm();
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to create delegation');
    },
  });

  const updateMutation = useMutation({
    mutationFn: ({ id, data }: { id: string; data: CreateApprovalDelegationInput }) =>
      workflowsApi.updateDelegation(id, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['delegations'] });
      toast.success('Delegation updated');
      resetForm();
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to update delegation');
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => workflowsApi.deleteDelegation(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['delegations'] });
      toast.success('Delegation deleted');
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to delete delegation');
    },
  });

  const resetForm = () => {
    setShowForm(false);
    setEditingId(null);
    setFormData({
      delegator_id: '',
      delegate_id: '',
      start_date: new Date().toISOString().slice(0, 16),
      end_date: new Date(Date.now() + 30 * 24 * 60 * 60 * 1000).toISOString().slice(0, 16),
    });
  };

  const handleEdit = (delegation: any) => {
    setEditingId(delegation.id);
    setFormData({
      delegator_id: delegation.delegator_id,
      delegate_id: delegation.delegate_id,
      start_date: delegation.start_date.slice(0, 16),
      end_date: delegation.end_date.slice(0, 16),
    });
    setShowForm(true);
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const payload = {
      ...formData,
      start_date: new Date(formData.start_date).toISOString(),
      end_date: new Date(formData.end_date).toISOString(),
    };
    if (editingId) {
      updateMutation.mutate({ id: editingId, data: payload });
    } else {
      createMutation.mutate(payload);
    }
  };

  const isActive = (delegation: any) => {
    const now = new Date();
    return delegation.is_active &&
      new Date(delegation.start_date) <= now &&
      new Date(delegation.end_date) >= now;
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
            <h1 className="text-2xl font-semibold text-foreground">Approval Delegations</h1>
            <p className="text-muted-foreground mt-0.5">
              Delegate approval authority to another user for a specified period
            </p>
          </div>
          <button
            onClick={() => { resetForm(); setShowForm(true); }}
            className="btn btn-primary btn-sm"
          >
            <Plus className="w-4 h-4 mr-1.5" />
            New Delegation
          </button>
        </div>
      </div>

      {/* Info Card */}
      <div className="card overflow-hidden">
        <div className="h-1 bg-gradient-to-r from-primary to-primary/50" />
        <div className="p-6">
          <div className="flex items-center gap-3 mb-3">
            <div className="p-2 rounded-lg bg-primary/10">
              <UserCheck className="w-5 h-5 text-primary" />
            </div>
            <div>
              <h3 className="font-semibold text-foreground">How Delegations Work</h3>
              <p className="text-sm text-muted-foreground">
                When a user delegates their approval authority, the delegate can approve invoices on their behalf during the specified date range.
              </p>
            </div>
          </div>
          <div className="flex items-start gap-2 p-2.5 bg-warning/5 border border-warning/15 rounded-lg">
            <AlertCircle className="w-4 h-4 text-warning mt-0.5 flex-shrink-0" />
            <p className="text-xs text-muted-foreground">
              Delegations are time-bound. Once the end date passes, the delegation automatically expires.
              Both the original approver and the delegate can approve during the active period.
            </p>
          </div>
        </div>
      </div>

      {/* Form */}
      {showForm && (
        <div className="card p-6 animate-scale-in">
          <div className="flex items-center justify-between mb-4">
            <h3 className="font-semibold text-foreground">
              {editingId ? 'Edit Delegation' : 'Create Delegation'}
            </h3>
            <button onClick={resetForm} className="p-1.5 text-muted-foreground hover:text-foreground rounded-lg hover:bg-secondary transition-colors">
              <X className="w-4 h-4" />
            </button>
          </div>
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div>
                <label className="block text-sm font-medium text-foreground mb-1.5">
                  Delegator (User ID)
                </label>
                <input
                  type="text"
                  value={formData.delegator_id}
                  onChange={(e) => setFormData({ ...formData, delegator_id: e.target.value })}
                  className="input"
                  placeholder="User ID of the person delegating"
                  required
                />
                <p className="text-xs text-muted-foreground mt-1">The user granting approval authority</p>
              </div>
              <div>
                <label className="block text-sm font-medium text-foreground mb-1.5">
                  Delegate (User ID)
                </label>
                <input
                  type="text"
                  value={formData.delegate_id}
                  onChange={(e) => setFormData({ ...formData, delegate_id: e.target.value })}
                  className="input"
                  placeholder="User ID of the delegate"
                  required
                />
                <p className="text-xs text-muted-foreground mt-1">The user receiving approval authority</p>
              </div>
            </div>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div>
                <label className="block text-sm font-medium text-foreground mb-1.5">
                  Start Date
                </label>
                <input
                  type="datetime-local"
                  value={formData.start_date}
                  onChange={(e) => setFormData({ ...formData, start_date: e.target.value })}
                  className="input"
                  required
                />
              </div>
              <div>
                <label className="block text-sm font-medium text-foreground mb-1.5">
                  End Date
                </label>
                <input
                  type="datetime-local"
                  value={formData.end_date}
                  onChange={(e) => setFormData({ ...formData, end_date: e.target.value })}
                  className="input"
                  required
                />
              </div>
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

      {/* Delegations List */}
      {isLoading ? (
        <div className="space-y-3">
          {[1, 2, 3].map((i) => (
            <div key={i} className="card p-5 animate-pulse">
              <div className="h-5 bg-secondary rounded w-1/3 mb-3" />
              <div className="h-4 bg-secondary rounded w-2/3" />
            </div>
          ))}
        </div>
      ) : !delegations || delegations.length === 0 ? (
        <div className="card p-12 text-center">
          <div className="w-14 h-14 rounded-xl bg-primary/10 flex items-center justify-center mx-auto mb-4">
            <UserCheck className="w-7 h-7 text-primary" />
          </div>
          <h3 className="text-lg font-semibold text-foreground mb-2">No Delegations</h3>
          <p className="text-muted-foreground mb-4 max-w-sm mx-auto">
            Create a delegation to allow another user to approve invoices on someone's behalf.
          </p>
          <button
            onClick={() => { resetForm(); setShowForm(true); }}
            className="btn btn-primary btn-sm inline-flex"
          >
            <Plus className="w-4 h-4 mr-1.5" />
            Create your first delegation
          </button>
        </div>
      ) : (
        <div className="space-y-3">
          {delegations.map((delegation) => {
            const active = isActive(delegation);
            return (
              <div key={delegation.id} className="card p-5 hover:bg-secondary/30 transition-colors">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-4">
                    <span className={`px-2.5 py-1 rounded-full text-xs font-medium ${
                      active
                        ? 'bg-success/10 text-success'
                        : 'bg-secondary text-muted-foreground'
                    }`}>
                      {active ? 'Active' : 'Inactive'}
                    </span>
                    <div className="flex items-center gap-2">
                      <div className="text-sm">
                        <span className="text-muted-foreground">From:</span>{' '}
                        <span className="font-medium text-foreground font-mono text-xs">
                          {delegation.delegator_id.slice(0, 8)}...
                        </span>
                      </div>
                      <ArrowRight className="w-4 h-4 text-muted-foreground" />
                      <div className="text-sm">
                        <span className="text-muted-foreground">To:</span>{' '}
                        <span className="font-medium text-foreground font-mono text-xs">
                          {delegation.delegate_id.slice(0, 8)}...
                        </span>
                      </div>
                    </div>
                  </div>
                  <div className="flex items-center gap-3">
                    <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
                      <Calendar className="w-3.5 h-3.5" />
                      {new Date(delegation.start_date).toLocaleDateString()} - {new Date(delegation.end_date).toLocaleDateString()}
                    </div>
                    <button
                      onClick={() => handleEdit(delegation)}
                      className="p-2 text-muted-foreground hover:text-primary rounded-lg hover:bg-primary/10 transition-colors"
                    >
                      <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                      </svg>
                    </button>
                    <button
                      onClick={() => {
                        if (confirm('Delete this delegation?')) {
                          deleteMutation.mutate(delegation.id);
                        }
                      }}
                      className="p-2 text-muted-foreground hover:text-error rounded-lg hover:bg-error/10 transition-colors"
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
