'use client';

import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { workflowsApi } from '@/lib/api';
import {
  Plus,
  Trash2,
  Edit,
  ChevronDown,
  ChevronUp,
  GitBranch,
  User,
  Users,
  Building,
  Briefcase,
  DollarSign,
  Tag,
  ArrowLeft,
  CheckCircle,
  Workflow,
  Search,
  Filter,
} from 'lucide-react';
import { toast } from 'sonner';
import Link from 'next/link';

export default function AssignmentRulesPage() {
  const queryClient = useQueryClient();
  const [expandedRule, setExpandedRule] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState('');

  const { data: rules, isLoading: rulesLoading } = useQuery({
    queryKey: ['assignment-rules'],
    queryFn: () => workflowsApi.listAssignmentRules(),
  });

  const { data: queues } = useQuery({
    queryKey: ['queues'],
    queryFn: () => workflowsApi.listQueues(),
  });

  const deleteRule = useMutation({
    mutationFn: (id: string) => workflowsApi.deleteAssignmentRule(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['assignment-rules'] });
      toast.success('Rule deleted');
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to delete rule');
    },
  });

  const getQueueName = (queueId: string) => {
    return queues?.find((q: any) => q.id === queueId)?.name || 'Unknown Queue';
  };

  const getFieldIcon = (field: string) => {
    switch (field) {
      case 'vendor_id':
      case 'vendor_name':
        return <Building className="w-4 h-4" />;
      case 'department':
        return <Briefcase className="w-4 h-4" />;
      case 'amount':
        return <DollarSign className="w-4 h-4" />;
      case 'tag':
        return <Tag className="w-4 h-4" />;
      default:
        return <GitBranch className="w-4 h-4" />;
    }
  };

  const formatCondition = (condition: any) => {
    const fieldLabels: Record<string, string> = {
      vendor_id: 'Vendor ID',
      vendor_name: 'Vendor Name',
      department: 'Department',
      amount: 'Amount',
      gl_code: 'GL Code',
      tag: 'Tag',
      custom_field: 'Custom Field',
    };

    const operatorLabels: Record<string, string> = {
      equals: 'equals',
      not_equals: 'does not equal',
      greater_than: 'is greater than',
      less_than: 'is less than',
      contains: 'contains',
      starts_with: 'starts with',
    };

    const field = fieldLabels[condition.field] || condition.field;
    const operator = operatorLabels[condition.operator] || condition.operator;
    let value = condition.value;

    // Format amount in dollars
    if (condition.field === 'amount' && typeof value === 'number') {
      value = `$${(value / 100).toLocaleString()}`;
    }

    return `${field} ${operator} ${value}`;
  };

  const formatAssignTarget = (target: any) => {
    if (typeof target === 'object') {
      if ('User' in target) return { type: 'User', value: target.User };
      if ('Role' in target) return { type: 'Role', value: target.Role };
      if ('RoundRobin' in target) return { type: 'Round Robin', value: `${target.RoundRobin.length} users` };
      if ('LeastLoaded' in target) return { type: 'Least Loaded', value: `${target.LeastLoaded.length} users` };
    }
    if (target === 'VendorApprover') return { type: 'Vendor Approver', value: 'Registered approver' };
    if (target === 'DepartmentApprover') return { type: 'Department Approver', value: 'Registered approver' };
    return { type: 'Unknown', value: JSON.stringify(target) };
  };

  // Group rules by queue
  const rulesByQueue = rules?.reduce((acc: Record<string, any[]>, rule: any) => {
    const queueId = rule.queue_id;
    if (!acc[queueId]) acc[queueId] = [];
    acc[queueId].push(rule);
    return acc;
  }, {}) || {};

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
            <h1 className="text-2xl font-semibold text-foreground">Assignment Rules</h1>
            <p className="text-muted-foreground mt-0.5">
              Configure automatic invoice routing and assignment within queues
            </p>
          </div>
          <Link href="/processing/assignment-rules/new" className="btn btn-primary btn-sm">
            <Plus className="w-4 h-4 mr-1.5" />
            Create Rule
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
              <h3 className="font-semibold text-foreground">How Assignment Rules Work</h3>
              <p className="text-sm text-muted-foreground">
                Rules are evaluated in priority order (highest first). When an invoice matches a rule's conditions, it is assigned to the specified target.
              </p>
            </div>
          </div>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <div className="p-3 bg-secondary/50 rounded-xl">
              <div className="flex items-center gap-2 mb-1">
                <Building className="w-4 h-4 text-vendor" />
                <span className="font-medium text-foreground text-sm">Vendor-based</span>
              </div>
              <p className="text-xs text-muted-foreground">Route invoices from specific vendors</p>
            </div>
            <div className="p-3 bg-secondary/50 rounded-xl">
              <div className="flex items-center gap-2 mb-1">
                <Briefcase className="w-4 h-4 text-processing" />
                <span className="font-medium text-foreground text-sm">Department-based</span>
              </div>
              <p className="text-xs text-muted-foreground">Route by department or cost center</p>
            </div>
            <div className="p-3 bg-secondary/50 rounded-xl">
              <div className="flex items-center gap-2 mb-1">
                <DollarSign className="w-4 h-4 text-accent" />
                <span className="font-medium text-foreground text-sm">Amount-based</span>
              </div>
              <p className="text-xs text-muted-foreground">Route based on invoice value</p>
            </div>
          </div>
        </div>
      </div>

      {/* Search & Filter */}
      <div className="card p-4">
        <div className="flex flex-col sm:flex-row gap-3">
          <div className="flex-1 relative">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
            <input
              type="text"
              placeholder="Search rules..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="input pl-9"
            />
          </div>
          <button className="btn btn-secondary">
            <Filter className="w-4 h-4 mr-1.5" />
            Filters
          </button>
        </div>
      </div>

      {/* Rules by Queue */}
      {rulesLoading ? (
        <div className="space-y-4">
          {[1, 2].map((i) => (
            <div key={i} className="card p-6 animate-pulse">
              <div className="h-6 bg-secondary rounded w-1/4 mb-4" />
              <div className="h-4 bg-secondary rounded w-3/4" />
            </div>
          ))}
        </div>
      ) : !rules || rules.length === 0 ? (
        <div className="card p-12 text-center">
          <div className="w-14 h-14 rounded-xl bg-processing/10 flex items-center justify-center mx-auto mb-4">
            <GitBranch className="w-7 h-7 text-processing" />
          </div>
          <h3 className="text-lg font-semibold text-foreground mb-2">No Assignment Rules</h3>
          <p className="text-muted-foreground mb-4 max-w-sm mx-auto">
            Create your first assignment rule to start routing invoices automatically.
          </p>
          <Link href="/processing/assignment-rules/new" className="btn btn-primary btn-sm inline-flex">
            <Plus className="w-4 h-4 mr-1.5" />
            Create your first rule
          </Link>
        </div>
      ) : (
        <div className="space-y-4">
          {Object.entries(rulesByQueue).map(([queueId, queueRules]) => (
            <div key={queueId} className="card overflow-hidden">
              <div className="p-4 bg-secondary/50 border-b border-border">
                <h3 className="font-semibold text-foreground">{getQueueName(queueId)}</h3>
                <p className="text-sm text-muted-foreground">
                  {(queueRules as any[]).length} rule{(queueRules as any[]).length !== 1 ? 's' : ''}
                </p>
              </div>
              <div className="divide-y divide-border">
                {(queueRules as any[])
                  .sort((a, b) => b.priority - a.priority)
                  .map((rule) => {
                    const isExpanded = expandedRule === rule.id;
                    const assignTarget = formatAssignTarget(rule.assign_to);

                    return (
                      <div key={rule.id} className="p-4 hover:bg-secondary/30 transition-colors">
                        <div
                          className="flex items-center justify-between cursor-pointer"
                          onClick={() => setExpandedRule(isExpanded ? null : rule.id)}
                        >
                          <div className="flex items-center gap-4">
                            <span className={`px-2 py-1 rounded-full text-xs font-medium ${
                              rule.is_active
                                ? 'bg-success/10 text-success'
                                : 'bg-secondary text-muted-foreground'
                            }`}>
                              Priority: {rule.priority}
                            </span>
                            <div>
                              <p className="font-medium text-foreground">{rule.name}</p>
                              <p className="text-sm text-muted-foreground">
                                {rule.description || 'No description'}
                              </p>
                            </div>
                          </div>
                          <div className="flex items-center gap-2">
                            <button
                              onClick={(e) => {
                                e.stopPropagation();
                                if (confirm('Delete this rule?')) {
                                  deleteRule.mutate(rule.id);
                                }
                              }}
                              className="p-2 text-muted-foreground hover:text-error rounded-lg hover:bg-error/10 transition-colors"
                            >
                              <Trash2 className="w-4 h-4" />
                            </button>
                            {isExpanded ? (
                              <ChevronUp className="w-5 h-5 text-muted-foreground" />
                            ) : (
                              <ChevronDown className="w-5 h-5 text-muted-foreground" />
                            )}
                          </div>
                        </div>

                        {isExpanded && (
                          <div className="mt-4 pl-4 border-l-2 border-primary/20 space-y-4 animate-scale-in">
                            {/* Conditions */}
                            <div>
                              <p className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-2">
                                When
                              </p>
                              <div className="space-y-2">
                                {rule.conditions.map((condition: any, idx: number) => (
                                  <div
                                    key={idx}
                                    className="flex items-center gap-2 text-sm p-2 bg-secondary/50 rounded-lg"
                                  >
                                    <span className="text-primary">{getFieldIcon(condition.field)}</span>
                                    <span className="text-foreground">{formatCondition(condition)}</span>
                                  </div>
                                ))}
                              </div>
                            </div>

                            {/* Assignment Target */}
                            <div>
                              <p className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-2">
                                Assign To
                              </p>
                              <div className="flex items-center gap-2 text-sm p-2 bg-secondary/50 rounded-lg">
                                {assignTarget.type === 'User' ? (
                                  <User className="w-4 h-4 text-processing" />
                                ) : assignTarget.type === 'Role' ? (
                                  <Users className="w-4 h-4 text-vendor" />
                                ) : (
                                  <Building className="w-4 h-4 text-success" />
                                )}
                                <span className="text-foreground">
                                  {assignTarget.type}: <span className="font-medium">{assignTarget.value}</span>
                                </span>
                              </div>
                            </div>
                          </div>
                        )}
                      </div>
                    );
                  })}
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Help Section */}
      <div className="p-4 bg-processing/5 border border-processing/20 rounded-xl">
        <h3 className="font-medium text-foreground mb-2">Tips for Effective Rules</h3>
        <ul className="text-sm text-muted-foreground space-y-1">
          <li className="flex items-start gap-2">
            <CheckCircle className="w-4 h-4 text-processing mt-0.5 flex-shrink-0" />
            Use higher priority numbers for more specific rules
          </li>
          <li className="flex items-start gap-2">
            <CheckCircle className="w-4 h-4 text-processing mt-0.5 flex-shrink-0" />
            Combine multiple conditions for precise targeting
          </li>
          <li className="flex items-start gap-2">
            <CheckCircle className="w-4 h-4 text-processing mt-0.5 flex-shrink-0" />
            Use Round Robin to distribute work evenly across team members
          </li>
        </ul>
      </div>
    </div>
  );
}
