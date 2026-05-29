/**
 * Push notification registration for mobile approvals.
 *
 * Requests notification permissions via expo-notifications, obtains a
 * push token, and calls the backend device-registration endpoint so the
 * server can deliver push notifications to this approver.
 *
 * Designed to be testable: the notification primitives are injected so
 * tests can provide fakes without reaching for device APIs.
 */

import { ApiConfig, registerDevice } from './api';

/** Platform-specific push token. */
export interface PushToken {
  /** expo push token string, e.g. "ExponentPushToken[xxxxx]" */
  token: string;
  /** "ios" | "android" */
  platform: string;
}

/** Device info that may be available from expo constants. */
export interface DeviceInfo {
  deviceId: string;
  deviceName?: string;
  osVersion?: string;
  appVersion?: string;
}

/**
 * Platform primitives this module needs.
 *
 * In production these come from expo-notifications and expo-application.
 * In tests they are faked.
 */
export interface NotificationPlatform {
  /** Request notification permissions. Returns true if granted. */
  requestPermissions(): Promise<boolean>;
  /** Obtain an Expo push token. Returns null if unavailable. */
  getPushToken(): Promise<string | null>;
  /** Detect platform: "ios" or "android". */
  getPlatform(): 'ios' | 'android';
  /** Device identity info. */
  getDeviceInfo(): DeviceInfo;
}

/**
 * Attempt to register the device for push notifications.
 *
 * - Requests permission from the OS.
 * - Obtains an Expo push token.
 * - Calls the backend /devices/register endpoint.
 *
 * Returns true on success, false on any failure (permission denied, no
 * token, network error).  Errors are logged but never thrown so the
 * caller can proceed without blocking onboarding.
 */
export async function registerForPushNotifications(
  config: ApiConfig,
  platform: NotificationPlatform,
): Promise<boolean> {
  try {
    const granted = await platform.requestPermissions();
    if (!granted) {
      return false;
    }

    const pushToken = await platform.getPushToken();
    if (!pushToken) {
      return false;
    }

    const info = platform.getDeviceInfo();

    await registerDevice(config, {
      device_id: info.deviceId,
      platform: platform.getPlatform(),
      token: pushToken,
      device_name: info.deviceName,
      os_version: info.osVersion,
      app_version: info.appVersion,
    });

    return true;
  } catch {
    // Non-fatal: push registration failure must not block login.
    return false;
  }
}
