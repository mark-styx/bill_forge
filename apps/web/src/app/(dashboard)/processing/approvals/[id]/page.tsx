'use client';

import { useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useParams, useRouter } from 'next/navigation';
import Link from 'next/link';
import { workflowsApi, invoicesApi } from '@/lib/api';
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
  Building2,
  MessageSquare,
  History,
  Download,
  ExternalLink,
  Send,
  Flag,
  Eye,
} from 'lucide-react';

const statusColors: Record<string, { bg: string; text: string; label: string }> = {
  pending: { bg: 'bg-warning/10', text: 'text-warning', label: 'Pending Review' },
  approved: { bg: 'bg-success/10', text: 'text-success', label: 'Approved' },
  rejected: { bg: 'bg-error/10', text: 'text-error', label: 'Rejected' },
  on_hold: { bg: 'bg-warning/10', text: 'text-warning', label: 'On Hold' },
};

export default function ApprovalDetailPage() {
  const params = useParams();
  const router = useRouter();
  const queryClient = useQueryClient();
  const id = params.id as string;

  const [comment, setComment] = useState('');
  const [showRejectModal, setShowRejectModal] = useState(false);
  const [rejectReason, setRejectReason] = useState('');

  // Fetch approval details
  const { data: approval, isLoading: loadingApproval } = useQuery({
    queryKey: ['approval', id],
    queryFn: () => workflowsApi.getApproval(id),
  });

  // Fetch related invoice details
  const { data: invoice, isLoading: loadingInvoice } = useQuery({
    queryKey: ['invoice', approval?.invoice_id],
    queryFn: () => invoicesApi.get(approval!.invoice_id),
    enabled: !!approval?.invoice_id,
  });

  const approveMutation = useMutation({
    mutationFn: () => workflowsApi.approve(id, comment || undefined),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['approval', id] });
      queryClient.invalidateQueries({ queryKey: ['pending-approvals'] });
      toast.success('Invoice approved successfully');
      router.push('/processing/approvals');
    },
    onError: (error: Error) => {
      toast.error(error.message || 'Failed to approve invoice');
    },
  });

  const rejectMutation = useMutation({
    mutationFn: () => workflowsApi.reject(id, rejectReason || 'Rejected by approver'),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['approval', id] });
      queryClient.invalidateQueries({ queryKey: ['pending-approvals'] });
      toast.success('Invoice rejected');
      router.push('/processing/approvals');
    },
    onError: (error: Error) => {
      toast.error(error.message || 'Failed to reject invoice');
    },
  });

  const handleApprove = () => {
    approveMutation.mutate();
  };

  const handleReject = () => {
    rejectMutation.mutate();
    setShowRejectModal(false);
  };

  const isLoading = loadingApproval || loadingInvoice;
  const isPending = approval?.status === 'pending';

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-24">
        <div className="flex items-center gap-3 text-muted-foreground">
          <Loader2 className="w-5 h-5 animate-spin" />
          Loading approval details...
        </div>
      </div>
    );
  }

  if (!approval) {
    return (
      <div className="text-center py-24">
        <div className="w-16 h-16 rounded-2xl bg-processing/10 flex items-center justify-center mx-auto mb-4">
          <FileText className="w-8 h-8 text-processing" />
        </div>
        <h2 className="text-xl font-semibold text-foreground mb-2">Approval not found</h2>
        <p className="text-muted-foreground mb-4">This approval request doesn&apos;t exist or has been processed</p>
        <Link href="/processing/approvals" className="btn btn-primary btn-sm">
          Back to Approvals
        </Link>
      </div>
    );
  }

  const status = statusColors[approval.status] || statusColors.pending;

  return (
    <div className="space-y-6 max-w-5xl mx-auto">
      {/* Header */}
      <div>
        <Link
          href="/processing/approvals"
          className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors mb-3"
        >
          <ArrowLeft className="w-4 h-4" />
          Back to Approvals
        </Link>

        <div className="flex flex-col sm:flex-row sm:items-start sm:justify-between gap-4">
          <div>
            <div className="flex items-center gap-3">
              <h1 className="text-2xl font-semibold text-foreground">
                Invoice #{approval.invoice_number || approval.invoice_id.slice(0, 8)}
              </h1>
              <span className={`px-2.5 py-0.5 rounded-full text-xs font-medium ${status.bg} ${status.text}`}>
                {status.label}
              </span>
            </div>
            <p className="text-muted-foreground mt-0.5">
              Submitted for approval on {new Date(approval.created_at).toLocaleDateString()}
            </p>
          </div>

          {isPending && (
            <div className="flex items-center gap-2">
              <button
                onClick={() => setShowRejectModal(true)}
                disabled={rejectMutation.isPending}
                className="btn px-4 py-2 bg-error/10 text-error hover:bg-error/20 disabled:opacity-50"
              >
                {rejectMutation.isPending ? (
                  <Loader2 className="w-4 h-4 animate-spin" />
                ) : (
                  <>
                    <XCircle className="w-4 h-4 mr-1.5" />
                    Reject
                  </>
                )}
              </button>
              <button
                onClick={handleApprove}
                disabled={approveMutation.isPending}
                className="btn bg-success text-success-foreground hover:bg-success/90 disabled:opacity-50"
              >
                {approveMutation.isPending ? (
                  <Loader2 className="w-4 h-4 animate-spin" />
                ) : (
                  <>
                    <CheckCircle className="w-4 h-4 mr-1.5" />
                    Approve
                  </>
                )}
              </button>
            </div>
          )}
        </div>
      </div>

      {/* Main Content */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Left Column - Invoice Details */}
        <div className="lg:col-span-2 space-y-6">
          {/* Invoice Summary Card */}
          <div className="card overflow-hidden">
            <div className="h-1 bg-gradient-to-r from-processing to-processing/50" />
            <div className="p-6">
              <div className="flex items-center gap-3 mb-6">
                <div className="p-3 rounded-xl bg-processing/10">
                  <FileText className="w-6 h-6 text-processing" />
                </div>
                <div>
                  <h2 className="text-lg font-semibold text-foreground">Invoice Details</h2>
                  <p className="text-sm text-muted-foreground">Review the invoice information below</p>
                </div>
              </div>

              <div className="grid grid-cols-2 gap-6">
                <div>
                  <label className="text-xs text-muted-foreground flex items-center gap-1.5 mb-1.5">
                    <Building2 className="w-3.5 h-3.5" />
                    Vendor
                  </label>
                  <p className="text-sm font-medium text-foreground">
                    {approval.vendor_name || invoice?.vendor_name || 'Unknown Vendor'}
                  </p>
                </div>

                <div>
                  <label className="text-xs text-muted-foreground flex items-center gap-1.5 mb-1.5">
                    <DollarSign className="w-3.5 h-3.5" />
                    Amount
                  </label>
                  <p className="text-xl font-semibold text-foreground">
                    ${((approval.amount || invoice?.total_amount?.amount || 0) / 100).toLocaleString()}
                    <span className="text-sm font-normal text-muted-foreground ml-1">
                      {approval.currency || invoice?.total_amount?.currency || 'USD'}
                    </span>
                  </p>
                </div>

                <div>
                  <label className="text-xs text-muted-foreground flex items-center gap-1.5 mb-1.5">
                    <Calendar className="w-3.5 h-3.5" />
                    Invoice Date
                  </label>
                  <p className="text-sm font-medium text-foreground">
                    {invoice?.invoice_date || 'Not specified'}
                  </p>
                </div>

                <div>
                  <label className="text-xs text-muted-foreground flex items-center gap-1.5 mb-1.5">
                    <Clock className="w-3.5 h-3.5" />
                    Due Date
                  </label>
                  <p className="text-sm font-medium text-foreground">
                    {approval.due_date || invoice?.due_date || 'Not specified'}
                  </p>
                </div>

                <div>
                  <label className="text-xs text-muted-foreground flex items-center gap-1.5 mb-1.5">
                    <User className="w-3.5 h-3.5" />
                    Requester
                  </label>
                  <p className="text-sm font-medium text-foreground">
                    {approval.requester || 'System'}
                  </p>
                </div>

                <div>
                  <label className="text-xs text-muted-foreground flex items-center gap-1.5 mb-1.5">
                    <Flag className="w-3.5 h-3.5" />
                    Priority
                  </label>
                  <span className={`inline-flex px-2 py-0.5 rounded text-xs font-medium ${
                    (approval as any).priority === 'high'
                      ? 'bg-error/10 text-error'
                      : (approval as any).priority === 'medium'
                      ? 'bg-warning/10 text-warning'
                      : 'bg-secondary text-muted-foreground'
                  }`}>
                    {(approval as any).priority || 'Normal'}
                  </span>
                </div>
              </div>

              {/* Description / Notes */}
              {(approval.notes || invoice?.description) && (
                <div className="mt-6 pt-6 border-t border-border">
                  <label className="text-xs text-muted-foreground flex items-center gap-1.5 mb-2">
                    <MessageSquare className="w-3.5 h-3.5" />
                    Notes
                  </label>
                  <div className="p-4 bg-secondary/50 rounded-xl text-sm text-foreground">
                    {approval.notes || invoice?.description || 'No notes provided'}
                  </div>
                </div>
              )}

              {/* View Invoice Link */}
              <div className="mt-6 pt-6 border-t border-border flex items-center gap-3">
                <Link
                  href={`/invoices/${approval.invoice_id}`}
                  className="btn btn-secondary btn-sm"
                >
                  <Eye className="w-4 h-4 mr-1.5" />
                  View Full Invoice
                </Link>
                <button className="btn btn-secondary btn-sm">
                  <Download className="w-4 h-4 mr-1.5" />
                  Download PDF
                </button>
              </div>
            </div>
          </div>

          {/* Line Items Preview */}
          {invoice?.line_items && invoice.line_items.length > 0 && (
            <div className="card p-6">
              <h3 className="font-semibold text-foreground mb-4">Line Items</h3>
              <div className="table-container">
                <table className="table">
                  <thead>
                    <tr>
                      <th>Description</th>
                      <th className="text-right">Qty</th>
                      <th className="text-right">Unit Price</th>
                      <th className="text-right">Amount</th>
                    </tr>
                  </thead>
                  <tbody>
                    {invoice.line_items.map((item: any, index: number) => (
                      <tr key={index}>
                        <td className="text-foreground">{item.description}</td>
                        <td className="text-right text-muted-foreground">{item.quantity || 1}</td>
                        <td className="text-right text-muted-foreground">
                          ${((item.unit_price?.amount || 0) / 100).toFixed(2)}
                        </td>
                        <td className="text-right font-medium text-foreground">
                          ${((item.total_price?.amount || 0) / 100).toFixed(2)}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          )}
        </div>

        {/* Right Column - Actions & History */}
        <div className="space-y-6">
          {/* Add Comment */}
          {isPending && (
            <div className="card p-6">
              <h3 className="font-semibold text-foreground mb-3">Add Comment</h3>
              <p className="text-sm text-muted-foreground mb-3">
                Optional: Add a note with your decision
              </p>
              <textarea
                value={comment}
                onChange={(e) => setComment(e.target.value)}
                placeholder="Enter your comments here..."
                className="input min-h-[100px] resize-none mb-3"
              />
              <button
                onClick={handleApprove}
                disabled={approveMutation.isPending}
                className="btn bg-success text-success-foreground hover:bg-success/90 w-full disabled:opacity-50"
              >
                {approveMutation.isPending ? (
                  <Loader2 className="w-4 h-4 animate-spin mr-2" />
                ) : (
                  <CheckCircle className="w-4 h-4 mr-2" />
                )}
                Approve with Comment
              </button>
            </div>
          )}

          {/* Approval Workflow */}
          <div className="card p-6">
            <h3 className="font-semibold text-foreground mb-4 flex items-center gap-2">
              <History className="w-4 h-4 text-muted-foreground" />
              Approval History
            </h3>
            <div className="space-y-4">
              {/* Current/Pending Step */}
              <div className="flex items-start gap-3">
                <div className={`p-2 rounded-lg ${isPending ? 'bg-warning/10' : status.bg}`}>
                  {isPending ? (
                    <Clock className="w-4 h-4 text-warning" />
                  ) : approval.status === 'approved' ? (
                    <CheckCircle className="w-4 h-4 text-success" />
                  ) : (
                    <XCircle className="w-4 h-4 text-error" />
                  )}
                </div>
                <div>
                  <p className="text-sm font-medium text-foreground">
                    {isPending ? 'Awaiting your approval' : status.label}
                  </p>
                  <p className="text-xs text-muted-foreground">
                    {isPending ? 'Action required' : new Date(approval.updated_at || approval.created_at).toLocaleString()}
                  </p>
                </div>
              </div>

              {/* Submitted Step */}
              <div className="flex items-start gap-3 opacity-60">
                <div className="p-2 rounded-lg bg-success/10">
                  <Send className="w-4 h-4 text-success" />
                </div>
                <div>
                  <p className="text-sm font-medium text-foreground">Submitted for approval</p>
                  <p className="text-xs text-muted-foreground">
                    {new Date(approval.created_at).toLocaleString()}
                  </p>
                </div>
              </div>
            </div>
          </div>

          {/* Quick Actions */}
          <div className="card p-6">
            <h3 className="font-semibold text-foreground mb-4">Quick Actions</h3>
            <div className="space-y-2">
              <Link
                href={`/vendors/${invoice?.vendor_id}`}
                className="flex items-center gap-3 p-3 rounded-lg hover:bg-secondary transition-colors"
              >
                <div className="p-2 rounded-lg bg-vendor/10">
                  <Building2 className="w-4 h-4 text-vendor" />
                </div>
                <span className="text-sm font-medium text-foreground">View Vendor</span>
                <ExternalLink className="w-3.5 h-3.5 text-muted-foreground ml-auto" />
              </Link>
              <button className="w-full flex items-center gap-3 p-3 rounded-lg hover:bg-secondary transition-colors">
                <div className="p-2 rounded-lg bg-capture/10">
                  <MessageSquare className="w-4 h-4 text-capture" />
                </div>
                <span className="text-sm font-medium text-foreground">Request More Info</span>
              </button>
              <button className="w-full flex items-center gap-3 p-3 rounded-lg hover:bg-secondary transition-colors">
                <div className="p-2 rounded-lg bg-warning/10">
                  <Flag className="w-4 h-4 text-warning" />
                </div>
                <span className="text-sm font-medium text-foreground">Flag for Review</span>
              </button>
            </div>
          </div>

          {/* Guidelines */}
          <div className="p-4 bg-processing/5 border border-processing/20 rounded-xl">
            <h4 className="font-medium text-foreground mb-2 flex items-center gap-2">
              <AlertCircle className="w-4 h-4 text-processing" />
              Before Approving
            </h4>
            <ul className="text-sm text-muted-foreground space-y-1">
              <li>- Verify amount matches contract/PO</li>
              <li>- Confirm goods/services received</li>
              <li>- Check vendor information</li>
            </ul>
          </div>
        </div>
      </div>

      {/* Reject Modal */}
      {showRejectModal && (
        <>
          <div
            className="fixed inset-0 bg-black/50 backdrop-blur-sm z-50"
            onClick={() => setShowRejectModal(false)}
          />
          <div className="fixed inset-0 flex items-center justify-center z-50 p-4">
            <div className="bg-card border border-border rounded-xl shadow-xl max-w-md w-full p-6 animate-scale-in">
              <div className="flex items-center gap-4 mb-4">
                <div className="p-3 rounded-full bg-error/10">
                  <XCircle className="w-6 h-6 text-error" />
                </div>
                <div>
                  <h3 className="text-lg font-semibold text-foreground">Reject Invoice</h3>
                  <p className="text-sm text-muted-foreground">Provide a reason for rejection</p>
                </div>
              </div>

              <textarea
                value={rejectReason}
                onChange={(e) => setRejectReason(e.target.value)}
                placeholder="Enter rejection reason..."
                className="input min-h-[100px] resize-none mb-4"
              />

              <div className="flex gap-3 justify-end">
                <button
                  onClick={() => setShowRejectModal(false)}
                  className="btn btn-secondary"
                >
                  Cancel
                </button>
                <button
                  onClick={handleReject}
                  disabled={rejectMutation.isPending}
                  className="btn bg-error text-white hover:bg-error/90"
                >
                  {rejectMutation.isPending ? (
                    <Loader2 className="w-4 h-4 animate-spin mr-2" />
                  ) : (
                    <XCircle className="w-4 h-4 mr-2" />
                  )}
                  Reject Invoice
                </button>
              </div>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
