import {
  BiometricPlatform,
  ONE_TAP_CONFIDENCE_THRESHOLD,
  authenticateBiometric,
  buildOneTapApprove,
  isBiometricAvailable,
  isBiometricEnabled,
  setBiometricEnabled,
} from './biometric';
import { KVStore } from './offline-queue';

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

function makePlatform(
  overrides: Partial<BiometricPlatform> = {},
): BiometricPlatform {
  return {
    hasHardware: overrides.hasHardware ?? (async () => true),
    isEnrolled: overrides.isEnrolled ?? (async () => true),
    authenticate: overrides.authenticate ?? (async () => true),
  };
}

describe('biometric', () => {
  describe('isBiometricAvailable', () => {
    it('returns true when hardware and enrollment are present', async () => {
      const ok = await isBiometricAvailable(makePlatform());
      expect(ok).toBe(true);
    });

    it('returns false when no hardware', async () => {
      const ok = await isBiometricAvailable(
        makePlatform({ hasHardware: async () => false }),
      );
      expect(ok).toBe(false);
    });

    it('returns false when no biometric is enrolled', async () => {
      const ok = await isBiometricAvailable(
        makePlatform({ isEnrolled: async () => false }),
      );
      expect(ok).toBe(false);
    });

    it('returns false on platform error (does not throw)', async () => {
      const ok = await isBiometricAvailable(
        makePlatform({
          hasHardware: async () => {
            throw new Error('boom');
          },
        }),
      );
      expect(ok).toBe(false);
    });
  });

  describe('authenticateBiometric', () => {
    it('returns success: true when the platform authenticates', async () => {
      const result = await authenticateBiometric(makePlatform(), 'Unlock');
      expect(result.success).toBe(true);
    });

    it('returns success: false (does not throw) on user cancel', async () => {
      const result = await authenticateBiometric(
        makePlatform({ authenticate: async () => false }),
        'Unlock',
      );
      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
    });

    it('returns success: false (does not throw) on lockout', async () => {
      const result = await authenticateBiometric(
        makePlatform({
          authenticate: async () => {
            throw new Error('lockout');
          },
        }),
        'Unlock',
      );
      expect(result.success).toBe(false);
      expect(result.error).toBe('lockout');
    });
  });

  describe('opt-in persistence', () => {
    it('round-trips the enabled flag through the store', async () => {
      const store = createStore();
      expect(await isBiometricEnabled(store)).toBe(false);

      await setBiometricEnabled(store, true);
      expect(await isBiometricEnabled(store)).toBe(true);

      await setBiometricEnabled(store, false);
      expect(await isBiometricEnabled(store)).toBe(false);
    });
  });

  describe('buildOneTapApprove', () => {
    it('returns an action with source: one_tap when confidence meets the threshold', () => {
      const action = buildOneTapApprove('approval-1', ONE_TAP_CONFIDENCE_THRESHOLD);
      expect(action).not.toBeNull();
      expect(action?.source).toBe('one_tap');
      expect(action?.kind).toBe('approve');
      expect(action?.approvalId).toBe('approval-1');
      expect(action?.payload).toBe('');
    });

    it('passes through an explicit comment payload', () => {
      const action = buildOneTapApprove('approval-2', 0.95, 'looks good');
      expect(action?.payload).toBe('looks good');
    });

    it('returns null when confidence is below the threshold', () => {
      const action = buildOneTapApprove(
        'approval-3',
        ONE_TAP_CONFIDENCE_THRESHOLD - 0.01,
      );
      expect(action).toBeNull();
    });

    it('returns null when confidence is missing (null or undefined)', () => {
      expect(buildOneTapApprove('a', null)).toBeNull();
      expect(buildOneTapApprove('a', undefined)).toBeNull();
    });
  });
});
