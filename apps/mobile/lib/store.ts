/**
 * Zustand store for BillForge mobile app.
 *
 * Three slices:
 *  - Auth: token, userId, tenantId persisted in expo-secure-store
 *  - Sync: lastSyncAt timestamp persisted in AsyncStorage
 *  - Badge: pending approval count (ephemeral, drives tab badge)
 */

import { create } from 'zustand';
import * as SecureStore from 'expo-secure-store';
import {
  registerTokenGetter,
  registerRefreshTokenGetter,
  registerTokenSetter,
  registerLogoutHandler,
} from './api';

// ---- Auth slice ----

const SECURE_TOKEN_KEY = 'bf_auth_token';
const SECURE_USER_KEY = 'bf_user_id';
const SECURE_TENANT_KEY = 'bf_tenant_id';
const SECURE_REFRESH_KEY = 'bf_refresh_token';

// ---- Sync slice ----

const SYNC_TS_KEY = 'bf_last_sync_at';

interface AuthState {
  token: string | null;
  refreshToken: string | null;
  userId: string | null;
  tenantId: string | null;
  hydrated: boolean;
}

interface SyncState {
  lastSyncAt: string | null;
}

interface BadgeState {
  pendingCount: number;
}

interface AppStore extends AuthState, SyncState, BadgeState {
  // Auth actions
  setAuth(token: string, refreshToken: string, userId: string, tenantId: string): Promise<void>;
  logout(): Promise<void>;
  hydrateAuth(): Promise<void>;

  // Sync actions
  updateSyncTimestamp(): Promise<void>;
  hydrateSync(): Promise<void>;

  // Badge actions
  setPendingCount(count: number): void;
}

export const useAppStore = create<AppStore>((set) => {
  // Register the token getter so api.ts can read the current token.
  registerTokenGetter(async () => useAppStore.getState().token);
  registerRefreshTokenGetter(async () => useAppStore.getState().refreshToken);
  registerTokenSetter(async (accessToken, refreshToken) => {
    await Promise.all([
      SecureStore.setItemAsync(SECURE_TOKEN_KEY, accessToken),
      SecureStore.setItemAsync(SECURE_REFRESH_KEY, refreshToken),
    ]);
    set({ token: accessToken, refreshToken });
  });
  registerLogoutHandler(async () => {
    await useAppStore.getState().logout();
  });

  return {
    // Auth initial
    token: null,
    refreshToken: null,
    userId: null,
    tenantId: null,
    hydrated: false,

    // Sync initial
    lastSyncAt: null,

    // Badge initial
    pendingCount: 0,

    // ---- Auth ----

    async setAuth(token, refreshToken, userId, tenantId) {
      await Promise.all([
        SecureStore.setItemAsync(SECURE_TOKEN_KEY, token),
        SecureStore.setItemAsync(SECURE_REFRESH_KEY, refreshToken),
        SecureStore.setItemAsync(SECURE_USER_KEY, userId),
        SecureStore.setItemAsync(SECURE_TENANT_KEY, tenantId),
      ]);
      set({ token, refreshToken, userId, tenantId });
    },

    async logout() {
      await Promise.all([
        SecureStore.deleteItemAsync(SECURE_TOKEN_KEY).catch(() => {}),
        SecureStore.deleteItemAsync(SECURE_REFRESH_KEY).catch(() => {}),
        SecureStore.deleteItemAsync(SECURE_USER_KEY).catch(() => {}),
        SecureStore.deleteItemAsync(SECURE_TENANT_KEY).catch(() => {}),
      ]);
      set({
        token: null,
        refreshToken: null,
        userId: null,
        tenantId: null,
        pendingCount: 0,
      });
    },

    async hydrateAuth() {
      const [token, refreshToken, userId, tenantId] = await Promise.all([
        SecureStore.getItemAsync(SECURE_TOKEN_KEY),
        SecureStore.getItemAsync(SECURE_REFRESH_KEY),
        SecureStore.getItemAsync(SECURE_USER_KEY),
        SecureStore.getItemAsync(SECURE_TENANT_KEY),
      ]);
      set({ token, refreshToken, userId, tenantId, hydrated: true });
    },

    // ---- Sync ----

    async updateSyncTimestamp() {
      const now = new Date().toISOString();
      try {
        // AsyncStorage is not available in all test environments,
        // so we wrap in try/catch and just keep in-memory.
        const AsyncStorage = require('@react-native-async-storage/async-storage').default;
        await AsyncStorage.setItem(SYNC_TS_KEY, now);
      } catch {
        // AsyncStorage unavailable (test env) - keep in memory only
      }
      set({ lastSyncAt: now });
    },

    async hydrateSync() {
      try {
        const AsyncStorage = require('@react-native-async-storage/async-storage').default;
        const ts = await AsyncStorage.getItem(SYNC_TS_KEY);
        if (ts) set({ lastSyncAt: ts });
      } catch {
        // AsyncStorage unavailable
      }
    },

    // ---- Badge ----

    setPendingCount(count) {
      set({ pendingCount: count });
    },
  };
});
