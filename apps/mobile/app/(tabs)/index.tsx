import { useCallback, useRef, useState } from 'react';
import {
  View,
  Text,
  FlatList,
  TouchableOpacity,
  StyleSheet,
  RefreshControl,
  Alert,
  Animated,
} from 'react-native';
import { useRouter } from 'expo-router';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { api, type MobileApprovalRequest } from '../../lib/api';
import { useAppStore } from '../../lib/store';

/** Format cents to dollars string, e.g. 150050 → "$1,500.50" */
function formatAmount(cents: number, currency: string): string {
  const symbol = currency === 'USD' ? '$' : currency;
  return `${symbol}${(cents / 100).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
}

/** Color for days-until-due badge */
function dueDateColor(days: number | null): string {
  if (days == null) return '#64748b';
  if (days < 3) return '#dc2626';
  if (days < 7) return '#f59e0b';
  return '#22c55e';
}

function ApprovalItem({
  item,
  onPress,
  onQuickApprove,
}: {
  item: MobileApprovalRequest;
  onPress: () => void;
  onQuickApprove: () => void;
}) {
  const translateX = useRef(new Animated.Value(0)).current;
  const [swiped, setSwiped] = useState(false);

  const inv = item.invoice;
  const daysColor = dueDateColor(inv.days_until_due);

  return (
    <Animated.View style={{ transform: [{ translateX }] }}>
      <TouchableOpacity style={styles.item} onPress={onPress} activeOpacity={0.7}>
        <View style={styles.itemTop}>
          <Text style={styles.vendorName} numberOfLines={1}>
            {inv.vendor_name}
          </Text>
          <Text style={[styles.dueBadge, { color: daysColor }]}>
            {inv.days_until_due != null ? `${inv.days_until_due}d` : '—'}
          </Text>
        </View>

        <View style={styles.itemBottom}>
          <Text style={styles.invoiceNumber}>{inv.invoice_number}</Text>
          <Text style={styles.amount}>{formatAmount(inv.total_amount_cents, inv.currency)}</Text>
        </View>

        {item.can_approve && (
          <View style={styles.actionRow}>
            <TouchableOpacity
              style={[styles.quickButton, styles.approveButton]}
              onPress={onQuickApprove}
            >
              <Text style={styles.quickButtonText}>Approve</Text>
            </TouchableOpacity>
          </View>
        )}
      </TouchableOpacity>
    </Animated.View>
  );
}

export default function InboxScreen() {
  const router = useRouter();
  const queryClient = useQueryClient();
  const setPendingCount = useAppStore((s) => s.setPendingCount);

  const {
    data: approvals = [],
    isLoading,
    refetch,
    isRefetching,
  } = useQuery<MobileApprovalRequest[]>({
    queryKey: ['approvals'],
    queryFn: async () => {
      const result = await api.getApprovals();
      setPendingCount(result.length);
      return result;
    },
    refetchInterval: 30_000,
  });

  const handleQuickApprove = useCallback(
    (item: MobileApprovalRequest) => {
      Alert.alert('Approve Invoice', `Approve ${item.invoice.invoice_number}?`, [
        { text: 'Cancel', style: 'cancel' },
        {
          text: 'Approve',
          style: 'default',
          onPress: async () => {
            try {
              await api.approveInvoice(item.id);
              queryClient.invalidateQueries({ queryKey: ['approvals'] });
            } catch {
              Alert.alert('Error', 'Failed to approve invoice.');
            }
          },
        },
      ]);
    },
    [queryClient],
  );

  const renderItem = useCallback(
    ({ item }: { item: MobileApprovalRequest }) => (
      <ApprovalItem
        item={item}
        onPress={() => router.push(`/invoice/${item.invoice.id}`)}
        onQuickApprove={() => handleQuickApprove(item)}
      />
    ),
    [router, handleQuickApprove],
  );

  return (
    <View style={styles.container}>
      <FlatList
        data={approvals}
        keyExtractor={(item) => item.id}
        renderItem={renderItem}
        refreshControl={
          <RefreshControl refreshing={isRefetching} onRefresh={refetch} tintColor="#2563eb" />
        }
        ListEmptyComponent={
          <View style={styles.emptyState}>
            <Text style={styles.emptyIcon}>✅</Text>
            <Text style={styles.emptyTitle}>All caught up!</Text>
            <Text style={styles.emptySubtitle}>No pending approvals.</Text>
          </View>
        }
        contentContainerStyle={approvals.length === 0 ? styles.emptyList : styles.list}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: '#f8fafc' },
  list: { padding: 16 },
  emptyList: { flex: 1 },
  item: {
    backgroundColor: '#ffffff',
    borderRadius: 10,
    padding: 16,
    marginBottom: 10,
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 1 },
    shadowOpacity: 0.06,
    shadowRadius: 4,
    elevation: 2,
  },
  itemTop: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 6,
  },
  vendorName: { fontSize: 16, fontWeight: '600', color: '#0f172a', flex: 1, marginRight: 8 },
  dueBadge: { fontSize: 14, fontWeight: '700' },
  itemBottom: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  invoiceNumber: { fontSize: 13, color: '#64748b' },
  amount: { fontSize: 15, fontWeight: '600', color: '#334155' },
  actionRow: {
    flexDirection: 'row',
    justifyContent: 'flex-end',
    marginTop: 10,
    paddingTop: 10,
    borderTopWidth: 1,
    borderTopColor: '#f1f5f9',
  },
  quickButton: {
    paddingHorizontal: 16,
    paddingVertical: 6,
    borderRadius: 6,
  },
  approveButton: { backgroundColor: '#dcfce7' },
  quickButtonText: { fontSize: 13, fontWeight: '600', color: '#16a34a' },
  emptyState: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    paddingHorizontal: 32,
  },
  emptyIcon: { fontSize: 48, marginBottom: 8 },
  emptyTitle: { fontSize: 20, fontWeight: '600', color: '#0f172a' },
  emptySubtitle: { fontSize: 14, color: '#64748b', marginTop: 4 },
});
