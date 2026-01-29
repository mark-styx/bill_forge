'use client';

import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import Link from 'next/link';
import { workflowsApi } from '@/lib/api';
import { toast } from 'sonner';
import {
  ArrowLeft,
  CheckCircle,
  XCircle,
  Clock,
  FileText,
  User,
  DollarSign,
  Calendar,
  AlertCircle,
  Loader2,
  Filter,
  Search,
  ChevronRight,
  ListChecks,
} from 'lucide-react';

interface ApprovalItem {
  id: string;
  invoice_id: string;
  invoice_number?: string;
  vendor_name?: string;
  amount?: number;
  currency?: string;
  requester?: string;
  created_at: string;
  due_date?: string;
  notes?: string;
}

export default function ApprovalsPage() {
  const queryClient = useQueryClient();
  const [processingId, setProcessingId] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState('');

  const { data: approvals, isLoading } = useQuery({
    queryKey: ['pending-approvals'],
    queryFn: () => workflowsApi.listPendingApprovals(),
  });

  const approveMutation = useMutation({
    mutationFn: (approvalId: string) => workflowsApi.approve(approvalId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['pending-approvals'] });
      toast.success('Invoice approved successfully');
      setProcessingId(null);
    },
    onError: (error: Error) => {
      toast.error(error.message || 'Failed to approve invoice');
      setProcessingId(null);
    },
  });

  const rejectMutation = useMutation({
    mutationFn: (approvalId: string) => workflowsApi.reject(approvalId, 'Rejected by approver'),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['pending-approvals'] });
      toast.success('Invoice rejected');
      setProcessingId(null);
    },
    onError: (error: Error) => {
      toast.error(error.message || 'Failed to reject invoice');
      setProcessingId(null);
    },
  });

  const handleApprove = (id: string) => {
    setProcessingId(id);
    approveMutation.mutate(id);
  };

  const handleReject = (id: string) => {
    setProcessingId(id);
    rejectMutation.mutate(id);
  };

  const filteredApprovals = approvals?.filter((approval: ApprovalItem) => {
    if (!searchQuery) return true;
    const query = searchQuery.toLowerCase();
    return (
      approval.invoice_id.toLowerCase().includes(query) ||
      approval.invoice_number?.toLowerCase().includes(query) ||
      approval.vendor_name?.toLowerCase().includes(query)
    );
  });

  const pendingCount = approvals?.length || 0;

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
            <h1 className="text-2xl font-semibold text-foreground">Pending Approvals</h1>
            <p className="text-muted-foreground mt-0.5">
              Review and approve invoices requiring your attention
            </p>
          </div>
          {pendingCount > 0 && (
            <div className="flex items-center gap-2 px-3 py-1.5 bg-warning/10 text-warning rounded-full">
              <Clock className="w-4 h-4" />
              <span className="text-sm font-medium">{pendingCount} pending</span>
            </div>
          )}
        </div>
      </div>

      {/* Stats Cards */}
      <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
        <div className="card p-4">
          <div className="flex items-center gap-3">
            <div className="p-2.5 rounded-xl bg-warning/10">
              <Clock className="w-5 h-5 text-warning" />
            </div>
            <div>
              <p className="text-2xl font-semibold text-foreground">{pendingCount}</p>
              <p className="text-sm text-muted-foreground">Awaiting Review</p>
            </div>
          </div>
        </div>
        <div className="card p-4">
          <div className="flex items-center gap-3">
            <div className="p-2.5 rounded-xl bg-success/10">
              <CheckCircle className="w-5 h-5 text-success" />
            </div>
            <div>
              <p className="text-2xl font-semibold text-foreground">—</p>
              <p className="text-sm text-muted-foreground">Approved Today</p>
            </div>
          </div>
        </div>
        <div className="card p-4">
          <div className="flex items-center gap-3">
            <div className="p-2.5 rounded-xl bg-error/10">
              <AlertCircle className="w-5 h-5 text-error" />
            </div>
            <div>
              <p className="text-2xl font-semibold text-foreground">—</p>
              <p className="text-sm text-muted-foreground">Overdue</p>
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
              placeholder="Search by invoice number, vendor..."
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

      {/* Approvals List */}
      <div className="card overflow-hidden">
        <div className="h-1 bg-gradient-to-r from-processing to-processing/50" />

        {isLoading ? (
          <div className="p-12 text-center">
            <div className="flex items-center justify-center gap-2 text-muted-foreground">
              <Loader2 className="w-5 h-5 animate-spin" />
              Loading approvals...
            </div>
          </div>
        ) : !filteredApprovals || filteredApprovals.length === 0 ? (
          <div className="p-12 text-center">
            <div className="w-16 h-16 rounded-2xl bg-success/10 flex items-center justify-center mx-auto mb-4">
              <ListChecks className="w-8 h-8 text-success" />
            </div>
            <h3 className="text-lg font-semibold text-foreground mb-2">
              {searchQuery ? 'No Matching Approvals' : 'All Caught Up!'}
            </h3>
            <p className="text-muted-foreground max-w-sm mx-auto">
              {searchQuery
                ? 'No approvals match your search criteria. Try adjusting your filters.'
                : 'There are no invoices pending your approval. Great work!'}
            </p>
          </div>
        ) : (
          <div className="divide-y divide-border">
            {filteredApprovals.map((approval: ApprovalItem) => {
              const isProcessing = processingId === approval.id;

              return (
                <div
                  key={approval.id}
                  className="p-5 hover:bg-secondary/30 transition-colors"
                >
                  <div className="flex items-start justify-between gap-4">
                    {/* Left side - Invoice info */}
                    <div className="flex items-start gap-4 flex-1 min-w-0">
                      <div className="p-3 rounded-xl bg-warning/10 flex-shrink-0">
                        <Clock className="w-5 h-5 text-warning" />
                      </div>
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2 mb-1">
                          <p className="font-semibold text-foreground">
                            Invoice #{approval.invoice_number || approval.invoice_id.slice(0, 8)}
                          </p>
                          <ChevronRight className="w-4 h-4 text-muted-foreground" />
                        </div>

                        <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-sm text-muted-foreground">
                          {approval.vendor_name && (
                            <span className="flex items-center gap-1.5">
                              <User className="w-3.5 h-3.5" />
                              {approval.vendor_name}
                            </span>
                          )}
                          {approval.amount && (
                            <span className="flex items-center gap-1.5">
                              <DollarSign className="w-3.5 h-3.5" />
                              {(approval.amount / 100).toLocaleString()} {approval.currency || 'USD'}
                            </span>
                          )}
                          <span className="flex items-center gap-1.5">
                            <Calendar className="w-3.5 h-3.5" />
                            Requested {new Date(approval.created_at).toLocaleDateString()}
                          </span>
                        </div>

                        {approval.notes && (
                          <p className="text-sm text-muted-foreground mt-2 bg-secondary/50 rounded-lg px-3 py-2">
                            {approval.notes}
                          </p>
                        )}
                      </div>
                    </div>

                    {/* Right side - Actions */}
                    <div className="flex items-center gap-2 flex-shrink-0">
                      <button
                        onClick={() => handleReject(approval.id)}
                        disabled={isProcessing}
                        className="btn px-4 py-2 bg-error/10 text-error hover:bg-error/20 disabled:opacity-50"
                      >
                        {isProcessing && rejectMutation.isPending ? (
                          <Loader2 className="w-4 h-4 animate-spin" />
                        ) : (
                          <>
                            <XCircle className="w-4 h-4 mr-1.5" />
                            Reject
                          </>
                        )}
                      </button>
                      <button
                        onClick={() => handleApprove(approval.id)}
                        disabled={isProcessing}
                        className="btn bg-success text-success-foreground hover:bg-success/90 disabled:opacity-50"
                      >
                        {isProcessing && approveMutation.isPending ? (
                          <Loader2 className="w-4 h-4 animate-spin" />
                        ) : (
                          <>
                            <CheckCircle className="w-4 h-4 mr-1.5" />
                            Approve
                          </>
                        )}
                      </button>
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Help Section */}
      <div className="p-4 bg-processing/5 border border-processing/20 rounded-xl">
        <h3 className="font-medium text-foreground mb-2">Approval Guidelines</h3>
        <ul className="text-sm text-muted-foreground space-y-1">
          <li className="flex items-start gap-2">
            <CheckCircle className="w-4 h-4 text-processing mt-0.5 flex-shrink-0" />
            Verify the invoice amount matches the purchase order or contract
          </li>
          <li className="flex items-start gap-2">
            <CheckCircle className="w-4 h-4 text-processing mt-0.5 flex-shrink-0" />
            Confirm goods or services have been received before approving
          </li>
          <li className="flex items-start gap-2">
            <CheckCircle className="w-4 h-4 text-processing mt-0.5 flex-shrink-0" />
            Check that the vendor information is correct and up to date
          </li>
        </ul>
      </div>
    </div>
  );
}
