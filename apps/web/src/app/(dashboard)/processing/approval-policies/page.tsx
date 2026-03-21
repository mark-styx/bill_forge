'use client';

import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import Link from 'next/link';
import { approvalChainsApi } from '@/lib/api';
import type { ApprovalPolicy, CreatePolicyInput, ApprovalChainLevel } from '@/lib/api';
import { toast } from 'sonner';
import {
  ArrowLeft,
  Plus,
  Trash2,
  Edit,
  Shield,
  Layers,
  CheckCircle,
  XCircle,
  Loader2,
  ChevronDown,
  ChevronUp,
  GripVertical,
  Users,
  Clock,
  AlertCircle,
  ToggleLeft,
  ToggleRight,
  Save,
  X,
} from 'lucide-react';

type FormLevel = {
  level: number;
  name: string;
  approver_type: string;
  approver_ids: string[];
  required_approvals: number;
  escalation_timeout_hours: number | undefined;
  escalate_to: string | undefined;
  allow_delegation: boolean;
  auto_approve_below: number | undefined;
};

const emptyLevel = (level: number): FormLevel => ({
  level,
  name: '',
  approver_type: 'user',
  approver_ids: [],
  required_approvals: 1,
  escalation_timeout_hours: undefined,
  escalate_to: undefined,
  allow_delegation: false,
  auto_approve_below: undefined,
});

type PolicyForm = {
  name: string;
  description: string;
  is_active: boolean;
  priority: number;
  match_criteria: {
    min_amount?: number;
    max_amount?: number;
    departments?: string[];
  };
  require_sequential: boolean;
  allow_self_approval: boolean;
  levels: FormLevel[];
};

const emptyForm = (): PolicyForm => ({
  name: '',
  description: '',
  is_active: true,
  priority: 100,
  match_criteria: {},
  require_sequential: true,
  allow_self_approval: false,
  levels: [emptyLevel(1)],
});

export default function ApprovalPoliciesPage() {
  const queryClient = useQueryClient();
  const [showForm, setShowForm] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [form, setForm] = useState<PolicyForm>(emptyForm());
  const [expandedPolicy, setExpandedPolicy] = useState<string | null>(null);

  const { data: policies, isLoading } = useQuery({
    queryKey: ['approval-policies'],
    queryFn: () => approvalChainsApi.listPolicies(),
  });

  const createMutation = useMutation({
    mutationFn: (data: CreatePolicyInput) => approvalChainsApi.createPolicy(data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['approval-policies'] });
      toast.success('Approval policy created');
      resetForm();
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to create policy');
    },
  });

  const updateMutation = useMutation({
    mutationFn: ({ id, data }: { id: string; data: Partial<CreatePolicyInput> }) =>
      approvalChainsApi.updatePolicy(id, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['approval-policies'] });
      toast.success('Approval policy updated');
      resetForm();
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to update policy');
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => approvalChainsApi.deletePolicy(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['approval-policies'] });
      toast.success('Approval policy deleted');
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to delete policy');
    },
  });

  const resetForm = () => {
    setShowForm(false);
    setEditingId(null);
    setForm(emptyForm());
  };

  const startEdit = (policy: ApprovalPolicy) => {
    setEditingId(policy.id);
    setForm({
      name: policy.name,
      description: policy.description || '',
      is_active: policy.is_active,
      priority: policy.priority,
      match_criteria: policy.match_criteria as PolicyForm['match_criteria'],
      require_sequential: policy.require_sequential,
      allow_self_approval: policy.allow_self_approval,
      levels: policy.levels?.map((l) => ({
        level: l.level,
        name: l.name,
        approver_type: l.approver_type,
        approver_ids: l.approver_ids,
        required_approvals: l.required_approvals,
        escalation_timeout_hours: l.escalation_timeout_hours ?? undefined,
        escalate_to: l.escalate_to ?? undefined,
        allow_delegation: l.allow_delegation,
        auto_approve_below: l.auto_approve_below ?? undefined,
      })) || [emptyLevel(1)],
    });
    setShowForm(true);
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!form.name.trim()) {
      toast.error('Policy name is required');
      return;
    }
    if (form.levels.length === 0) {
      toast.error('At least one approval level is required');
      return;
    }
    for (const level of form.levels) {
      if (!level.name.trim()) {
        toast.error(`Level ${level.level} needs a name`);
        return;
      }
    }

    const payload: CreatePolicyInput = {
      name: form.name,
      description: form.description || undefined,
      is_active: form.is_active,
      priority: form.priority,
      match_criteria: form.match_criteria,
      require_sequential: form.require_sequential,
      allow_self_approval: form.allow_self_approval,
      levels: form.levels.map((l) => ({
        level: l.level,
        name: l.name,
        approver_type: l.approver_type,
        approver_ids: l.approver_ids,
        required_approvals: l.required_approvals,
        escalation_timeout_hours: l.escalation_timeout_hours,
        escalate_to: l.escalate_to,
        allow_delegation: l.allow_delegation,
        auto_approve_below: l.auto_approve_below,
      })),
    };

    if (editingId) {
      updateMutation.mutate({ id: editingId, data: payload });
    } else {
      createMutation.mutate(payload);
    }
  };

  const addLevel = () => {
    setForm((prev) => ({
      ...prev,
      levels: [...prev.levels, emptyLevel(prev.levels.length + 1)],
    }));
  };

  const removeLevel = (index: number) => {
    setForm((prev) => ({
      ...prev,
      levels: prev.levels
        .filter((_, i) => i !== index)
        .map((l, i) => ({ ...l, level: i + 1 })),
    }));
  };

  const updateLevel = (index: number, updates: Partial<FormLevel>) => {
    setForm((prev) => ({
      ...prev,
      levels: prev.levels.map((l, i) => (i === index ? { ...l, ...updates } : l)),
    }));
  };

  const isSaving = createMutation.isPending || updateMutation.isPending;

  return (
    <div className="max-w-4xl mx-auto space-y-6">
      {/* Header */}
      <div className="flex items-start justify-between">
        <div>
          <Link
            href="/processing"
            className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors mb-3"
          >
            <ArrowLeft className="w-4 h-4" />
            Back to Processing
          </Link>
          <h1 className="text-2xl font-semibold text-foreground">Approval Policies</h1>
          <p className="text-muted-foreground mt-0.5">
            Configure multi-level approval chains with threshold-based routing
          </p>
        </div>
        {!showForm && (
          <button
            onClick={() => { setShowForm(true); setForm(emptyForm()); setEditingId(null); }}
            className="btn bg-processing text-processing-foreground hover:bg-processing/90 shadow-sm"
          >
            <Plus className="w-4 h-4 mr-2" />
            New Policy
          </button>
        )}
      </div>

      {/* Policy Form */}
      {showForm && (
        <form onSubmit={handleSubmit} className="card overflow-hidden">
          <div className="h-1 bg-gradient-to-r from-processing to-processing/50" />
          <div className="p-6 space-y-6">
            <div className="flex items-center justify-between">
              <h2 className="text-lg font-semibold text-foreground">
                {editingId ? 'Edit Policy' : 'Create Approval Policy'}
              </h2>
              <button type="button" onClick={resetForm} className="p-1 text-muted-foreground hover:text-foreground">
                <X className="w-5 h-5" />
              </button>
            </div>

            {/* Basic Info */}
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div>
                <label className="block text-sm font-medium text-foreground mb-1">Policy Name</label>
                <input
                  type="text"
                  className="input w-full"
                  placeholder="e.g., High-Value Invoice Approval"
                  value={form.name}
                  onChange={(e) => setForm((prev) => ({ ...prev, name: e.target.value }))}
                />
              </div>
              <div>
                <label className="block text-sm font-medium text-foreground mb-1">Priority</label>
                <input
                  type="number"
                  className="input w-full"
                  placeholder="100"
                  value={form.priority}
                  onChange={(e) => setForm((prev) => ({ ...prev, priority: parseInt(e.target.value) || 100 }))}
                />
                <p className="text-xs text-muted-foreground mt-1">Lower number = higher priority</p>
              </div>
            </div>

            <div>
              <label className="block text-sm font-medium text-foreground mb-1">Description</label>
              <textarea
                className="input w-full"
                rows={2}
                placeholder="Describe when this policy applies..."
                value={form.description}
                onChange={(e) => setForm((prev) => ({ ...prev, description: e.target.value }))}
              />
            </div>

            {/* Match Criteria */}
            <div className="p-4 bg-secondary/50 rounded-xl space-y-4">
              <h3 className="text-sm font-medium text-foreground">Match Criteria</h3>
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div>
                  <label className="block text-xs text-muted-foreground mb-1">Min Amount (cents)</label>
                  <input
                    type="number"
                    className="input w-full"
                    placeholder="e.g., 100000"
                    value={form.match_criteria.min_amount ?? ''}
                    onChange={(e) =>
                      setForm((prev) => ({
                        ...prev,
                        match_criteria: {
                          ...prev.match_criteria,
                          min_amount: e.target.value ? parseInt(e.target.value) : undefined,
                        },
                      }))
                    }
                  />
                </div>
                <div>
                  <label className="block text-xs text-muted-foreground mb-1">Max Amount (cents)</label>
                  <input
                    type="number"
                    className="input w-full"
                    placeholder="e.g., 1000000"
                    value={form.match_criteria.max_amount ?? ''}
                    onChange={(e) =>
                      setForm((prev) => ({
                        ...prev,
                        match_criteria: {
                          ...prev.match_criteria,
                          max_amount: e.target.value ? parseInt(e.target.value) : undefined,
                        },
                      }))
                    }
                  />
                </div>
              </div>
              <div>
                <label className="block text-xs text-muted-foreground mb-1">Departments (comma separated)</label>
                <input
                  type="text"
                  className="input w-full"
                  placeholder="e.g., engineering, marketing"
                  value={form.match_criteria.departments?.join(', ') ?? ''}
                  onChange={(e) =>
                    setForm((prev) => ({
                      ...prev,
                      match_criteria: {
                        ...prev.match_criteria,
                        departments: e.target.value
                          ? e.target.value.split(',').map((s) => s.trim()).filter(Boolean)
                          : undefined,
                      },
                    }))
                  }
                />
              </div>
            </div>

            {/* Toggles */}
            <div className="flex flex-wrap gap-6">
              <label className="flex items-center gap-2 cursor-pointer">
                <button
                  type="button"
                  onClick={() => setForm((prev) => ({ ...prev, is_active: !prev.is_active }))}
                  className="text-foreground"
                >
                  {form.is_active ? (
                    <ToggleRight className="w-6 h-6 text-success" />
                  ) : (
                    <ToggleLeft className="w-6 h-6 text-muted-foreground" />
                  )}
                </button>
                <span className="text-sm text-foreground">Active</span>
              </label>
              <label className="flex items-center gap-2 cursor-pointer">
                <button
                  type="button"
                  onClick={() => setForm((prev) => ({ ...prev, require_sequential: !prev.require_sequential }))}
                  className="text-foreground"
                >
                  {form.require_sequential ? (
                    <ToggleRight className="w-6 h-6 text-processing" />
                  ) : (
                    <ToggleLeft className="w-6 h-6 text-muted-foreground" />
                  )}
                </button>
                <span className="text-sm text-foreground">Sequential Levels</span>
              </label>
              <label className="flex items-center gap-2 cursor-pointer">
                <button
                  type="button"
                  onClick={() => setForm((prev) => ({ ...prev, allow_self_approval: !prev.allow_self_approval }))}
                  className="text-foreground"
                >
                  {form.allow_self_approval ? (
                    <ToggleRight className="w-6 h-6 text-warning" />
                  ) : (
                    <ToggleLeft className="w-6 h-6 text-muted-foreground" />
                  )}
                </button>
                <span className="text-sm text-foreground">Allow Self-Approval</span>
              </label>
            </div>

            {/* Chain Levels */}
            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <h3 className="text-sm font-medium text-foreground flex items-center gap-2">
                  <Layers className="w-4 h-4" />
                  Approval Levels
                </h3>
                <button
                  type="button"
                  onClick={addLevel}
                  className="text-xs text-processing hover:text-processing/80 font-medium flex items-center gap-1"
                >
                  <Plus className="w-3 h-3" />
                  Add Level
                </button>
              </div>

              {form.levels.map((level, index) => (
                <div key={index} className="border border-border rounded-xl p-4 space-y-3">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <span className="flex items-center justify-center w-6 h-6 rounded-full bg-processing/10 text-processing text-xs font-bold">
                        {level.level}
                      </span>
                      <span className="text-sm font-medium text-foreground">Level {level.level}</span>
                    </div>
                    {form.levels.length > 1 && (
                      <button
                        type="button"
                        onClick={() => removeLevel(index)}
                        className="p-1 text-muted-foreground hover:text-error transition-colors"
                      >
                        <Trash2 className="w-4 h-4" />
                      </button>
                    )}
                  </div>

                  <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                    <div>
                      <label className="block text-xs text-muted-foreground mb-1">Level Name</label>
                      <input
                        type="text"
                        className="input w-full"
                        placeholder="e.g., Manager Approval"
                        value={level.name}
                        onChange={(e) => updateLevel(index, { name: e.target.value })}
                      />
                    </div>
                    <div>
                      <label className="block text-xs text-muted-foreground mb-1">Approver Type</label>
                      <select
                        className="input w-full"
                        value={level.approver_type}
                        onChange={(e) => updateLevel(index, { approver_type: e.target.value })}
                      >
                        <option value="user">Specific Users</option>
                        <option value="role">Role-Based</option>
                        <option value="department_head">Department Head</option>
                        <option value="manager_chain">Manager Chain</option>
                      </select>
                    </div>
                  </div>

                  <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
                    <div>
                      <label className="block text-xs text-muted-foreground mb-1">Required Approvals</label>
                      <input
                        type="number"
                        className="input w-full"
                        min={1}
                        value={level.required_approvals}
                        onChange={(e) => updateLevel(index, { required_approvals: parseInt(e.target.value) || 1 })}
                      />
                    </div>
                    <div>
                      <label className="block text-xs text-muted-foreground mb-1">Escalation Timeout (hrs)</label>
                      <input
                        type="number"
                        className="input w-full"
                        placeholder="Optional"
                        value={level.escalation_timeout_hours ?? ''}
                        onChange={(e) =>
                          updateLevel(index, {
                            escalation_timeout_hours: e.target.value ? parseInt(e.target.value) : undefined,
                          })
                        }
                      />
                    </div>
                    <div>
                      <label className="block text-xs text-muted-foreground mb-1">Auto-Approve Below (cents)</label>
                      <input
                        type="number"
                        className="input w-full"
                        placeholder="Optional"
                        value={level.auto_approve_below ?? ''}
                        onChange={(e) =>
                          updateLevel(index, {
                            auto_approve_below: e.target.value ? parseInt(e.target.value) : undefined,
                          })
                        }
                      />
                    </div>
                  </div>

                  <div>
                    <label className="block text-xs text-muted-foreground mb-1">Approver IDs (comma separated)</label>
                    <input
                      type="text"
                      className="input w-full"
                      placeholder="e.g., user-uuid-1, user-uuid-2"
                      value={level.approver_ids.join(', ')}
                      onChange={(e) =>
                        updateLevel(index, {
                          approver_ids: e.target.value
                            .split(',')
                            .map((s) => s.trim())
                            .filter(Boolean),
                        })
                      }
                    />
                  </div>

                  <label className="flex items-center gap-2 cursor-pointer">
                    <input
                      type="checkbox"
                      checked={level.allow_delegation}
                      onChange={(e) => updateLevel(index, { allow_delegation: e.target.checked })}
                      className="rounded"
                    />
                    <span className="text-xs text-muted-foreground">Allow delegation at this level</span>
                  </label>
                </div>
              ))}
            </div>

            {/* Form Actions */}
            <div className="flex justify-end gap-3 pt-2">
              <button type="button" onClick={resetForm} className="btn btn-secondary">
                Cancel
              </button>
              <button type="submit" className="btn bg-processing text-processing-foreground hover:bg-processing/90 shadow-sm" disabled={isSaving}>
                {isSaving ? (
                  <>
                    <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                    Saving...
                  </>
                ) : (
                  <>
                    <Save className="w-4 h-4 mr-2" />
                    {editingId ? 'Update Policy' : 'Create Policy'}
                  </>
                )}
              </button>
            </div>
          </div>
        </form>
      )}

      {/* Policy List */}
      <div className="space-y-3">
        {isLoading ? (
          <div className="card p-12 flex items-center justify-center">
            <Loader2 className="w-6 h-6 animate-spin text-muted-foreground" />
          </div>
        ) : !policies || policies.length === 0 ? (
          <div className="card p-12 text-center">
            <div className="p-3 rounded-xl bg-secondary w-fit mx-auto mb-3">
              <Shield className="w-8 h-8 text-muted-foreground" />
            </div>
            <h3 className="font-medium text-foreground mb-1">No approval policies</h3>
            <p className="text-sm text-muted-foreground">Create your first policy to set up multi-level approval chains.</p>
          </div>
        ) : (
          (policies as ApprovalPolicy[]).map((policy) => (
            <div key={policy.id} className="card overflow-hidden">
              <div
                className={`h-1 ${
                  policy.is_active
                    ? 'bg-gradient-to-r from-processing to-processing/50'
                    : 'bg-gradient-to-r from-muted-foreground/30 to-muted-foreground/10'
                }`}
              />
              <div className="p-4">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-3">
                    <div className={`p-2 rounded-lg ${policy.is_active ? 'bg-processing/10' : 'bg-secondary'}`}>
                      <Shield className={`w-5 h-5 ${policy.is_active ? 'text-processing' : 'text-muted-foreground'}`} />
                    </div>
                    <div>
                      <div className="flex items-center gap-2">
                        <h3 className="font-medium text-foreground">{policy.name}</h3>
                        <span
                          className={`px-2 py-0.5 rounded text-xs font-medium ${
                            policy.is_active
                              ? 'bg-success/10 text-success'
                              : 'bg-secondary text-muted-foreground'
                          }`}
                        >
                          {policy.is_active ? 'Active' : 'Inactive'}
                        </span>
                        <span className="px-2 py-0.5 rounded text-xs bg-secondary text-muted-foreground">
                          Priority: {policy.priority}
                        </span>
                      </div>
                      {policy.description && (
                        <p className="text-sm text-muted-foreground mt-0.5">{policy.description}</p>
                      )}
                    </div>
                  </div>
                  <div className="flex items-center gap-2">
                    <button
                      onClick={() => startEdit(policy)}
                      className="p-2 text-muted-foreground hover:text-foreground hover:bg-secondary rounded-lg transition-colors"
                    >
                      <Edit className="w-4 h-4" />
                    </button>
                    <button
                      onClick={() => {
                        if (confirm('Delete this approval policy?')) {
                          deleteMutation.mutate(policy.id);
                        }
                      }}
                      className="p-2 text-muted-foreground hover:text-error hover:bg-error/10 rounded-lg transition-colors"
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                    <button
                      onClick={() => setExpandedPolicy(expandedPolicy === policy.id ? null : policy.id)}
                      className="p-2 text-muted-foreground hover:text-foreground hover:bg-secondary rounded-lg transition-colors"
                    >
                      {expandedPolicy === policy.id ? (
                        <ChevronUp className="w-4 h-4" />
                      ) : (
                        <ChevronDown className="w-4 h-4" />
                      )}
                    </button>
                  </div>
                </div>

                {/* Expanded Details */}
                {expandedPolicy === policy.id && (
                  <div className="mt-4 pt-4 border-t border-border space-y-3">
                    <div className="flex flex-wrap gap-4 text-xs text-muted-foreground">
                      <span className="flex items-center gap-1">
                        <Layers className="w-3 h-3" />
                        {policy.require_sequential ? 'Sequential' : 'Parallel'} approval
                      </span>
                      <span className="flex items-center gap-1">
                        <Users className="w-3 h-3" />
                        Self-approval: {policy.allow_self_approval ? 'Allowed' : 'Not allowed'}
                      </span>
                    </div>

                    {policy.levels && policy.levels.length > 0 && (
                      <div className="space-y-2">
                        <h4 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
                          Approval Levels
                        </h4>
                        {policy.levels.map((level) => (
                          <div
                            key={level.id}
                            className="flex items-center gap-3 p-3 bg-secondary/50 rounded-lg"
                          >
                            <span className="flex items-center justify-center w-6 h-6 rounded-full bg-processing/10 text-processing text-xs font-bold">
                              {level.level}
                            </span>
                            <div className="flex-1">
                              <p className="text-sm font-medium text-foreground">{level.name}</p>
                              <div className="flex flex-wrap gap-3 mt-1 text-xs text-muted-foreground">
                                <span>Type: {level.approver_type}</span>
                                <span>Required: {level.required_approvals}</span>
                                {level.escalation_timeout_hours && (
                                  <span className="flex items-center gap-1">
                                    <Clock className="w-3 h-3" />
                                    Escalation: {level.escalation_timeout_hours}h
                                  </span>
                                )}
                                {level.allow_delegation && <span>Delegation allowed</span>}
                              </div>
                            </div>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                )}
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
