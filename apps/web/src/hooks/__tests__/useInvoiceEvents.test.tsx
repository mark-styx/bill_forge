import { renderHook } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { createElement, type ReactNode } from 'react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { useInvoiceEvents } from '../useInvoiceEvents';

// Mock useAuthStore
const mockAuthStore = {
  accessToken: 'test-jwt-token',
};
vi.mock('@/stores/auth', () => ({
  useAuthStore: {
    getState: () => mockAuthStore,
  },
}));

// Mock EventSource
class MockEventSource {
  static instances: MockEventSource[] = [];
  url: string;
  onmessage: ((e: MessageEvent) => void) | null = null;
  onerror: (() => void) | null = null;
  onopen: (() => void) | null = null;
  readyState = 0;

  constructor(url: string) {
    this.url = url;
    MockEventSource.instances.push(this);
  }

  close() { this.readyState = 2; }

  /** Simulate an incoming SSE message */
  dispatch(data: object) {
    if (this.onmessage) {
      this.onmessage(new MessageEvent('message', { data: JSON.stringify(data) }));
    }
  }
}

const originalES = global.EventSource;
beforeEach(() => { MockEventSource.instances = []; (global as any).EventSource = MockEventSource; });
afterEach(() => { (global as any).EventSource = originalES; });

function createWrapper() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client: qc }, children);
}

describe('useInvoiceEvents', () => {
  it('opens an EventSource with the JWT token as a query parameter', () => {
    const { unmount } = renderHook(() => useInvoiceEvents(), { wrapper: createWrapper() });
    expect(MockEventSource.instances.length).toBe(1);
    expect(MockEventSource.instances[0].url).toBe('/api/v1/invoices/stream?token=test-jwt-token');
    unmount();
  });

  it('does not open EventSource when no access token is present', () => {
    const origToken = mockAuthStore.accessToken;
    mockAuthStore.accessToken = null as any;
    const { unmount } = renderHook(() => useInvoiceEvents(), { wrapper: createWrapper() });
    expect(MockEventSource.instances.length).toBe(0);
    mockAuthStore.accessToken = origToken;
    unmount();
  });

  it('invalidates invoice list and detail queries on message', () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const wrapper = ({ children }: { children: ReactNode }) =>
      createElement(QueryClientProvider, { client: qc }, children);

    const invalidateSpy = vi.spyOn(qc, 'invalidateQueries');
    const { unmount } = renderHook(() => useInvoiceEvents(), { wrapper });

    const es = MockEventSource.instances[0];
    es.dispatch({ invoice_id: 'abc-123', status: 'approved', kind: 'status_changed' });

    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['invoices'] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['invoice', 'abc-123'] });

    unmount();
  });

  it('closes EventSource on unmount', () => {
    const { unmount } = renderHook(() => useInvoiceEvents(), { wrapper: createWrapper() });
    const es = MockEventSource.instances[0];
    const closeSpy = vi.spyOn(es, 'close');
    unmount();
    expect(closeSpy).toHaveBeenCalled();
  });
});
