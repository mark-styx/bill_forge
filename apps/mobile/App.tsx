import React, { useCallback, useEffect, useState } from 'react';
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

// AsyncStorage-backed KVStore
const store: KVStore = {
  async getItem(key: string) {
    return AsyncStorage.getItem(key);
  },
  async setItem(key: string, value: string) {
    await AsyncStorage.setItem(key, value);
  },
};

// Read config from Expo's extra config (set via app.json or app.config.ts)
function getConfig(): api.ApiConfig {
  // In a real build these come from expo-constants or secure storage.
  // For development they can be set in app.json `extra` or environment.
  const extra = (global as unknown as { expo?: { extra?: Record<string, string> } }).expo?.extra;
  return {
    baseUrl: extra?.apiBaseUrl || 'http://localhost:8080',
    jwt: extra?.jwt || '',
    tenantId: extra?.tenantId || '',
  };
}

function formatCents(cents: number, currency: string): string {
  return `${(cents / 100).toFixed(2)} ${currency}`;
}

export default function App() {
  const [approvals, setApprovals] = useState<ApprovalItem[]>([]);
  const [pendingCount, setPendingCount] = useState(0);
  const [isOnline, setIsOnline] = useState(true);
  const [loading, setLoading] = useState(true);
  const [syncing, setSyncing] = useState(false);
  const [modalVisible, setModalVisible] = useState(false);
  const [modalKind, setModalKind] = useState<'approve' | 'reject'>('approve');
  const [modalItem, setModalItem] = useState<ApprovalItem | null>(null);
  const [modalText, setModalText] = useState('');

  // Flush the offline queue on mount
  useEffect(() => {
    const doFlush = async () => {
      setSyncing(true);
      try {
        const config = getConfig();
        const offlineApi = {
          approve: (id: string, comment: string) => api.approve(config, id, comment),
          reject: (id: string, reason: string) => api.reject(config, id, reason),
        };
        await flushQueue(store, offlineApi);
      } catch {
        // flushQueue itself handles errors; this catches unexpected issues
      } finally {
        setSyncing(false);
        refreshPendingCount();
      }
    };
    doFlush();
  }, []);

  // Subscribe to connectivity changes and flush on reconnect
  useEffect(() => {
    const unsubscribe = NetInfo.addEventListener((state) => {
      const online = state.isConnected === true;
      setIsOnline(online);

      if (online) {
        // Flush queued actions when coming back online
        const doFlush = async () => {
          setSyncing(true);
          try {
            const config = getConfig();
            const offlineApi = {
              approve: (id: string, comment: string) => api.approve(config, id, comment),
              reject: (id: string, reason: string) => api.reject(config, id, reason),
            };
            const summary = await flushQueue(store, offlineApi);
            if (summary.synced > 0 || summary.conflicts > 0) {
              // Refresh approvals after successful flush
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
  }, []);

  // Load cached approvals immediately, then fetch fresh if online
  useEffect(() => {
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
  }, []);

  const refreshPendingCount = async () => {
    const queue = await getQueuedActions(store);
    setPendingCount(queue.length);
  };

  const refreshApprovals = async () => {
    try {
      const config = getConfig();
      if (!config.jwt) return; // not configured yet
      const items = await api.listApprovals(config);
      await cacheApprovals(store, items);
      setApprovals(items);
    } catch {
      // Network error: keep showing cached data
    }
  };

  const handleAction = useCallback(
    (approvalId: string, kind: 'approve' | 'reject', payload: string) => {
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
            const config = getConfig();
            const offlineApi = {
              approve: (id: string, comment: string) => api.approve(config, id, comment),
              reject: (id: string, reason: string) => api.reject(config, id, reason),
            };
            await flushQueue(store, offlineApi);
            // Flush succeeded, refresh the list from server
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
    [],
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
  },
  title: {
    fontSize: 20,
    fontWeight: '700',
    color: '#111',
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
  buttonText: {
    color: '#fff',
    fontSize: 15,
    fontWeight: '600',
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
