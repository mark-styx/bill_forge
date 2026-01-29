'use client';

import { useEffect, useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { invoicesApi, workflowsApi, documentsApi, Invoice } from '@/lib/api';
import { useAuthStore } from '@/stores/auth';
import { toast } from 'sonner';
import {
  X,
  FileText,
  Calendar,
  Building2,
  DollarSign,
  Clock,
  Save,
  Send,
  CheckCircle,
  XCircle,
  PauseCircle,
  Upload,
  Download,
  Eye,
  Edit2,
  Loader2,
  AlertTriangle,
  Hash,
  Tag,
  Briefcase,
  CreditCard,
} from 'lucide-react';

interface InvoicePanelProps {
  invoiceId: string | null;
  onClose: () => void;
}

const statusStyles: Record<string, { bg: string; text: string; icon: typeof Clock }> = {
  pending: { bg: 'bg-warning/10', text: 'text-warning', icon: Clock },
  processing: { bg: 'bg-primary/10', text: 'text-primary', icon: Loader2 },
  ready_for_review: { bg: 'bg-warning/10', text: 'text-warning', icon: Eye },
  reviewed: { bg: 'bg-success/10', text: 'text-success', icon: CheckCircle },
  submitted: { bg: 'bg-primary/10', text: 'text-primary', icon: Send },
  pending_approval: { bg: 'bg-warning/10', text: 'text-warning', icon: Clock },
  approved: { bg: 'bg-success/10', text: 'text-success', icon: CheckCircle },
  rejected: { bg: 'bg-error/10', text: 'text-error', icon: XCircle },
  on_hold: { bg: 'bg-warning/10', text: 'text-warning', icon: PauseCircle },
  ready_for_payment: { bg: 'bg-success/10', text: 'text-success', icon: CreditCard },
  paid: { bg: 'bg-success/10', text: 'text-success', icon: CheckCircle },
  draft: { bg: 'bg-secondary', text: 'text-muted-foreground', icon: Edit2 },
  failed: { bg: 'bg-error/10', text: 'text-error', icon: AlertTriangle },
};

export default function InvoicePanel({ invoiceId, onClose }: InvoicePanelProps) {
  const queryClient = useQueryClient();
  const { hasModule } = useAuthStore();
  const [isEditing, setIsEditing] = useState(false);
  const [formData, setFormData] = useState<Partial<Invoice>>({});

  const { data: invoice, isLoading, error } = useQuery({
    queryKey: ['invoice', invoiceId],
    queryFn: () => invoicesApi.get(invoiceId!),
    enabled: !!invoiceId,
  });

  useEffect(() => {
    if (invoice) {
      setFormData({
        vendor_name: invoice.vendor_name,
        invoice_number: invoice.invoice_number,
        invoice_date: invoice.invoice_date,
        due_date: invoice.due_date,
        po_number: invoice.po_number,
        total_amount: invoice.total_amount,
      });
    }
  }, [invoice]);

  const updateMutation = useMutation({
    mutationFn: (data: Partial<Invoice>) => invoicesApi.update(invoiceId!, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['invoice', invoiceId] });
      queryClient.invalidateQueries({ queryKey: ['invoices'] });
      toast.success('Invoice updated');
      setIsEditing(false);
    },
    onError: (err: any) => {
      toast.error(err.message || 'Failed to update invoice');
    },
  });

  const submitMutation = useMutation({
    mutationFn: () => invoicesApi.submitForProcessing(invoiceId!),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['invoice', invoiceId] });
      queryClient.invalidateQueries({ queryKey: ['invoices'] });
      toast.success('Invoice submitted for processing');
    },
    onError: (err: any) => {
      toast.error(err.message || 'Failed to submit invoice');
    },
  });

  const approveMutation = useMutation({
    mutationFn: () => workflowsApi.markReadyForPayment(invoiceId!),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['invoice', invoiceId] });
      queryClient.invalidateQueries({ queryKey: ['invoices'] });
      toast.success('Invoice approved');
    },
    onError: (err: any) => {
      toast.error(err.message || 'Failed to approve invoice');
    },
  });

  const holdMutation = useMutation({
    mutationFn: () => workflowsApi.putOnHold(invoiceId!, 'Review required'),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['invoice', invoiceId] });
      queryClient.invalidateQueries({ queryKey: ['invoices'] });
      toast.success('Invoice put on hold');
    },
    onError: (err: any) => {
      toast.error(err.message || 'Failed to put invoice on hold');
    },
  });

  if (!invoiceId) return null;

  const status = invoice ? statusStyles[invoice.processing_status] || statusStyles.draft : statusStyles.draft;
  const StatusIcon = status.icon;

  // Determine which actions are available based on status and modules
  const canEdit = invoice && ['draft', 'pending', 'ready_for_review', 'submitted'].includes(invoice.processing_status);
  const canSubmit = invoice && hasModule('invoice_processing') && ['draft', 'ready_for_review', 'reviewed'].includes(invoice.processing_status);
  const canApprove = invoice && hasModule('invoice_processing') && ['submitted', 'pending_approval'].includes(invoice.processing_status);
  const canHold = invoice && hasModule('invoice_processing') && !['paid', 'voided', 'on_hold'].includes(invoice.processing_status);

  return (
    <>
      {/* Backdrop */}
      <div 
        className="fixed inset-0 bg-black/30 backdrop-blur-sm z-40 transition-opacity"
        onClick={onClose}
      />
      
      {/* Panel */}
      <div className="fixed inset-y-0 right-0 w-full max-w-xl bg-card border-l border-border shadow-2xl z-50 animate-slide-in-right overflow-hidden flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between p-4 border-b border-border bg-card/95 backdrop-blur-sm">
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 rounded-xl bg-primary/10 flex items-center justify-center">
              <FileText className="w-5 h-5 text-primary" />
            </div>
            <div>
              <h2 className="text-lg font-semibold text-foreground">
                {isLoading ? 'Loading...' : invoice?.invoice_number || 'Invoice'}
              </h2>
              {invoice && (
                <p className="text-sm text-muted-foreground">{invoice.vendor_name}</p>
              )}
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-2 rounded-lg text-muted-foreground hover:text-foreground hover:bg-secondary transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto">
          {isLoading ? (
            <div className="flex items-center justify-center h-64">
              <Loader2 className="w-8 h-8 text-primary animate-spin" />
            </div>
          ) : error ? (
            <div className="flex flex-col items-center justify-center h-64 text-error">
              <AlertTriangle className="w-12 h-12 mb-4" />
              <p>Failed to load invoice</p>
            </div>
          ) : invoice ? (
            <div className="p-4 space-y-6">
              {/* Status Badge */}
              <div className={`inline-flex items-center gap-2 px-3 py-1.5 rounded-full text-sm font-medium ${status.bg} ${status.text}`}>
                <StatusIcon className="w-4 h-4" />
                {invoice.processing_status.replace(/_/g, ' ')}
              </div>

              {/* Amount */}
              <div className="p-4 rounded-xl bg-gradient-to-br from-primary/5 to-accent/5 border border-primary/10">
                <p className="text-sm text-muted-foreground mb-1">Total Amount</p>
                <p className="text-3xl font-bold text-foreground">
                  ${(invoice.total_amount.amount / 100).toLocaleString('en-US', { minimumFractionDigits: 2 })}
                  <span className="text-lg text-muted-foreground ml-2">{invoice.total_amount.currency}</span>
                </p>
              </div>

              {/* Details Grid */}
              <div className="grid grid-cols-2 gap-4">
                <div className="space-y-1">
                  <label className="text-xs text-muted-foreground flex items-center gap-1.5">
                    <Building2 className="w-3.5 h-3.5" />
                    Vendor
                  </label>
                  {isEditing ? (
                    <input
                      type="text"
                      value={formData.vendor_name || ''}
                      onChange={(e) => setFormData({ ...formData, vendor_name: e.target.value })}
                      className="input"
                    />
                  ) : (
                    <p className="text-sm font-medium text-foreground">{invoice.vendor_name}</p>
                  )}
                </div>

                <div className="space-y-1">
                  <label className="text-xs text-muted-foreground flex items-center gap-1.5">
                    <Hash className="w-3.5 h-3.5" />
                    Invoice #
                  </label>
                  {isEditing ? (
                    <input
                      type="text"
                      value={formData.invoice_number || ''}
                      onChange={(e) => setFormData({ ...formData, invoice_number: e.target.value })}
                      className="input"
                    />
                  ) : (
                    <p className="text-sm font-medium text-foreground">{invoice.invoice_number}</p>
                  )}
                </div>

                <div className="space-y-1">
                  <label className="text-xs text-muted-foreground flex items-center gap-1.5">
                    <Calendar className="w-3.5 h-3.5" />
                    Invoice Date
                  </label>
                  {isEditing ? (
                    <input
                      type="date"
                      value={formData.invoice_date || ''}
                      onChange={(e) => setFormData({ ...formData, invoice_date: e.target.value })}
                      className="input"
                    />
                  ) : (
                    <p className="text-sm font-medium text-foreground">{invoice.invoice_date || '—'}</p>
                  )}
                </div>

                <div className="space-y-1">
                  <label className="text-xs text-muted-foreground flex items-center gap-1.5">
                    <Clock className="w-3.5 h-3.5" />
                    Due Date
                  </label>
                  {isEditing ? (
                    <input
                      type="date"
                      value={formData.due_date || ''}
                      onChange={(e) => setFormData({ ...formData, due_date: e.target.value })}
                      className="input"
                    />
                  ) : (
                    <p className="text-sm font-medium text-foreground">{invoice.due_date || '—'}</p>
                  )}
                </div>

                <div className="space-y-1">
                  <label className="text-xs text-muted-foreground flex items-center gap-1.5">
                    <Tag className="w-3.5 h-3.5" />
                    PO Number
                  </label>
                  {isEditing ? (
                    <input
                      type="text"
                      value={formData.po_number || ''}
                      onChange={(e) => setFormData({ ...formData, po_number: e.target.value })}
                      className="input"
                    />
                  ) : (
                    <p className="text-sm font-medium text-foreground">{invoice.po_number || '—'}</p>
                  )}
                </div>

                <div className="space-y-1">
                  <label className="text-xs text-muted-foreground flex items-center gap-1.5">
                    <Briefcase className="w-3.5 h-3.5" />
                    Department
                  </label>
                  <p className="text-sm font-medium text-foreground">{(invoice as any).department || '—'}</p>
                </div>
              </div>

              {/* Amount Edit (when editing) */}
              {isEditing && (
                <div className="space-y-1">
                  <label className="text-xs text-muted-foreground flex items-center gap-1.5">
                    <DollarSign className="w-3.5 h-3.5" />
                    Amount (in cents)
                  </label>
                  <input
                    type="number"
                    value={formData.total_amount?.amount || 0}
                    onChange={(e) => setFormData({ 
                      ...formData, 
                      total_amount: { 
                        amount: parseInt(e.target.value) || 0, 
                        currency: formData.total_amount?.currency || 'USD' 
                      } 
                    })}
                    className="input"
                  />
                </div>
              )}

              {/* Capture Status */}
              <div className="p-3 rounded-lg bg-secondary/50">
                <p className="text-xs text-muted-foreground mb-1">Capture Status</p>
                <p className="text-sm font-medium text-foreground capitalize">{invoice.capture_status.replace(/_/g, ' ')}</p>
              </div>

              {/* Notes */}
              {(invoice as any).notes && (
                <div className="p-3 rounded-lg bg-secondary/50">
                  <p className="text-xs text-muted-foreground mb-1">Notes</p>
                  <p className="text-sm text-foreground">{(invoice as any).notes}</p>
                </div>
              )}
            </div>
          ) : null}
        </div>

        {/* Actions Footer */}
        {invoice && (
          <div className="p-4 border-t border-border bg-card/95 backdrop-blur-sm space-y-3">
            {isEditing ? (
              <div className="flex gap-2">
                <button
                  onClick={() => setIsEditing(false)}
                  className="btn btn-secondary flex-1"
                >
                  Cancel
                </button>
                <button
                  onClick={() => updateMutation.mutate(formData)}
                  disabled={updateMutation.isPending}
                  className="btn btn-primary flex-1"
                >
                  {updateMutation.isPending ? (
                    <Loader2 className="w-4 h-4 animate-spin mr-2" />
                  ) : (
                    <Save className="w-4 h-4 mr-2" />
                  )}
                  Save Changes
                </button>
              </div>
            ) : (
              <div className="flex flex-wrap gap-2">
                {canEdit && (
                  <button
                    onClick={() => setIsEditing(true)}
                    className="btn btn-secondary btn-sm"
                  >
                    <Edit2 className="w-4 h-4 mr-1.5" />
                    Edit
                  </button>
                )}
                
                {canSubmit && (
                  <button
                    onClick={() => submitMutation.mutate()}
                    disabled={submitMutation.isPending}
                    className="btn btn-primary btn-sm"
                  >
                    {submitMutation.isPending ? (
                      <Loader2 className="w-4 h-4 animate-spin mr-1.5" />
                    ) : (
                      <Send className="w-4 h-4 mr-1.5" />
                    )}
                    Submit
                  </button>
                )}

                {canApprove && (
                  <button
                    onClick={() => approveMutation.mutate()}
                    disabled={approveMutation.isPending}
                    className="btn btn-sm bg-success text-white hover:bg-success/90"
                  >
                    {approveMutation.isPending ? (
                      <Loader2 className="w-4 h-4 animate-spin mr-1.5" />
                    ) : (
                      <CheckCircle className="w-4 h-4 mr-1.5" />
                    )}
                    Approve
                  </button>
                )}

                {canHold && (
                  <button
                    onClick={() => holdMutation.mutate()}
                    disabled={holdMutation.isPending}
                    className="btn btn-secondary btn-sm"
                  >
                    {holdMutation.isPending ? (
                      <Loader2 className="w-4 h-4 animate-spin mr-1.5" />
                    ) : (
                      <PauseCircle className="w-4 h-4 mr-1.5" />
                    )}
                    Hold
                  </button>
                )}
              </div>
            )}
          </div>
        )}
      </div>

      <style jsx>{`
        @keyframes slide-in-right {
          from {
            transform: translateX(100%);
          }
          to {
            transform: translateX(0);
          }
        }
        .animate-slide-in-right {
          animation: slide-in-right 0.3s ease-out;
        }
      `}</style>
    </>
  );
}
