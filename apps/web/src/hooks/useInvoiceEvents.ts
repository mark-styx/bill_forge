'use client';

import { useEffect, useRef, useCallback } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useAuthStore } from '@/stores/auth';

const MIN_BACKOFF = 1000;
const MAX_BACKOFF = 15000;

/** Opens an SSE connection to /api/v1/invoices/stream and invalidates
 *  React Query caches for the invoice list and any open detail panel
 *  whenever a server-pushed event arrives.
 *
 *  Because the browser `EventSource` API cannot set custom headers,
 *  the JWT is passed as a `?token=` query parameter. The backend
 *  accepts this fallback ONLY on the stream endpoint. */
export function useInvoiceEvents() {
  const queryClient = useQueryClient();
  const accessToken = useAuthStore((state) => state.accessToken);
  const esRef = useRef<EventSource | null>(null);
  const retryRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const backoffRef = useRef(MIN_BACKOFF);

  const connect = useCallback(() => {
    if (esRef.current) return;

    if (!accessToken) return; // not authenticated yet
    if (typeof EventSource === 'undefined') return;

    const url = `/api/v1/invoices/stream?token=${encodeURIComponent(accessToken)}`;
    const es = new EventSource(url);
    esRef.current = es;

    es.onmessage = (e) => {
      // Reset backoff on successful message
      backoffRef.current = MIN_BACKOFF;

      // Invalidate the invoice list query
      queryClient.invalidateQueries({ queryKey: ['invoices'] });

      // Invoice state changes ripple through dashboard KPIs, queue counts,
      // pending approvals, and the audit log - invalidate them all so every
      // subscribed page (invoices, dashboard, processing) refreshes live.
      queryClient.invalidateQueries({ queryKey: ['dashboard-summary'] });
      queryClient.invalidateQueries({ queryKey: ['dashboard-metrics'] });
      queryClient.invalidateQueries({ queryKey: ['dashboard-kpis'] });
      queryClient.invalidateQueries({ queryKey: ['pending-approvals'] });
      queryClient.invalidateQueries({ queryKey: ['work-queues'] });
      queryClient.invalidateQueries({ queryKey: ['audit-recent'] });

      // If the payload contains an invoice_id, also invalidate the detail query
      try {
        const payload = JSON.parse(e.data);
        if (payload?.invoice_id) {
          queryClient.invalidateQueries({
            queryKey: ['invoice', payload.invoice_id],
          });
        }
      } catch {
        // Non-JSON or missing invoice_id - list invalidation is sufficient
      }
    };

    es.onerror = () => {
      es.close();
      esRef.current = null;
      // Exponential backoff: 1s -> 2s -> 4s -> 8s -> 15s cap
      const delay = backoffRef.current;
      backoffRef.current = Math.min(backoffRef.current * 2, MAX_BACKOFF);
      retryRef.current = setTimeout(connect, delay);
    };
  }, [accessToken, queryClient]);

  useEffect(() => {
    connect();
    return () => {
      if (retryRef.current) clearTimeout(retryRef.current);
      esRef.current?.close();
      esRef.current = null;
    };
  }, [connect]);
}
