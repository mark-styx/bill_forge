/**
 * Typed mobile API client for the BillForge backend.
 *
 * Reads base URL, JWT, and X-Tenant-Id from a config object.
 * Maps non-2xx responses to typed errors (ConflictError / NetworkError)
 * so the offline queue can branch on them.
 */

import {
  ApprovalItem,
  ConflictError,
  NetworkError,
} from './offline-queue';

export interface ApiConfig {
  baseUrl: string;
  jwt: string;
  tenantId: string;
}

interface SyncInvoice {
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
}

export interface SyncResponse {
  sync_timestamp: string;
  changes: {
    invoices: SyncInvoice[];
    vendors: Array<{ id: string; name: string; total_invoices: number; total_amount_cents: number }>;
    approval_requests: ApprovalItem[];
  };
  deleted: {
    invoice_ids: string[];
    vendor_ids: string[];
  };
  has_more: boolean;
}

class ApiError extends Error {
  constructor(
    public readonly status: number,
    message: string,
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

async function request(
  config: ApiConfig,
  path: string,
  options: RequestInit = {},
): Promise<Response> {
  const url = `${config.baseUrl}/api/v1/mobile${path}`;
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    Authorization: `Bearer ${config.jwt}`,
    'X-Tenant-Id': config.tenantId,
    ...(options.headers as Record<string, string> | undefined),
  };

  try {
    const response = await fetch(url, { ...options, headers });
    return response;
  } catch (err) {
    throw new NetworkError(
      err instanceof Error ? err.message : undefined,
    );
  }
}

function assertOk(response: Response, body: string): void {
  if (response.ok) return;

  if (response.status === 409) {
    let payload: unknown = body;
    try {
      payload = JSON.parse(body);
    } catch {
      // keep raw text as payload
    }
    throw new ConflictError(payload, response.status);
  }

  throw new ApiError(
    response.status,
    body || `HTTP ${response.status}`,
  );
}

/** List pending approvals for the current user. */
export async function listApprovals(
  config: ApiConfig,
): Promise<ApprovalItem[]> {
  const res = await request(config, '/approvals');
  const body = await res.text();

  if (!res.ok) {
    // Treat non-409 errors from list as network-level so the caller
    // can fall back to cache.
    throw new ApiError(res.status, body || `HTTP ${res.status}`);
  }

  return JSON.parse(body) as ApprovalItem[];
}

/** Approve an invoice approval request. */
export async function approve(
  config: ApiConfig,
  id: string,
  comment: string,
): Promise<void> {
  const res = await request(config, `/approvals/${id}/approve`, {
    method: 'POST',
    body: JSON.stringify({ comment }),
  });
  const body = await res.text();
  assertOk(res, body);
}

/** Reject an invoice approval request. */
export async function reject(
  config: ApiConfig,
  id: string,
  reason: string,
): Promise<void> {
  const res = await request(config, `/approvals/${id}/reject`, {
    method: 'POST',
    body: JSON.stringify({ reason }),
  });
  const body = await res.text();
  assertOk(res, body);
}

/** Register a device token for push notifications. */
export async function registerDevice(
  config: ApiConfig,
  payload: {
    device_id: string;
    platform: string;
    token: string;
    device_name?: string;
    os_version?: string;
    app_version?: string;
  },
): Promise<void> {
  const res = await request(config, '/devices/register', {
    method: 'POST',
    body: JSON.stringify(payload),
  });
  const body = await res.text();
  assertOk(res, body);
}

/** Delta sync: fetch changes since the given timestamp. */
export async function syncInvoices(
  config: ApiConfig,
  since?: string,
): Promise<SyncResponse> {
  const params = since ? `?last_sync_at=${encodeURIComponent(since)}` : '';
  const res = await request(config, `/sync/invoices${params}`);
  const body = await res.text();

  if (!res.ok) {
    throw new ApiError(res.status, body || `HTTP ${res.status}`);
  }

  return JSON.parse(body) as SyncResponse;
}
