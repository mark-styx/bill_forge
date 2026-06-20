'use client';

import { useCallback, useEffect, useMemo, useState } from 'react';
import { addinFetch } from '@/lib/auth/getAddinToken';

const API_BASE = process.env.NEXT_PUBLIC_API_BASE_URL ?? '';

interface OfficeMailbox {
  item?: {
    itemId?: string;
    internetMessageId?: string;
    subject?: string;
    from?: { emailAddress?: string; displayName?: string };
    attachments?: OfficeAttachment[];
    getAttachmentContentAsync?: (
      id: string,
      cb: (result: {
        status: string;
        value?: { content: string; format: string };
      }) => void,
    ) => void;
  };
}

interface OfficeAttachment {
  id: string;
  name: string;
  contentType?: string;
  attachmentType?: string;
  size?: number;
  isInline?: boolean;
}

interface Lookup {
  invoice_id: string;
  vendor: { id: string | null; name: string; email: string | null };
  totals: {
    subtotal_cents: number | null;
    tax_amount_cents: number | null;
    total_amount_cents: number;
    currency: string;
    invoice_number: string;
    invoice_date: string | null;
    due_date: string | null;
  };
  line_items: Array<{
    description: string | null;
    quantity: number | null;
    unit_price_cents: number | null;
    total_cents: number | null;
  }>;
  gl_coding: Array<{
    gl_code: string | null;
    department: string | null;
    cost_center: string | null;
  }>;
  policy_warnings: Array<{ severity: string; message: string }>;
  vendor_history_summary: {
    invoice_count_last_90d: number;
    total_spend_cents_last_90d: number;
    last_invoice_date: string | null;
  };
  comments: Array<{ author: string; body: string; created_at: string }>;
  approval_state: string;
}

interface MessageContext {
  messageId: string;
  fromAddress: string;
  subject: string;
  attachments: OfficeAttachment[];
}

function formatCents(cents: number | null | undefined, currency = 'USD'): string {
  if (cents == null) return '—';
  return new Intl.NumberFormat('en-US', { style: 'currency', currency }).format(
    cents / 100,
  );
}

function readOfficeContext(): MessageContext | null {
  const office = (globalThis as { Office?: { context?: { mailbox?: OfficeMailbox } } }).Office;
  const item = office?.context?.mailbox?.item;
  if (!item) return null;
  const fromAddress = item.from?.emailAddress ?? '';
  const messageId = item.internetMessageId ?? item.itemId ?? '';
  return {
    messageId,
    fromAddress,
    subject: item.subject ?? '',
    attachments: (item.attachments ?? []).filter((a) => !a.isInline),
  };
}

function getAttachmentBase64(attachmentId: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const office = (globalThis as { Office?: { context?: { mailbox?: OfficeMailbox } } }).Office;
    const item = office?.context?.mailbox?.item;
    if (!item?.getAttachmentContentAsync) {
      reject(new Error('getAttachmentContentAsync unavailable'));
      return;
    }
    item.getAttachmentContentAsync(attachmentId, (result) => {
      if (result.status === 'succeeded' && result.value) {
        resolve(result.value.content);
      } else {
        reject(new Error('Failed to read attachment content'));
      }
    });
  });
}

function base64ToBlob(b64: string, contentType: string): Blob {
  const binary = atob(b64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
  return new Blob([bytes], { type: contentType });
}

export default function AddinTaskpane() {
  const [context, setContext] = useState<MessageContext | null>(null);
  const [lookup, setLookup] = useState<Lookup | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [toast, setToast] = useState<string | null>(null);

  useEffect(() => {
    setContext(readOfficeContext());
  }, []);

  useEffect(() => {
    if (!context) return;
    let cancelled = false;
    setLoading(true);
    setError(null);
    const params = new URLSearchParams({
      message_id: context.messageId,
      from_address: context.fromAddress,
      subject: context.subject,
    });
    addinFetch(`${API_BASE}/api/v1/addin/lookup?${params.toString()}`)
      .then(async (res) => {
        if (cancelled) return;
        if (res.status === 404) {
          setLookup(null);
          setError('No matching BillForge invoice for this email.');
        } else if (!res.ok) {
          setError(`Lookup failed (${res.status})`);
        } else {
          const body = (await res.json()) as Lookup;
          setLookup(body);
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(err instanceof Error ? err.message : 'Lookup failed');
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [context]);

  const previewUrl = useMemo(() => {
    if (!lookup) return null;
    return `${API_BASE}/invoices/${lookup.invoice_id}/preview`;
  }, [lookup]);

  const approve = useCallback(async () => {
    if (!lookup) return;
    setBusy(true);
    try {
      const res = await addinFetch(`${API_BASE}/api/v1/addin/approve`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ invoice_id: lookup.invoice_id }),
      });
      if (!res.ok) throw new Error(`Approve failed (${res.status})`);
      setToast('Invoice approved');
    } catch (err) {
      setToast(err instanceof Error ? err.message : 'Approve failed');
    } finally {
      setBusy(false);
    }
  }, [lookup]);

  const reject = useCallback(async () => {
    if (!lookup) return;
    const reason = window.prompt('Reason for rejection?', '');
    if (reason == null) return;
    setBusy(true);
    try {
      const res = await addinFetch(`${API_BASE}/api/v1/addin/reject`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ invoice_id: lookup.invoice_id, reason }),
      });
      if (!res.ok) throw new Error(`Reject failed (${res.status})`);
      setToast('Invoice rejected');
    } catch (err) {
      setToast(err instanceof Error ? err.message : 'Reject failed');
    } finally {
      setBusy(false);
    }
  }, [lookup]);

  const pushToBillForge = useCallback(
    async (att: OfficeAttachment) => {
      if (!context) return;
      setBusy(true);
      try {
        const base64 = await getAttachmentBase64(att.id);
        const blob = base64ToBlob(base64, att.contentType ?? 'application/octet-stream');
        const form = new FormData();
        form.append('bytes', blob, att.name);
        form.append('filename', att.name);
        form.append('content_type', att.contentType ?? 'application/octet-stream');
        form.append('source_message_id', context.messageId);
        form.append('from_address', context.fromAddress);
        const res = await addinFetch(`${API_BASE}/api/v1/addin/ingest-attachment`, {
          method: 'POST',
          body: form,
        });
        if (!res.ok) throw new Error(`Push failed (${res.status})`);
        setToast(`Pushed ${att.name} to BillForge`);
      } catch (err) {
        setToast(err instanceof Error ? err.message : 'Push failed');
      } finally {
        setBusy(false);
      }
    },
    [context],
  );

  if (!context) {
    return (
      <div className="p-4 text-sm text-gray-600">
        Waiting for Outlook to provide the active message…
      </div>
    );
  }

  if (loading) {
    return <div className="p-4 text-sm text-gray-600" data-testid="loading">Looking up invoice…</div>;
  }

  if (!lookup) {
    const ingestableAttachments = context.attachments.filter(
      (a) => !a.isInline && (a.attachmentType ?? 'file') === 'file',
    );
    return (
      <div className="space-y-4 p-4 text-sm">
        <p className="text-gray-600" data-testid="not-found-message">
          {error ?? 'No matching BillForge invoice for this email.'}
        </p>
        {ingestableAttachments.length > 0 && (
          <div className="space-y-2" data-testid="push-attachments">
            <h2 className="text-base font-semibold">Attachments</h2>
            {ingestableAttachments.map((att) => (
              <div
                key={att.id}
                className="flex items-center justify-between rounded border border-gray-200 bg-white p-2"
              >
                <div>
                  <div className="font-medium">{att.name}</div>
                  <div className="text-xs text-gray-500">{att.contentType}</div>
                </div>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => pushToBillForge(att)}
                  className="rounded bg-blue-600 px-3 py-1 text-white disabled:opacity-50"
                >
                  Push to BillForge
                </button>
              </div>
            ))}
          </div>
        )}
        {toast && <div className="text-xs text-gray-600">{toast}</div>}
      </div>
    );
  }

  return (
    <div className="space-y-4 p-4 text-sm">
      <header className="flex items-start justify-between">
        <div>
          <div className="text-xs uppercase tracking-wide text-gray-500">
            {lookup.vendor.name}
          </div>
          <h1 className="text-lg font-semibold">{lookup.totals.invoice_number}</h1>
          <div className="text-base font-medium">
            {formatCents(lookup.totals.total_amount_cents, lookup.totals.currency)}
          </div>
        </div>
        <span className="rounded bg-gray-100 px-2 py-1 text-xs uppercase tracking-wide text-gray-700">
          {lookup.approval_state}
        </span>
      </header>

      <section data-testid="preview">
        <h2 className="mb-1 text-xs font-semibold uppercase tracking-wide text-gray-500">
          Preview
        </h2>
        {previewUrl ? (
          <iframe
            title="Invoice preview"
            src={previewUrl}
            className="h-48 w-full rounded border border-gray-200 bg-gray-50"
          />
        ) : (
          <div className="rounded border border-dashed border-gray-300 p-3 text-xs text-gray-500">
            No preview available
          </div>
        )}
      </section>

      <section data-testid="line-items">
        <h2 className="mb-1 text-xs font-semibold uppercase tracking-wide text-gray-500">
          Line items
        </h2>
        {lookup.line_items.length === 0 ? (
          <div className="text-xs text-gray-500">No line items captured.</div>
        ) : (
          <table className="w-full table-fixed text-xs">
            <thead>
              <tr className="text-left text-gray-500">
                <th className="py-1">Description</th>
                <th className="w-12 py-1">Qty</th>
                <th className="w-20 py-1 text-right">Total</th>
              </tr>
            </thead>
            <tbody>
              {lookup.line_items.map((li, idx) => (
                <tr key={idx} className="border-t border-gray-100">
                  <td className="py-1 pr-2">{li.description ?? '—'}</td>
                  <td className="py-1">{li.quantity ?? '—'}</td>
                  <td className="py-1 text-right">
                    {formatCents(li.total_cents, lookup.totals.currency)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>

      <section data-testid="gl-coding">
        <h2 className="mb-1 text-xs font-semibold uppercase tracking-wide text-gray-500">
          GL coding
        </h2>
        <dl className="grid grid-cols-2 gap-x-3 gap-y-1 text-xs">
          {lookup.gl_coding.map((entry, idx) => (
            <div key={idx} className="contents">
              <dt className="text-gray-500">GL code</dt>
              <dd>{entry.gl_code ?? '—'}</dd>
              <dt className="text-gray-500">Department</dt>
              <dd>{entry.department ?? '—'}</dd>
              <dt className="text-gray-500">Cost center</dt>
              <dd>{entry.cost_center ?? '—'}</dd>
            </div>
          ))}
        </dl>
      </section>

      <section data-testid="vendor-history">
        <h2 className="mb-1 text-xs font-semibold uppercase tracking-wide text-gray-500">
          Vendor history (90d)
        </h2>
        <div className="text-xs text-gray-700">
          {lookup.vendor_history_summary.invoice_count_last_90d} invoices ·{' '}
          {formatCents(
            lookup.vendor_history_summary.total_spend_cents_last_90d,
            lookup.totals.currency,
          )}
        </div>
      </section>

      {lookup.policy_warnings.length > 0 && (
        <section data-testid="policy-warnings">
          <h2 className="mb-1 text-xs font-semibold uppercase tracking-wide text-gray-500">
            Policy warnings
          </h2>
          <ul className="space-y-1">
            {lookup.policy_warnings.map((warn, idx) => (
              <li
                key={idx}
                className={
                  warn.severity === 'error'
                    ? 'rounded bg-red-50 px-2 py-1 text-xs text-red-700'
                    : 'rounded bg-amber-50 px-2 py-1 text-xs text-amber-700'
                }
              >
                {warn.message}
              </li>
            ))}
          </ul>
        </section>
      )}

      <section data-testid="comments">
        <h2 className="mb-1 text-xs font-semibold uppercase tracking-wide text-gray-500">
          Comments
        </h2>
        {lookup.comments.length === 0 ? (
          <div className="text-xs text-gray-500">No comments yet.</div>
        ) : (
          <ul className="space-y-1">
            {lookup.comments.map((c, idx) => (
              <li key={idx} className="rounded bg-gray-50 px-2 py-1 text-xs">
                <div className="font-medium text-gray-700">{c.author}</div>
                <div className="text-gray-600">{c.body}</div>
              </li>
            ))}
          </ul>
        )}
      </section>

      <footer className="flex items-center justify-end gap-2 border-t border-gray-200 pt-3">
        <button
          type="button"
          onClick={reject}
          disabled={busy}
          className="rounded border border-red-300 px-3 py-1 text-red-700 disabled:opacity-50"
        >
          Reject
        </button>
        <button
          type="button"
          onClick={approve}
          disabled={busy}
          className="rounded bg-green-600 px-3 py-1 text-white disabled:opacity-50"
        >
          Approve
        </button>
      </footer>
      {toast && (
        <div className="text-xs text-gray-600" data-testid="toast">
          {toast}
        </div>
      )}
    </div>
  );
}
