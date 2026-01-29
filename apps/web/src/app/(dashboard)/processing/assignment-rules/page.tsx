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
  Tag
} from 'lucide-react';
import { toast } from 'sonner';
import Link from 'next/link';

export default function AssignmentRulesPage() {
  const queryClient = useQueryClient();
  const [expandedRule, setExpandedRule] = useState<string | null>(null);

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
    <div className="space-y-6">
      {/* Page header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-slate-900 dark:text-white">
            Assignment Rules
          </h1>
          <p className="text-slate-500 dark:text-slate-400">
            Configure automatic invoice routing and assignment within queues
          </p>
        </div>
        <Link
          href="/processing/assignment-rules/new"
          className="px-4 py-2 bg-processing text-white rounded-lg hover:bg-processing/90 transition-colors flex items-center space-x-2"
        >
          <Plus className="w-4 h-4" />
          <span>Create Rule</span>
        </Link>
      </div>

      {/* Explanation Card */}
      <div className="bg-blue-50 dark:bg-blue-900/20 rounded-xl border border-blue-200 dark:border-blue-800 p-6">
        <h3 className="font-semibold text-blue-900 dark:text-blue-100 mb-2">
          How Assignment Rules Work
        </h3>
        <p className="text-sm text-blue-700 dark:text-blue-300 mb-3">
          Assignment rules automatically route invoices to the right person when they enter a queue.
          Rules are evaluated in priority order (highest first). When an invoice matches a rule's conditions,
          it is assigned to the specified target.
        </p>
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4 text-sm">
          <div className="flex items-start space-x-2">
            <Building className="w-4 h-4 text-blue-500 mt-0.5" />
            <div>
              <span className="font-medium text-blue-900 dark:text-blue-100">Vendor-based</span>
              <p className="text-blue-600 dark:text-blue-400">Route invoices from specific vendors</p>
            </div>
          </div>
          <div className="flex items-start space-x-2">
            <Briefcase className="w-4 h-4 text-blue-500 mt-0.5" />
            <div>
              <span className="font-medium text-blue-900 dark:text-blue-100">Department-based</span>
              <p className="text-blue-600 dark:text-blue-400">Route by department or cost center</p>
            </div>
          </div>
          <div className="flex items-start space-x-2">
            <DollarSign className="w-4 h-4 text-blue-500 mt-0.5" />
            <div>
              <span className="font-medium text-blue-900 dark:text-blue-100">Amount-based</span>
              <p className="text-blue-600 dark:text-blue-400">Route based on invoice value</p>
            </div>
          </div>
        </div>
      </div>

      {/* Rules by Queue */}
      {rulesLoading ? (
        <div className="space-y-4">
          {[1, 2].map((i) => (
            <div key={i} className="bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700 p-6 animate-pulse">
              <div className="h-6 bg-slate-200 dark:bg-slate-700 rounded w-1/4 mb-4"></div>
              <div className="h-4 bg-slate-200 dark:bg-slate-700 rounded w-3/4"></div>
            </div>
          ))}
        </div>
      ) : !rules || rules.length === 0 ? (
        <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700 p-12 text-center">
          <GitBranch className="w-12 h-12 text-slate-300 dark:text-slate-600 mx-auto mb-4" />
          <p className="text-slate-500 dark:text-slate-400 mb-4">
            No assignment rules configured yet.
          </p>
          <Link
            href="/processing/assignment-rules/new"
            className="px-4 py-2 bg-processing text-white rounded-lg hover:bg-processing/90 transition-colors inline-flex items-center space-x-2"
          >
            <Plus className="w-4 h-4" />
            <span>Create your first rule</span>
          </Link>
        </div>
      ) : (
        <div className="space-y-6">
          {Object.entries(rulesByQueue).map(([queueId, queueRules]) => (
            <div
              key={queueId}
              className="bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700 overflow-hidden"
            >
              <div className="p-4 bg-slate-50 dark:bg-slate-700/50 border-b border-slate-200 dark:border-slate-700">
                <h3 className="font-semibold text-slate-900 dark:text-white">
                  {getQueueName(queueId)}
                </h3>
                <p className="text-sm text-slate-500 dark:text-slate-400">
                  {(queueRules as any[]).length} rule{(queueRules as any[]).length !== 1 ? 's' : ''}
                </p>
              </div>
              <div className="divide-y divide-slate-200 dark:divide-slate-700">
                {(queueRules as any[])
                  .sort((a, b) => b.priority - a.priority)
                  .map((rule) => {
                    const isExpanded = expandedRule === rule.id;
                    const assignTarget = formatAssignTarget(rule.assign_to);
                    
                    return (
                      <div key={rule.id} className="p-4">
                        <div
                          className="flex items-center justify-between cursor-pointer"
                          onClick={() => setExpandedRule(isExpanded ? null : rule.id)}
                        >
                          <div className="flex items-center space-x-4">
                            <div className={`px-2 py-1 rounded text-xs font-medium ${rule.is_active ? 'bg-green-100 text-green-700' : 'bg-slate-100 text-slate-500'}`}>
                              Priority: {rule.priority}
                            </div>
                            <div>
                              <p className="font-medium text-slate-900 dark:text-white">
                                {rule.name}
                              </p>
                              <p className="text-sm text-slate-500 dark:text-slate-400">
                                {rule.description || 'No description'}
                              </p>
                            </div>
                          </div>
                          <div className="flex items-center space-x-3">
                            <button
                              onClick={(e) => {
                                e.stopPropagation();
                                if (confirm('Delete this rule?')) {
                                  deleteRule.mutate(rule.id);
                                }
                              }}
                              className="p-2 text-slate-400 hover:text-red-500 transition-colors"
                            >
                              <Trash2 className="w-4 h-4" />
                            </button>
                            {isExpanded ? (
                              <ChevronUp className="w-5 h-5 text-slate-400" />
                            ) : (
                              <ChevronDown className="w-5 h-5 text-slate-400" />
                            )}
                          </div>
                        </div>
                        
                        {isExpanded && (
                          <div className="mt-4 pl-4 border-l-2 border-slate-200 dark:border-slate-600 space-y-4">
                            {/* Conditions */}
                            <div>
                              <p className="text-xs font-medium text-slate-500 dark:text-slate-400 uppercase tracking-wider mb-2">
                                When
                              </p>
                              <div className="space-y-2">
                                {rule.conditions.map((condition: any, idx: number) => (
                                  <div
                                    key={idx}
                                    className="flex items-center space-x-2 text-sm"
                                  >
                                    {getFieldIcon(condition.field)}
                                    <span className="text-slate-700 dark:text-slate-300">
                                      {formatCondition(condition)}
                                    </span>
                                  </div>
                                ))}
                              </div>
                            </div>
                            
                            {/* Assignment Target */}
                            <div>
                              <p className="text-xs font-medium text-slate-500 dark:text-slate-400 uppercase tracking-wider mb-2">
                                Assign To
                              </p>
                              <div className="flex items-center space-x-2 text-sm">
                                {assignTarget.type === 'User' ? (
                                  <User className="w-4 h-4 text-blue-500" />
                                ) : assignTarget.type === 'Role' ? (
                                  <Users className="w-4 h-4 text-purple-500" />
                                ) : (
                                  <Building className="w-4 h-4 text-green-500" />
                                )}
                                <span className="text-slate-700 dark:text-slate-300">
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
    </div>
  );
}
