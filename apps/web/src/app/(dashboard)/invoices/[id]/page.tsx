'use client';

import { useState, useRef, useEffect } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useParams, useRouter } from 'next/navigation';
import Link from 'next/link';
import { invoicesApi, workflowsApi, vendorsApi, documentsApi, DocumentMetadata } from '@/lib/api';
import { toast } from 'sonner';
import {
  ArrowLeft,
  FileText,
  DollarSign,
  Calendar,
  Building,
  Edit,
  Send,
  CheckCircle,
  XCircle,
  Pause,
  Play,
  Trash2,
  Save,
  X,
  Layers,
  User,
  Tag,
  Briefcase,
  Hash,
  Eye,
  Upload,
  Download,
  File,
  Image,
} from 'lucide-react';

export default function InvoiceDetailPage() {
  const params = useParams();
  const router = useRouter();
  const queryClient = useQueryClient();
  const id = params.id as string;
  
  const [isEditing, setIsEditing] = useState(false);
  const [editedFields, setEditedFields] = useState<Record<string, any>>({});
  const [showMoveToQueueModal, setShowMoveToQueueModal] = useState(false);
  const [selectedQueueId, setSelectedQueueId] = useState('');
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [uploadingDocument, setUploadingDocument] = useState(false);
  const [previewBlobUrl, setPreviewBlobUrl] = useState<string | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [showQuickAddVendor, setShowQuickAddVendor] = useState(false);
  const [quickAddVendorName, setQuickAddVendorName] = useState('');
  const [quickAddVendorType, setQuickAddVendorType] = useState('business');

  const { data: invoice, isLoading } = useQuery({
    queryKey: ['invoice', id],
    queryFn: () => invoicesApi.get(id),
  });

  const { data: queues } = useQuery({
    queryKey: ['queues'],
    queryFn: () => workflowsApi.listQueues(),
  });

  const { data: vendors } = useQuery({
    queryKey: ['vendors'],
    queryFn: () => vendorsApi.list(),
  });

  // Documents for this invoice
  const { data: documents, refetch: refetchDocuments } = useQuery({
    queryKey: ['invoice-documents', id],
    queryFn: () => documentsApi.listForInvoice(id),
    enabled: !!id,
  });

  // Fetch document blob for preview with authentication
  useEffect(() => {
    if (documents && documents.length > 0) {
      const doc = documents[0];
      setPreviewError(null);
      documentsApi.downloadBlob(doc.id)
        .then((blob) => {
          // Revoke previous blob URL to avoid memory leaks
          if (previewBlobUrl) {
            URL.revokeObjectURL(previewBlobUrl);
          }
          const url = URL.createObjectURL(blob);
          setPreviewBlobUrl(url);
        })
        .catch((err) => {
          setPreviewError(err.message || 'Failed to load document preview');
          setPreviewBlobUrl(null);
        });
    } else {
      setPreviewBlobUrl(null);
    }
    // Cleanup on unmount
    return () => {
      if (previewBlobUrl) {
        URL.revokeObjectURL(previewBlobUrl);
      }
    };
  }, [documents]);

  // Upload document mutation
  const uploadDocument = useMutation({
    mutationFn: (file: File) => documentsApi.uploadForInvoice(id, file),
    onSuccess: () => {
      refetchDocuments();
      toast.success('Document uploaded successfully');
      setUploadingDocument(false);
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to upload document');
      setUploadingDocument(false);
    },
  });

  const handleFileUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) {
      setUploadingDocument(true);
      uploadDocument.mutate(file);
    }
  };

  const handleDownload = async (doc: DocumentMetadata) => {
    try {
      const blob = await documentsApi.downloadBlob(doc.id);
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = doc.filename;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    } catch (err: any) {
      toast.error(err.message || 'Failed to download document');
    }
  };

  // Mutations
  const updateInvoice = useMutation({
    mutationFn: (data: any) => invoicesApi.update(id, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['invoice', id] });
      toast.success('Invoice updated successfully');
      setIsEditing(false);
      setEditedFields({});
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to update invoice');
    },
  });

  const submitForProcessing = useMutation({
    mutationFn: () => invoicesApi.submitForProcessing(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['invoice', id] });
      toast.success('Invoice submitted for processing');
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to submit invoice');
    },
  });

  const approveInvoice = useMutation({
    mutationFn: () => workflowsApi.markReadyForPayment(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['invoice', id] });
      toast.success('Invoice approved');
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to approve invoice');
    },
  });

  const putOnHold = useMutation({
    mutationFn: (reason: string) => workflowsApi.putOnHold(id, reason),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['invoice', id] });
      toast.success('Invoice placed on hold');
    },
  });

  const releaseHold = useMutation({
    mutationFn: () => workflowsApi.releaseFromHold(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['invoice', id] });
      toast.success('Invoice released from hold');
    },
  });

  const voidInvoice = useMutation({
    mutationFn: () => workflowsApi.voidInvoice(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['invoice', id] });
      toast.success('Invoice voided');
      router.push('/invoices');
    },
  });

  const moveToQueue = useMutation({
    mutationFn: (queueId: string) => workflowsApi.moveToQueue(id, queueId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['invoice', id] });
      toast.success('Invoice moved to queue');
      setShowMoveToQueueModal(false);
    },
  });

  // Quick add vendor mutation
  const quickAddVendor = useMutation({
    mutationFn: async (data: { name: string; vendor_type: string }) => {
      // Create the vendor
      const vendor = await vendorsApi.create(data);
      // Link it to the invoice
      await invoicesApi.update(id, { vendor_id: vendor.id, vendor_name: vendor.name });
      return vendor;
    },
    onSuccess: (vendor) => {
      queryClient.invalidateQueries({ queryKey: ['invoice', id] });
      queryClient.invalidateQueries({ queryKey: ['vendors'] });
      toast.success(`Vendor "${vendor.name}" created and linked to invoice`);
      setShowQuickAddVendor(false);
      setQuickAddVendorName('');
      setQuickAddVendorType('business');
    },
    onError: (error: any) => {
      toast.error(error.message || 'Failed to create vendor');
    },
  });

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-12">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-500"></div>
      </div>
    );
  }

  if (!invoice) {
    return (
      <div className="text-center py-12">
        <p className="text-slate-500">Invoice not found</p>
        <Link href="/invoices" className="text-blue-500 hover:underline mt-4 inline-block">
          Back to invoices
        </Link>
      </div>
    );
  }

  const handleFieldChange = (field: string, value: any) => {
    setEditedFields({ ...editedFields, [field]: value });
  };

  const handleSave = () => {
    updateInvoice.mutate(editedFields);
  };

  const handleCancel = () => {
    setIsEditing(false);
    setEditedFields({});
  };

  const currentQueue = queues?.find((q: any) => q.id === invoice.current_queue_id);

  const getStatusColor = (status: string) => {
    switch (status) {
      case 'approved':
      case 'ready_for_payment':
      case 'paid':
      case 'reviewed':
        return 'status-badge-approved';
      case 'rejected':
      case 'failed':
        return 'status-badge-rejected';
      default:
        return 'status-badge-pending';
    }
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center space-x-4">
          <Link
            href="/invoices"
            className="p-2 text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 transition-colors"
          >
            <ArrowLeft className="w-5 h-5" />
          </Link>
          <div>
            <h1 className="text-2xl font-bold text-slate-900 dark:text-white">
              {invoice.invoice_number}
            </h1>
            <p className="text-slate-500 dark:text-slate-400">
              {invoice.vendor_name}
            </p>
          </div>
        </div>
        <div className="flex space-x-3">
          {isEditing ? (
            <>
              <button
                onClick={handleCancel}
                className="px-4 py-2 bg-slate-100 dark:bg-slate-700 text-slate-700 dark:text-slate-200 rounded-lg hover:bg-slate-200 dark:hover:bg-slate-600 transition-colors flex items-center space-x-2"
              >
                <X className="w-4 h-4" />
                <span>Cancel</span>
              </button>
              <button
                onClick={handleSave}
                disabled={updateInvoice.isPending}
                className="px-4 py-2 bg-blue-500 text-white rounded-lg hover:bg-blue-600 transition-colors flex items-center space-x-2 disabled:opacity-50"
              >
                <Save className="w-4 h-4" />
                <span>{updateInvoice.isPending ? 'Saving...' : 'Save Changes'}</span>
              </button>
            </>
          ) : (
            <>
              <button
                onClick={() => setIsEditing(true)}
                className="px-4 py-2 bg-slate-100 dark:bg-slate-700 text-slate-700 dark:text-slate-200 rounded-lg hover:bg-slate-200 dark:hover:bg-slate-600 transition-colors flex items-center space-x-2"
              >
                <Edit className="w-4 h-4" />
                <span>Edit</span>
              </button>
              {invoice.processing_status === 'draft' && (
                <button
                  onClick={() => submitForProcessing.mutate()}
                  disabled={submitForProcessing.isPending}
                  className="px-4 py-2 bg-blue-500 text-white rounded-lg hover:bg-blue-600 transition-colors flex items-center space-x-2 disabled:opacity-50"
                >
                  <Send className="w-4 h-4" />
                  <span>Submit</span>
                </button>
              )}
              {invoice.processing_status === 'pending_approval' && (
                <button
                  onClick={() => approveInvoice.mutate()}
                  disabled={approveInvoice.isPending}
                  className="px-4 py-2 bg-green-500 text-white rounded-lg hover:bg-green-600 transition-colors flex items-center space-x-2 disabled:opacity-50"
                >
                  <CheckCircle className="w-4 h-4" />
                  <span>Approve</span>
                </button>
              )}
            </>
          )}
        </div>
      </div>

      {/* Status & Queue Info */}
      <div className="flex flex-wrap items-center gap-3">
        <span className={`status-badge ${getStatusColor(invoice.capture_status)}`}>
          Capture: {invoice.capture_status.replace(/_/g, ' ')}
        </span>
        <span className={`status-badge ${getStatusColor(invoice.processing_status)}`}>
          Processing: {invoice.processing_status.replace(/_/g, ' ')}
        </span>
        {currentQueue && (
          <span className="px-3 py-1 bg-purple-100 dark:bg-purple-900/30 text-purple-700 dark:text-purple-300 rounded-full text-sm flex items-center space-x-1">
            <Layers className="w-3 h-3" />
            <span>Queue: {currentQueue.name}</span>
          </span>
        )}
        {invoice.assigned_to && (
          <span className="px-3 py-1 bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300 rounded-full text-sm flex items-center space-x-1">
            <User className="w-3 h-3" />
            <span>Assigned</span>
          </span>
        )}
      </div>

      {/* Main Content Grid */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Left Column - PDF Viewer and Documents */}
        <div className="lg:col-span-1 space-y-4">
          {/* Document Preview */}
          <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700 overflow-hidden">
            <div className="p-4 border-b border-slate-200 dark:border-slate-700 flex items-center justify-between">
              <h2 className="font-semibold text-slate-900 dark:text-white flex items-center space-x-2">
                <Eye className="w-4 h-4" />
                <span>Document Preview</span>
              </h2>
              <button
                onClick={() => fileInputRef.current?.click()}
                disabled={uploadingDocument}
                className="px-3 py-1 text-sm bg-blue-500 text-white rounded-lg hover:bg-blue-600 transition-colors flex items-center space-x-1 disabled:opacity-50"
              >
                <Upload className="w-3 h-3" />
                <span>{uploadingDocument ? 'Uploading...' : 'Upload'}</span>
              </button>
              <input
                ref={fileInputRef}
                type="file"
                accept="application/pdf,image/*"
                onChange={handleFileUpload}
                className="hidden"
              />
            </div>
            {documents && documents.length > 0 ? (
              <div className="aspect-[8.5/11] bg-slate-100 dark:bg-slate-900 relative">
                {previewError ? (
                  <div className="flex items-center justify-center h-full">
                    <div className="text-center p-6">
                      <XCircle className="w-12 h-12 text-red-400 mx-auto mb-2" />
                      <p className="text-red-500 text-sm">{previewError}</p>
                    </div>
                  </div>
                ) : !previewBlobUrl ? (
                  <div className="flex items-center justify-center h-full">
                    <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-500"></div>
                  </div>
                ) : documents[0].mime_type === 'application/pdf' ? (
                  <iframe
                    src={previewBlobUrl}
                    className="w-full h-full border-0"
                    title="Invoice Document"
                  />
                ) : documents[0].mime_type.startsWith('image/') ? (
                  <img
                    src={previewBlobUrl}
                    alt="Invoice Document"
                    className="w-full h-full object-contain"
                  />
                ) : (
                  <div className="flex items-center justify-center h-full">
                    <File className="w-16 h-16 text-slate-300 dark:text-slate-600" />
                  </div>
                )}
              </div>
            ) : (
              <div className="aspect-[8.5/11] bg-slate-100 dark:bg-slate-900 flex items-center justify-center">
                <div className="text-center p-6">
                  <FileText className="w-16 h-16 text-slate-300 dark:text-slate-600 mx-auto mb-4" />
                  <p className="text-slate-500 dark:text-slate-400 text-sm mb-4">
                    No document uploaded
                  </p>
                  <button
                    onClick={() => fileInputRef.current?.click()}
                    disabled={uploadingDocument}
                    className="px-4 py-2 bg-blue-500 text-white rounded-lg hover:bg-blue-600 transition-colors flex items-center space-x-2 mx-auto disabled:opacity-50"
                  >
                    <Upload className="w-4 h-4" />
                    <span>{uploadingDocument ? 'Uploading...' : 'Upload Document'}</span>
                  </button>
                </div>
              </div>
            )}
          </div>

          {/* Document List */}
          {documents && documents.length > 0 && (
            <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700">
              <div className="p-4 border-b border-slate-200 dark:border-slate-700">
                <h3 className="font-semibold text-slate-900 dark:text-white text-sm">
                  Attached Documents ({documents.length})
                </h3>
              </div>
              <div className="divide-y divide-slate-200 dark:divide-slate-700">
                {documents.map((doc: DocumentMetadata) => (
                  <div key={doc.id} className="p-3 flex items-center justify-between hover:bg-slate-50 dark:hover:bg-slate-700/50">
                    <div className="flex items-center space-x-3 flex-1 min-w-0">
                      {doc.mime_type === 'application/pdf' ? (
                        <FileText className="w-5 h-5 text-red-500 flex-shrink-0" />
                      ) : doc.mime_type.startsWith('image/') ? (
                        <Image className="w-5 h-5 text-blue-500 flex-shrink-0" />
                      ) : (
                        <File className="w-5 h-5 text-slate-400 flex-shrink-0" />
                      )}
                      <div className="min-w-0">
                        <p className="text-sm font-medium text-slate-900 dark:text-white truncate">
                          {doc.filename}
                        </p>
                        <p className="text-xs text-slate-500 dark:text-slate-400">
                          {(doc.size_bytes / 1024).toFixed(1)} KB • {doc.doc_type.replace(/_/g, ' ')}
                        </p>
                      </div>
                    </div>
                    <button
                      onClick={() => handleDownload(doc)}
                      className="p-2 text-slate-400 hover:text-blue-500 transition-colors"
                      title="Download"
                    >
                      <Download className="w-4 h-4" />
                    </button>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>

        {/* Right Column - Editable Fields */}
        <div className="lg:col-span-2 space-y-6">
          {/* Invoice Details Card */}
          <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700">
            <div className="p-4 border-b border-slate-200 dark:border-slate-700">
              <h2 className="font-semibold text-slate-900 dark:text-white">
                Invoice Details
              </h2>
            </div>
            <div className="p-6 grid grid-cols-2 gap-6">
              {/* Invoice Number */}
              <div>
                <label className="block text-sm font-medium text-slate-500 dark:text-slate-400 mb-1">
                  Invoice Number
                </label>
                {isEditing ? (
                  <input
                    type="text"
                    value={editedFields.invoice_number ?? invoice.invoice_number}
                    onChange={(e) => handleFieldChange('invoice_number', e.target.value)}
                    className="w-full px-3 py-2 bg-slate-100 dark:bg-slate-700 border border-slate-300 dark:border-slate-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
                  />
                ) : (
                  <p className="font-medium text-slate-900 dark:text-white">{invoice.invoice_number}</p>
                )}
              </div>

              {/* Vendor */}
              <div>
                <label className="block text-sm font-medium text-slate-500 dark:text-slate-400 mb-1">
                  Vendor
                </label>
                {isEditing ? (
                  <div className="space-y-2">
                    <select
                      value={editedFields.vendor_id ?? invoice.vendor_id ?? ''}
                      onChange={(e) => {
                        const vendor = vendors?.data?.find((v: any) => v.id === e.target.value);
                        handleFieldChange('vendor_id', e.target.value);
                        if (vendor) {
                          handleFieldChange('vendor_name', vendor.name);
                        }
                      }}
                      className="w-full px-3 py-2 bg-slate-100 dark:bg-slate-700 border border-slate-300 dark:border-slate-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
                    >
                      <option value="">Select Vendor</option>
                      {vendors?.data?.map((vendor: any) => (
                        <option key={vendor.id} value={vendor.id}>{vendor.name}</option>
                      ))}
                    </select>
                    <button
                      type="button"
                      onClick={() => {
                        setQuickAddVendorName(invoice.vendor_name !== 'Unknown Vendor' ? invoice.vendor_name : '');
                        setShowQuickAddVendor(true);
                      }}
                      className="text-sm text-blue-500 hover:text-blue-600 flex items-center space-x-1"
                    >
                      <span>+ Quick Add Vendor</span>
                    </button>
                  </div>
                ) : invoice.vendor_id ? (
                  <Link href={`/vendors/${invoice.vendor_id}`} className="font-medium text-blue-500 hover:underline">
                    {invoice.vendor_name}
                  </Link>
                ) : (
                  <div className="flex items-center space-x-2">
                    <span className="font-medium text-amber-600 dark:text-amber-400">{invoice.vendor_name}</span>
                    <button
                      type="button"
                      onClick={() => {
                        setQuickAddVendorName(invoice.vendor_name !== 'Unknown Vendor' ? invoice.vendor_name : '');
                        setShowQuickAddVendor(true);
                      }}
                      className="text-xs px-2 py-1 bg-blue-100 dark:bg-blue-900/30 text-blue-600 dark:text-blue-400 rounded hover:bg-blue-200 dark:hover:bg-blue-900/50"
                    >
                      + Quick Add
                    </button>
                  </div>
                )}
              </div>

              {/* Invoice Date */}
              <div>
                <label className="block text-sm font-medium text-slate-500 dark:text-slate-400 mb-1">
                  Invoice Date
                </label>
                {isEditing ? (
                  <input
                    type="date"
                    value={editedFields.invoice_date ?? invoice.invoice_date ?? ''}
                    onChange={(e) => handleFieldChange('invoice_date', e.target.value)}
                    className="w-full px-3 py-2 bg-slate-100 dark:bg-slate-700 border border-slate-300 dark:border-slate-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
                  />
                ) : (
                  <p className="font-medium text-slate-900 dark:text-white">{invoice.invoice_date || 'Not set'}</p>
                )}
              </div>

              {/* Due Date */}
              <div>
                <label className="block text-sm font-medium text-slate-500 dark:text-slate-400 mb-1">
                  Due Date
                </label>
                {isEditing ? (
                  <input
                    type="date"
                    value={editedFields.due_date ?? invoice.due_date ?? ''}
                    onChange={(e) => handleFieldChange('due_date', e.target.value)}
                    className="w-full px-3 py-2 bg-slate-100 dark:bg-slate-700 border border-slate-300 dark:border-slate-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
                  />
                ) : (
                  <p className="font-medium text-slate-900 dark:text-white">{invoice.due_date || 'Not set'}</p>
                )}
              </div>

              {/* PO Number */}
              <div>
                <label className="block text-sm font-medium text-slate-500 dark:text-slate-400 mb-1">
                  PO Number
                </label>
                {isEditing ? (
                  <input
                    type="text"
                    value={editedFields.po_number ?? invoice.po_number ?? ''}
                    onChange={(e) => handleFieldChange('po_number', e.target.value)}
                    className="w-full px-3 py-2 bg-slate-100 dark:bg-slate-700 border border-slate-300 dark:border-slate-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
                  />
                ) : (
                  <p className="font-medium text-slate-900 dark:text-white">{invoice.po_number || 'Not set'}</p>
                )}
              </div>

              {/* Total Amount */}
              <div>
                <label className="block text-sm font-medium text-slate-500 dark:text-slate-400 mb-1">
                  Total Amount
                </label>
                <p className="text-2xl font-bold text-green-600">
                  ${(invoice.total_amount.amount / 100).toLocaleString('en-US', { minimumFractionDigits: 2 })}
                </p>
              </div>
            </div>
          </div>

          {/* Coding Card */}
          <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700">
            <div className="p-4 border-b border-slate-200 dark:border-slate-700">
              <h2 className="font-semibold text-slate-900 dark:text-white flex items-center space-x-2">
                <Briefcase className="w-4 h-4" />
                <span>Coding</span>
              </h2>
            </div>
            <div className="p-6 grid grid-cols-3 gap-6">
              {/* Department */}
              <div>
                <label className="block text-sm font-medium text-slate-500 dark:text-slate-400 mb-1">
                  Department
                </label>
                {isEditing ? (
                  <input
                    type="text"
                    value={editedFields.department ?? invoice.department ?? ''}
                    onChange={(e) => handleFieldChange('department', e.target.value)}
                    className="w-full px-3 py-2 bg-slate-100 dark:bg-slate-700 border border-slate-300 dark:border-slate-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
                    placeholder="e.g. Operations"
                  />
                ) : (
                  <p className="font-medium text-slate-900 dark:text-white">{invoice.department || 'Not set'}</p>
                )}
              </div>

              {/* GL Code */}
              <div>
                <label className="block text-sm font-medium text-slate-500 dark:text-slate-400 mb-1">
                  GL Code
                </label>
                {isEditing ? (
                  <input
                    type="text"
                    value={editedFields.gl_code ?? invoice.gl_code ?? ''}
                    onChange={(e) => handleFieldChange('gl_code', e.target.value)}
                    className="w-full px-3 py-2 bg-slate-100 dark:bg-slate-700 border border-slate-300 dark:border-slate-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
                    placeholder="e.g. 6000-100"
                  />
                ) : (
                  <p className="font-medium text-slate-900 dark:text-white">{invoice.gl_code || 'Not set'}</p>
                )}
              </div>

              {/* Cost Center */}
              <div>
                <label className="block text-sm font-medium text-slate-500 dark:text-slate-400 mb-1">
                  Cost Center
                </label>
                {isEditing ? (
                  <input
                    type="text"
                    value={editedFields.cost_center ?? invoice.cost_center ?? ''}
                    onChange={(e) => handleFieldChange('cost_center', e.target.value)}
                    className="w-full px-3 py-2 bg-slate-100 dark:bg-slate-700 border border-slate-300 dark:border-slate-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
                    placeholder="e.g. CC-001"
                  />
                ) : (
                  <p className="font-medium text-slate-900 dark:text-white">{invoice.cost_center || 'Not set'}</p>
                )}
              </div>
            </div>
          </div>

          {/* Notes Card */}
          <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700">
            <div className="p-4 border-b border-slate-200 dark:border-slate-700">
              <h2 className="font-semibold text-slate-900 dark:text-white">Notes</h2>
            </div>
            <div className="p-6">
              {isEditing ? (
                <textarea
                  value={editedFields.notes ?? invoice.notes ?? ''}
                  onChange={(e) => handleFieldChange('notes', e.target.value)}
                  rows={4}
                  className="w-full px-3 py-2 bg-slate-100 dark:bg-slate-700 border border-slate-300 dark:border-slate-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
                  placeholder="Add notes about this invoice..."
                />
              ) : (
                <p className="text-slate-700 dark:text-slate-300">{invoice.notes || 'No notes'}</p>
              )}
            </div>
          </div>
        </div>
      </div>

      {/* Actions Card */}
      <div className="bg-white dark:bg-slate-800 rounded-xl border border-slate-200 dark:border-slate-700">
        <div className="p-4 border-b border-slate-200 dark:border-slate-700">
          <h2 className="font-semibold text-slate-900 dark:text-white">Actions</h2>
        </div>
        <div className="p-6 flex flex-wrap gap-3">
          {invoice.processing_status !== 'on_hold' && invoice.processing_status !== 'paid' && (
            <button
              onClick={() => putOnHold.mutate('Needs review')}
              disabled={putOnHold.isPending}
              className="px-4 py-2 bg-yellow-100 text-yellow-700 rounded-lg hover:bg-yellow-200 transition-colors flex items-center space-x-2"
            >
              <Pause className="w-4 h-4" />
              <span>Put on Hold</span>
            </button>
          )}
          {invoice.processing_status === 'on_hold' && (
            <button
              onClick={() => releaseHold.mutate()}
              disabled={releaseHold.isPending}
              className="px-4 py-2 bg-green-100 text-green-700 rounded-lg hover:bg-green-200 transition-colors flex items-center space-x-2"
            >
              <Play className="w-4 h-4" />
              <span>Release Hold</span>
            </button>
          )}
          <button
            onClick={() => setShowMoveToQueueModal(true)}
            className="px-4 py-2 bg-purple-100 text-purple-700 rounded-lg hover:bg-purple-200 transition-colors flex items-center space-x-2"
          >
            <Layers className="w-4 h-4" />
            <span>Move to Queue</span>
          </button>
          {invoice.processing_status !== 'voided' && invoice.processing_status !== 'paid' && (
            <button
              onClick={() => {
                if (confirm('Are you sure you want to void this invoice?')) {
                  voidInvoice.mutate();
                }
              }}
              disabled={voidInvoice.isPending}
              className="px-4 py-2 bg-red-100 text-red-700 rounded-lg hover:bg-red-200 transition-colors flex items-center space-x-2"
            >
              <Trash2 className="w-4 h-4" />
              <span>Void Invoice</span>
            </button>
          )}
        </div>
      </div>

      {/* Move to Queue Modal */}
      {showMoveToQueueModal && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-white dark:bg-slate-800 rounded-xl p-6 w-full max-w-md">
            <h3 className="text-lg font-semibold text-slate-900 dark:text-white mb-4">
              Move Invoice to Queue
            </h3>
            <select
              value={selectedQueueId}
              onChange={(e) => setSelectedQueueId(e.target.value)}
              className="w-full px-3 py-2 bg-slate-100 dark:bg-slate-700 border border-slate-300 dark:border-slate-600 rounded-lg mb-4"
            >
              <option value="">Select a queue</option>
              {queues?.map((queue: any) => (
                <option key={queue.id} value={queue.id}>{queue.name}</option>
              ))}
            </select>
            <div className="flex justify-end space-x-3">
              <button
                onClick={() => setShowMoveToQueueModal(false)}
                className="px-4 py-2 bg-slate-100 dark:bg-slate-700 text-slate-700 dark:text-slate-200 rounded-lg"
              >
                Cancel
              </button>
              <button
                onClick={() => selectedQueueId && moveToQueue.mutate(selectedQueueId)}
                disabled={!selectedQueueId || moveToQueue.isPending}
                className="px-4 py-2 bg-blue-500 text-white rounded-lg disabled:opacity-50"
              >
                {moveToQueue.isPending ? 'Moving...' : 'Move'}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Quick Add Vendor Modal */}
      {showQuickAddVendor && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-white dark:bg-slate-800 rounded-xl p-6 w-full max-w-md">
            <h3 className="text-lg font-semibold text-slate-900 dark:text-white mb-4">
              Quick Add Vendor
            </h3>
            <p className="text-sm text-slate-500 dark:text-slate-400 mb-4">
              Create a new vendor and link it to this invoice. You can add more details later.
            </p>
            <div className="space-y-4">
              <div>
                <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-1">
                  Vendor Name *
                </label>
                <input
                  type="text"
                  value={quickAddVendorName}
                  onChange={(e) => setQuickAddVendorName(e.target.value)}
                  placeholder="Enter vendor name"
                  className="w-full px-3 py-2 bg-slate-100 dark:bg-slate-700 border border-slate-300 dark:border-slate-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
                />
              </div>
              <div>
                <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-1">
                  Vendor Type
                </label>
                <select
                  value={quickAddVendorType}
                  onChange={(e) => setQuickAddVendorType(e.target.value)}
                  className="w-full px-3 py-2 bg-slate-100 dark:bg-slate-700 border border-slate-300 dark:border-slate-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500"
                >
                  <option value="business">Business</option>
                  <option value="contractor">Contractor</option>
                  <option value="individual">Individual</option>
                </select>
              </div>
            </div>
            <div className="flex justify-end space-x-3 mt-6">
              <button
                onClick={() => {
                  setShowQuickAddVendor(false);
                  setQuickAddVendorName('');
                  setQuickAddVendorType('business');
                }}
                className="px-4 py-2 bg-slate-100 dark:bg-slate-700 text-slate-700 dark:text-slate-200 rounded-lg hover:bg-slate-200 dark:hover:bg-slate-600"
              >
                Cancel
              </button>
              <button
                onClick={() => {
                  if (quickAddVendorName.trim()) {
                    quickAddVendor.mutate({
                      name: quickAddVendorName.trim(),
                      vendor_type: quickAddVendorType,
                    });
                  }
                }}
                disabled={!quickAddVendorName.trim() || quickAddVendor.isPending}
                className="px-4 py-2 bg-blue-500 text-white rounded-lg hover:bg-blue-600 disabled:opacity-50"
              >
                {quickAddVendor.isPending ? 'Creating...' : 'Create & Link'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
