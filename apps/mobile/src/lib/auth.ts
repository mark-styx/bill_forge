/**
 * Mobile auth: login via /api/v1/mobile/auth/* endpoints.
 *
 * Persists JWT + tenantId in AsyncStorage so the app survives restarts.
 * Uses the backend's tenant auto-discovery (login) and explicit-tenant
 * (login/tenant) flows.
 */

import { KVStore } from './offline-queue';

// ---- Storage keys ----
const AUTH_KEY = 'auth_state';

// ---- Wire types ----

export interface AuthState {
  jwt: string;
  tenantId: string;
  userId: string;
  email: string;
  /** ISO timestamp when the JWT was obtained */
  issuedAt: string;
}

export interface LoginRequest {
  email: string;
  password: string;
}

export interface TenantOption {
  tenantId: string;
  tenantName: string;
  role: string;
}

export type LoginResult =
  | { kind: 'logged_in'; state: AuthState }
  | { kind: 'tenant_picker'; jwt: string; email: string; tenants: TenantOption[] };

// ---- API calls ----

export async function login(
  baseUrl: string,
  req: LoginRequest,
): Promise<LoginResult> {
  const res = await fetch(`${baseUrl}/api/v1/mobile/auth/login`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(req),
  });

  if (!res.ok) {
    const body = await res.text().catch(() => '');
    throw new Error(body || `Login failed (HTTP ${res.status})`);
  }

  const json = (await res.json()) as Record<string, unknown>;

  // Multi-tenant: backend returns a tenant list for the user to pick
  if (Array.isArray(json['tenants']) && json['tenants'].length > 0) {
    return {
      kind: 'tenant_picker',
      jwt: json['jwt'] as string,
      email: req.email,
      tenants: json['tenants'] as TenantOption[],
    };
  }

  // Single-tenant: fully logged in
  return {
    kind: 'logged_in',
    state: {
      jwt: json['jwt'] as string,
      tenantId: json['tenant_id'] as string,
      userId: json['user_id'] as string,
      email: req.email,
      issuedAt: new Date().toISOString(),
    },
  };
}

export async function loginWithTenant(
  baseUrl: string,
  jwt: string,
  tenantId: string,
): Promise<AuthState> {
  const res = await fetch(`${baseUrl}/api/v1/mobile/auth/login/tenant`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${jwt}`,
    },
    body: JSON.stringify({ tenant_id: tenantId }),
  });

  if (!res.ok) {
    const body = await res.text().catch(() => '');
    throw new Error(body || `Tenant login failed (HTTP ${res.status})`);
  }

  const json = (await res.json()) as Record<string, unknown>;

  return {
    jwt: json['jwt'] as string,
    tenantId: json['tenant_id'] as string,
    userId: json['user_id'] as string,
    email: json['email'] as string,
    issuedAt: new Date().toISOString(),
  };
}

// ---- Persistence helpers ----

export async function loadAuth(store: KVStore): Promise<AuthState | null> {
  const raw = await store.getItem(AUTH_KEY);
  if (!raw) return null;
  try {
    return JSON.parse(raw) as AuthState;
  } catch {
    return null;
  }
}

export async function saveAuth(store: KVStore, state: AuthState): Promise<void> {
  await store.setItem(AUTH_KEY, JSON.stringify(state));
}

export async function clearAuth(store: KVStore): Promise<void> {
  await store.setItem(AUTH_KEY, '');
}
