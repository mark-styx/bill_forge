import {
  ApprovalItem,
  KVStore,
  cacheApprovals,
  getCachedApprovals,
  enqueueAction,
  getQueuedActions,
  clearAction,
  flushQueue,
  ApiClient,
  ConflictError,
  NetworkError,
} from './offline-queue';

// In-memory KVStore for testing
function createStore(): KVStore {
  const map = new Map<string, string>();
  return {
    async getItem(key: string) {
      return map.get(key) ?? null;
    },
    async setItem(key: string, value: string) {
      map.set(key, value);
    },
  };
}

function makeApproval(id: string): ApprovalItem {
  return {
    id,
    invoice: {
      id: `inv-${id}`,
      vendor_name: `Vendor ${id}`,
      invoice_number: `INV-${id}`,
      total_amount_cents: 10000,
      currency: 'USD',
      due_date: '2026-06-01',
      status: 'pending_approval',
      days_until_due: 7,
      requires_action: true,
      created_at: '2026-05-25T00:00:00Z',
    },
    requested_at: '2026-05-25T00:00:00Z',
    expires_at: null,
    can_approve: true,
  };
}

describe('offline-queue', () => {
  let store: KVStore;

  beforeEach(() => {
    store = createStore();
  });

  // ---- Cache ----

  describe('cacheApprovals / getCachedApprovals', () => {
    it('returns empty array when nothing is cached', async () => {
      const result = await getCachedApprovals(store);
      expect(result).toEqual([]);
    });

    it('round-trips cached approvals', async () => {
      const items = [makeApproval('a1'), makeApproval('a2')];
      await cacheApprovals(store, items);
      const result = await getCachedApprovals(store);
      expect(result).toEqual(items);
    });

    it('survives a read after write (offline render path)', async () => {
      const items = [makeApproval('a1')];
      await cacheApprovals(store, items);
      // Simulate app restart by reading from a fresh reference
      const result = await getCachedApprovals(store);
      expect(result).toHaveLength(1);
      expect(result[0].id).toBe('a1');
    });
  });

  // ---- Action queue ----

  describe('enqueueAction', () => {
    it('persists the action and optimistically removes the item from cache', async () => {
      const items = [makeApproval('a1'), makeApproval('a2')];
      await cacheApprovals(store, items);

      const queued = await enqueueAction(store, {
        approvalId: 'a1',
        kind: 'approve',
        payload: 'Looks good',
      });

      expect(queued.actionId).toBeTruthy();
      expect(queued.approvalId).toBe('a1');
      expect(queued.kind).toBe('approve');
      expect(queued.payload).toBe('Looks good');

      // Action is in the queue
      const actions = await getQueuedActions(store);
      expect(actions).toHaveLength(1);
      expect(actions[0].actionId).toBe(queued.actionId);

      // a1 is removed from cache
      const cached = await getCachedApprovals(store);
      expect(cached).toHaveLength(1);
      expect(cached[0].id).toBe('a2');
    });

    it('enqueues multiple actions', async () => {
      await enqueueAction(store, { approvalId: 'a1', kind: 'approve', payload: '' });
      await enqueueAction(store, { approvalId: 'a2', kind: 'reject', payload: 'Bad' });

      const actions = await getQueuedActions(store);
      expect(actions).toHaveLength(2);
      expect(actions[0].approvalId).toBe('a1');
      expect(actions[1].approvalId).toBe('a2');
    });
  });

  describe('clearAction', () => {
    it('removes a specific action by actionId', async () => {
      const q1 = await enqueueAction(store, { approvalId: 'a1', kind: 'approve', payload: '' });
      const q2 = await enqueueAction(store, { approvalId: 'a2', kind: 'reject', payload: 'Bad' });

      await clearAction(store, q1.actionId);

      const remaining = await getQueuedActions(store);
      expect(remaining).toHaveLength(1);
      expect(remaining[0].actionId).toBe(q2.actionId);
    });
  });

  // ---- Flush queue ----

  describe('flushQueue', () => {
    function mockApi(responses: Map<string, () => Promise<void>>): ApiClient {
      return {
        async approve(id: string, _comment: string) {
          const handler = responses.get(`approve:${id}`);
          if (handler) return handler();
          // default: success
        },
        async reject(id: string, _reason: string) {
          const handler = responses.get(`reject:${id}`);
          if (handler) return handler();
          // default: success
        },
      };
    }

    it('all-success clears the queue', async () => {
      await enqueueAction(store, { approvalId: 'a1', kind: 'approve', payload: '' });
      await enqueueAction(store, { approvalId: 'a2', kind: 'reject', payload: 'Bad' });

      const api = mockApi(new Map());
      const summary = await flushQueue(store, api);

      expect(summary).toEqual({ synced: 2, conflicts: 0, remaining: 0 });
      const remaining = await getQueuedActions(store);
      expect(remaining).toHaveLength(0);
    });

    it('409 conflict drops that action but continues', async () => {
      await enqueueAction(store, { approvalId: 'a1', kind: 'approve', payload: '' });
      await enqueueAction(store, { approvalId: 'a2', kind: 'approve', payload: '' });
      await enqueueAction(store, { approvalId: 'a3', kind: 'approve', payload: '' });

      const responses = new Map<string, () => Promise<void>>([
        [
          'approve:a2',
          async () => {
            throw new ConflictError();
          },
        ],
      ]);
      const api = mockApi(responses);
      const summary = await flushQueue(store, api);

      expect(summary).toEqual({ synced: 2, conflicts: 1, remaining: 0 });
      const remaining = await getQueuedActions(store);
      expect(remaining).toHaveLength(0);
    });

    it('network error halts and preserves remaining actions', async () => {
      await enqueueAction(store, { approvalId: 'a1', kind: 'approve', payload: '' });
      await enqueueAction(store, { approvalId: 'a2', kind: 'approve', payload: '' });
      await enqueueAction(store, { approvalId: 'a3', kind: 'approve', payload: '' });

      const responses = new Map<string, () => Promise<void>>([
        [
          'approve:a2',
          async () => {
            throw new NetworkError('timeout');
          },
        ],
      ]);
      const api = mockApi(responses);
      const summary = await flushQueue(store, api);

      expect(summary).toEqual({ synced: 1, conflicts: 0, remaining: 2 });
      const remaining = await getQueuedActions(store);
      expect(remaining).toHaveLength(2);
      expect(remaining[0].approvalId).toBe('a2');
      expect(remaining[1].approvalId).toBe('a3');
    });

    it('idempotency: re-flushing after partial success does not double-submit', async () => {
      await enqueueAction(store, { approvalId: 'a1', kind: 'approve', payload: '' });
      await enqueueAction(store, { approvalId: 'a2', kind: 'approve', payload: '' });
      await enqueueAction(store, { approvalId: 'a3', kind: 'approve', payload: '' });

      // First flush: a1 succeeds, a2 fails with network error
      const responses1 = new Map<string, () => Promise<void>>([
        [
          'approve:a2',
          async () => {
            throw new NetworkError('timeout');
          },
        ],
      ]);
      const api1 = mockApi(responses1);
      const summary1 = await flushQueue(store, api1);

      expect(summary1).toEqual({ synced: 1, conflicts: 0, remaining: 2 });

      // Second flush: all remaining succeed
      const api2 = mockApi(new Map());
      const summary2 = await flushQueue(store, api2);

      expect(summary2).toEqual({ synced: 2, conflicts: 0, remaining: 0 });

      // Queue is now empty
      const remaining = await getQueuedActions(store);
      expect(remaining).toHaveLength(0);
    });

    it('handles conflict followed by network error', async () => {
      await enqueueAction(store, { approvalId: 'a1', kind: 'approve', payload: '' });
      await enqueueAction(store, { approvalId: 'a2', kind: 'reject', payload: 'Bad' });
      await enqueueAction(store, { approvalId: 'a3', kind: 'approve', payload: '' });

      const responses = new Map<string, () => Promise<void>>([
        [
          'approve:a1',
          async () => {
            throw new ConflictError();
          },
        ],
        [
          'reject:a2',
          async () => {
            throw new NetworkError('down');
          },
        ],
      ]);
      const api = mockApi(responses);
      const summary = await flushQueue(store, api);

      // a1: conflict (dropped), a2: network error (stops flush)
      expect(summary).toEqual({ synced: 0, conflicts: 1, remaining: 2 });
      const remaining = await getQueuedActions(store);
      expect(remaining).toHaveLength(2);
      expect(remaining[0].approvalId).toBe('a2');
      expect(remaining[1].approvalId).toBe('a3');
    });
  });
});
