'use client';

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import Link from 'next/link';
import { workflowsApi } from '@/lib/api';
import type { WorkflowTemplate } from '@/lib/api';
import {
  Plus,
  ArrowLeft,
  Workflow,
  ChevronRight,
  Trash2,
  Power,
  PowerOff,
  Star,
  Clock,
  CheckCircle,
  AlertCircle,
  CreditCard,
  Layers,
  Search,
  ArrowDownUp,
} from 'lucide-react';
import { useState } from 'react';
import { toast } from 'sonner';

const stageTypeConfig: Record<string, { icon: typeof Clock; color: string; bg: string }> = {
  intake: { icon: ArrowDownUp, color: 'text-muted-foreground', bg: 'bg-secondary' },
  review: { icon: Clock, color: 'text-processing', bg: 'bg-processing/10' },
  approval: { icon: CheckCircle, color: 'text-warning', bg: 'bg-warning/10' },
  exception: { icon: AlertCircle, color: 'text-error', bg: 'bg-error/10' },
  payment: { icon: CreditCard, color: 'text-success', bg: 'bg-success/10' },
  custom: { icon: Layers, color: 'text-vendor', bg: 'bg-vendor/10' },
};

export default function WorkflowTemplatesPage() {
  const queryClient = useQueryClient();
  const [searchQuery, setSearchQuery] = useState('');

  const { data: templates, isLoading } = useQuery({
    queryKey: ['workflow-templates'],
    queryFn: () => workflowsApi.listTemplates(),
  });

  const deleteTemplate = useMutation({
    mutationFn: (id: string) => workflowsApi.deleteTemplate(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['workflow-templates'] });
      toast.success('Workflow template deleted');
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to delete template');
    },
  });

  const toggleActive = useMutation({
    mutationFn: ({ id, activate }: { id: string; activate: boolean }) =>
      activate ? workflowsApi.activateTemplate(id) : workflowsApi.deactivateTemplate(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['workflow-templates'] });
      toast.success('Template status updated');
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to update template');
    },
  });

  const filteredTemplates = templates?.filter((t: WorkflowTemplate) =>
    !searchQuery || t.name.toLowerCase().includes(searchQuery.toLowerCase())
  );

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
            <h1 className="text-2xl font-semibold text-foreground">Workflow Templates</h1>
            <p className="text-muted-foreground mt-0.5">
              Define multi-step invoice processing pipelines
            </p>
          </div>
          <Link href="/processing/workflows/new" className="btn btn-primary btn-sm">
            <Plus className="w-4 h-4 mr-1.5" />
            Create Template
          </Link>
        </div>
      </div>

      {/* Explanation Card */}
      <div className="card overflow-hidden">
        <div className="h-1 bg-gradient-to-r from-processing to-processing/50" />
        <div className="p-6">
          <div className="flex items-center gap-3 mb-4">
            <div className="p-2 rounded-lg bg-processing/10">
              <Workflow className="w-5 h-5 text-processing" />
            </div>
            <div>
              <h3 className="font-semibold text-foreground">How Workflow Templates Work</h3>
              <p className="text-sm text-muted-foreground">
                Templates define the sequence of stages invoices pass through, from intake to payment. Each stage can have SLA timers, auto-advance rules, and escalation policies.
              </p>
            </div>
          </div>
          <div className="flex items-center gap-2 text-xs">
            {['Intake', 'Review', 'Approval', 'Payment'].map((stage, i) => (
              <div key={stage} className="flex items-center gap-2">
                <span className="px-3 py-1.5 rounded-lg bg-secondary font-medium text-foreground">
                  {stage}
                </span>
                {i < 3 && <ChevronRight className="w-4 h-4 text-muted-foreground" />}
              </div>
            ))}
          </div>
        </div>
      </div>

      {/* Search */}
      <div className="card p-4">
        <div className="relative">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
          <input
            type="text"
            placeholder="Search templates..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="input pl-9"
          />
        </div>
      </div>

      {/* Templates List */}
      {isLoading ? (
        <div className="space-y-4">
          {[1, 2].map((i) => (
            <div key={i} className="card p-6 animate-pulse">
              <div className="h-6 bg-secondary rounded w-1/4 mb-4" />
              <div className="h-4 bg-secondary rounded w-3/4" />
            </div>
          ))}
        </div>
      ) : !filteredTemplates || filteredTemplates.length === 0 ? (
        <div className="card p-12 text-center">
          <div className="w-14 h-14 rounded-xl bg-processing/10 flex items-center justify-center mx-auto mb-4">
            <Workflow className="w-7 h-7 text-processing" />
          </div>
          <h3 className="text-lg font-semibold text-foreground mb-2">No Workflow Templates</h3>
          <p className="text-muted-foreground mb-4 max-w-sm mx-auto">
            Create your first workflow template to define how invoices flow through your organization.
          </p>
          <Link href="/processing/workflows/new" className="btn btn-primary btn-sm inline-flex">
            <Plus className="w-4 h-4 mr-1.5" />
            Create your first template
          </Link>
        </div>
      ) : (
        <div className="space-y-4">
          {filteredTemplates.map((template: WorkflowTemplate) => (
            <div key={template.id} className="card overflow-hidden hover:shadow-md transition-shadow">
              <div className="p-5">
                <div className="flex items-start justify-between mb-4">
                  <div className="flex items-center gap-3">
                    <div className="p-2 rounded-lg bg-processing/10">
                      <Workflow className="w-5 h-5 text-processing" />
                    </div>
                    <div>
                      <div className="flex items-center gap-2">
                        <h3 className="font-semibold text-foreground">{template.name}</h3>
                        {template.is_default && (
                          <span className="flex items-center gap-1 px-2 py-0.5 text-xs font-medium bg-warning/10 text-warning rounded-full">
                            <Star className="w-3 h-3" />
                            Default
                          </span>
                        )}
                        <span className={`px-2 py-0.5 text-xs font-medium rounded-full ${
                          template.is_active
                            ? 'bg-success/10 text-success'
                            : 'bg-secondary text-muted-foreground'
                        }`}>
                          {template.is_active ? 'Active' : 'Inactive'}
                        </span>
                      </div>
                      <p className="text-sm text-muted-foreground mt-0.5">
                        {template.description || `${template.stages.length} stage pipeline`}
                      </p>
                    </div>
                  </div>
                  <div className="flex items-center gap-1">
                    <button
                      onClick={() => toggleActive.mutate({
                        id: template.id,
                        activate: !template.is_active,
                      })}
                      className={`p-2 rounded-lg transition-colors ${
                        template.is_active
                          ? 'text-muted-foreground hover:text-warning hover:bg-warning/10'
                          : 'text-muted-foreground hover:text-success hover:bg-success/10'
                      }`}
                      title={template.is_active ? 'Deactivate' : 'Activate'}
                    >
                      {template.is_active ? <PowerOff className="w-4 h-4" /> : <Power className="w-4 h-4" />}
                    </button>
                    <button
                      onClick={() => {
                        if (confirm('Delete this workflow template?')) {
                          deleteTemplate.mutate(template.id);
                        }
                      }}
                      className="p-2 text-muted-foreground hover:text-error rounded-lg hover:bg-error/10 transition-colors"
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </div>
                </div>

                {/* Pipeline visualization */}
                <div className="flex items-center gap-1.5 overflow-x-auto pb-1">
                  {template.stages
                    .sort((a, b) => a.order - b.order)
                    .map((stage, idx) => {
                      const config = stageTypeConfig[stage.stage_type] || stageTypeConfig.custom;
                      const StageIcon = config.icon;
                      return (
                        <div key={idx} className="flex items-center gap-1.5 flex-shrink-0">
                          <div className={`flex items-center gap-1.5 px-3 py-1.5 rounded-lg ${config.bg} border border-transparent`}>
                            <StageIcon className={`w-3.5 h-3.5 ${config.color}`} />
                            <span className={`text-xs font-medium ${config.color}`}>{stage.name}</span>
                            {stage.sla_hours && (
                              <span className="text-[10px] text-muted-foreground ml-1">
                                {stage.sla_hours}h
                              </span>
                            )}
                          </div>
                          {idx < template.stages.length - 1 && (
                            <ChevronRight className="w-3.5 h-3.5 text-muted-foreground flex-shrink-0" />
                          )}
                        </div>
                      );
                    })}
                </div>

                {/* Stats row */}
                <div className="flex items-center gap-4 mt-3 pt-3 border-t border-border text-xs text-muted-foreground">
                  <span>{template.stages.length} stages</span>
                  <span>
                    {template.stages.filter(s => s.requires_action).length} requiring action
                  </span>
                  {template.stages.some(s => s.sla_hours) && (
                    <span>
                      Total SLA: {template.stages.reduce((sum, s) => sum + (s.sla_hours || 0), 0)}h
                    </span>
                  )}
                  <span className="ml-auto">
                    Updated {new Date(template.updated_at).toLocaleDateString()}
                  </span>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
