/**
 * Resolve a bearer token for the Outlook add-in taskpane.
 *
 * The add-in runs inside the Office iframe but on the same origin as the
 * dashboard, so the existing zustand-backed `billforge-auth` localStorage
 * entry holds the bearer the API expects. This helper centralizes the
 * lookup so the taskpane can stay agnostic of where the token lives.
 *
 * Falls back to `null` when no session is present — the taskpane renders a
 * prompt to sign in to BillForge in another tab in that case.
 */
export function getAddinToken(): string | null {
  if (typeof window === 'undefined') return null;
  try {
    const raw = window.localStorage.getItem('billforge-auth');
    if (!raw) return null;
    const parsed = JSON.parse(raw) as { state?: { accessToken?: string | null } };
    return parsed.state?.accessToken ?? null;
  } catch {
    return null;
  }
}

export function addinFetch(input: string, init: RequestInit = {}): Promise<Response> {
  const token = getAddinToken();
  const headers = new Headers(init.headers ?? {});
  if (token) headers.set('Authorization', `Bearer ${token}`);
  if (!headers.has('Accept')) headers.set('Accept', 'application/json');
  return fetch(input, { ...init, headers, credentials: 'include' });
}
