import { View, Text, StyleSheet, ScrollView, ActivityIndicator } from 'react-native';
import { useQuery } from '@tanstack/react-query';
import { api } from '../../lib/api';
import type { MobileDashboard } from '../../lib/api';

function StatCard({ label, value, color }: { label: string; value: number; color: string }) {
  return (
    <View style={[styles.statCard, { borderLeftColor: color }]}>
      <Text style={styles.statValue}>{value}</Text>
      <Text style={styles.statLabel}>{label}</Text>
    </View>
  );
}

export default function DashboardScreen() {
  const { data, isLoading, error } = useQuery<MobileDashboard>({
    queryKey: ['dashboard'],
    queryFn: () => api.getDashboard(),
    staleTime: 60_000,
  });

  if (isLoading) {
    return (
      <View style={styles.center}>
        <ActivityIndicator size="large" color="#2563eb" />
      </View>
    );
  }

  if (error) {
    return (
      <View style={styles.center}>
        <Text style={styles.errorText}>Failed to load dashboard.</Text>
      </View>
    );
  }

  return (
    <ScrollView style={styles.container} contentContainerStyle={styles.content}>
      <Text style={styles.heading}>Dashboard</Text>

      <View style={styles.statsRow}>
        <StatCard label="Pending Approvals" value={data?.pending_approvals ?? 0} color="#f59e0b" />
        <StatCard label="Pending Review" value={data?.pending_review ?? 0} color="#3b82f6" />
      </View>
      <View style={styles.statsRow}>
        <StatCard label="Needs Attention" value={data?.requires_attention ?? 0} color="#ef4444" />
      </View>

      <Text style={styles.sectionTitle}>Upcoming Due Dates</Text>
      {(data?.upcoming_due_dates?.length ?? 0) === 0 ? (
        <Text style={styles.emptyText}>No upcoming due dates.</Text>
      ) : (
        (data?.upcoming_due_dates ?? []).map((inv) => (
          <View key={inv.id} style={styles.invoiceRow}>
            <View style={styles.invoiceInfo}>
              <Text style={styles.invoiceVendor}>{inv.vendor_name}</Text>
              <Text style={styles.invoiceNumber}>{inv.invoice_number}</Text>
            </View>
            <Text style={styles.invoiceDue}>
              {inv.days_until_due != null
                ? `${inv.days_until_due}d`
                : '—'}
            </Text>
          </View>
        ))
      )}

      <Text style={styles.sectionTitle}>Recent Activity</Text>
      {(data?.recent_activity?.length ?? 0) === 0 ? (
        <Text style={styles.emptyText}>No recent activity.</Text>
      ) : (
        (data?.recent_activity ?? []).map((item) => (
          <View key={item.id} style={styles.activityRow}>
            <Text style={styles.activityTitle}>{item.title}</Text>
            <Text style={styles.activityDesc}>{item.description}</Text>
          </View>
        ))
      )}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: '#f8fafc' },
  content: { padding: 16 },
  center: { flex: 1, justifyContent: 'center', alignItems: 'center' },
  heading: { fontSize: 24, fontWeight: '700', color: '#0f172a', marginBottom: 16 },
  statsRow: { flexDirection: 'row', gap: 12, marginBottom: 12 },
  statCard: {
    flex: 1,
    backgroundColor: '#fff',
    borderRadius: 8,
    padding: 16,
    borderLeftWidth: 4,
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 1 },
    shadowOpacity: 0.05,
    shadowRadius: 4,
    elevation: 2,
  },
  statValue: { fontSize: 28, fontWeight: '700', color: '#0f172a' },
  statLabel: { fontSize: 12, color: '#64748b', marginTop: 4 },
  sectionTitle: { fontSize: 16, fontWeight: '600', color: '#334155', marginTop: 20, marginBottom: 8 },
  emptyText: { color: '#94a3b8', fontSize: 14 },
  invoiceRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    backgroundColor: '#fff',
    borderRadius: 8,
    padding: 12,
    marginBottom: 8,
  },
  invoiceInfo: { flex: 1 },
  invoiceVendor: { fontSize: 14, fontWeight: '500', color: '#0f172a' },
  invoiceNumber: { fontSize: 12, color: '#64748b', marginTop: 2 },
  invoiceDue: { fontSize: 14, fontWeight: '600', color: '#64748b' },
  activityRow: {
    backgroundColor: '#fff',
    borderRadius: 8,
    padding: 12,
    marginBottom: 8,
  },
  activityTitle: { fontSize: 14, fontWeight: '500', color: '#0f172a' },
  activityDesc: { fontSize: 12, color: '#64748b', marginTop: 2 },
  errorText: { color: '#dc2626', fontSize: 14 },
});
