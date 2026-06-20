'use client';

import { Fragment, useState, useEffect, useCallback } from 'react';
import { useSearchParams } from 'next/navigation';
import { FileText } from 'lucide-react';
import { vendorPortalApi } from '@/lib/api';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';

interface InvoiceRow {
  id: string;
  invoice_number: string;
  invoice_date: string | null;
  due_date: string | null;
  total_amount: number;
  currency: string;
  processing_status: string;
}

interface ThreadMessage {
  id: string;
  invoice_id: string;
  sender_kind: 'vendor' | 'ap_user';
  sender_user_id: string | null;
  sender_vendor_contact_id: string | null;
  body: string;
  created_at: string;
}

function formatRelative(iso: string): string {
  const diffMs = Date.now() - new Date(iso).getTime();
  if (diffMs < 0) return 'just now';
  const sec = Math.floor(diffMs / 1000);
  if (sec < 60) return `${sec}s ago`;
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h ago`;
  const day = Math.floor(hr / 24);
  return `${day}d ago`;
}

const STATUS_COLORS: Record<string, string> = {
  submitted: 'bg-blue-100 text-blue-800',
  pending_review: 'bg-yellow-100 text-yellow-800',
  pending_approval: 'bg-orange-100 text-orange-800',
  approved: 'bg-green-100 text-green-800',
  ready_for_payment: 'bg-emerald-100 text-emerald-800',
  paid: 'bg-green-200 text-green-900',
  on_hold: 'bg-red-100 text-red-800',
  rejected: 'bg-red-200 text-red-900',
  draft: 'bg-gray-100 text-gray-800',
};

function StatusBadge({ status }: { status: string }) {
  const color = STATUS_COLORS[status] || 'bg-gray-100 text-gray-800';
  return (
    <span className={`inline-flex items-center px-2 py-0.5 rounded text-xs font-medium ${color}`}>
      {status.replace(/_/g, ' ')}
    </span>
  );
}

function formatCents(cents: number, currency: string) {
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency,
  }).format(cents / 100);
}

export default function VendorPortalPage() {
  const searchParams = useSearchParams();
  const [token, setToken] = useState<string | null>(null);
  const [invoices, setInvoices] = useState<InvoiceRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Tab state
  const [activeTab, setActiveTab] = useState<'form' | 'upload'>('form');

  // Form state
  const [invoiceNumber, setInvoiceNumber] = useState('');
  const [invoiceDate, setInvoiceDate] = useState('');
  const [dueDate, setDueDate] = useState('');
  const [amount, setAmount] = useState('');
  const [currency, setCurrency] = useState('USD');
  const [notes, setNotes] = useState('');
  const [submitting, setSubmitting] = useState(false);

  // Upload state
  const [pdfFile, setPdfFile] = useState<File | null>(null);
  const [uploadInvoiceNumber, setUploadInvoiceNumber] = useState('');
  const [uploadAmount, setUploadAmount] = useState('');
  const [uploadNotes, setUploadNotes] = useState('');
  const [uploading, setUploading] = useState(false);

  // Per-invoice message thread state
  const [openThreadId, setOpenThreadId] = useState<string | null>(null);
  const [threadsById, setThreadsById] = useState<Record<string, ThreadMessage[]>>({});
  const [threadLoadingId, setThreadLoadingId] = useState<string | null>(null);
  const [threadErrorById, setThreadErrorById] = useState<Record<string, string | null>>({});
  const [draftById, setDraftById] = useState<Record<string, string>>({});
  const [sendingThreadId, setSendingThreadId] = useState<string | null>(null);

  const loadThread = useCallback(
    async (invoiceId: string) => {
      if (!token) return;
      setThreadLoadingId(invoiceId);
      setThreadErrorById((prev) => ({ ...prev, [invoiceId]: null }));
      try {
        const data = await vendorPortalApi.listInvoiceMessages(token, invoiceId);
        setThreadsById((prev) => ({ ...prev, [invoiceId]: data }));
      } catch (err: any) {
        setThreadErrorById((prev) => ({
          ...prev,
          [invoiceId]: err?.message || 'Failed to load messages',
        }));
      } finally {
        setThreadLoadingId(null);
      }
    },
    [token],
  );

  const handleToggleThread = (invoiceId: string) => {
    if (openThreadId === invoiceId) {
      setOpenThreadId(null);
      return;
    }
    setOpenThreadId(invoiceId);
    if (!threadsById[invoiceId]) {
      void loadThread(invoiceId);
    }
  };

  const handleSendMessage = async (invoiceId: string) => {
    if (!token) return;
    const draft = (draftById[invoiceId] ?? '').trim();
    if (!draft) return;
    setSendingThreadId(invoiceId);
    setThreadErrorById((prev) => ({ ...prev, [invoiceId]: null }));

    const optimistic: ThreadMessage = {
      id: `pending-${Date.now()}`,
      invoice_id: invoiceId,
      sender_kind: 'vendor',
      sender_user_id: null,
      sender_vendor_contact_id: null,
      body: draft,
      created_at: new Date().toISOString(),
    };
    setThreadsById((prev) => ({
      ...prev,
      [invoiceId]: [...(prev[invoiceId] ?? []), optimistic],
    }));
    setDraftById((prev) => ({ ...prev, [invoiceId]: '' }));

    try {
      const saved = await vendorPortalApi.postInvoiceMessage(token, invoiceId, draft);
      setThreadsById((prev) => ({
        ...prev,
        [invoiceId]: (prev[invoiceId] ?? []).map((m) => (m.id === optimistic.id ? saved : m)),
      }));
    } catch (err: any) {
      setThreadsById((prev) => ({
        ...prev,
        [invoiceId]: (prev[invoiceId] ?? []).filter((m) => m.id !== optimistic.id),
      }));
      setDraftById((prev) => ({ ...prev, [invoiceId]: draft }));
      setThreadErrorById((prev) => ({
        ...prev,
        [invoiceId]: err?.message || 'Failed to send message',
      }));
    } finally {
      setSendingThreadId(null);
    }
  };

  // Extract and store token
  useEffect(() => {
    const queryToken = searchParams.get('token');
    if (queryToken) {
      localStorage.setItem('vendor_portal_token', queryToken);
      setToken(queryToken);
    } else {
      const stored = localStorage.getItem('vendor_portal_token');
      if (stored) {
        setToken(stored);
      } else {
        setLoading(false);
        setError('No access token provided. Please use the link sent by the AP team.');
      }
    }
  }, [searchParams]);

  const fetchInvoices = useCallback(async () => {
    if (!token) return;
    try {
      setLoading(true);
      setError(null);
      const data = await vendorPortalApi.listInvoices(token);
      setInvoices(data);
    } catch (err: any) {
      if (err?.status === 401) {
        setError('Your access token is invalid or expired. Please request a new link.');
        localStorage.removeItem('vendor_portal_token');
        setToken(null);
      } else {
        setError(err?.message || 'Failed to load invoices');
      }
    } finally {
      setLoading(false);
    }
  }, [token]);

  useEffect(() => {
    fetchInvoices();
  }, [fetchInvoices]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!token) return;

    setSubmitting(true);
    setError(null);

    try {
      const amountCents = Math.round(parseFloat(amount) * 100);
      if (isNaN(amountCents) || amountCents <= 0) {
        setError('Please enter a valid amount');
        return;
      }

      await vendorPortalApi.submitInvoice(token, {
        invoice_number: invoiceNumber,
        invoice_date: invoiceDate || undefined,
        due_date: dueDate || undefined,
        amount: amountCents,
        currency: currency || undefined,
        notes: notes || undefined,
      });

      setInvoiceNumber('');
      setInvoiceDate('');
      setDueDate('');
      setAmount('');
      setNotes('');
      await fetchInvoices();
    } catch (err: any) {
      setError(err?.message || 'Failed to submit invoice');
    } finally {
      setSubmitting(false);
    }
  };

  const handlePdfUpload = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!token || !pdfFile) return;

    if (pdfFile.type !== 'application/pdf') {
      setError('Only PDF files are accepted');
      return;
    }
    if (pdfFile.size > 15 * 1024 * 1024) {
      setError('File size must be under 15 MB');
      return;
    }

    setUploading(true);
    setError(null);

    try {
      const formData = new FormData();
      formData.append('file', pdfFile);
      formData.append('invoice_number', uploadInvoiceNumber);
      if (uploadAmount) {
        const cents = Math.round(parseFloat(uploadAmount) * 100);
        if (!isNaN(cents) && cents > 0) {
          formData.append('amount', String(cents));
        }
      }
      if (uploadNotes) {
        formData.append('notes', uploadNotes);
      }

      await vendorPortalApi.uploadInvoicePdf(token, formData);

      setPdfFile(null);
      setUploadInvoiceNumber('');
      setUploadAmount('');
      setUploadNotes('');
      await fetchInvoices();
    } catch (err: any) {
      setError(err?.message || 'Failed to upload invoice');
    } finally {
      setUploading(false);
    }
  };

  if (!token) {
    return (
      <div className="min-h-screen bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 flex items-center justify-center p-4">
        <div className="w-full max-w-md bg-card border border-border rounded-2xl shadow-2xl p-8 text-center">
          <div className="inline-flex items-center justify-center w-12 h-12 rounded-xl bg-red-500/20 mb-3">
            <FileText className="w-6 h-6 text-red-400" />
          </div>
          <h1 className="text-xl font-bold text-foreground mb-2">Access Required</h1>
          <p className="text-muted-foreground text-sm">
            {error || 'No access token found. Please use the link provided by the AP team.'}
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 p-4 md:p-8">
      <div className="max-w-4xl mx-auto">
        <div className="text-center mb-8">
          <div className="inline-flex items-center justify-center w-12 h-12 rounded-xl bg-blue-500/20 mb-3">
            <FileText className="w-6 h-6 text-blue-400" />
          </div>
          <h1 className="text-2xl font-bold text-foreground">Vendor Portal</h1>
          <p className="text-muted-foreground mt-1">Submit invoices and track payment status</p>
        </div>

        {error && (
          <div className="mb-6 p-3 bg-red-500/10 border border-red-500/20 rounded-lg text-red-400 text-sm">
            {error}
          </div>
        )}

        <div className="bg-card border border-border rounded-xl shadow-lg p-6 mb-6">
          <div className="flex items-center gap-4 mb-4">
            <h2 className="text-lg font-semibold text-foreground">Submit Invoice</h2>
            <div className="flex rounded-lg border border-border overflow-hidden">
              <button
                type="button"
                onClick={() => setActiveTab('form')}
                className={`px-3 py-1.5 text-sm font-medium transition-colors ${activeTab === 'form' ? 'bg-primary text-primary-foreground' : 'bg-background text-muted-foreground hover:bg-muted'}`}
              >
                Structured Form
              </button>
              <button
                type="button"
                onClick={() => setActiveTab('upload')}
                className={`px-3 py-1.5 text-sm font-medium transition-colors ${activeTab === 'upload' ? 'bg-primary text-primary-foreground' : 'bg-background text-muted-foreground hover:bg-muted'}`}
              >
                Upload PDF
              </button>
            </div>
          </div>

          {activeTab === 'form' && (
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div>
                <Label htmlFor="invoiceNumber">Invoice Number</Label>
                <Input id="invoiceNumber" placeholder="INV-001" value={invoiceNumber} onChange={(e) => setInvoiceNumber(e.target.value)} required className="mt-1" />
              </div>
              <div>
                <Label htmlFor="amount">Amount</Label>
                <div className="flex gap-2 mt-1">
                  <Input id="amount" type="number" step="0.01" min="0.01" placeholder="0.00" value={amount} onChange={(e) => setAmount(e.target.value)} required className="flex-1" />
                  <select value={currency} onChange={(e) => setCurrency(e.target.value)} className="rounded-md border border-input bg-background px-3 py-2 text-sm">
                    <option value="USD">USD</option>
                    <option value="EUR">EUR</option>
                    <option value="GBP">GBP</option>
                    <option value="CAD">CAD</option>
                  </select>
                </div>
              </div>
              <div>
                <Label htmlFor="invoiceDate">Invoice Date</Label>
                <Input id="invoiceDate" type="date" value={invoiceDate} onChange={(e) => setInvoiceDate(e.target.value)} className="mt-1" />
              </div>
              <div>
                <Label htmlFor="dueDate">Due Date</Label>
                <Input id="dueDate" type="date" value={dueDate} onChange={(e) => setDueDate(e.target.value)} className="mt-1" />
              </div>
            </div>
            <div>
              <Label htmlFor="notes">Notes (optional)</Label>
              <Input id="notes" placeholder="Additional details" value={notes} onChange={(e) => setNotes(e.target.value)} className="mt-1" />
            </div>
            <Button type="submit" disabled={submitting} className="w-full md:w-auto">
              {submitting ? 'Submitting...' : 'Submit Invoice'}
            </Button>
          </form>
          )}

          {activeTab === 'upload' && (
          <form onSubmit={handlePdfUpload} className="space-y-4">
            <div>
              <Label htmlFor="pdfFile">PDF Invoice</Label>
              <input
                id="pdfFile"
                type="file"
                accept="application/pdf"
                onChange={(e) => setPdfFile(e.target.files?.[0] ?? null)}
                required
                className="mt-1 block w-full text-sm text-muted-foreground file:mr-4 file:py-2 file:px-4 file:rounded-md file:border-0 file:text-sm file:font-medium file:bg-primary/10 file:text-primary hover:file:bg-primary/20"
              />
              {pdfFile && (
                <p className="mt-1 text-xs text-muted-foreground">
                  {pdfFile.name} ({(pdfFile.size / 1024 / 1024).toFixed(2)} MB)
                </p>
              )}
            </div>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div>
                <Label htmlFor="uploadInvoiceNumber">Invoice Number</Label>
                <Input id="uploadInvoiceNumber" placeholder="INV-001" value={uploadInvoiceNumber} onChange={(e) => setUploadInvoiceNumber(e.target.value)} required className="mt-1" />
              </div>
              <div>
                <Label htmlFor="uploadAmount">Amount (optional)</Label>
                <Input id="uploadAmount" type="number" step="0.01" min="0.01" placeholder="0.00" value={uploadAmount} onChange={(e) => setUploadAmount(e.target.value)} className="mt-1" />
              </div>
            </div>
            <div>
              <Label htmlFor="uploadNotes">Notes (optional)</Label>
              <Input id="uploadNotes" placeholder="Additional details" value={uploadNotes} onChange={(e) => setUploadNotes(e.target.value)} className="mt-1" />
            </div>
            <Button type="submit" disabled={uploading || !pdfFile} className="w-full md:w-auto">
              {uploading ? 'Uploading...' : 'Upload PDF Invoice'}
            </Button>
          </form>
          )}
        </div>

        <div className="bg-card border border-border rounded-xl shadow-lg p-6">
          <h2 className="text-lg font-semibold text-foreground mb-4">Invoice History</h2>
          {loading ? (
            <p className="text-muted-foreground text-sm">Loading invoices...</p>
          ) : invoices.length === 0 ? (
            <p className="text-muted-foreground text-sm">No invoices found. Submit your first invoice above.</p>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-border">
                    <th className="text-left py-2 px-3 text-muted-foreground font-medium">Invoice #</th>
                    <th className="text-left py-2 px-3 text-muted-foreground font-medium">Date</th>
                    <th className="text-left py-2 px-3 text-muted-foreground font-medium">Due</th>
                    <th className="text-right py-2 px-3 text-muted-foreground font-medium">Amount</th>
                    <th className="text-center py-2 px-3 text-muted-foreground font-medium">Status</th>
                    <th className="text-right py-2 px-3 text-muted-foreground font-medium">Thread</th>
                  </tr>
                </thead>
                <tbody>
                  {invoices.map((inv) => {
                    const isOpen = openThreadId === inv.id;
                    const thread = threadsById[inv.id] ?? [];
                    const threadError = threadErrorById[inv.id];
                    return (
                      <Fragment key={inv.id}>
                        <tr className="border-b border-border/50 hover:bg-muted/30">
                          <td className="py-2 px-3 font-medium">{inv.invoice_number}</td>
                          <td className="py-2 px-3 text-muted-foreground">{inv.invoice_date || '-'}</td>
                          <td className="py-2 px-3 text-muted-foreground">{inv.due_date || '-'}</td>
                          <td className="py-2 px-3 text-right font-medium">{formatCents(inv.total_amount, inv.currency)}</td>
                          <td className="py-2 px-3 text-center"><StatusBadge status={inv.processing_status} /></td>
                          <td className="py-2 px-3 text-right">
                            <button
                              type="button"
                              aria-label={`Toggle messages for ${inv.invoice_number}`}
                              onClick={() => handleToggleThread(inv.id)}
                              className="text-xs font-medium text-primary hover:underline"
                            >
                              {isOpen ? 'Hide messages' : 'Messages'}
                            </button>
                          </td>
                        </tr>
                        {isOpen && (
                          <tr className="bg-muted/10">
                            <td colSpan={6} className="px-3 py-3">
                              <div data-testid={`thread-${inv.id}`} className="space-y-3">
                                {threadLoadingId === inv.id && (
                                  <p className="text-xs text-muted-foreground">Loading messages...</p>
                                )}
                                {threadError && (
                                  <p className="text-xs text-red-400">{threadError}</p>
                                )}
                                {thread.length === 0 && threadLoadingId !== inv.id && !threadError && (
                                  <p className="text-xs text-muted-foreground">No messages yet.</p>
                                )}
                                {thread.length > 0 && (
                                  <ul className="space-y-2">
                                    {thread.map((m) => (
                                      <li
                                        key={m.id}
                                        className="rounded border border-border/60 bg-card/60 p-2 text-sm"
                                      >
                                        <div className="flex items-center justify-between text-xs text-muted-foreground">
                                          <span className="font-medium">
                                            {m.sender_kind === 'vendor' ? 'Vendor' : 'AP team'}
                                          </span>
                                          <span>{formatRelative(m.created_at)}</span>
                                        </div>
                                        <p className="mt-1 whitespace-pre-wrap text-foreground">{m.body}</p>
                                      </li>
                                    ))}
                                  </ul>
                                )}
                                <div className="flex flex-col gap-2 sm:flex-row sm:items-end">
                                  <textarea
                                    aria-label={`Message body for ${inv.invoice_number}`}
                                    value={draftById[inv.id] ?? ''}
                                    onChange={(e) =>
                                      setDraftById((prev) => ({ ...prev, [inv.id]: e.target.value }))
                                    }
                                    placeholder="Reply to the AP team..."
                                    rows={2}
                                    className="flex-1 rounded-md border border-border bg-background px-2 py-1 text-sm"
                                  />
                                  <Button
                                    type="button"
                                    disabled={
                                      sendingThreadId === inv.id ||
                                      !((draftById[inv.id] ?? '').trim())
                                    }
                                    onClick={() => handleSendMessage(inv.id)}
                                  >
                                    {sendingThreadId === inv.id ? 'Sending...' : 'Send'}
                                  </Button>
                                </div>
                              </div>
                            </td>
                          </tr>
                        )}
                      </Fragment>
                    );
                  })}
                </tbody>
              </table>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
