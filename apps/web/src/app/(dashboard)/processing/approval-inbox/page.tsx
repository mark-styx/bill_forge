'use client';

import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import Link from 'next/link';
import { approvalChainsApi } from '@/lib/api';
import type {
  PendingApproval,
  ApprovalChain,
  ApprovalChainStep,
  ApprovalChainDetail,
} from '@/lib/api';
import { toast } from 'sonner';
import {
  ArrowLeft,
  CheckCircle,
  XCircle,
  Clock,
  Loader2,
  Search,
  ChevronRight,
  Shield,
  AlertTriangle,
  Users,
  Send,
  FileText,
  Filter,
  Inbox,
  X,
} from 'lucide-react';

export default function ApprovalInboxPage() {
  const queryClient = useQueryClient();
  const [searchQuery, setSearchQuery] = useState('');
  const [processingId, setProcessingId] = useState<string | null>(null);
  const [showDelegateModal, setShowDelegateModal] = useState<string | null>(null);
  const [delegateTo, setDelegateTo] = useState('');
  const [delegateReason, setDelegateReason] = useState('');
  const [comments, setComments] = useState('');
  const [statusFilter, setStatusFilter] = useState<string>('');

  const { data: pendingApprovals, isLoading: pendingLoading } = useQuery({
    queryKey: ['my-pending-approvals'],
    queryFn: () => approvalChainsApi.myPendingApprovals(),
  });

  const { data: chains } = useQuery({
    queryKey: ['approval-chains', statusFilter],
    queryFn: () => approvalChainsApi.listChains({ status: statusFilter || undefined, limit: 50 }),
  });

  const approveMutation = useMutation({
    mutationFn: ({ stepId, comments }: { stepId: string; comments?: string }) =>
      approvalChainsApi.approveStep(stepId, { comments }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['my-pending-approvals'] });
      queryClient.invalidateQueries({ queryKey: ['approval-chains'] });
      toast.success('Approval recorded');
      setProcessingId(null);
      setComments('');
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to approve');
      setProcessingId(null);
    },
  });

  const rejectMutation = useMutation({
    mutationFn: ({ stepId, comments }: { stepId: string; comments?: string }) =>
      approvalChainsApi.rejectStep(stepId, { comments }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['my-pending-approvals'] });
      queryClient.invalidateQueries({ queryKey: ['approval-chains'] });
      toast.success('Rejection recorded');
      setProcessingId(null);
      setComments('');
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to reject');
      setProcessingId(null);
    },
  });

  const delegateMutation = useMutation({
    mutationFn: ({ stepId, delegate_to, reason }: { stepId: string; delegate_to: string; reason?: string }) =>
      approvalChainsApi.delegateStep(stepId, { delegate_to, reason }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['my-pending-approvals'] });
      queryClient.invalidateQueries({ queryKey: ['approval-chains'] });
      toast.success('Step delegated');
      setShowDelegateModal(null);
      setDelegateTo('');
      setDelegateReason('');
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to delegate');
    },
  });

  const handleApprove = (stepId: string) => {
    setProcessingId(stepId);
    approveMutation.mutate({ stepId, comments: comments || undefined });
  };

  const handleReject = (stepId: string) => {
    setProcessingId(stepId);
    rejectMutation.mutate({ stepId, comments: comments || undefined });
  };

  const handleDelegate = () => {
    if (!showDelegateModal || !delegateTo.trim()) {
      toast.error('Please enter a user ID to delegate to');
      return;
    }
    delegateMutation.mutate({
      stepId: showDelegateModal,
      delegate_to: delegateTo.trim(),
      reason: delegateReason || undefined,
    });
  };

  const pending = (pendingApprovals as PendingApproval[] | undefined) || [];
  const chainList = (chains as ApprovalChain[] | undefined) || [];

  const filteredPending = searchQuery
    ? pending.filter(
        (p) =>
          p.policy_name.toLowerCase().includes(searchQuery.toLowerCase()) ||
          p.invoice_id.toLowerCase().includes(searchQuery.toLowerCase())
      )
    : pending;

  const urgentCount = pending.filter((p) => p.escalated).length;
  const dueSoonCount = pending.filter((p) => {
    if (!p.due_at) return false;
    const due = new Date(p.due_at);
    const now = new Date();
    const hoursUntilDue = (due.getTime() - now.getTime()) / (1000 * 60 * 60);
    return hoursUntilDue > 0 && hoursUntilDue < 24;
  }).length;

  return (
    <div className="max-w-4xl mx-auto space-y-6">
      {/* Header */}
      <div>
        <Link
          href="/processing"
          className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors mb-3"
        >
          <ArrowLeft className="w-4 h-4" />
          Back to Processing
        </Link>
        <h1 className="text-2xl font-semibold text-foreground">Approval Inbox</h1>
        <p className="text-muted-foreground mt-0.5">
          Review and action pending approval requests
        </p>
      </div>

      {/* Summary Cards */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <div className="card p-4">
          <div className="flex items-center gap-2 mb-1">
            <Inbox className="w-4 h-4 text-processing" />
            <span className="text-xs text-muted-foreground">Pending</span>
          </div>
          <p className="text-xl font-semibold text-foreground">{pending.length}</p>
        </div>
        <div className="card p-4">
          <div className="flex items-center gap-2 mb-1">
            <AlertTriangle className="w-4 h-4 text-error" />
            <span className="text-xs text-muted-foreground">Escalated</span>
          </div>
          <p className="text-xl font-semibold text-foreground">{urgentCount}</p>
        </div>
        <div className="card p-4">
          <div className="flex items-center gap-2 mb-1">
            <Clock className="w-4 h-4 text-warning" />
            <span className="text-xs text-muted-foreground">Due Soon</span>
          </div>
          <p className="text-xl font-semibold text-foreground">{dueSoonCount}</p>
        </div>
        <div className="card p-4">
          <div className="flex items-center gap-2 mb-1">
            <Shield className="w-4 h-4 text-capture" />
            <span className="text-xs text-muted-foreground">Active Chains</span>
          </div>
          <p className="text-xl font-semibold text-foreground">{chainList.length}</p>
        </div>
      </div>

      {/* Search & Filter */}
      <div className="flex items-center gap-3">
        <div className="relative flex-1">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
          <input
            type="text"
            placeholder="Search by policy name or invoice ID..."
            className="input w-full pl-9"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
          />
        </div>
        <select
          className="input"
          value={statusFilter}
          onChange={(e) => setStatusFilter(e.target.value)}
        >
          <option value="">All Chains</option>
          <option value="pending">Pending</option>
          <option value="in_progress">In Progress</option>
          <option value="approved">Approved</option>
          <option value="rejected">Rejected</option>
        </select>
      </div>

      {/* Pending Approvals List */}
      <div className="space-y-3">
        <h2 className="text-sm font-medium text-muted-foreground uppercase tracking-wider">
          My Pending Approvals
        </h2>

        {pendingLoading ? (
          <div className="card p-12 flex items-center justify-center">
            <Loader2 className="w-6 h-6 animate-spin text-muted-foreground" />
          </div>
        ) : filteredPending.length === 0 ? (
          <div className="card p-12 text-center">
            <div className="p-3 rounded-xl bg-secondary w-fit mx-auto mb-3">
              <CheckCircle className="w-8 h-8 text-success" />
            </div>
            <h3 className="font-medium text-foreground mb-1">All caught up!</h3>
            <p className="text-sm text-muted-foreground">No pending approvals at this time.</p>
          </div>
        ) : (
          filteredPending.map((item) => (
            <div key={item.step_id} className="card overflow-hidden">
              <div
                className={`h-1 ${
                  item.escalated
                    ? 'bg-gradient-to-r from-error to-error/50'
                    : 'bg-gradient-to-r from-processing to-processing/50'
                }`}
              />
              <div className="p-4">
                <div className="flex items-start justify-between">
                  <div className="flex items-start gap-3">
                    <div className={`p-2 rounded-lg ${item.escalated ? 'bg-error/10' : 'bg-processing/10'}`}>
                      {item.escalated ? (
                        <AlertTriangle className="w-5 h-5 text-error" />
                      ) : (
                        <FileText className="w-5 h-5 text-processing" />
                      )}
                    </div>
                    <div>
                      <div className="flex items-center gap-2">
                        <h3 className="font-medium text-foreground">{item.policy_name}</h3>
                        <span className="px-2 py-0.5 rounded text-xs bg-processing/10 text-processing font-medium">
                          Level {item.level}
                        </span>
                        {item.escalated && (
                          <span className="px-2 py-0.5 rounded text-xs bg-error/10 text-error font-medium">
                            Escalated
                          </span>
                        )}
                      </div>
                      <p className="text-sm text-muted-foreground mt-0.5">
                        Invoice: {item.invoice_id.slice(0, 8)}...
                      </p>
                      <div className="flex items-center gap-3 mt-1 text-xs text-muted-foreground">
                        <span>Chain: {item.chain_id.slice(0, 8)}...</span>
                        {item.due_at && (
                          <span className="flex items-center gap-1">
                            <Clock className="w-3 h-3" />
                            Due: {new Date(item.due_at).toLocaleDateString()}
                          </span>
                        )}
                        <span>
                          Created: {new Date(item.created_at).toLocaleDateString()}
                        </span>
                      </div>
                    </div>
                  </div>
                </div>

                {/* Quick Actions */}
                <div className="mt-4 pt-3 border-t border-border">
                  <div className="flex items-center gap-2">
                    <input
                      type="text"
                      placeholder="Comments (optional)..."
                      className="input flex-1 text-sm"
                      value={processingId === item.step_id ? comments : ''}
                      onChange={(e) => {
                        setProcessingId(item.step_id);
                        setComments(e.target.value);
                      }}
                      onFocus={() => setProcessingId(item.step_id)}
                    />
                    <button
                      onClick={() => handleApprove(item.step_id)}
                      className="btn bg-success text-success-foreground hover:bg-success/90 text-sm"
                      disabled={approveMutation.isPending && processingId === item.step_id}
                    >
                      {approveMutation.isPending && processingId === item.step_id ? (
                        <Loader2 className="w-4 h-4 animate-spin" />
                      ) : (
                        <>
                          <CheckCircle className="w-4 h-4 mr-1" />
                          Approve
                        </>
                      )}
                    </button>
                    <button
                      onClick={() => handleReject(item.step_id)}
                      className="btn bg-error text-error-foreground hover:bg-error/90 text-sm"
                      disabled={rejectMutation.isPending && processingId === item.step_id}
                    >
                      {rejectMutation.isPending && processingId === item.step_id ? (
                        <Loader2 className="w-4 h-4 animate-spin" />
                      ) : (
                        <>
                          <XCircle className="w-4 h-4 mr-1" />
                          Reject
                        </>
                      )}
                    </button>
                    <button
                      onClick={() => setShowDelegateModal(item.step_id)}
                      className="btn btn-secondary text-sm"
                    >
                      <Send className="w-4 h-4 mr-1" />
                      Delegate
                    </button>
                  </div>
                </div>
              </div>
            </div>
          ))
        )}
      </div>

      {/* Recent Chains */}
      {chainList.length > 0 && (
        <div className="space-y-3">
          <h2 className="text-sm font-medium text-muted-foreground uppercase tracking-wider">
            Recent Approval Chains
          </h2>
          <div className="card overflow-hidden">
            <div className="divide-y divide-border">
              {chainList.slice(0, 10).map((chain) => (
                <div key={chain.id} className="p-3 flex items-center justify-between hover:bg-secondary/30 transition-colors">
                  <div className="flex items-center gap-3">
                    <span
                      className={`px-2 py-0.5 rounded text-xs font-medium ${
                        chain.status === 'approved'
                          ? 'bg-success/10 text-success'
                          : chain.status === 'rejected'
                          ? 'bg-error/10 text-error'
                          : chain.status === 'cancelled'
                          ? 'bg-secondary text-muted-foreground'
                          : 'bg-processing/10 text-processing'
                      }`}
                    >
                      {chain.status}
                    </span>
                    <div>
                      <p className="text-sm font-medium text-foreground">
                        Invoice: {chain.invoice_id.slice(0, 8)}...
                      </p>
                      <p className="text-xs text-muted-foreground">
                        Level {chain.current_level}/{chain.total_levels}
                        {chain.escalation_count > 0 && ` • ${chain.escalation_count} escalation(s)`}
                      </p>
                    </div>
                  </div>
                  <span className="text-xs text-muted-foreground">
                    {new Date(chain.created_at).toLocaleDateString()}
                  </span>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}

      {/* Delegation Modal */}
      {showDelegateModal && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="card w-full max-w-md mx-4 overflow-hidden">
            <div className="h-1 bg-gradient-to-r from-processing to-processing/50" />
            <div className="p-6 space-y-4">
              <div className="flex items-center justify-between">
                <h3 className="text-lg font-semibold text-foreground">Delegate Approval</h3>
                <button
                  onClick={() => {
                    setShowDelegateModal(null);
                    setDelegateTo('');
                    setDelegateReason('');
                  }}
                  className="p-1 text-muted-foreground hover:text-foreground"
                >
                  <X className="w-5 h-5" />
                </button>
              </div>

              <div>
                <label className="block text-sm font-medium text-foreground mb-1">Delegate To (User ID)</label>
                <input
                  type="text"
                  className="input w-full"
                  placeholder="Enter user ID..."
                  value={delegateTo}
                  onChange={(e) => setDelegateTo(e.target.value)}
                />
              </div>

              <div>
                <label className="block text-sm font-medium text-foreground mb-1">Reason (optional)</label>
                <textarea
                  className="input w-full"
                  rows={3}
                  placeholder="Why are you delegating this approval?"
                  value={delegateReason}
                  onChange={(e) => setDelegateReason(e.target.value)}
                />
              </div>

              <div className="flex justify-end gap-3">
                <button
                  onClick={() => {
                    setShowDelegateModal(null);
                    setDelegateTo('');
                    setDelegateReason('');
                  }}
                  className="btn btn-secondary"
                >
                  Cancel
                </button>
                <button
                  onClick={handleDelegate}
                  className="btn bg-processing text-processing-foreground hover:bg-processing/90"
                  disabled={delegateMutation.isPending}
                >
                  {delegateMutation.isPending ? (
                    <Loader2 className="w-4 h-4 animate-spin" />
                  ) : (
                    <>
                      <Send className="w-4 h-4 mr-2" />
                      Delegate
                    </>
                  )}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
