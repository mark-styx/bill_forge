import {
  ApprovalItem,
  KVStore,
  OfflineConflict,
  cacheApprovals,
  getCachedApprovals,
  enqueueAction,
  getQueuedActions,
  clearAction,
  flushQueue,
  ApiClient,
  ConflictError,
  NetworkError,
  listConflicts,
  dismissConflict,
  clearConflicts,
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

      expect(summary).toEqual({ synced: 2, conflicts: [], remaining: 0 });
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

      expect(summary.synced).toBe(2);
      expect(summary.conflicts).toHaveLength(1);
      expect(summary.remaining).toBe(0);
      expect(summary.conflicts[0].approvalId).toBe('a2');
      expect(summary.conflicts[0].reason).toBe('conflict');
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

      expect(summary).toEqual({ synced: 1, conflicts: [] as OfflineConflict[], remaining: 2 });
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

      expect(summary1).toEqual({ synced: 1, conflicts: [] as OfflineConflict[], remaining: 2 });

      // Second flush: all remaining succeed
      const api2 = mockApi(new Map());
      const summary2 = await flushQueue(store, api2);

      expect(summary2).toEqual({ synced: 2, conflicts: [] as OfflineConflict[], remaining: 0 });

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
      expect(summary.synced).toBe(0);
      expect(summary.conflicts).toHaveLength(1);
      expect(summary.remaining).toBe(2);
      const remaining = await getQueuedActions(store);
      expect(remaining).toHaveLength(2);
      expect(remaining[0].approvalId).toBe('a2');
      expect(remaining[1].approvalId).toBe('a3');
    });

    it('captures server payload on 409 conflict', async () => {
      await enqueueAction(store, { approvalId: 'a1', kind: 'approve', payload: '' });

      const serverBody = { error: 'already_approved', processedBy: 'user@example.com' };
      const responses = new Map<string, () => Promise<void>>([
        [
          'approve:a1',
          async () => {
            throw new ConflictError(serverBody, 409);
          },
        ],
      ]);
      const api = mockApi(responses);
      const summary = await flushQueue(store, api);

      expect(summary.conflicts).toHaveLength(1);
      expect(summary.conflicts[0].serverPayload).toEqual(serverBody);
      expect(summary.conflicts[0].action).toBe('approve');
      expect(summary.conflicts[0].approvalId).toBe('a1');
      expect(summary.conflicts[0].reason).toBe('conflict');
      expect(summary.conflicts[0].occurredAt).toBeTruthy();
    });

    it('restores approval to cache on conflict', async () => {
      const approval = makeApproval('a1');
      await cacheApprovals(store, [approval]);

      await enqueueAction(store, { approvalId: 'a1', kind: 'approve', payload: '' });

      // Approval should be removed from cache after optimistic enqueue
      let cached = await getCachedApprovals(store);
      expect(cached).toHaveLength(0);

      const responses = new Map<string, () => Promise<void>>([
        [
          'approve:a1',
          async () => {
            throw new ConflictError({ error: 'conflict' }, 409);
          },
        ],
      ]);
      const api = mockApi(responses);
      await flushQueue(store, api);

      // Approval should be restored to cache after conflict
      cached = await getCachedApprovals(store);
      expect(cached).toHaveLength(1);
      expect(cached[0].id).toBe('a1');
    });

    it('persists conflict to conflict store', async () => {
      await enqueueAction(store, { approvalId: 'a1', kind: 'reject', payload: 'Bad' });

      const responses = new Map<string, () => Promise<void>>([
        [
          'reject:a1',
          async () => {
            throw new ConflictError({ detail: 'gone' }, 409);
          },
        ],
      ]);
      const api = mockApi(responses);
      await flushQueue(store, api);

      const stored = await listConflicts(store);
      expect(stored).toHaveLength(1);
      expect(stored[0].approvalId).toBe('a1');
      expect(stored[0].action).toBe('reject');
      expect(stored[0].serverPayload).toEqual({ detail: 'gone' });
    });

    it('removes action from queue on conflict', async () => {
      await enqueueAction(store, { approvalId: 'a1', kind: 'approve', payload: '' });

      const responses = new Map<string, () => Promise<void>>([
        [
          'approve:a1',
          async () => {
            throw new ConflictError(null, 409);
          },
        ],
      ]);
      const api = mockApi(responses);
      await flushQueue(store, api);

      const remaining = await getQueuedActions(store);
      expect(remaining).toHaveLength(0);
    });
  });

  // ---- Conflict store ----

  describe('conflictStore', () => {
    it('listConflicts returns empty when none stored', async () => {
      const result = await listConflicts(store);
      expect(result).toEqual([]);
    });

    it('dismissConflict removes a specific conflict', async () => {
      // Seed a conflict by flushing a conflict-producing action
      await enqueueAction(store, { approvalId: 'a1', kind: 'approve', payload: '' });
      const responses = new Map<string, () => Promise<void>>([
        [
          'approve:a1',
          async () => {
            throw new ConflictError({}, 409);
          },
        ],
      ]);
      const api: ApiClient = {
        approve: async (id: string) => {
          const handler = responses.get(`approve:${id}`);
          if (handler) return handler();
        },
        reject: async () => {},
      };
      await flushQueue(store, api);

      const stored = await listConflicts(store);
      expect(stored).toHaveLength(1);

      await dismissConflict(store, stored[0].id);
      const after = await listConflicts(store);
      expect(after).toHaveLength(0);
    });

    it('clearConflicts removes all conflicts', async () => {
      // Seed two conflicts
      await enqueueAction(store, { approvalId: 'a1', kind: 'approve', payload: '' });
      await enqueueAction(store, { approvalId: 'a2', kind: 'approve', payload: '' });
      const responses = new Map<string, () => Promise<void>>([
        [
          'approve:a1',
          async () => {
            throw new ConflictError({}, 409);
          },
        ],
        [
          'approve:a2',
          async () => {
            throw new ConflictError({}, 409);
          },
        ],
      ]);
      const api: ApiClient = {
        approve: async (id: string) => {
          const handler = responses.get(`approve:${id}`);
          if (handler) return handler();
        },
        reject: async () => {},
      };
      await flushQueue(store, api);

      expect(await listConflicts(store)).toHaveLength(2);

      await clearConflicts(store);
      expect(await listConflicts(store)).toHaveLength(0);
    });
  });

  // ---- ConflictError ----

  describe('ConflictError', () => {
    it('carries server payload and status', () => {
      const err = new ConflictError({ message: 'already processed' }, 409);
      expect(err.serverPayload).toEqual({ message: 'already processed' });
      expect(err.status).toBe(409);
      expect(err.name).toBe('ConflictError');
    });

    it('defaults serverPayload to null and status to 409', () => {
      const err = new ConflictError();
      expect(err.serverPayload).toBeNull();
      expect(err.status).toBe(409);
    });
  });
});
