'use client';

import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRouter } from 'next/navigation';
import { workflowsApi } from '@/lib/api';
import type { WorkflowTemplateStage, RuleCondition } from '@/lib/api';
import {
  ArrowLeft,
  Plus,
  Trash2,
  GripVertical,
  ChevronRight,
  ChevronDown,
  ChevronUp,
  Workflow,
  Clock,
  CheckCircle,
  AlertCircle,
  CreditCard,
  Layers,
  ArrowDownUp,
  Save,
  Zap,
  Shield,
  Timer,
  Eye,
} from 'lucide-react';
import Link from 'next/link';
import { toast } from 'sonner';

const stageTypes = [
  { value: 'intake', label: 'Intake', icon: ArrowDownUp, color: 'text-muted-foreground', bg: 'bg-secondary', description: 'OCR processing and data capture' },
  { value: 'review', label: 'Review', icon: Clock, color: 'text-processing', bg: 'bg-processing/10', description: 'AP staff review and verification' },
  { value: 'approval', label: 'Approval', icon: CheckCircle, color: 'text-warning', bg: 'bg-warning/10', description: 'Manager or role-based approval' },
  { value: 'exception', label: 'Exception', icon: AlertCircle, color: 'text-error', bg: 'bg-error/10', description: 'Error handling and resolution' },
  { value: 'payment', label: 'Payment', icon: CreditCard, color: 'text-success', bg: 'bg-success/10', description: 'Payment processing and submission' },
  { value: 'custom', label: 'Custom', icon: Layers, color: 'text-vendor', bg: 'bg-vendor/10', description: 'Custom workflow stage' },
] as const;

const conditionFields = [
  { value: 'amount', label: 'Amount', type: 'numeric' },
  { value: 'vendor_id', label: 'Vendor ID', type: 'string' },
  { value: 'vendor_name', label: 'Vendor Name', type: 'string' },
  { value: 'department', label: 'Department', type: 'string' },
  { value: 'gl_code', label: 'GL Code', type: 'string' },
  { value: 'invoice_date', label: 'Invoice Date', type: 'date' },
  { value: 'due_date', label: 'Due Date', type: 'date' },
  { value: 'tag', label: 'Tag', type: 'string' },
];

const conditionOperators = [
  { value: 'equals', label: 'equals' },
  { value: 'not_equals', label: 'does not equal' },
  { value: 'greater_than', label: 'is greater than' },
  { value: 'less_than', label: 'is less than' },
  { value: 'contains', label: 'contains' },
  { value: 'is_null', label: 'is empty' },
  { value: 'is_not_null', label: 'is not empty' },
];

interface StageForm {
  name: string;
  stage_type: string;
  queue_id: string;
  sla_hours: string;
  escalation_hours: string;
  requires_action: boolean;
  skip_conditions: ConditionForm[];
  auto_advance_conditions: ConditionForm[];
}

interface ConditionForm {
  field: string;
  operator: string;
  value: string;
}

function createEmptyStage(order: number): StageForm {
  return {
    name: '',
    stage_type: 'review',
    queue_id: '',
    sla_hours: '',
    escalation_hours: '',
    requires_action: true,
    skip_conditions: [],
    auto_advance_conditions: [],
  };
}

// Pre-built template definitions
const prebuiltTemplates = {
  standard_ap: {
    name: 'Standard AP',
    description: 'Standard accounts payable processing: intake, review, approval, and payment.',
    stages: [
      { name: 'OCR & Data Capture', stage_type: 'intake', sla_hours: '4', escalation_hours: '', requires_action: false, queue_id: '', skip_conditions: [], auto_advance_conditions: [] },
      { name: 'AP Review', stage_type: 'review', sla_hours: '24', escalation_hours: '48', requires_action: true, queue_id: '', skip_conditions: [], auto_advance_conditions: [] },
      { name: 'Manager Approval', stage_type: 'approval', sla_hours: '48', escalation_hours: '72', requires_action: true, queue_id: '', skip_conditions: [{ field: 'amount', operator: 'less_than', value: '10000' }], auto_advance_conditions: [] },
      { name: 'Ready for Payment', stage_type: 'payment', sla_hours: '', escalation_hours: '', requires_action: false, queue_id: '', skip_conditions: [], auto_advance_conditions: [] },
    ],
  },
  high_value: {
    name: 'High-Value Review',
    description: 'Enhanced review for high-value invoices with multi-level approval.',
    stages: [
      { name: 'OCR & Data Capture', stage_type: 'intake', sla_hours: '4', escalation_hours: '', requires_action: false, queue_id: '', skip_conditions: [], auto_advance_conditions: [] },
      { name: 'AP Review', stage_type: 'review', sla_hours: '24', escalation_hours: '48', requires_action: true, queue_id: '', skip_conditions: [], auto_advance_conditions: [] },
      { name: 'Manager Approval', stage_type: 'approval', sla_hours: '48', escalation_hours: '72', requires_action: true, queue_id: '', skip_conditions: [], auto_advance_conditions: [] },
      { name: 'VP Approval', stage_type: 'approval', sla_hours: '72', escalation_hours: '96', requires_action: true, queue_id: '', skip_conditions: [{ field: 'amount', operator: 'less_than', value: '100000' }], auto_advance_conditions: [] },
      { name: 'Ready for Payment', stage_type: 'payment', sla_hours: '', escalation_hours: '', requires_action: false, queue_id: '', skip_conditions: [], auto_advance_conditions: [] },
    ],
  },
  exception_handling: {
    name: 'Exception Handling',
    description: 'Pipeline for invoices that failed OCR or have data quality issues.',
    stages: [
      { name: 'Error Triage', stage_type: 'exception', sla_hours: '8', escalation_hours: '24', requires_action: true, queue_id: '', skip_conditions: [], auto_advance_conditions: [] },
      { name: 'Manual Data Entry', stage_type: 'review', sla_hours: '24', escalation_hours: '48', requires_action: true, queue_id: '', skip_conditions: [], auto_advance_conditions: [] },
      { name: 'Verification', stage_type: 'review', sla_hours: '12', escalation_hours: '', requires_action: true, queue_id: '', skip_conditions: [], auto_advance_conditions: [] },
      { name: 'Return to Normal Flow', stage_type: 'custom', sla_hours: '', escalation_hours: '', requires_action: false, queue_id: '', skip_conditions: [], auto_advance_conditions: [] },
    ],
  },
};

export default function NewWorkflowTemplatePage() {
  const router = useRouter();
  const queryClient = useQueryClient();

  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [isDefault, setIsDefault] = useState(false);
  const [stages, setStages] = useState<StageForm[]>([createEmptyStage(0)]);
  const [expandedStage, setExpandedStage] = useState<number | null>(0);
  const [showPreview, setShowPreview] = useState(true);

  const { data: queues } = useQuery({
    queryKey: ['work-queues'],
    queryFn: () => workflowsApi.listQueues(),
  });

  const createTemplate = useMutation({
    mutationFn: (data: any) => workflowsApi.createTemplate(data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['workflow-templates'] });
      toast.success('Workflow template created');
      router.push('/processing/workflows');
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to create template');
    },
  });

  const addStage = () => {
    setStages([...stages, createEmptyStage(stages.length)]);
    setExpandedStage(stages.length);
  };

  const removeStage = (index: number) => {
    if (stages.length <= 1) return;
    setStages(stages.filter((_, i) => i !== index));
    if (expandedStage === index) {
      setExpandedStage(null);
    } else if (expandedStage !== null && expandedStage > index) {
      setExpandedStage(expandedStage - 1);
    }
  };

  const moveStage = (index: number, direction: 'up' | 'down') => {
    const newIndex = direction === 'up' ? index - 1 : index + 1;
    if (newIndex < 0 || newIndex >= stages.length) return;
    const newStages = [...stages];
    [newStages[index], newStages[newIndex]] = [newStages[newIndex], newStages[index]];
    setStages(newStages);
    setExpandedStage(newIndex);
  };

  const updateStage = (index: number, updates: Partial<StageForm>) => {
    setStages(stages.map((s, i) => i === index ? { ...s, ...updates } : s));
  };

  const addCondition = (stageIndex: number, type: 'skip' | 'auto_advance') => {
    const key = type === 'skip' ? 'skip_conditions' : 'auto_advance_conditions';
    const stage = stages[stageIndex];
    updateStage(stageIndex, {
      [key]: [...stage[key], { field: 'amount', operator: 'greater_than', value: '' }],
    });
  };

  const removeCondition = (stageIndex: number, type: 'skip' | 'auto_advance', condIndex: number) => {
    const key = type === 'skip' ? 'skip_conditions' : 'auto_advance_conditions';
    const stage = stages[stageIndex];
    updateStage(stageIndex, {
      [key]: stage[key].filter((_, i) => i !== condIndex),
    });
  };

  const updateCondition = (stageIndex: number, type: 'skip' | 'auto_advance', condIndex: number, updates: Partial<ConditionForm>) => {
    const key = type === 'skip' ? 'skip_conditions' : 'auto_advance_conditions';
    const stage = stages[stageIndex];
    updateStage(stageIndex, {
      [key]: stage[key].map((c, i) => i === condIndex ? { ...c, ...updates } : c),
    });
  };

  const loadPrebuilt = (templateKey: keyof typeof prebuiltTemplates) => {
    const template = prebuiltTemplates[templateKey];
    setName(template.name);
    setDescription(template.description);
    setStages(template.stages);
    setExpandedStage(null);
    toast.success(`Loaded "${template.name}" template`);
  };

  const buildConditions = (conditions: ConditionForm[]): RuleCondition[] => {
    return conditions
      .filter(c => c.field && c.operator)
      .map(c => {
        let value: unknown = c.value;
        if (c.field === 'amount' && c.value) {
          value = parseInt(c.value, 10);
        }
        if (c.operator === 'is_null' || c.operator === 'is_not_null') {
          value = null;
        }
        return { field: c.field, operator: c.operator, value };
      });
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();

    if (!name.trim()) {
      toast.error('Template name is required');
      return;
    }
    if (stages.length === 0) {
      toast.error('At least one stage is required');
      return;
    }
    if (stages.some(s => !s.name.trim())) {
      toast.error('All stages must have a name');
      return;
    }

    const templateStages: WorkflowTemplateStage[] = stages.map((s, i) => ({
      order: i,
      name: s.name,
      stage_type: s.stage_type as WorkflowTemplateStage['stage_type'],
      queue_id: s.queue_id || undefined,
      sla_hours: s.sla_hours ? parseInt(s.sla_hours, 10) : undefined,
      escalation_hours: s.escalation_hours ? parseInt(s.escalation_hours, 10) : undefined,
      requires_action: s.requires_action,
      skip_conditions: buildConditions(s.skip_conditions),
      auto_advance_conditions: buildConditions(s.auto_advance_conditions),
    }));

    createTemplate.mutate({
      name: name.trim(),
      description: description.trim() || undefined,
      is_default: isDefault,
      stages: templateStages,
    });
  };

  const getStageTypeConfig = (type: string) => {
    return stageTypes.find(s => s.value === type) || stageTypes[5];
  };

  return (
    <div className="space-y-6 max-w-5xl mx-auto">
      {/* Header */}
      <div>
        <Link
          href="/processing/workflows"
          className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors mb-3"
        >
          <ArrowLeft className="w-4 h-4" />
          Back to Workflow Templates
        </Link>
        <h1 className="text-2xl font-semibold text-foreground">Create Workflow Template</h1>
        <p className="text-muted-foreground mt-0.5">
          Design a multi-step pipeline for invoice processing
        </p>
      </div>

      {/* Quick-start templates */}
      <div className="card overflow-hidden">
        <div className="h-1 bg-gradient-to-r from-processing to-vendor" />
        <div className="p-5">
          <h3 className="font-semibold text-foreground mb-3">Start from a Template</h3>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
            <button
              type="button"
              onClick={() => loadPrebuilt('standard_ap')}
              className="p-3 bg-secondary/50 hover:bg-secondary rounded-xl text-left transition-colors"
            >
              <div className="flex items-center gap-2 mb-1">
                <Workflow className="w-4 h-4 text-processing" />
                <span className="font-medium text-foreground text-sm">Standard AP</span>
              </div>
              <p className="text-xs text-muted-foreground">4-stage pipeline: intake, review, approval, payment</p>
            </button>
            <button
              type="button"
              onClick={() => loadPrebuilt('high_value')}
              className="p-3 bg-secondary/50 hover:bg-secondary rounded-xl text-left transition-colors"
            >
              <div className="flex items-center gap-2 mb-1">
                <Shield className="w-4 h-4 text-warning" />
                <span className="font-medium text-foreground text-sm">High-Value Review</span>
              </div>
              <p className="text-xs text-muted-foreground">5-stage with multi-level approval (manager + VP)</p>
            </button>
            <button
              type="button"
              onClick={() => loadPrebuilt('exception_handling')}
              className="p-3 bg-secondary/50 hover:bg-secondary rounded-xl text-left transition-colors"
            >
              <div className="flex items-center gap-2 mb-1">
                <AlertCircle className="w-4 h-4 text-error" />
                <span className="font-medium text-foreground text-sm">Exception Handling</span>
              </div>
              <p className="text-xs text-muted-foreground">Error triage, manual entry, and verification</p>
            </button>
          </div>
        </div>
      </div>

      <form onSubmit={handleSubmit} className="space-y-6">
        {/* Template Info */}
        <div className="card p-5 space-y-4">
          <h3 className="font-semibold text-foreground flex items-center gap-2">
            <Workflow className="w-4 h-4 text-processing" />
            Template Details
          </h3>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div>
              <label className="block text-sm font-medium text-foreground mb-1.5">
                Template Name <span className="text-error">*</span>
              </label>
              <input
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="e.g., Standard AP Processing"
                className="input"
                required
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-foreground mb-1.5">Description</label>
              <input
                type="text"
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder="Brief description of this workflow"
                className="input"
              />
            </div>
          </div>
          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="checkbox"
              checked={isDefault}
              onChange={(e) => setIsDefault(e.target.checked)}
              className="rounded border-border"
            />
            <span className="text-sm text-foreground">Set as default template for new invoices</span>
          </label>
        </div>

        {/* Pipeline Preview */}
        {showPreview && stages.some(s => s.name) && (
          <div className="card p-5">
            <div className="flex items-center justify-between mb-3">
              <h3 className="font-semibold text-foreground flex items-center gap-2">
                <Eye className="w-4 h-4 text-processing" />
                Pipeline Preview
              </h3>
              <button
                type="button"
                onClick={() => setShowPreview(false)}
                className="text-xs text-muted-foreground hover:text-foreground"
              >
                Hide
              </button>
            </div>
            <div className="flex items-center gap-1.5 overflow-x-auto pb-1">
              {stages.filter(s => s.name).map((stage, idx) => {
                const config = getStageTypeConfig(stage.stage_type);
                const Icon = config.icon;
                return (
                  <div key={idx} className="flex items-center gap-1.5 flex-shrink-0">
                    <div className={`flex items-center gap-1.5 px-3 py-2 rounded-lg ${config.bg} border border-transparent`}>
                      <Icon className={`w-3.5 h-3.5 ${config.color}`} />
                      <span className={`text-xs font-medium ${config.color}`}>{stage.name}</span>
                      {stage.sla_hours && (
                        <span className="text-[10px] text-muted-foreground ml-1">
                          {stage.sla_hours}h SLA
                        </span>
                      )}
                      {stage.skip_conditions.length > 0 && (
                        <span title="Has skip conditions">
                          <Zap className="w-3 h-3 text-warning ml-0.5" />
                        </span>
                      )}
                    </div>
                    {idx < stages.filter(s => s.name).length - 1 && (
                      <ChevronRight className="w-3.5 h-3.5 text-muted-foreground flex-shrink-0" />
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        )}

        {/* Stages */}
        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <h3 className="font-semibold text-foreground">Pipeline Stages</h3>
            <button
              type="button"
              onClick={addStage}
              className="btn btn-secondary btn-sm"
            >
              <Plus className="w-4 h-4 mr-1" />
              Add Stage
            </button>
          </div>

          {stages.map((stage, index) => {
            const isExpanded = expandedStage === index;
            const config = getStageTypeConfig(stage.stage_type);
            const Icon = config.icon;

            return (
              <div key={index} className="card overflow-hidden">
                {/* Stage header */}
                <div
                  className="flex items-center gap-3 p-4 cursor-pointer hover:bg-secondary/30 transition-colors"
                  onClick={() => setExpandedStage(isExpanded ? null : index)}
                >
                  <div className="flex flex-col gap-0.5">
                    <button
                      type="button"
                      onClick={(e) => { e.stopPropagation(); moveStage(index, 'up'); }}
                      disabled={index === 0}
                      className="p-0.5 text-muted-foreground hover:text-foreground disabled:opacity-30"
                    >
                      <ChevronUp className="w-3.5 h-3.5" />
                    </button>
                    <button
                      type="button"
                      onClick={(e) => { e.stopPropagation(); moveStage(index, 'down'); }}
                      disabled={index === stages.length - 1}
                      className="p-0.5 text-muted-foreground hover:text-foreground disabled:opacity-30"
                    >
                      <ChevronDown className="w-3.5 h-3.5" />
                    </button>
                  </div>

                  <span className="text-xs font-mono text-muted-foreground w-6 text-center">{index + 1}</span>

                  <div className={`p-1.5 rounded-lg ${config.bg}`}>
                    <Icon className={`w-4 h-4 ${config.color}`} />
                  </div>

                  <div className="flex-1 min-w-0">
                    <p className="font-medium text-foreground truncate">
                      {stage.name || <span className="text-muted-foreground italic">Unnamed stage</span>}
                    </p>
                    <p className="text-xs text-muted-foreground">
                      {config.label}
                      {stage.sla_hours && ` - ${stage.sla_hours}h SLA`}
                      {stage.skip_conditions.length > 0 && ` - ${stage.skip_conditions.length} skip condition(s)`}
                      {stage.auto_advance_conditions.length > 0 && ` - ${stage.auto_advance_conditions.length} auto-advance rule(s)`}
                    </p>
                  </div>

                  <div className="flex items-center gap-1">
                    {stages.length > 1 && (
                      <button
                        type="button"
                        onClick={(e) => { e.stopPropagation(); removeStage(index); }}
                        className="p-1.5 text-muted-foreground hover:text-error rounded-lg hover:bg-error/10 transition-colors"
                      >
                        <Trash2 className="w-4 h-4" />
                      </button>
                    )}
                    {isExpanded ? (
                      <ChevronUp className="w-5 h-5 text-muted-foreground" />
                    ) : (
                      <ChevronDown className="w-5 h-5 text-muted-foreground" />
                    )}
                  </div>
                </div>

                {/* Stage details (expanded) */}
                {isExpanded && (
                  <div className="border-t border-border p-5 space-y-5 animate-scale-in">
                    {/* Basic info */}
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                      <div>
                        <label className="block text-sm font-medium text-foreground mb-1.5">
                          Stage Name <span className="text-error">*</span>
                        </label>
                        <input
                          type="text"
                          value={stage.name}
                          onChange={(e) => updateStage(index, { name: e.target.value })}
                          placeholder="e.g., AP Review"
                          className="input"
                        />
                      </div>
                      <div>
                        <label className="block text-sm font-medium text-foreground mb-1.5">Stage Type</label>
                        <select
                          value={stage.stage_type}
                          onChange={(e) => updateStage(index, { stage_type: e.target.value })}
                          className="input"
                        >
                          {stageTypes.map((t) => (
                            <option key={t.value} value={t.value}>{t.label} - {t.description}</option>
                          ))}
                        </select>
                      </div>
                    </div>

                    {/* Link to queue */}
                    <div>
                      <label className="block text-sm font-medium text-foreground mb-1.5">
                        Linked Queue (optional)
                      </label>
                      <select
                        value={stage.queue_id}
                        onChange={(e) => updateStage(index, { queue_id: e.target.value })}
                        className="input"
                      >
                        <option value="">No linked queue - create automatically</option>
                        {queues?.map((q: any) => (
                          <option key={q.id} value={q.id}>{q.name} ({q.queue_type})</option>
                        ))}
                      </select>
                      <p className="text-xs text-muted-foreground mt-1">
                        Link this stage to an existing work queue, or leave empty to auto-create one.
                      </p>
                    </div>

                    {/* SLA & Escalation */}
                    <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                      <div>
                        <label className="block text-sm font-medium text-foreground mb-1.5 flex items-center gap-1.5">
                          <Timer className="w-3.5 h-3.5 text-processing" />
                          SLA Hours
                        </label>
                        <input
                          type="number"
                          min="0"
                          value={stage.sla_hours}
                          onChange={(e) => updateStage(index, { sla_hours: e.target.value })}
                          placeholder="e.g., 24"
                          className="input"
                        />
                      </div>
                      <div>
                        <label className="block text-sm font-medium text-foreground mb-1.5 flex items-center gap-1.5">
                          <AlertCircle className="w-3.5 h-3.5 text-warning" />
                          Escalation Hours
                        </label>
                        <input
                          type="number"
                          min="0"
                          value={stage.escalation_hours}
                          onChange={(e) => updateStage(index, { escalation_hours: e.target.value })}
                          placeholder="e.g., 48"
                          className="input"
                        />
                        <p className="text-xs text-muted-foreground mt-1">Auto-escalate if not actioned</p>
                      </div>
                      <div>
                        <label className="block text-sm font-medium text-foreground mb-1.5">
                          Requires Action
                        </label>
                        <label className="flex items-center gap-2 mt-2 cursor-pointer">
                          <input
                            type="checkbox"
                            checked={stage.requires_action}
                            onChange={(e) => updateStage(index, { requires_action: e.target.checked })}
                            className="rounded border-border"
                          />
                          <span className="text-sm text-foreground">User must take action to advance</span>
                        </label>
                      </div>
                    </div>

                    {/* Skip Conditions */}
                    <div className="border-t border-border pt-4">
                      <div className="flex items-center justify-between mb-3">
                        <div>
                          <h4 className="text-sm font-medium text-foreground flex items-center gap-1.5">
                            <Zap className="w-3.5 h-3.5 text-warning" />
                            Skip Conditions
                          </h4>
                          <p className="text-xs text-muted-foreground">
                            Invoices matching these conditions will skip this stage entirely
                          </p>
                        </div>
                        <button
                          type="button"
                          onClick={() => addCondition(index, 'skip')}
                          className="btn btn-secondary btn-xs"
                        >
                          <Plus className="w-3.5 h-3.5 mr-1" />
                          Add Condition
                        </button>
                      </div>
                      {stage.skip_conditions.map((cond, ci) => (
                        <div key={ci} className="flex items-center gap-2 mb-2">
                          <select
                            value={cond.field}
                            onChange={(e) => updateCondition(index, 'skip', ci, { field: e.target.value })}
                            className="input text-sm flex-1"
                          >
                            {conditionFields.map((f) => (
                              <option key={f.value} value={f.value}>{f.label}</option>
                            ))}
                          </select>
                          <select
                            value={cond.operator}
                            onChange={(e) => updateCondition(index, 'skip', ci, { operator: e.target.value })}
                            className="input text-sm flex-1"
                          >
                            {conditionOperators.map((o) => (
                              <option key={o.value} value={o.value}>{o.label}</option>
                            ))}
                          </select>
                          {cond.operator !== 'is_null' && cond.operator !== 'is_not_null' && (
                            <input
                              type="text"
                              value={cond.value}
                              onChange={(e) => updateCondition(index, 'skip', ci, { value: e.target.value })}
                              placeholder="Value"
                              className="input text-sm flex-1"
                            />
                          )}
                          <button
                            type="button"
                            onClick={() => removeCondition(index, 'skip', ci)}
                            className="p-1.5 text-muted-foreground hover:text-error rounded-lg hover:bg-error/10"
                          >
                            <Trash2 className="w-3.5 h-3.5" />
                          </button>
                        </div>
                      ))}
                    </div>

                    {/* Auto-advance Conditions */}
                    <div className="border-t border-border pt-4">
                      <div className="flex items-center justify-between mb-3">
                        <div>
                          <h4 className="text-sm font-medium text-foreground flex items-center gap-1.5">
                            <ChevronRight className="w-3.5 h-3.5 text-success" />
                            Auto-Advance Conditions
                          </h4>
                          <p className="text-xs text-muted-foreground">
                            Invoices matching these conditions will automatically advance to the next stage
                          </p>
                        </div>
                        <button
                          type="button"
                          onClick={() => addCondition(index, 'auto_advance')}
                          className="btn btn-secondary btn-xs"
                        >
                          <Plus className="w-3.5 h-3.5 mr-1" />
                          Add Condition
                        </button>
                      </div>
                      {stage.auto_advance_conditions.map((cond, ci) => (
                        <div key={ci} className="flex items-center gap-2 mb-2">
                          <select
                            value={cond.field}
                            onChange={(e) => updateCondition(index, 'auto_advance', ci, { field: e.target.value })}
                            className="input text-sm flex-1"
                          >
                            {conditionFields.map((f) => (
                              <option key={f.value} value={f.value}>{f.label}</option>
                            ))}
                          </select>
                          <select
                            value={cond.operator}
                            onChange={(e) => updateCondition(index, 'auto_advance', ci, { operator: e.target.value })}
                            className="input text-sm flex-1"
                          >
                            {conditionOperators.map((o) => (
                              <option key={o.value} value={o.value}>{o.label}</option>
                            ))}
                          </select>
                          {cond.operator !== 'is_null' && cond.operator !== 'is_not_null' && (
                            <input
                              type="text"
                              value={cond.value}
                              onChange={(e) => updateCondition(index, 'auto_advance', ci, { value: e.target.value })}
                              placeholder="Value"
                              className="input text-sm flex-1"
                            />
                          )}
                          <button
                            type="button"
                            onClick={() => removeCondition(index, 'auto_advance', ci)}
                            className="p-1.5 text-muted-foreground hover:text-error rounded-lg hover:bg-error/10"
                          >
                            <Trash2 className="w-3.5 h-3.5" />
                          </button>
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            );
          })}

          {/* Add stage button (below list) */}
          <button
            type="button"
            onClick={addStage}
            className="w-full card p-4 border-2 border-dashed border-border hover:border-processing/50 hover:bg-processing/5 transition-colors flex items-center justify-center gap-2 text-muted-foreground hover:text-processing"
          >
            <Plus className="w-4 h-4" />
            <span className="text-sm font-medium">Add another stage</span>
          </button>
        </div>

        {/* Submit */}
        <div className="flex items-center justify-between pt-4 border-t border-border">
          <Link
            href="/processing/workflows"
            className="btn btn-secondary"
          >
            Cancel
          </Link>
          <button
            type="submit"
            disabled={createTemplate.isPending}
            className="btn btn-primary"
          >
            <Save className="w-4 h-4 mr-1.5" />
            {createTemplate.isPending ? 'Creating...' : 'Create Template'}
          </button>
        </div>
      </form>
    </div>
  );
}
