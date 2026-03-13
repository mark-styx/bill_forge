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
  ArrowRightLeft,
  Image,
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
  const [showMoveToQueue, setShowMoveToQueue] = useState(false);
  const [selectedQueueId, setSelectedQueueId] = useState('');

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && onClose) {
        onClose();
      }
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [onClose]);

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

  // Fetch documents associated with this invoice
  const { data: documents } = useQuery({
    queryKey: ['invoice-documents', invoiceId],
    queryFn: () => documentsApi.listForInvoice(invoiceId!),
    enabled: !!invoiceId,
  });

  // Fetch available queues for move-to-queue
  const { data: queues } = useQuery({
    queryKey: ['work-queues'],
    queryFn: () => workflowsApi.listQueues(),
    enabled: showMoveToQueue,
  });

  const moveToQueueMutation = useMutation({
    mutationFn: () => workflowsApi.moveToQueue(invoiceId!, selectedQueueId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['invoice', invoiceId] });
      queryClient.invalidateQueries({ queryKey: ['invoices'] });
      queryClient.invalidateQueries({ queryKey: ['queue-items'] });
      toast.success('Invoice moved to queue');
      setShowMoveToQueue(false);
      setSelectedQueueId('');
    },
    onError: (err: any) => {
      toast.error(err.message || 'Failed to move invoice');
    },
  });

  // Document preview handler
  const handleDocumentPreview = async (docId: string) => {
    try {
      const blob = await documentsApi.downloadBlob(docId);
      const url = URL.createObjectURL(blob);
      window.open(url, '_blank');
    } catch {
      toast.error('Failed to load document');
    }
  };

  if (!invoiceId) return null;

  const status = invoice ? statusStyles[invoice.processing_status] || statusStyles.draft : statusStyles.draft;
  const StatusIcon = status.icon;

  // Determine which actions are available based on status and modules
  const canEdit = invoice && ['draft', 'pending', 'ready_for_review', 'submitted', 'on_hold', 'in_review', 'pending_review'].includes(invoice.processing_status);
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
                    Amount
                  </label>
                  <div className="relative">
                    <span className="absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground text-sm">$</span>
                    <input
                      type="number"
                      step="0.01"
                      min="0"
                      value={((formData.total_amount?.amount || 0) / 100).toFixed(2)}
                      onChange={(e) => {
                        const dollars = parseFloat(e.target.value) || 0;
                        const cents = Math.round(dollars * 100);
                        setFormData({
                          ...formData,
                          total_amount: {
                            amount: cents,
                            currency: formData.total_amount?.currency || 'USD'
                          }
                        });
                      }}
                      className="input pl-7"
                    />
                  </div>
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

              {/* Document Preview */}
              <div className="space-y-2">
                <h3 className="text-sm font-medium text-muted-foreground flex items-center gap-1.5">
                  <Image className="w-3.5 h-3.5" />
                  Documents
                </h3>
                {documents && documents.length > 0 ? (
                  <div className="space-y-2">
                    {documents.map((doc: any) => (
                      <div
                        key={doc.id}
                        className="flex items-center justify-between p-3 rounded-lg bg-secondary/50 hover:bg-secondary transition-colors"
                      >
                        <div className="flex items-center gap-2 min-w-0">
                          <FileText className="w-4 h-4 text-primary flex-shrink-0" />
                          <span className="text-sm text-foreground truncate">
                            {doc.file_name || doc.original_name || 'Document'}
                          </span>
                        </div>
                        <button
                          onClick={() => handleDocumentPreview(doc.id)}
                          className="btn btn-sm bg-primary/10 text-primary hover:bg-primary/20 flex-shrink-0"
                        >
                          <Eye className="w-3.5 h-3.5 mr-1" />
                          View
                        </button>
                      </div>
                    ))}
                  </div>
                ) : (
                  <div className="p-3 rounded-lg bg-secondary/30 text-center">
                    <p className="text-sm text-muted-foreground">No documents attached</p>
                  </div>
                )}
              </div>

              {/* Move to Queue Dialog */}
              {showMoveToQueue && (
                <div className="p-4 rounded-xl border border-primary/20 bg-primary/5 space-y-3">
                  <h3 className="text-sm font-medium text-foreground">Move to Queue</h3>
                  <select
                    value={selectedQueueId}
                    onChange={(e) => setSelectedQueueId(e.target.value)}
                    className="input w-full"
                  >
                    <option value="">Select a queue...</option>
                    {queues?.map((q: any) => (
                      <option key={q.id} value={q.id}>
                        {q.name} ({q.queue_type})
                      </option>
                    ))}
                  </select>
                  <div className="flex gap-2">
                    <button
                      onClick={() => { setShowMoveToQueue(false); setSelectedQueueId(''); }}
                      className="btn btn-secondary btn-sm flex-1"
                    >
                      Cancel
                    </button>
                    <button
                      onClick={() => moveToQueueMutation.mutate()}
                      disabled={!selectedQueueId || moveToQueueMutation.isPending}
                      className="btn btn-primary btn-sm flex-1"
                    >
                      {moveToQueueMutation.isPending ? (
                        <Loader2 className="w-4 h-4 animate-spin mr-1" />
                      ) : (
                        <ArrowRightLeft className="w-4 h-4 mr-1" />
                      )}
                      Move
                    </button>
                  </div>
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

                {invoice && hasModule('invoice_processing') && (
                  <button
                    onClick={() => setShowMoveToQueue(true)}
                    className="btn btn-secondary btn-sm"
                  >
                    <ArrowRightLeft className="w-4 h-4 mr-1.5" />
                    Move to Queue
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
