import { registerForPushNotifications, NotificationPlatform } from './notifications';
import { ApiConfig } from './api';

function makePlatform(overrides: Partial<NotificationPlatform> = {}): NotificationPlatform {
  return {
    requestPermissions: overrides.requestPermissions ?? (async () => true),
    getPushToken: overrides.getPushToken ?? (async () => 'ExponentPushToken[test123]'),
    getPlatform: overrides.getPlatform ?? (() => 'ios' as const),
    getDeviceInfo: overrides.getDeviceInfo ?? (() => ({
      deviceId: 'test-device-1',
      deviceName: 'Test Phone',
      osVersion: '17.0',
      appVersion: '0.1.0',
    })),
  };
}

const testConfig: ApiConfig = {
  baseUrl: 'http://localhost:8080',
  jwt: 'test-jwt',
  tenantId: 'tenant-1',
};

describe('notifications', () => {
  describe('registerForPushNotifications', () => {
    it('returns true when registration succeeds', async () => {
      const calls: Array<Record<string, string>> = [];

      // Mock global.fetch for the registerDevice API call
      const originalFetch = global.fetch;
      global.fetch = async (_url: string, init?: RequestInit) => {
        calls.push(JSON.parse(init?.body as string) as Record<string, string>);
        return new Response(JSON.stringify({ id: '1', device_id: 'test-device-1', platform: 'ios', is_active: true }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        });
      };

      try {
        const result = await registerForPushNotifications(testConfig, makePlatform());
        expect(result).toBe(true);
        expect(calls).toHaveLength(1);
        expect(calls[0].token).toBe('ExponentPushToken[test123]');
        expect(calls[0].platform).toBe('ios');
        expect(calls[0].device_id).toBe('test-device-1');
      } finally {
        global.fetch = originalFetch;
      }
    });

    it('returns false when permission is denied', async () => {
      const platform = makePlatform({
        requestPermissions: async () => false,
      });

      const result = await registerForPushNotifications(testConfig, platform);
      expect(result).toBe(false);
    });

    it('returns false when push token is unavailable', async () => {
      const platform = makePlatform({
        getPushToken: async () => null,
      });

      const result = await registerForPushNotifications(testConfig, platform);
      expect(result).toBe(false);
    });

    it('returns false on network error without throwing', async () => {
      const originalFetch = global.fetch;
      global.fetch = async () => {
        throw new Error('Network down');
      };

      try {
        const result = await registerForPushNotifications(testConfig, makePlatform());
        expect(result).toBe(false);
      } finally {
        global.fetch = originalFetch;
      }
    });

    it('returns false when backend returns error', async () => {
      const originalFetch = global.fetch;
      global.fetch = async () =>
        new Response('Invalid token', { status: 400 });

      try {
        const result = await registerForPushNotifications(testConfig, makePlatform());
        expect(result).toBe(false);
      } finally {
        global.fetch = originalFetch;
      }
    });

    it('passes device info from platform to the API', async () => {
      const captured: Array<Record<string, string>> = [];

      const originalFetch = global.fetch;
      global.fetch = async (_url: string, init?: RequestInit) => {
        captured.push(JSON.parse(init?.body as string) as Record<string, string>);
        return new Response(JSON.stringify({ id: '1' }), {
          status: 200,
          headers: { 'Content-Type': 'application/json' },
        });
      };

      try {
        const platform = makePlatform({
          getDeviceInfo: () => ({
            deviceId: 'pixel-8',
            deviceName: 'Pixel 8',
            osVersion: '14',
            appVersion: '1.2.3',
          }),
          getPlatform: () => 'android',
        });

        const result = await registerForPushNotifications(testConfig, platform);
        expect(result).toBe(true);
        expect(captured[0].device_id).toBe('pixel-8');
        expect(captured[0].platform).toBe('android');
        expect(captured[0].device_name).toBe('Pixel 8');
        expect(captured[0].os_version).toBe('14');
        expect(captured[0].app_version).toBe('1.2.3');
      } finally {
        global.fetch = originalFetch;
      }
    });
  });
});
