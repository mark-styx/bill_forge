'use client';

import { useState, useEffect } from 'react';
import { useRouter, useParams } from 'next/navigation';
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
  Eye,
  Info,
} from 'lucide-react';

// All backend AssignmentField variants (snake_case serialization)
const conditionFields = [
  { value: 'amount', label: 'Invoice Amount', type: 'numeric' as const },
  { value: 'vendor_id', label: 'Vendor ID', type: 'string' as const },
  { value: 'vendor_name', label: 'Vendor Name', type: 'string' as const },
  { value: 'department', label: 'Department', type: 'string' as const },
  { value: 'gl_code', label: 'GL Code', type: 'string' as const },
  { value: 'tag', label: 'Tag', type: 'string' as const },
  { value: 'custom_field', label: 'Custom Field', type: 'custom' as const },
];

// All backend ConditionOperator variants (snake_case serialization)
const allOperators = [
  { value: 'equals', label: 'Equals', types: ['numeric', 'string', 'custom'] },
  { value: 'not_equals', label: 'Does not equal', types: ['numeric', 'string', 'custom'] },
  { value: 'greater_than', label: 'Greater than', types: ['numeric'] },
  { value: 'greater_than_or_equal', label: 'Greater than or equal', types: ['numeric'] },
  { value: 'less_than', label: 'Less than', types: ['numeric'] },
  { value: 'less_than_or_equal', label: 'Less than or equal', types: ['numeric'] },
  { value: 'contains', label: 'Contains', types: ['string', 'custom'] },
  { value: 'starts_with', label: 'Starts with', types: ['string'] },
  { value: 'ends_with', label: 'Ends with', types: ['string'] },
  { value: 'in', label: 'Is one of', types: ['string', 'numeric'] },
  { value: 'not_in', label: 'Is not one of', types: ['string', 'numeric'] },
  { value: 'between', label: 'Is between', types: ['numeric'] },
  { value: 'is_null', label: 'Is empty', types: ['numeric', 'string', 'custom'] },
  { value: 'is_not_null', label: 'Is not empty', types: ['numeric', 'string', 'custom'] },
];

// All backend AssignmentTarget variants
const assignmentTypes = [
  { value: 'role', label: 'Role', description: 'Assign to users with a specific role', needsInput: true },
  { value: 'user', label: 'Specific User', description: 'Assign to a specific user', needsInput: true },
  { value: 'round_robin', label: 'Round Robin', description: 'Distribute evenly among users', needsInput: true },
  { value: 'least_loaded', label: 'Least Loaded', description: 'Assign to user with lowest workload', needsInput: true },
  { value: 'vendor_approver', label: 'Vendor Approver', description: 'Registered approver for the vendor', needsInput: false },
  { value: 'department_approver', label: 'Dept. Approver', description: 'Registered approver for the department', needsInput: false },
];

interface ConditionRow {
  field: string;
  operator: string;
  value: string;
  value2: string;
  customFieldName: string;
}

function getFieldType(fieldValue: string): string {
  return conditionFields.find(f => f.value === fieldValue)?.type || 'string';
}

function getOperatorsForField(fieldValue: string) {
  const fieldType = getFieldType(fieldValue);
  return allOperators.filter(op => op.types.includes(fieldType));
}

function operatorNeedsValue(operator: string): boolean {
  return operator !== 'is_null' && operator !== 'is_not_null';
}

function getFieldLabel(fieldValue: string): string {
  return conditionFields.find(f => f.value === fieldValue)?.label || fieldValue;
}

function getOperatorLabel(operatorValue: string): string {
  return allOperators.find(o => o.value === operatorValue)?.label?.toLowerCase() || operatorValue;
}

function formatConditionPreview(c: ConditionRow): string {
  const field = c.field === 'custom_field' && c.customFieldName
    ? `Custom: ${c.customFieldName}`
    : getFieldLabel(c.field);
  const op = getOperatorLabel(c.operator);

  if (!operatorNeedsValue(c.operator)) {
    return `${field} ${op}`;
  }

  if (c.operator === 'between') {
    const min = c.field === 'amount' ? `$${Number(c.value || 0).toLocaleString()}` : c.value;
    const max = c.field === 'amount' ? `$${Number(c.value2 || 0).toLocaleString()}` : c.value2;
    return `${field} ${op} ${min} and ${max}`;
  }

  if (c.operator === 'in' || c.operator === 'not_in') {
    return `${field} ${op} [${c.value}]`;
  }

  const displayValue = c.field === 'amount' && c.value
    ? `$${Number(c.value).toLocaleString()}`
    : c.value;

  return `${field} ${op} ${displayValue || '...'}`;
}

// Parse backend AssignmentTarget into form-friendly values
function parseAssignTarget(target: any): { type: string; value: string } {
  if (typeof target === 'object') {
    if ('User' in target) return { type: 'user', value: target.User };
    if ('Role' in target) return { type: 'role', value: target.Role };
    if ('RoundRobin' in target) return { type: 'round_robin', value: target.RoundRobin.join(', ') };
    if ('LeastLoaded' in target) return { type: 'least_loaded', value: target.LeastLoaded.join(', ') };
  }
  if (target === 'VendorApprover') return { type: 'vendor_approver', value: '' };
  if (target === 'DepartmentApprover') return { type: 'department_approver', value: '' };
  return { type: 'role', value: '' };
}

// Parse backend condition into ConditionRow
function parseCondition(condition: any): ConditionRow {
  let value = '';
  let value2 = '';
  let customFieldName = '';

  if (condition.field === 'custom_field' && typeof condition.value === 'object' && condition.value?.field) {
    customFieldName = condition.value.field;
    value = String(condition.value.value ?? '');
  } else if (condition.operator === 'between' && Array.isArray(condition.value)) {
    value = String(condition.value[0] ?? '');
    value2 = String(condition.value[1] ?? '');
  } else if ((condition.operator === 'in' || condition.operator === 'not_in') && Array.isArray(condition.value)) {
    value = condition.value.join(', ');
  } else if (condition.value != null) {
    value = String(condition.value);
  }

  return {
    field: condition.field,
    operator: condition.operator,
    value,
    value2,
    customFieldName,
  };
}

export default function EditAssignmentRulePage() {
  const router = useRouter();
  const params = useParams();
  const ruleId = params.id as string;
  const queryClient = useQueryClient();

  const { data: existingRule, isLoading: ruleLoading } = useQuery({
    queryKey: ['assignment-rule', ruleId],
    queryFn: () => workflowsApi.getAssignmentRule(ruleId),
  });

  const { data: queues } = useQuery({
    queryKey: ['work-queues'],
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

  const [conditions, setConditions] = useState<ConditionRow[]>([
    { field: 'amount', operator: 'greater_than', value: '', value2: '', customFieldName: '' },
  ]);

  const [showPreview, setShowPreview] = useState(false);
  const [initialized, setInitialized] = useState(false);

  // Pre-populate form when existing rule loads
  useEffect(() => {
    if (existingRule && !initialized) {
      const assignTarget = parseAssignTarget(existingRule.assign_to);
      setFormData({
        name: existingRule.name,
        description: existingRule.description || '',
        queue_id: existingRule.queue_id,
        priority: existingRule.priority,
        assign_type: assignTarget.type,
        assign_value: assignTarget.value,
      });

      if (existingRule.conditions && existingRule.conditions.length > 0) {
        setConditions(existingRule.conditions.map(parseCondition));
      }

      setInitialized(true);
    }
  }, [existingRule, initialized]);

  const updateMutation = useMutation({
    mutationFn: (data: any) => workflowsApi.updateAssignmentRule(ruleId, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['assignment-rules'] });
      queryClient.invalidateQueries({ queryKey: ['assignment-rule', ruleId] });
      toast.success('Assignment rule updated');
      router.push('/processing/assignment-rules');
    },
    onError: (error: Error) => {
      toast.error(error.message || 'Failed to update rule');
    },
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();

    let assign_to: any;
    switch (formData.assign_type) {
      case 'role':
        assign_to = { Role: formData.assign_value };
        break;
      case 'user':
        assign_to = { User: formData.assign_value };
        break;
      case 'round_robin':
        assign_to = { RoundRobin: formData.assign_value.split(',').map(s => s.trim()).filter(Boolean) };
        break;
      case 'least_loaded':
        assign_to = { LeastLoaded: formData.assign_value.split(',').map(s => s.trim()).filter(Boolean) };
        break;
      case 'vendor_approver':
        assign_to = 'VendorApprover';
        break;
      case 'department_approver':
        assign_to = 'DepartmentApprover';
        break;
    }

    const builtConditions = conditions
      .filter(c => {
        if (!operatorNeedsValue(c.operator)) return true;
        return c.value.trim() !== '';
      })
      .map(c => {
        let value: any;

        if (!operatorNeedsValue(c.operator)) {
          value = null;
        } else if (c.operator === 'between') {
          const v1 = c.field === 'amount' ? (parseInt(c.value) || 0) : c.value;
          const v2 = c.field === 'amount' ? (parseInt(c.value2) || 0) : c.value2;
          value = [v1, v2];
        } else if (c.operator === 'in' || c.operator === 'not_in') {
          const items = c.value.split(',').map(s => s.trim()).filter(Boolean);
          value = c.field === 'amount' ? items.map(s => parseInt(s) || 0) : items;
        } else if (c.field === 'amount') {
          value = parseInt(c.value) || 0;
        } else if (c.field === 'custom_field') {
          value = { field: c.customFieldName, value: c.value };
        } else {
          value = c.value;
        }

        return {
          field: c.field,
          operator: c.operator,
          value,
        };
      });

    updateMutation.mutate({
      name: formData.name,
      description: formData.description || undefined,
      queue_id: formData.queue_id,
      priority: formData.priority,
      conditions: builtConditions,
      assign_to,
    });
  };

  const addCondition = () => {
    setConditions([...conditions, { field: 'amount', operator: 'equals', value: '', value2: '', customFieldName: '' }]);
  };

  const removeCondition = (index: number) => {
    setConditions(conditions.filter((_, i) => i !== index));
  };

  const updateCondition = (index: number, key: keyof ConditionRow, value: string) => {
    setConditions(conditions.map((c, i) => {
      if (i !== index) return c;
      const updated = { ...c, [key]: value };

      if (key === 'field') {
        const validOps = getOperatorsForField(value);
        if (!validOps.find(op => op.value === updated.operator)) {
          updated.operator = validOps[0]?.value || 'equals';
        }
      }

      return updated;
    }));
  };

  const selectedAssignType = assignmentTypes.find(t => t.value === formData.assign_type);
  const isValid = formData.name.trim().length > 0
    && formData.queue_id.length > 0
    && (!selectedAssignType?.needsInput || formData.assign_value.trim().length > 0);

  const hasValidConditions = conditions.some(c =>
    !operatorNeedsValue(c.operator) || c.value.trim() !== ''
  );

  if (ruleLoading) {
    return (
      <div className="flex items-center justify-center py-20">
        <div className="flex items-center gap-3 text-muted-foreground">
          <Loader2 className="w-5 h-5 animate-spin" />
          Loading rule...
        </div>
      </div>
    );
  }

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
        <h1 className="text-2xl font-semibold text-foreground">Edit Assignment Rule</h1>
        <p className="text-muted-foreground mt-0.5">Modify conditions and assignment targets for this rule</p>
      </div>

      {/* Condition Preview */}
      {hasValidConditions && (
        <div className="card overflow-hidden">
          <button
            type="button"
            onClick={() => setShowPreview(!showPreview)}
            className="w-full p-4 flex items-center justify-between text-left hover:bg-secondary/30 transition-colors"
          >
            <div className="flex items-center gap-2">
              <Eye className="w-4 h-4 text-processing" />
              <span className="text-sm font-medium text-foreground">Rule Preview</span>
            </div>
            <span className="text-xs text-muted-foreground">{showPreview ? 'Hide' : 'Show'}</span>
          </button>
          {showPreview && (
            <div className="px-4 pb-4 space-y-2">
              <div className="p-3 bg-processing/5 border border-processing/20 rounded-lg text-sm">
                <p className="text-muted-foreground mb-1">When ALL of:</p>
                <ul className="space-y-1 ml-4">
                  {conditions.filter(c => !operatorNeedsValue(c.operator) || c.value.trim() !== '').map((c, i) => (
                    <li key={i} className="text-foreground font-medium">
                      {formatConditionPreview(c)}
                    </li>
                  ))}
                </ul>
                {formData.assign_type && (
                  <p className="text-muted-foreground mt-2">
                    Then assign to: <span className="text-foreground font-medium">
                      {selectedAssignType?.label}
                      {selectedAssignType?.needsInput && formData.assign_value && ` (${formData.assign_value})`}
                    </span>
                  </p>
                )}
              </div>
            </div>
          )}
        </div>
      )}

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
              <div>
                <h3 className="text-sm font-medium text-foreground">Conditions</h3>
                <p className="text-xs text-muted-foreground mt-0.5">All conditions must match (AND logic)</p>
              </div>
              <button type="button" onClick={addCondition} className="btn btn-secondary btn-sm">
                <Plus className="w-4 h-4 mr-1" />
                Add Condition
              </button>
            </div>
            <div className="space-y-3">
              {conditions.map((condition, index) => {
                const availableOperators = getOperatorsForField(condition.field);
                const needsValue = operatorNeedsValue(condition.operator);
                const isBetween = condition.operator === 'between';
                const isMultiValue = condition.operator === 'in' || condition.operator === 'not_in';
                const isCustomField = condition.field === 'custom_field';
                const isAmount = condition.field === 'amount';

                return (
                  <div key={index} className="p-3 bg-secondary/30 rounded-lg space-y-2">
                    {index > 0 && (
                      <div className="flex items-center gap-2 mb-1">
                        <span className="text-xs font-semibold text-processing uppercase tracking-wider">AND</span>
                        <div className="flex-1 h-px bg-border" />
                      </div>
                    )}
                    <div className="flex items-center gap-2">
                      {/* Field selector */}
                      <select
                        value={condition.field}
                        onChange={(e) => updateCondition(index, 'field', e.target.value)}
                        className="input flex-1"
                      >
                        {conditionFields.map(f => (
                          <option key={f.value} value={f.value}>{f.label}</option>
                        ))}
                      </select>
                      {/* Operator selector */}
                      <select
                        value={condition.operator}
                        onChange={(e) => updateCondition(index, 'operator', e.target.value)}
                        className="input flex-1"
                      >
                        {availableOperators.map(o => (
                          <option key={o.value} value={o.value}>{o.label}</option>
                        ))}
                      </select>
                      {conditions.length > 1 && (
                        <button
                          type="button"
                          onClick={() => removeCondition(index)}
                          className="p-2 text-muted-foreground hover:text-error transition-colors flex-shrink-0"
                        >
                          <Trash2 className="w-4 h-4" />
                        </button>
                      )}
                    </div>

                    {/* Custom field name input + help text (FB-19) */}
                    {isCustomField && (
                      <>
                        {needsValue && (
                          <input
                            type="text"
                            value={condition.customFieldName}
                            onChange={(e) => updateCondition(index, 'customFieldName', e.target.value)}
                            className="input"
                            placeholder="Custom field name (e.g., project, cost_center, region)"
                          />
                        )}
                        <div className="flex items-start gap-2 p-2.5 bg-primary/5 border border-primary/15 rounded-lg">
                          <Info className="w-4 h-4 text-primary mt-0.5 flex-shrink-0" />
                          <div className="text-xs text-muted-foreground">
                            <p className="font-medium text-foreground mb-1">About Custom Fields</p>
                            <p>
                              Custom fields match against metadata attached to invoices during capture or import.
                              Enter the exact field name as it appears in the invoice data.
                            </p>
                            <p className="mt-1">
                              <span className="font-medium">Common examples:</span>{' '}
                              project, cost_center, region, business_unit, contract_id
                            </p>
                          </div>
                        </div>
                      </>
                    )}

                    {/* Value inputs */}
                    {needsValue && (
                      <div className="flex items-center gap-2">
                        {isBetween ? (
                          <>
                            <input
                              type={isAmount ? 'number' : 'text'}
                              value={condition.value}
                              onChange={(e) => updateCondition(index, 'value', e.target.value)}
                              className="input flex-1"
                              placeholder={isAmount ? 'Min (cents)' : 'Min value'}
                            />
                            <span className="text-xs text-muted-foreground">and</span>
                            <input
                              type={isAmount ? 'number' : 'text'}
                              value={condition.value2}
                              onChange={(e) => updateCondition(index, 'value2', e.target.value)}
                              className="input flex-1"
                              placeholder={isAmount ? 'Max (cents)' : 'Max value'}
                            />
                          </>
                        ) : isMultiValue ? (
                          <input
                            type="text"
                            value={condition.value}
                            onChange={(e) => updateCondition(index, 'value', e.target.value)}
                            className="input flex-1"
                            placeholder="Comma-separated values (e.g., Engineering, Sales, Marketing)"
                          />
                        ) : (
                          <input
                            type={isAmount ? 'number' : 'text'}
                            value={condition.value}
                            onChange={(e) => updateCondition(index, 'value', e.target.value)}
                            className="input flex-1"
                            placeholder={
                              isAmount ? 'Amount in cents (e.g., 50000 = $500)' :
                              isCustomField ? 'Field value' :
                              'Value'
                            }
                          />
                        )}
                      </div>
                    )}
                  </div>
                );
              })}
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
                    onClick={() => setFormData({ ...formData, assign_type: type.value, assign_value: type.needsInput ? formData.assign_value : '' })}
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
            {selectedAssignType?.needsInput && (
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
            )}
          </div>

          {/* Actions */}
          <div className="flex items-center justify-end gap-3 pt-4 border-t border-border">
            <Link href="/processing/assignment-rules" className="btn btn-secondary">
              Cancel
            </Link>
            <button
              type="submit"
              disabled={!isValid || updateMutation.isPending}
              className="btn bg-processing text-processing-foreground hover:bg-processing/90 disabled:opacity-50"
            >
              {updateMutation.isPending ? (
                <>
                  <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                  Saving...
                </>
              ) : (
                <>
                  <CheckCircle className="w-4 h-4 mr-2" />
                  Save Changes
                </>
              )}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
