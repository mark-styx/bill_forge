'use client';

import { useState } from 'react';
import { useRouter } from 'next/navigation';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { workflowsApi } from '@/lib/api';
import { toast } from 'sonner';
import Link from 'next/link';
import {
  ArrowLeft,
  Workflow,
  Loader2,
  CheckCircle,
  Plus,
  Trash2,
} from 'lucide-react';

const conditionFields = [
  { value: 'amount', label: 'Invoice Amount' },
  { value: 'vendor', label: 'Vendor Name' },
  { value: 'department', label: 'Department' },
  { value: 'gl_code', label: 'GL Code' },
  { value: 'vendor_type', label: 'Vendor Type' },
  { value: 'tag', label: 'Tag' },
];

const conditionOperators = [
  { value: 'equals', label: 'Equals' },
  { value: 'not_equals', label: 'Does not equal' },
  { value: 'greater_than', label: 'Greater than' },
  { value: 'less_than', label: 'Less than' },
  { value: 'contains', label: 'Contains' },
];

const assignmentTypes = [
  { value: 'role', label: 'Role', description: 'Assign to users with a specific role' },
  { value: 'user', label: 'Specific User', description: 'Assign to a specific user' },
  { value: 'round_robin', label: 'Round Robin', description: 'Distribute evenly among users' },
];

export default function NewAssignmentRulePage() {
  const router = useRouter();
  const queryClient = useQueryClient();

  const { data: queues } = useQuery({
    queryKey: ['queues'],
    queryFn: () => workflowsApi.listQueues(),
  });

  const [formData, setFormData] = useState({
    name: '',
    description: '',
    queue_id: '',
    priority: 50,
    assign_type: 'role',
    assign_value: '',
  });

  const [conditions, setConditions] = useState([
    { field: 'amount', operator: 'greater_than', value: '' },
  ]);

  const createMutation = useMutation({
    mutationFn: (data: any) => workflowsApi.createAssignmentRule(data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['assignment-rules'] });
      toast.success('Assignment rule created');
      router.push('/processing/assignment-rules');
    },
    onError: (error: Error) => {
      toast.error(error.message || 'Failed to create rule');
    },
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();

    let assign_to: any;
    if (formData.assign_type === 'role') {
      assign_to = { Role: formData.assign_value };
    } else if (formData.assign_type === 'user') {
      assign_to = { User: formData.assign_value };
    } else {
      assign_to = formData.assign_value;
    }

    createMutation.mutate({
      name: formData.name,
      description: formData.description || undefined,
      queue_id: formData.queue_id,
      priority: formData.priority,
      conditions: conditions.filter(c => c.value.trim() !== '').map(c => ({
        field: c.field,
        operator: c.operator,
        value: c.field === 'amount' ? parseInt(c.value) || 0 : c.value,
      })),
      assign_to,
    });
  };

  const addCondition = () => {
    setConditions([...conditions, { field: 'amount', operator: 'equals', value: '' }]);
  };

  const removeCondition = (index: number) => {
    setConditions(conditions.filter((_, i) => i !== index));
  };

  const updateCondition = (index: number, field: string, value: string) => {
    setConditions(conditions.map((c, i) => i === index ? { ...c, [field]: value } : c));
  };

  const isValid = formData.name.trim().length > 0 && formData.queue_id.length > 0;

  return (
    <div className="space-y-6 max-w-2xl mx-auto">
      {/* Header */}
      <div>
        <Link
          href="/processing/assignment-rules"
          className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors mb-3"
        >
          <ArrowLeft className="w-4 h-4" />
          Back to Assignment Rules
        </Link>
        <h1 className="text-2xl font-semibold text-foreground">New Assignment Rule</h1>
        <p className="text-muted-foreground mt-0.5">Define conditions for automatically assigning invoices</p>
      </div>

      {/* Form Card */}
      <div className="card overflow-hidden">
        <div className="h-1 bg-gradient-to-r from-processing to-processing/50" />
        <form onSubmit={handleSubmit} className="p-6 space-y-6">
          {/* Rule Name */}
          <div>
            <label className="block text-sm font-medium text-foreground mb-1.5">
              <Workflow className="w-4 h-4 inline mr-1.5 text-processing" />
              Rule Name <span className="text-error">*</span>
            </label>
            <input
              type="text"
              value={formData.name}
              onChange={(e) => setFormData({ ...formData, name: e.target.value })}
              className="input"
              placeholder="e.g., High Value to Manager"
              required
            />
          </div>

          {/* Description */}
          <div>
            <label className="block text-sm font-medium text-foreground mb-1.5">Description</label>
            <textarea
              value={formData.description}
              onChange={(e) => setFormData({ ...formData, description: e.target.value })}
              className="input min-h-[80px]"
              placeholder="Describe what this rule does"
            />
          </div>

          {/* Queue */}
          <div>
            <label className="block text-sm font-medium text-foreground mb-1.5">
              Target Queue <span className="text-error">*</span>
            </label>
            <select
              value={formData.queue_id}
              onChange={(e) => setFormData({ ...formData, queue_id: e.target.value })}
              className="input"
              required
            >
              <option value="">Select a queue...</option>
              {queues?.map((q) => (
                <option key={q.id} value={q.id}>{q.name}</option>
              ))}
            </select>
          </div>

          {/* Priority */}
          <div>
            <label className="block text-sm font-medium text-foreground mb-1.5">
              Priority (higher = runs first)
            </label>
            <input
              type="number"
              value={formData.priority}
              onChange={(e) => setFormData({ ...formData, priority: parseInt(e.target.value) || 0 })}
              className="input max-w-[200px]"
              min={0}
              max={1000}
            />
          </div>

          {/* Conditions */}
          <div className="pt-4 border-t border-border">
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-sm font-medium text-foreground">Conditions</h3>
              <button type="button" onClick={addCondition} className="btn btn-secondary btn-sm">
                <Plus className="w-4 h-4 mr-1" />
                Add Condition
              </button>
            </div>
            <div className="space-y-3">
              {conditions.map((condition, index) => (
                <div key={index} className="flex items-center gap-2">
                  <select
                    value={condition.field}
                    onChange={(e) => updateCondition(index, 'field', e.target.value)}
                    className="input flex-1"
                  >
                    {conditionFields.map(f => (
                      <option key={f.value} value={f.value}>{f.label}</option>
                    ))}
                  </select>
                  <select
                    value={condition.operator}
                    onChange={(e) => updateCondition(index, 'operator', e.target.value)}
                    className="input flex-1"
                  >
                    {conditionOperators.map(o => (
                      <option key={o.value} value={o.value}>{o.label}</option>
                    ))}
                  </select>
                  <input
                    type="text"
                    value={condition.value}
                    onChange={(e) => updateCondition(index, 'value', e.target.value)}
                    className="input flex-1"
                    placeholder="Value"
                  />
                  {conditions.length > 1 && (
                    <button
                      type="button"
                      onClick={() => removeCondition(index)}
                      className="p-2 text-muted-foreground hover:text-error transition-colors"
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                  )}
                </div>
              ))}
            </div>
          </div>

          {/* Assignment Target */}
          <div className="pt-4 border-t border-border">
            <h3 className="text-sm font-medium text-foreground mb-3">Assign To</h3>
            <div className="grid grid-cols-1 sm:grid-cols-3 gap-3 mb-4">
              {assignmentTypes.map((type) => {
                const isSelected = formData.assign_type === type.value;
                return (
                  <button
                    key={type.value}
                    type="button"
                    onClick={() => setFormData({ ...formData, assign_type: type.value })}
                    className={`p-3 rounded-xl border-2 text-left transition-all ${
                      isSelected
                        ? 'border-processing bg-processing/5'
                        : 'border-border hover:border-processing/30'
                    }`}
                  >
                    <p className={`font-medium text-sm ${isSelected ? 'text-processing' : 'text-foreground'}`}>
                      {type.label}
                    </p>
                    <p className="text-xs text-muted-foreground mt-0.5">{type.description}</p>
                  </button>
                );
              })}
            </div>
            <input
              type="text"
              value={formData.assign_value}
              onChange={(e) => setFormData({ ...formData, assign_value: e.target.value })}
              className="input"
              placeholder={
                formData.assign_type === 'role' ? 'e.g., approver, tenant_admin' :
                formData.assign_type === 'user' ? 'User ID' :
                'Comma-separated user IDs'
              }
            />
          </div>

          {/* Actions */}
          <div className="flex items-center justify-end gap-3 pt-4 border-t border-border">
            <Link href="/processing/assignment-rules" className="btn btn-secondary">
              Cancel
            </Link>
            <button
              type="submit"
              disabled={!isValid || createMutation.isPending}
              className="btn bg-processing text-processing-foreground hover:bg-processing/90 disabled:opacity-50"
            >
              {createMutation.isPending ? (
                <>
                  <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                  Creating...
                </>
              ) : (
                <>
                  <CheckCircle className="w-4 h-4 mr-2" />
                  Create Rule
                </>
              )}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
