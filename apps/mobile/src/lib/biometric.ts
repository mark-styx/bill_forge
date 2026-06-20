/**
 * Biometric authentication for the mobile app.
 *
 * Wraps expo-local-authentication so:
 *   - the approval queue can require Face ID / Touch ID before unlocking
 *     a cached session on cold start
 *   - the one-tap approve gesture can do a step-up confirmation before
 *     enqueuing the action
 *
 * Designed to be testable: the device primitives are injected so tests
 * can provide fakes without reaching for native modules.
 *
 * Biometrics here are a LOCAL gate over the existing JWT. The backend is
 * unchanged; a stolen unlocked phone is the threat we protect against.
 */

import { KVStore } from './offline-queue';

const BIOMETRIC_OPT_IN_KEY = 'biometric_enabled';

/**
 * Platform primitives this module needs.
 *
 * In production these come from expo-local-authentication.
 * In tests they are faked.
 */
export interface BiometricPlatform {
  /** Whether the device has biometric hardware available. */
  hasHardware(): Promise<boolean>;
  /** Whether the user has at least one biometric enrolled. */
  isEnrolled(): Promise<boolean>;
  /**
   * Prompt for biometric verification. Returns true on success.
   * Must NOT throw on user cancel / lockout - return false instead.
   */
  authenticate(promptMessage: string): Promise<boolean>;
}

/**
 * Result of a biometric authentication attempt.
 *
 * Callers should treat `success: false` as a soft failure and fall back
 * to a password / explicit-confirm flow rather than crashing.
 */
export interface BiometricResult {
  success: boolean;
  error?: string;
}

export async function isBiometricAvailable(
  platform: BiometricPlatform,
): Promise<boolean> {
  try {
    const [hardware, enrolled] = await Promise.all([
      platform.hasHardware(),
      platform.isEnrolled(),
    ]);
    return hardware && enrolled;
  } catch {
    return false;
  }
}

export async function authenticateBiometric(
  platform: BiometricPlatform,
  reason: string,
): Promise<BiometricResult> {
  try {
    const ok = await platform.authenticate(reason);
    return ok ? { success: true } : { success: false, error: 'cancelled' };
  } catch (err) {
    return {
      success: false,
      error: err instanceof Error ? err.message : 'unknown_error',
    };
  }
}

export async function setBiometricEnabled(
  store: KVStore,
  enabled: boolean,
): Promise<void> {
  await store.setItem(BIOMETRIC_OPT_IN_KEY, enabled ? '1' : '0');
}

export async function isBiometricEnabled(store: KVStore): Promise<boolean> {
  const raw = await store.getItem(BIOMETRIC_OPT_IN_KEY);
  return raw === '1';
}

/**
 * Confidence threshold above which one-tap approve is offered.
 *
 * Mirrors the backend processing-confidence threshold; kept in sync by
 * convention. Anything below this requires the user to open the detail
 * sheet and use the two-step approve flow.
 */
export const ONE_TAP_CONFIDENCE_THRESHOLD = 0.9;

export type OneTapSource = 'one_tap';

export interface OneTapAction {
  approvalId: string;
  kind: 'approve';
  payload: string;
  source: OneTapSource;
}

/**
 * Build a one-tap approve action, gating on the confidence score.
 *
 * Returns null when the invoice's confidence is below the threshold or
 * unknown, so the caller falls back to the standard two-step approve.
 */
export function buildOneTapApprove(
  approvalId: string,
  confidence: number | null | undefined,
  comment = '',
): OneTapAction | null {
  if (confidence == null) return null;
  if (confidence < ONE_TAP_CONFIDENCE_THRESHOLD) return null;
  return {
    approvalId,
    kind: 'approve',
    payload: comment,
    source: 'one_tap',
  };
}
