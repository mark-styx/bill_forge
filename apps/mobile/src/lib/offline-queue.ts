/**
 * Offline-first queue for approval actions.
 *
 * Storage is abstracted behind a small KVStore interface so the logic
 * is testable without a device. In production, pass an AsyncStorage-backed
 * store; in tests, pass an in-memory Map.
 */

/** A pending approval as returned by the backend. */
export interface ApprovalItem {
  id: string;
  invoice: {
    id: string;
    vendor_name: string;
    invoice_number: string;
    total_amount_cents: number;
    currency: string;
    due_date: string | null;
    status: string;
    days_until_due: number | null;
    requires_action: boolean;
    created_at: string;
  };
  requested_at: string;
  expires_at: string | null;
  can_approve: boolean;
}

export type ActionKind = 'approve' | 'reject';

export interface QueuedAction {
  actionId: string;
  approvalId: string;
  kind: ActionKind;
  payload: string; // comment for approve, reason for reject
  enqueuedAt: string;
}

export interface OfflineConflict {
  id: string;
  actionId: string;
  approvalId: string;
  action: ActionKind;
  reason: 'conflict';
  serverPayload: unknown;
  occurredAt: string;
}

export interface FlushSummary {
  synced: number;
  conflicts: OfflineConflict[];
  remaining: number;
}

/** Minimal key-value store interface (AsyncStorage or in-memory). */
export interface KVStore {
  getItem(key: string): Promise<string | null>;
  setItem(key: string, value: string): Promise<void>;
}

const CACHE_KEY = 'offline_approvals_cache';
const QUEUE_KEY = 'offline_action_queue';
const CONFLICTS_KEY = 'offline-queue.conflicts';

// ---- Cache ----

export async function cacheApprovals(
  store: KVStore,
  items: ApprovalItem[],
): Promise<void> {
  await store.setItem(CACHE_KEY, JSON.stringify(items));
}

export async function getCachedApprovals(
  store: KVStore,
): Promise<ApprovalItem[]> {
  const raw = await store.getItem(CACHE_KEY);
  if (!raw) return [];
  try {
    return JSON.parse(raw) as ApprovalItem[];
  } catch {
    return [];
  }
}

// ---- Action queue ----

async function readQueue(store: KVStore): Promise<QueuedAction[]> {
  const raw = await store.getItem(QUEUE_KEY);
  if (!raw) return [];
  try {
    return JSON.parse(raw) as QueuedAction[];
  } catch {
    return [];
  }
}

async function writeQueue(
  store: KVStore,
  queue: QueuedAction[],
): Promise<void> {
  await store.setItem(QUEUE_KEY, JSON.stringify(queue));
}

export function generateActionId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
}

export async function enqueueAction(
  store: KVStore,
  action: {
    approvalId: string;
    kind: ActionKind;
    payload: string;
  },
): Promise<QueuedAction> {
  const queued: QueuedAction = {
    actionId: generateActionId(),
    approvalId: action.approvalId,
    kind: action.kind,
    payload: action.payload,
    enqueuedAt: new Date().toISOString(),
  };

  const queue = await readQueue(store);
  queue.push(queued);
  await writeQueue(store, queue);

  // Optimistic: remove the approval from cache so the UI updates immediately.
  // Capture the removed item so flushQueue can restore it on conflict.
  const cached = await getCachedApprovals(store);
  const removed = cached.find((a) => a.id === action.approvalId);
  const updated = cached.filter((a) => a.id !== action.approvalId);
  await cacheApprovals(store, updated);

  // Store a rollback closure so flushQueue can restore the approval on 409.
  // We stash it in a side-channel keyed by actionId.
  if (removed) {
    await store.setItem(
      `offline_rollback_${queued.actionId}`,
      JSON.stringify(removed),
    );
  }

  return queued;
}

export async function getQueuedActions(
  store: KVStore,
): Promise<QueuedAction[]> {
  return readQueue(store);
}

export async function clearAction(
  store: KVStore,
  actionId: string,
): Promise<void> {
  const queue = await readQueue(store);
  const updated = queue.filter((a) => a.actionId !== actionId);
  await writeQueue(store, updated);
}

/** API client shape needed by flushQueue. */
export interface ApiClient {
  approve(id: string, comment: string): Promise<void>;
  reject(id: string, reason: string): Promise<void>;
}

/**
 * Error thrown by the API client when the server returns 409 Conflict
 * (approval already processed).
 */
export class ConflictError extends Error {
  constructor(
    public readonly serverPayload: unknown = null,
    public readonly status: number = 409,
  ) {
    super('Conflict');
    this.name = 'ConflictError';
  }
}

/**
 * Error thrown by the API client on network / connectivity failures.
 */
export class NetworkError extends Error {
  constructor(message?: string) {
    super(message ?? 'Network error');
    this.name = 'NetworkError';
  }
}

// ---- Conflict store ----

async function readConflicts(store: KVStore): Promise<OfflineConflict[]> {
  const raw = await store.getItem(CONFLICTS_KEY);
  if (!raw) return [];
  try {
    return JSON.parse(raw) as OfflineConflict[];
  } catch {
    return [];
  }
}

async function writeConflicts(
  store: KVStore,
  conflicts: OfflineConflict[],
): Promise<void> {
  await store.setItem(CONFLICTS_KEY, JSON.stringify(conflicts));
}

/** List all persisted conflicts (survives app restarts). */
export async function listConflicts(store: KVStore): Promise<OfflineConflict[]> {
  return readConflicts(store);
}

/** Dismiss a single conflict by id. */
export async function dismissConflict(
  store: KVStore,
  id: string,
): Promise<void> {
  const conflicts = await readConflicts(store);
  await writeConflicts(store, conflicts.filter((c) => c.id !== id));
}

/** Clear all persisted conflicts. */
export async function clearConflicts(store: KVStore): Promise<void> {
  await writeConflicts(store, []);
}

/**
 * Replay queued actions in FIFO order.
 * - On success: remove from queue.
 * - On ConflictError (409): record conflict details, restore approval to cache, drop action.
 * - On NetworkError: stop flushing, preserve remaining for next attempt.
 *
 * Returns a summary of what happened.
 */
export async function flushQueue(
  store: KVStore,
  api: ApiClient,
): Promise<FlushSummary> {
  const queue = await readQueue(store);
  let synced = 0;
  const conflictRecords: OfflineConflict[] = [];
  let i = 0;

  for (; i < queue.length; i++) {
    const action = queue[i];
    try {
      if (action.kind === 'approve') {
        await api.approve(action.approvalId, action.payload);
      } else {
        await api.reject(action.approvalId, action.payload);
      }
      synced++;
    } catch (err) {
      if (err instanceof ConflictError) {
        const conflict: OfflineConflict = {
          id: generateActionId(),
          actionId: action.actionId,
          approvalId: action.approvalId,
          action: action.kind,
          reason: 'conflict',
          serverPayload: err.serverPayload,
          occurredAt: new Date().toISOString(),
        };

        // Persist the conflict record
        const existing = await readConflicts(store);
        existing.push(conflict);
        await writeConflicts(store, existing);

        // Restore the optimistically-removed approval to cache
        const rollbackRaw = await store.getItem(`offline_rollback_${action.actionId}`);
        if (rollbackRaw) {
          try {
            const restored = JSON.parse(rollbackRaw) as ApprovalItem;
            const cached = await getCachedApprovals(store);
            if (!cached.some((a) => a.id === restored.id)) {
              cached.push(restored);
              await cacheApprovals(store, cached);
            }
          } catch {
            // best-effort rollback
          }
          await store.setItem(`offline_rollback_${action.actionId}`, '');
        }

        conflictRecords.push(conflict);
        continue; // already processed, drop it and move on
      }
      // Network or unexpected error: stop flushing
      break;
    }
  }

  // Actions 0..i-1 are fully processed (synced or conflict-dropped).
  // queue[i] is the action that failed on network error (if any).
  const remaining = queue.slice(i);
  await writeQueue(store, remaining);

  return {
    synced,
    conflicts: conflictRecords,
    remaining: remaining.length,
  };
}
