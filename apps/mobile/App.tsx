import React, { useCallback, useEffect, useRef, useState } from 'react';
import {
  ActivityIndicator,
  Alert,
  Modal,
  SafeAreaView,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  TouchableOpacity,
  View,
} from 'react-native';
import { StatusBar } from 'expo-status-bar';
import AsyncStorage from '@react-native-async-storage/async-storage';
import NetInfo from '@react-native-community/netinfo';

import {
  ApprovalItem,
  KVStore,
  cacheApprovals,
  enqueueAction,
  flushQueue,
  getCachedApprovals,
  getQueuedActions,
} from './src/lib/offline-queue';
import * as api from './src/lib/api';
import {
  AuthState,
  TenantOption,
  login as authLogin,
  loginWithTenant,
  loadAuth,
  saveAuth,
  clearAuth,
} from './src/lib/auth';
import {
  registerForPushNotifications,
  NotificationPlatform,
} from './src/lib/notifications';

// AsyncStorage-backed KVStore
const store: KVStore = {
  async getItem(key: string) {
    return AsyncStorage.getItem(key);
  },
  async setItem(key: string, value: string) {
    await AsyncStorage.setItem(key, value);
  },
};

// Base URL for the backend API. In a real build this would come from
// expo-constants or an EAS environment variable.
const API_BASE =
  (global as unknown as { expo?: { extra?: Record<string, string> } }).expo?.extra?.apiBaseUrl ||
  'http://localhost:8080';

/**
 * Build an ApiConfig from the current auth state.
 * Returns null when not authenticated.
 */
function configFromAuth(auth: AuthState): api.ApiConfig {
  return { baseUrl: API_BASE, jwt: auth.jwt, tenantId: auth.tenantId };
}

/**
 * Production implementation of NotificationPlatform.
 * Lazily requires expo modules so the test suite (which runs in node)
 * does not crash on missing native modules.
 */
function getNotificationPlatform(): NotificationPlatform {
  // eslint-disable-next-line @typescript-eslint/no-var-requires
  const Notifications = require('expo-notifications');
  // eslint-disable-next-line @typescript-eslint/no-var-requires
  const Application = require('expo-application');
  // eslint-disable-next-line @typescript-eslint/no-var-requires
  const Constants = require('expo-constants');
  // eslint-disable-next-line @typescript-eslint/no-var-requires
  const Platform = require('react-native').Platform;

  return {
    async requestPermissions() {
      const settings = await Notifications.requestPermissionsAsync({
        ios: { allowAlert: true, allowBadge: true, allowSound: true },
      });
      return settings.granted || settings.ios?.status === 'granted' || false;
    },
    async getPushToken() {
      try {
        const { data } = await Notifications.getExpoPushTokenAsync();
        return data;
      } catch {
        return null;
      }
    },
    getPlatform() {
      return Platform.OS === 'ios' ? 'ios' : 'android';
    },
    getDeviceInfo() {
      return {
        deviceId: Constants.sessionId ?? Constants.deviceId ?? 'unknown',
        deviceName: Constants.deviceName ?? undefined,
        osVersion: Platform.Version?.toString() ?? undefined,
        appVersion: Constants.manifest?.version ?? Application?.nativeAppVersion ?? undefined,
      };
    },
  };
}

function formatCents(cents: number, currency: string): string {
  return `${(cents / 100).toFixed(2)} ${currency}`;
}

// ===== Login Screen =====

function LoginScreen({
  onLogin,
  baseUrl,
}: {
  onLogin: (state: AuthState) => void;
  baseUrl: string;
}) {
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  // Tenant picker state
  const [pickerJwt, setPickerJwt] = useState('');
  const [tenants, setTenants] = useState<TenantOption[]>([]);

  const handleLogin = async () => {
    if (!email.trim() || !password) return;
    setLoading(true);
    setError('');
    try {
      const result = await authLogin(baseUrl, { email: email.trim(), password });
      if (result.kind === 'logged_in') {
        onLogin(result.state);
      } else {
        // Show tenant picker
        setPickerJwt(result.jwt);
        setTenants(result.tenants);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Login failed');
    } finally {
      setLoading(false);
    }
  };

  const handleTenantSelect = async (tenantId: string) => {
    setLoading(true);
    setError('');
    try {
      const state = await loginWithTenant(baseUrl, pickerJwt, tenantId);
      onLogin(state);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Tenant selection failed');
    } finally {
      setLoading(false);
    }
  };

  // Tenant picker UI
  if (tenants.length > 0) {
    return (
      <SafeAreaView style={styles.container}>
        <StatusBar style="auto" />
        <View style={styles.header}>
          <Text style={styles.title}>Select Organization</Text>
        </View>
        <ScrollView style={styles.list}>
          {tenants.map((t) => (
            <TouchableOpacity
              key={t.tenantId}
              style={styles.card}
              onPress={() => handleTenantSelect(t.tenantId)}
              disabled={loading}
            >
              <Text style={styles.vendorName}>{t.tenantName}</Text>
              <Text style={styles.invoiceNumber}>Role: {t.role}</Text>
            </TouchableOpacity>
          ))}
        </ScrollView>
        {error ? <Text style={styles.errorText}>{error}</Text> : null}
      </SafeAreaView>
    );
  }

  // Login form
  return (
    <SafeAreaView style={styles.container}>
      <StatusBar style="auto" />
      <View style={styles.loginCard}>
        <Text style={styles.title}>BillForge Approvals</Text>
        <Text style={styles.subtitle}>Sign in to review invoices</Text>

        <TextInput
          style={styles.loginInput}
          placeholder="Email"
          value={email}
          onChangeText={setEmail}
          keyboardType="email-address"
          autoCapitalize="none"
          autoCorrect={false}
          editable={!loading}
        />
        <TextInput
          style={styles.loginInput}
          placeholder="Password"
          value={password}
          onChangeText={setPassword}
          secureTextEntry
          autoCapitalize="none"
          editable={!loading}
        />

        {error ? <Text style={styles.errorText}>{error}</Text> : null}

        <TouchableOpacity
          style={[styles.button, styles.approveButton, loading && styles.buttonDisabled]}
          onPress={handleLogin}
          disabled={loading || !email.trim() || !password}
        >
          {loading ? (
            <ActivityIndicator color="#fff" />
          ) : (
            <Text style={styles.buttonText}>Sign In</Text>
          )}
        </TouchableOpacity>
      </View>
    </SafeAreaView>
  );
}

// ===== Main Approval Screen =====

export default function App() {
  const [auth, setAuth] = useState<AuthState | null>(null);
  const [authLoading, setAuthLoading] = useState(true);
  const [approvals, setApprovals] = useState<ApprovalItem[]>([]);
  const [pendingCount, setPendingCount] = useState(0);
  const [isOnline, setIsOnline] = useState(true);
  const [loading, setLoading] = useState(true);
  const [syncing, setSyncing] = useState(false);
  const [modalVisible, setModalVisible] = useState(false);
  const [modalKind, setModalKind] = useState<'approve' | 'reject'>('approve');
  const [modalItem, setModalItem] = useState<ApprovalItem | null>(null);
  const [modalText, setModalText] = useState('');

  // Track whether push registration was attempted this session
  const pushRegistered = useRef(false);

  // Restore persisted auth on mount
  useEffect(() => {
    const restore = async () => {
      const saved = await loadAuth(store);
      if (saved && saved.jwt) {
        setAuth(saved);
      }
      setAuthLoading(false);
    };
    restore();
  }, []);

  // When auth becomes available, register for push notifications (once)
  useEffect(() => {
    if (!auth || pushRegistered.current) return;
    pushRegistered.current = true;

    const config = configFromAuth(auth);
    registerForPushNotifications(config, getNotificationPlatform()).catch(() => {
      // Non-blocking: push registration failure does not affect the user
    });
  }, [auth]);

  // Flush the offline queue on mount when authenticated
  useEffect(() => {
    if (!auth) return;
    const doFlush = async () => {
      setSyncing(true);
      try {
        const config = configFromAuth(auth);
        const offlineApi = {
          approve: (id: string, comment: string) => api.approve(config, id, comment),
          reject: (id: string, reason: string) => api.reject(config, id, reason),
        };
        await flushQueue(store, offlineApi);
      } catch {
        // flushQueue itself handles errors
      } finally {
        setSyncing(false);
        refreshPendingCount();
      }
    };
    doFlush();
  }, [auth]);

  // Subscribe to connectivity changes and flush on reconnect
  useEffect(() => {
    if (!auth) return;
    const unsubscribe = NetInfo.addEventListener((state) => {
      const online = state.isConnected === true;
      setIsOnline(online);

      if (online) {
        const doFlush = async () => {
          setSyncing(true);
          try {
            const config = configFromAuth(auth);
            const offlineApi = {
              approve: (id: string, comment: string) => api.approve(config, id, comment),
              reject: (id: string, reason: string) => api.reject(config, id, reason),
            };
            const summary = await flushQueue(store, offlineApi);
            if (summary.synced > 0 || summary.conflicts > 0) {
              await refreshApprovals();
            }
          } catch {
            // best effort
          } finally {
            setSyncing(false);
            refreshPendingCount();
          }
        };
        doFlush();
      }
    });

    return unsubscribe;
  }, [auth]);

  // Load cached approvals immediately, then fetch fresh if online
  useEffect(() => {
    if (!auth) return;
    const load = async () => {
      setLoading(true);
      try {
        const cached = await getCachedApprovals(store);
        if (cached.length > 0) {
          setApprovals(cached);
        }
      } catch {
        // ignore cache errors
      }
      setLoading(false);
      refreshApprovals();
      refreshPendingCount();
    };
    load();
  }, [auth]);

  const refreshPendingCount = async () => {
    const queue = await getQueuedActions(store);
    setPendingCount(queue.length);
  };

  const refreshApprovals = useCallback(async () => {
    if (!auth) return;
    try {
      const config = configFromAuth(auth);
      const items = await api.listApprovals(config);
      await cacheApprovals(store, items);
      setApprovals(items);
    } catch {
      // Network error: keep showing cached data
    }
  }, [auth]);

  const handleAction = useCallback(
    (approvalId: string, kind: 'approve' | 'reject', payload: string) => {
      if (!auth) return;
      const actionFn = async () => {
        const action = await enqueueAction(store, {
          approvalId,
          kind,
          payload,
        });

        // Update local state immediately
        setApprovals((prev) => prev.filter((a) => a.id !== approvalId));
        setPendingCount((prev) => prev + 1);

        // If online, try to flush immediately
        const netState = await NetInfo.fetch();
        if (netState.isConnected) {
          setSyncing(true);
          try {
            const config = configFromAuth(auth);
            const offlineApi = {
              approve: (id: string, comment: string) => api.approve(config, id, comment),
              reject: (id: string, reason: string) => api.reject(config, id, reason),
            };
            await flushQueue(store, offlineApi);
            await refreshApprovals();
          } catch {
            // Will retry on next connectivity change
          } finally {
            setSyncing(false);
            await refreshPendingCount();
          }
        }
      };
      actionFn();
    },
    [auth, refreshApprovals],
  );

  const promptAction = (item: ApprovalItem, kind: 'approve' | 'reject') => {
    setModalItem(item);
    setModalKind(kind);
    setModalText('');
    setModalVisible(true);
  };

  const submitModal = () => {
    if (!modalItem) return;
    if (modalKind === 'reject' && !modalText.trim()) {
      Alert.alert('Reason required', 'Please provide a rejection reason.');
      return;
    }
    handleAction(modalItem.id, modalKind, modalText);
    setModalVisible(false);
    setModalItem(null);
    setModalText('');
  };

  const cancelModal = () => {
    setModalVisible(false);
    setModalItem(null);
    setModalText('');
  };

  const handleLogin = async (state: AuthState) => {
    await saveAuth(store, state);
    setAuth(state);
  };

  const handleSignOut = async () => {
    await clearAuth(store);
    setAuth(null);
    setApprovals([]);
    setPendingCount(0);
    pushRegistered.current = false;
  };

  // Auth loading splash
  if (authLoading) {
    return (
      <SafeAreaView style={styles.container}>
        <StatusBar style="auto" />
        <View style={styles.center}>
          <ActivityIndicator size="large" />
        </View>
      </SafeAreaView>
    );
  }

  // Not authenticated: show login screen
  if (!auth) {
    return <LoginScreen onLogin={handleLogin} baseUrl={API_BASE} />;
  }

  // Authenticated: show approvals
  return (
    <SafeAreaView style={styles.container}>
      <StatusBar style="auto" />

      {/* Connectivity banner */}
      <View style={[styles.banner, isOnline ? styles.bannerOnline : styles.bannerOffline]}>
        <Text style={styles.bannerText}>
          {!isOnline ? 'OFFLINE' : syncing ? 'SYNCING...' : 'ONLINE'}
          {pendingCount > 0 ? ` (${pendingCount} pending sync)` : ''}
        </Text>
      </View>

      {/* Header */}
      <View style={styles.header}>
        <Text style={styles.title}>Pending Approvals</Text>
        <TouchableOpacity onPress={handleSignOut} style={styles.signOutButton}>
          <Text style={styles.signOutText}>Sign Out</Text>
        </TouchableOpacity>
      </View>

      {/* Content */}
      {loading ? (
        <View style={styles.center}>
          <ActivityIndicator size="large" />
        </View>
      ) : approvals.length === 0 ? (
        <View style={styles.center}>
          <Text style={styles.emptyText}>No pending approvals</Text>
        </View>
      ) : (
        <ScrollView style={styles.list}>
          {approvals.map((item) => (
            <View key={item.id} style={styles.card}>
              <Text style={styles.vendorName}>{item.invoice.vendor_name}</Text>
              <Text style={styles.invoiceNumber}>{item.invoice.invoice_number}</Text>
              <Text style={styles.amount}>
                {formatCents(item.invoice.total_amount_cents, item.invoice.currency)}
              </Text>
              {item.invoice.due_date && (
                <Text style={styles.dueDate}>Due: {item.invoice.due_date}</Text>
              )}

              <View style={styles.actions}>
                <TouchableOpacity
                  style={[styles.button, styles.approveButton]}
                  onPress={() => promptAction(item, 'approve')}
                >
                  <Text style={styles.buttonText}>Approve</Text>
                </TouchableOpacity>
                <TouchableOpacity
                  style={[styles.button, styles.rejectButton]}
                  onPress={() => promptAction(item, 'reject')}
                >
                  <Text style={styles.buttonText}>Reject</Text>
                </TouchableOpacity>
              </View>
            </View>
          ))}
        </ScrollView>
      )}
      {/* Approve / Reject Modal */}
      <Modal
        visible={modalVisible}
        transparent
        animationType="fade"
        onRequestClose={cancelModal}
      >
        <View style={styles.modalOverlay}>
          <View style={styles.modalCard}>
            <Text style={styles.modalTitle}>
              {modalKind === 'approve' ? 'Approve Invoice' : 'Reject Invoice'}
            </Text>
            {modalItem && (
              <Text style={styles.modalSubtitle}>
                {modalItem.invoice.vendor_name} - {modalItem.invoice.invoice_number}
              </Text>
            )}
            <TextInput
              style={styles.modalInput}
              placeholder={modalKind === 'approve' ? 'Comment (optional)' : 'Reason (required)'}
              value={modalText}
              onChangeText={setModalText}
              autoFocus
            />
            <View style={styles.modalActions}>
              <TouchableOpacity
                style={[styles.modalButton, styles.modalCancelButton]}
                onPress={cancelModal}
              >
                <Text style={styles.modalButtonText}>Cancel</Text>
              </TouchableOpacity>
              <TouchableOpacity
                style={[
                  styles.modalButton,
                  modalKind === 'approve'
                    ? styles.approveButton
                    : styles.rejectButton,
                ]}
                onPress={submitModal}
              >
                <Text style={styles.buttonText}>
                  {modalKind === 'approve' ? 'Approve' : 'Reject'}
                </Text>
              </TouchableOpacity>
            </View>
          </View>
        </View>
      </Modal>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#f5f5f5',
  },
  banner: {
    paddingVertical: 6,
    paddingHorizontal: 16,
    alignItems: 'center',
  },
  bannerOnline: {
    backgroundColor: '#22c55e',
  },
  bannerOffline: {
    backgroundColor: '#ef4444',
  },
  bannerText: {
    color: '#fff',
    fontSize: 13,
    fontWeight: '600',
  },
  header: {
    paddingHorizontal: 16,
    paddingVertical: 12,
    borderBottomWidth: 1,
    borderBottomColor: '#e0e0e0',
    backgroundColor: '#fff',
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
  },
  title: {
    fontSize: 20,
    fontWeight: '700',
    color: '#111',
  },
  signOutButton: {
    paddingVertical: 4,
    paddingHorizontal: 12,
  },
  signOutText: {
    fontSize: 14,
    color: '#ef4444',
    fontWeight: '600',
  },
  center: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
  },
  emptyText: {
    fontSize: 16,
    color: '#888',
  },
  list: {
    flex: 1,
    padding: 12,
  },
  card: {
    backgroundColor: '#fff',
    borderRadius: 10,
    padding: 16,
    marginBottom: 10,
    shadowColor: '#000',
    shadowOpacity: 0.06,
    shadowRadius: 4,
    shadowOffset: { width: 0, height: 2 },
    elevation: 2,
  },
  vendorName: {
    fontSize: 16,
    fontWeight: '600',
    color: '#111',
  },
  invoiceNumber: {
    fontSize: 14,
    color: '#555',
    marginTop: 2,
  },
  amount: {
    fontSize: 18,
    fontWeight: '700',
    color: '#111',
    marginTop: 6,
  },
  dueDate: {
    fontSize: 13,
    color: '#888',
    marginTop: 2,
  },
  actions: {
    flexDirection: 'row',
    marginTop: 12,
    gap: 10,
  },
  button: {
    flex: 1,
    paddingVertical: 10,
    borderRadius: 8,
    alignItems: 'center',
  },
  approveButton: {
    backgroundColor: '#22c55e',
  },
  rejectButton: {
    backgroundColor: '#ef4444',
  },
  buttonDisabled: {
    opacity: 0.6,
  },
  buttonText: {
    color: '#fff',
    fontSize: 15,
    fontWeight: '600',
  },
  loginCard: {
    flex: 1,
    justifyContent: 'center',
    paddingHorizontal: 24,
  },
  subtitle: {
    fontSize: 16,
    color: '#555',
    marginTop: 4,
    marginBottom: 24,
  },
  loginInput: {
    backgroundColor: '#fff',
    borderWidth: 1,
    borderColor: '#ddd',
    borderRadius: 8,
    padding: 12,
    fontSize: 16,
    marginBottom: 12,
  },
  errorText: {
    color: '#ef4444',
    fontSize: 14,
    marginTop: 8,
    textAlign: 'center',
  },
  modalOverlay: {
    flex: 1,
    backgroundColor: 'rgba(0,0,0,0.4)',
    justifyContent: 'center',
    alignItems: 'center',
  },
  modalCard: {
    backgroundColor: '#fff',
    borderRadius: 12,
    padding: 20,
    width: '85%',
    maxWidth: 400,
  },
  modalTitle: {
    fontSize: 18,
    fontWeight: '700',
    color: '#111',
    marginBottom: 4,
  },
  modalSubtitle: {
    fontSize: 14,
    color: '#555',
    marginBottom: 16,
  },
  modalInput: {
    borderWidth: 1,
    borderColor: '#ddd',
    borderRadius: 8,
    padding: 10,
    fontSize: 15,
    marginBottom: 16,
  },
  modalActions: {
    flexDirection: 'row',
    gap: 10,
  },
  modalButton: {
    flex: 1,
    paddingVertical: 10,
    borderRadius: 8,
    alignItems: 'center',
  },
  modalCancelButton: {
    backgroundColor: '#e5e7eb',
  },
  modalButtonText: {
    fontSize: 15,
    fontWeight: '600',
    color: '#333',
  },
});
