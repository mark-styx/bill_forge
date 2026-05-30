'use client';

import { useState, useEffect } from 'react';
import { useRouter, useParams } from 'next/navigation';
import Link from 'next/link';
import { invoicesApi, duplicateApi } from '@/lib/api';
import type { Invoice, DuplicateMatch } from '@/lib/api';
import { toast } from 'sonner';
import {
  ArrowLeft,
  CheckCircle,
  AlertTriangle,
  XCircle,
  Loader2,
  GitMerge,
  X as XIcon,
} from 'lucide-react';

type SignalStatus = 'match' | 'fuzzy' | 'diff';

function classifySignal(score: number): SignalStatus {
  if (score >= 0.95) return 'match';
  if (score >= 0.7) return 'fuzzy';
  return 'diff';
}

function SignalBadge({ score, label }: { score: number; label: string }) {
  const status = classifySignal(score);
  const colors: Record<SignalStatus, string> = {
    match: 'bg-green-100 text-green-800 border-green-200',
    fuzzy: 'bg-amber-100 text-amber-800 border-amber-200',
    diff: 'bg-red-100 text-red-800 border-red-200',
  };
  const icons: Record<SignalStatus, typeof CheckCircle> = {
    match: CheckCircle,
    fuzzy: AlertTriangle,
    diff: XCircle,
  };
  const Icon = icons[status];
  return (
    <span className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium border ${colors[status]}`}>
      <Icon className="w-3 h-3" />
      {label}: {(score * 100).toFixed(0)}%
    </span>
  );
}

function FieldRow({ label, newValue, existingValue, signalScore }: {
  label: string;
  newValue: string;
  existingValue: string;
  signalScore?: number;
}) {
  const isMatch = newValue === existingValue;
  const isFuzzy = !isMatch && signalScore != null && signalScore >= 0.7;
  const bgClass = isMatch
    ? 'bg-green-50'
    : isFuzzy
    ? 'bg-amber-50'
    : 'bg-red-50';

  return (
    <tr className={bgClass}>
      <td className="px-4 py-2 text-sm font-medium text-muted-foreground border-b border-border w-36">
        {label}
      </td>
      <td className="px-4 py-2 text-sm text-foreground border-b border-border">
        {newValue || '-'}
      </td>
      <td className="px-4 py-2 text-sm text-foreground border-b border-border">
        {existingValue || '-'}
      </td>
    </tr>
  );
}

export default function DuplicateReviewPage() {
  const router = useRouter();
  const params = useParams();
  const invoiceId = params?.id as string;

  const [invoice, setInvoice] = useState<Invoice | null>(null);
  const [duplicates, setDuplicates] = useState<DuplicateMatch[]>([]);
  const [selectedDup, setSelectedDup] = useState<DuplicateMatch | null>(null);
  const [existingInvoice, setExistingInvoice] = useState<Invoice | null>(null);
  const [loading, setLoading] = useState(true);
  const [actioning, setActioning] = useState(false);

  // Load the new invoice and its potential duplicates from search params
  useEffect(() => {
    if (!invoiceId) return;
    (async () => {
      try {
        const inv = await invoicesApi.get(invoiceId);
        setInvoice(inv);
        // Duplicates come from the query string (passed from upload flow)
        const searchParams = new URLSearchParams(window.location.search);
        const dupsParam = searchParams.get('duplicates');
        if (dupsParam) {
          const parsed: DuplicateMatch[] = JSON.parse(decodeURIComponent(dupsParam));
          setDuplicates(parsed);
          if (parsed.length > 0) {
            setSelectedDup(parsed[0]);
          }
        }
      } catch (e: any) {
        toast.error('Failed to load invoice');
      } finally {
        setLoading(false);
      }
    })();
  }, [invoiceId]);

  // Load the existing invoice when a dup is selected
  useEffect(() => {
    if (!selectedDup) return;
    (async () => {
      try {
        const existing = await invoicesApi.get(selectedDup.existing_invoice_id);
        setExistingInvoice(existing);
      } catch {
        setExistingInvoice(null);
      }
    })();
  }, [selectedDup]);

  const handleMerge = async () => {
    if (!selectedDup) return;
    setActioning(true);
    try {
      await duplicateApi.mergeDuplicate(invoiceId, selectedDup.existing_invoice_id);
      toast.success('Duplicate merged into existing invoice');
      router.push(`/invoices/${selectedDup.existing_invoice_id}`);
    } catch (e: any) {
      toast.error(e?.message ?? 'Merge failed');
    } finally {
      setActioning(false);
    }
  };

  const handleReject = async () => {
    if (!selectedDup) return;
    setActioning(true);
    try {
      await duplicateApi.rejectDuplicate(invoiceId);
      toast.success('Marked as not a duplicate');
      router.push(`/invoices/${invoiceId}`);
    } catch (e: any) {
      toast.error(e?.message ?? 'Reject failed');
    } finally {
      setActioning(false);
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center min-h-[50vh]">
        <Loader2 className="w-8 h-8 text-capture animate-spin" />
      </div>
    );
  }

  if (!invoice) {
    return (
      <div className="max-w-3xl mx-auto space-y-6">
        <p className="text-error">Invoice not found.</p>
        <Link href="/invoices" className="btn btn-secondary">
          <ArrowLeft className="w-4 h-4 mr-2" /> Back to Invoices
        </Link>
      </div>
    );
  }

  const dup = selectedDup;
  const brk = dup?.signal_breakdown;

  return (
    <div className="max-w-5xl mx-auto space-y-6">
      {/* Header */}
      <div>
        <Link
          href="/invoices"
          className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors mb-3"
        >
          <ArrowLeft className="w-4 h-4" />
          Back to Invoices
        </Link>
        <h1 className="text-2xl font-semibold text-foreground">Duplicate Review</h1>
        <p className="text-muted-foreground mt-0.5">
          A potential duplicate was detected. Review the details below and choose an action.
        </p>
      </div>

      {/* Duplicate selector */}
      {duplicates.length > 1 && (
        <div className="flex gap-2 flex-wrap">
          {duplicates.map((d, i) => (
            <button
              key={d.existing_invoice_id}
              onClick={() => setSelectedDup(d)}
              className={`btn text-xs ${d === selectedDup ? 'btn-primary' : 'btn-secondary'}`}
            >
              Match {i + 1} ({(d.score * 100).toFixed(0)}% - {d.severity})
            </button>
          ))}
        </div>
      )}

      {/* Score summary */}
      {dup && brk && (
        <div className="card p-4">
          <div className="flex items-center justify-between mb-3">
            <div>
              <h2 className="font-semibold text-foreground">Similarity Score: {(dup.score * 100).toFixed(1)}%</h2>
              <p className="text-sm text-muted-foreground">Severity: {dup.severity}</p>
            </div>
            <div className="flex flex-wrap gap-2">
              <SignalBadge score={brk.vendor} label="Vendor" />
              <SignalBadge score={brk.invoice_number} label="Inv #" />
              <SignalBadge score={brk.amount} label="Amount" />
              <SignalBadge score={brk.date} label="Date" />
              <SignalBadge score={brk.line_item_fingerprint} label="Line Items" />
            </div>
          </div>
        </div>
      )}

      {/* Side-by-side comparison */}
      {dup && existingInvoice && (
        <div className="card overflow-hidden">
          <div className="grid grid-cols-3 bg-secondary/50 border-b border-border">
            <div className="px-4 py-2 text-sm font-semibold text-muted-foreground">Field</div>
            <div className="px-4 py-2 text-sm font-semibold text-capture">New Invoice</div>
            <div className="px-4 py-2 text-sm font-semibold text-warning">Existing Invoice</div>
          </div>
          <table className="w-full">
            <tbody>
              <FieldRow
                label="Vendor"
                newValue={invoice.vendor_name}
                existingValue={existingInvoice.vendor_name}
                signalScore={brk?.vendor}
              />
              <FieldRow
                label="Invoice #"
                newValue={invoice.invoice_number}
                existingValue={existingInvoice.invoice_number}
                signalScore={brk?.invoice_number}
              />
              <FieldRow
                label="Amount"
                newValue={`$${(invoice.total_amount.amount / 100).toFixed(2)}`}
                existingValue={`$${(existingInvoice.total_amount.amount / 100).toFixed(2)}`}
                signalScore={brk?.amount}
              />
              <FieldRow
                label="Date"
                newValue={invoice.invoice_date ?? '-'}
                existingValue={existingInvoice.invoice_date ?? '-'}
                signalScore={brk?.date}
              />
              <FieldRow
                label="Line Items"
                newValue={`${invoice.line_items.length} items`}
                existingValue={`${existingInvoice.line_items.length} items`}
                signalScore={brk?.line_item_fingerprint}
              />
            </tbody>
          </table>
        </div>
      )}

      {/* Actions */}
      <div className="flex justify-end gap-3">
        <button
          onClick={handleReject}
          disabled={actioning}
          className="btn btn-secondary"
        >
          <XIcon className="w-4 h-4 mr-2" />
          Keep Both / Not a Duplicate
        </button>
        <button
          onClick={handleMerge}
          disabled={actioning}
          className="btn btn-primary"
        >
          {actioning ? (
            <Loader2 className="w-4 h-4 mr-2 animate-spin" />
          ) : (
            <GitMerge className="w-4 h-4 mr-2" />
          )}
          Merge into Existing
        </button>
      </div>
    </div>
  );
}
