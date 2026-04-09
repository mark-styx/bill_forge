/**
 * Typed API client for BillForge mobile backend.
 *
 * Base URL is configured via EXPO_PUBLIC_API_URL env var
 * (defaults to http://localhost:8000 for development).
 *
 * All authenticated requests attach Authorization: Bearer <token>
 * from SecureStore via the auth store.
 */

// ---- Types matching backend DTOs (dto.rs) ----

export type MobileInvoiceStatus =
  | 'pending_review'
  | 'pending_approval'
  | 'approved'
  | 'processing'
  | 'paid'
  | 'cancelled'
  | 'rejected';

export interface MobileInvoiceSummary {
  id: string;
  vendor_name: string;
  invoice_number: string;
  total_amount_cents: number;
  currency: string;
  due_date: string | null;
  status: MobileInvoiceStatus;
  days_until_due: number | null;
  requires_action: boolean;
  created_at: string;
}

export interface MobileApprovalRequest {
  id: string;
  invoice: MobileInvoiceSummary;
  requested_at: string;
  expires_at: string | null;
  can_approve: boolean;
}

export interface MobileActivityItem {
  id: string;
  activity_type: 'invoice_uploaded' | 'approval_requested' | 'approval_completed' | 'invoice_paid' | 'comment_added';
  title: string;
  description: string;
  timestamp: string;
}

export interface MobileDashboard {
  pending_approvals: number;
  pending_review: number;
  requires_attention: number;
  upcoming_due_dates: MobileInvoiceSummary[];
  recent_activity: MobileActivityItem[];
}

// ---- Auth types matching backend AuthResponse ----

export interface AuthResponse {
  access_token: string;
  refresh_token: string;
  user: {
    id: string;
    tenant_id: string;
    email: string;
    name: string;
    roles: string[];
  };
  tenant: {
    id: string;
    name: string;
    enabled_modules: string[];
    settings: Record<string, unknown>;
  };
}

// ---- Device registration ----

export interface RegisterDevicePayload {
  device_id: string;
  platform: 'ios' | 'android';
  token: string;
  device_name?: string;
  os_version?: string;
  app_version?: string;
}

export interface DeviceResponse {
  id: string;
  device_id: string;
  platform: string;
  device_name: string | null;
  is_active: boolean;
  last_used_at: string | null;
  created_at: string;
}

// ---- API error shape ----

import type { ApiErrorBody } from '@billforge/shared-types';
export type { ApiErrorBody } from '@billforge/shared-types';

// ---- Helpers ----

const BASE_URL = process.env.EXPO_PUBLIC_API_URL ?? 'http://localhost:8000';

/** Retrieve the stored JWT. Exported so tests can inject via store mock. */
let _getToken: () => Promise<string | null> = async () => null;
let _getRefreshToken: () => Promise<string | null> = async () => null;
let _setTokens: (accessToken: string, refreshToken: string) => Promise<void> = async () => {};
let _onLogout: () => Promise<void> = async () => {};

/** Allow the store to register its token getter after initialisation. */
export function registerTokenGetter(getter: () => Promise<string | null>) {
  _getToken = getter;
}

/** Register the refresh token getter so the client can attempt silent refresh. */
export function registerRefreshTokenGetter(getter: () => Promise<string | null>) {
  _getRefreshToken = getter;
}

/** Register a callback to persist new tokens after a successful refresh. */
export function registerTokenSetter(setter: (accessToken: string, refreshToken: string) => Promise<void>) {
  _setTokens = setter;
}

/** Register a callback invoked when token refresh fails (forces re-login). */
export function registerLogoutHandler(handler: () => Promise<void>) {
  _onLogout = handler;
}

class ApiClientError extends Error {
  status: number;
  code: string;
  body: ApiErrorBody | null;
  fieldErrors: Record<string, string[]> | undefined;

  constructor(status: number, body: ApiErrorBody | null) {
    super(body?.error?.message ?? `API error ${status}`);
    this.name = 'ApiClientError';
    this.status = status;
    this.code = body?.error?.code ?? 'UNKNOWN';
    this.body = body;
    this.fieldErrors = body?.error?.field_errors ?? undefined;
  }
}

export { ApiClientError };

// ---- Token refresh with concurrent request coalescing ----

let _isRefreshing = false;
let _refreshPromise: Promise<RefreshResult> | null = null;

/** Refresh result: 'ok' = new tokens stored, 'terminal' = invalid refresh token (logout),
 *  'transient' = network/server error (don't logout, surface the original 401). */
type RefreshResult = 'ok' | 'terminal' | 'transient';

async function doRefresh(): Promise<RefreshResult> {
  try {
    const refreshToken = await _getRefreshToken();
    if (!refreshToken) return 'terminal';

    const response = await fetch(`${BASE_URL}/api/v1/auth/refresh`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ refresh_token: refreshToken }),
    });

    if (!response.ok) {
      // 401/403 = refresh token is invalid or revoked, force logout
      if (response.status === 401 || response.status === 403) return 'terminal';
      // 5xx or other = transient, don't logout
      return 'transient';
    }

    const data = await response.json();
    await _setTokens(data.access_token, data.refresh_token);
    return 'ok';
  } catch {
    // Network error = transient
    return 'transient';
  }
}

async function refreshAccessToken(): Promise<RefreshResult> {
  if (_isRefreshing && _refreshPromise) {
    return _refreshPromise;
  }

  _isRefreshing = true;
  _refreshPromise = doRefresh();

  try {
    return await _refreshPromise;
  } finally {
    _isRefreshing = false;
    _refreshPromise = null;
  }
}

async function request<T>(
  path: string,
  options: RequestInit & { token?: string; _skipRefresh?: boolean } = {},
): Promise<T> {
  const token = options.token ?? (await _getToken());
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...(options.headers as Record<string, string> | undefined),
  };
  if (token) {
    headers['Authorization'] = `Bearer ${token}`;
  }

  const res = await fetch(`${BASE_URL}${path}`, {
    ...options,
    headers,
  });

  // On 401, attempt a silent token refresh and retry once
  if (res.status === 401 && !options._skipRefresh && options.token === undefined) {
    const result = await refreshAccessToken();
    if (result === 'ok') {
      return request<T>(path, { ...options, _skipRefresh: true });
    }
    if (result === 'terminal') {
      await _onLogout();
      throw new ApiClientError(401, {
        error: { code: 'SESSION_EXPIRED', message: 'Session expired. Please login again.' },
      });
    }
    // transient: don't logout, throw the original 401 so caller can retry
  }

  if (!res.ok) {
    let body: ApiErrorBody | null = null;
    try {
      body = await res.json();
    } catch {
      // non-JSON error body
    }
    throw new ApiClientError(res.status, body);
  }

  // 204 No Content
  if (res.status === 204) {
    return undefined as unknown as T;
  }

  return res.json() as Promise<T>;
}

// ---- Public API ----

export const api = {
  /**
   * Authenticate with email + password.
   * POST /api/v1/auth/login  { tenant_id, email, password }
   */
  async login(tenantId: string, email: string, password: string): Promise<AuthResponse> {
    return request<AuthResponse>('/api/v1/auth/login', {
      method: 'POST',
      body: JSON.stringify({ tenant_id: tenantId, email, password }),
      token: undefined,
      _skipRefresh: true,
    });
  },

  /** GET /api/v1/mobile/dashboard */
  async getDashboard(): Promise<MobileDashboard> {
    return request<MobileDashboard>('/api/v1/mobile/dashboard');
  },

  /** GET /api/v1/mobile/approvals */
  async getApprovals(): Promise<MobileApprovalRequest[]> {
    return request<MobileApprovalRequest[]>('/api/v1/mobile/approvals');
  },

  /**
   * GET /api/v1/mobile/invoices/:id?fields=...
   * Without fields returns full MobileInvoiceSummary.
   */
  async getInvoice(id: string, fields?: string[]): Promise<MobileInvoiceSummary> {
    const query = fields?.length ? `?fields=${fields.join(',')}` : '';
    return request<MobileInvoiceSummary>(`/api/v1/mobile/invoices/${id}${query}`);
  },

  /** POST /api/v1/mobile/approvals/:id/approve  { comment? } */
  async approveInvoice(id: string, comment?: string): Promise<{ success: boolean }> {
    return request<{ success: boolean }>(`/api/v1/mobile/approvals/${id}/approve`, {
      method: 'POST',
      body: JSON.stringify({ comment: comment ?? null }),
    });
  },

  /** POST /api/v1/mobile/approvals/:id/reject  { reason } */
  async rejectInvoice(id: string, reason: string): Promise<{ success: boolean }> {
    return request<{ success: boolean }>(`/api/v1/mobile/approvals/${id}/reject`, {
      method: 'POST',
      body: JSON.stringify({ reason }),
    });
  },

  /** POST /api/v1/mobile/devices/register */
  async registerDevice(payload: RegisterDevicePayload): Promise<DeviceResponse> {
    return request<DeviceResponse>('/api/v1/mobile/devices/register', {
      method: 'POST',
      body: JSON.stringify(payload),
    });
  },

  /** GET /api/v1/mobile/sync/invoices?last_sync_at=ISO8601 */
  async deltaSync(lastSyncAt: string): Promise<MobileInvoiceSummary[]> {
    return request<MobileInvoiceSummary[]>(
      `/api/v1/mobile/sync/invoices?last_sync_at=${encodeURIComponent(lastSyncAt)}`,
    );
  },

  /** POST /api/v1/auth/logout - Revoke server-side refresh tokens */
  async logout(): Promise<void> {
    try {
      await request<{ success: boolean }>('/api/v1/auth/logout', { method: 'POST' });
    } catch {
      // Best-effort: if the server is unreachable, local logout still proceeds
    }
  },
};
