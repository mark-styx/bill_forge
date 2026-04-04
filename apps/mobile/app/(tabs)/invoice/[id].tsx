import { useState } from 'react';
import {
  View,
  Text,
  ScrollView,
  TouchableOpacity,
  TextInput,
  StyleSheet,
  ActivityIndicator,
  Alert,
  Modal,
} from 'react-native';
import { useRouter, useLocalSearchParams } from 'expo-router';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { api, type MobileInvoiceSummary } from '../../../lib/api';

function formatAmount(cents: number, currency: string): string {
  const symbol = currency === 'USD' ? '$' : currency;
  return `${symbol}${(cents / 100).toLocaleString('en-US', {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`;
}

export default function InvoiceDetailScreen() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const router = useRouter();
  const queryClient = useQueryClient();

  const [approveModalVisible, setApproveModalVisible] = useState(false);
  const [rejectModalVisible, setRejectModalVisible] = useState(false);
  const [comment, setComment] = useState('');
  const [reason, setReason] = useState('');
  const [submitting, setSubmitting] = useState(false);

  const {
    data: invoice,
    isLoading,
    error,
  } = useQuery<MobileInvoiceSummary>({
    queryKey: ['invoice', id],
    queryFn: () => api.getInvoice(id!),
    enabled: !!id,
  });

  async function handleApprove() {
    if (submitting) return;
    setSubmitting(true);
    try {
      await api.approveInvoice(id!, comment.trim() || undefined);
      queryClient.invalidateQueries({ queryKey: ['approvals'] });
      queryClient.invalidateQueries({ queryKey: ['invoice', id] });
      setApproveModalVisible(false);
      setComment('');
      Alert.alert('Approved', 'Invoice has been approved.', [
        { text: 'OK', onPress: () => router.back() },
      ]);
    } catch {
      Alert.alert('Error', 'Failed to approve invoice.');
    } finally {
      setSubmitting(false);
    }
  }

  async function handleReject() {
    if (submitting) return;
    if (!reason.trim()) {
      Alert.alert('Required', 'Please enter a rejection reason.');
      return;
    }
    setSubmitting(true);
    try {
      await api.rejectInvoice(id!, reason.trim());
      queryClient.invalidateQueries({ queryKey: ['approvals'] });
      queryClient.invalidateQueries({ queryKey: ['invoice', id] });
      setRejectModalVisible(false);
      setReason('');
      Alert.alert('Rejected', 'Invoice has been rejected.', [
        { text: 'OK', onPress: () => router.back() },
      ]);
    } catch {
      Alert.alert('Error', 'Failed to reject invoice.');
    } finally {
      setSubmitting(false);
    }
  }

  if (isLoading) {
    return (
      <View style={styles.center}>
        <ActivityIndicator size="large" color="#2563eb" />
      </View>
    );
  }

  if (error || !invoice) {
    return (
      <View style={styles.center}>
        <Text style={styles.errorText}>Failed to load invoice.</Text>
      </View>
    );
  }

  const dueDateStr = invoice.due_date
    ? new Date(invoice.due_date).toLocaleDateString('en-US', {
        month: 'short',
        day: 'numeric',
        year: 'numeric',
      })
    : 'No due date';

  return (
    <View style={styles.container}>
      <ScrollView style={styles.scroll} contentContainerStyle={styles.content}>
        {/* Header */}
        <Text style={styles.vendorName}>{invoice.vendor_name}</Text>
        <Text style={styles.invoiceNumber}>{invoice.invoice_number}</Text>

        {/* Amount */}
        <View style={styles.amountRow}>
          <Text style={styles.amountLabel}>Amount</Text>
          <Text style={styles.amountValue}>
            {formatAmount(invoice.total_amount_cents, invoice.currency)}
          </Text>
        </View>

        {/* Details */}
        <View style={styles.detailGrid}>
          <View style={styles.detailItem}>
            <Text style={styles.detailLabel}>Status</Text>
            <Text style={styles.detailValue}>{invoice.status.replace(/_/g, ' ')}</Text>
          </View>
          <View style={styles.detailItem}>
            <Text style={styles.detailLabel}>Due Date</Text>
            <Text style={styles.detailValue}>{dueDateStr}</Text>
          </View>
          <View style={styles.detailItem}>
            <Text style={styles.detailLabel}>Days Until Due</Text>
            <Text
              style={[
                styles.detailValue,
                invoice.days_until_due != null && invoice.days_until_due < 3 && styles.urgent,
              ]}
            >
              {invoice.days_until_due != null ? `${invoice.days_until_due} days` : '—'}
            </Text>
          </View>
          <View style={styles.detailItem}>
            <Text style={styles.detailLabel}>Currency</Text>
            <Text style={styles.detailValue}>{invoice.currency}</Text>
          </View>
        </View>

        {invoice.requires_action && (
          <View style={styles.actionBanner}>
            <Text style={styles.actionBannerText}>This invoice requires your action.</Text>
          </View>
        )}
      </ScrollView>

      {/* Action Buttons */}
      {invoice.status === 'pending_approval' && (
        <View style={styles.actionBar}>
          <TouchableOpacity
            style={[styles.actionButton, styles.rejectButton]}
            onPress={() => setRejectModalVisible(true)}
          >
            <Text style={styles.rejectButtonText}>Reject</Text>
          </TouchableOpacity>
          <TouchableOpacity
            style={[styles.actionButton, styles.approveButton]}
            onPress={() => setApproveModalVisible(true)}
          >
            <Text style={styles.approveButtonText}>Approve</Text>
          </TouchableOpacity>
        </View>
      )}

      {/* Approve Modal */}
      <Modal visible={approveModalVisible} transparent animationType="slide">
        <View style={styles.modalOverlay}>
          <View style={styles.modalContent}>
            <Text style={styles.modalTitle}>Approve Invoice</Text>
            <Text style={styles.modalSubtitle}>
              {invoice.vendor_name} - {invoice.invoice_number}
            </Text>
            <TextInput
              style={styles.modalInput}
              placeholder="Comment (optional)"
              value={comment}
              onChangeText={setComment}
              multiline
              maxLength={500}
            />
            <View style={styles.modalActions}>
              <TouchableOpacity
                style={styles.modalCancel}
                onPress={() => {
                  setApproveModalVisible(false);
                  setComment('');
                }}
              >
                <Text style={styles.modalCancelText}>Cancel</Text>
              </TouchableOpacity>
              <TouchableOpacity
                style={[styles.modalConfirm, styles.approveButton]}
                onPress={handleApprove}
                disabled={submitting}
              >
                <Text style={styles.approveButtonText}>
                  {submitting ? 'Approving...' : 'Confirm'}
                </Text>
              </TouchableOpacity>
            </View>
          </View>
        </View>
      </Modal>

      {/* Reject Modal */}
      <Modal visible={rejectModalVisible} transparent animationType="slide">
        <View style={styles.modalOverlay}>
          <View style={styles.modalContent}>
            <Text style={styles.modalTitle}>Reject Invoice</Text>
            <Text style={styles.modalSubtitle}>
              {invoice.vendor_name} - {invoice.invoice_number}
            </Text>
            <TextInput
              style={styles.modalInput}
              placeholder="Reason for rejection *"
              value={reason}
              onChangeText={setReason}
              multiline
              maxLength={500}
              autoFocus
            />
            <View style={styles.modalActions}>
              <TouchableOpacity
                style={styles.modalCancel}
                onPress={() => {
                  setRejectModalVisible(false);
                  setReason('');
                }}
              >
                <Text style={styles.modalCancelText}>Cancel</Text>
              </TouchableOpacity>
              <TouchableOpacity
                style={[styles.modalConfirm, styles.rejectButton]}
                onPress={handleReject}
                disabled={submitting}
              >
                <Text style={styles.rejectButtonText}>
                  {submitting ? 'Rejecting...' : 'Reject'}
                </Text>
              </TouchableOpacity>
            </View>
          </View>
        </View>
      </Modal>
    </View>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: '#f8fafc' },
  scroll: { flex: 1 },
  content: { padding: 16 },
  center: { flex: 1, justifyContent: 'center', alignItems: 'center' },
  vendorName: { fontSize: 22, fontWeight: '700', color: '#0f172a' },
  invoiceNumber: { fontSize: 14, color: '#64748b', marginTop: 2, marginBottom: 20 },
  amountRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    backgroundColor: '#fff',
    borderRadius: 10,
    padding: 16,
    marginBottom: 16,
  },
  amountLabel: { fontSize: 14, color: '#64748b' },
  amountValue: { fontSize: 24, fontWeight: '700', color: '#0f172a' },
  detailGrid: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    gap: 12,
    marginBottom: 16,
  },
  detailItem: {
    flex: 1,
    minWidth: '45%',
    backgroundColor: '#fff',
    borderRadius: 8,
    padding: 12,
  },
  detailLabel: { fontSize: 11, color: '#94a3b8', textTransform: 'uppercase', marginBottom: 4 },
  detailValue: { fontSize: 14, fontWeight: '500', color: '#0f172a' },
  urgent: { color: '#dc2626' },
  actionBanner: {
    backgroundColor: '#fef3c7',
    borderRadius: 8,
    padding: 12,
    borderLeftWidth: 4,
    borderLeftColor: '#f59e0b',
  },
  actionBannerText: { fontSize: 14, color: '#92400e' },
  actionBar: {
    flexDirection: 'row',
    gap: 12,
    padding: 16,
    backgroundColor: '#fff',
    borderTopWidth: 1,
    borderTopColor: '#e2e8f0',
  },
  actionButton: {
    flex: 1,
    paddingVertical: 14,
    borderRadius: 10,
    alignItems: 'center',
  },
  approveButton: { backgroundColor: '#16a34a' },
  approveButtonText: { color: '#fff', fontSize: 16, fontWeight: '600' },
  rejectButton: { backgroundColor: '#fff', borderWidth: 1.5, borderColor: '#dc2626' },
  rejectButtonText: { color: '#dc2626', fontSize: 16, fontWeight: '600' },
  modalOverlay: {
    flex: 1,
    justifyContent: 'flex-end',
    backgroundColor: 'rgba(0,0,0,0.4)',
  },
  modalContent: {
    backgroundColor: '#fff',
    borderTopLeftRadius: 16,
    borderTopRightRadius: 16,
    padding: 24,
    paddingBottom: 40,
  },
  modalTitle: { fontSize: 20, fontWeight: '700', color: '#0f172a', marginBottom: 4 },
  modalSubtitle: { fontSize: 14, color: '#64748b', marginBottom: 16 },
  modalInput: {
    borderWidth: 1,
    borderColor: '#e2e8f0',
    borderRadius: 8,
    padding: 12,
    fontSize: 16,
    minHeight: 80,
    textAlignVertical: 'top',
    marginBottom: 16,
  },
  modalActions: { flexDirection: 'row', gap: 12 },
  modalCancel: {
    flex: 1,
    paddingVertical: 12,
    alignItems: 'center',
    borderRadius: 8,
    backgroundColor: '#f1f5f9',
  },
  modalCancelText: { fontSize: 14, fontWeight: '600', color: '#64748b' },
  modalConfirm: { flex: 1, paddingVertical: 12, alignItems: 'center', borderRadius: 8 },
  errorText: { color: '#dc2626', fontSize: 14 },
});
