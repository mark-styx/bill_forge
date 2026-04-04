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

export interface ApiErrorBody {
  error: {
    code: string;
    message: string;
    details?: unknown;
  };
}

// ---- Helpers ----

const BASE_URL = process.env.EXPO_PUBLIC_API_URL ?? 'http://localhost:8000';

/** Retrieve the stored JWT. Exported so tests can inject via store mock. */
let _getToken: () => Promise<string | null> = async () => null;

/** Allow the store to register its token getter after initialisation. */
export function registerTokenGetter(getter: () => Promise<string | null>) {
  _getToken = getter;
}

class ApiClientError extends Error {
  status: number;
  body: ApiErrorBody | null;

  constructor(status: number, body: ApiErrorBody | null) {
    super(body?.error?.message ?? `API error ${status}`);
    this.status = status;
    this.body = body;
  }
}

export { ApiClientError };

async function request<T>(
  path: string,
  options: RequestInit & { token?: string } = {},
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
