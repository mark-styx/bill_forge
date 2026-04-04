import { Redirect, Tabs } from 'expo-router';
import { View, Text, StyleSheet } from 'react-native';
import { useAppStore } from '../../lib/store';

function TabBarIcon({ name, focused }: { name: 'inbox' | 'chart' | 'gear'; focused: boolean }) {
  // Minimal text-based icons (no icon library dependency for MVP)
  const labels = { inbox: '🔔', chart: '📊', gear: '⚙️' };
  return (
    <Text style={[styles.icon, focused && styles.iconFocused]}>{labels[name]}</Text>
  );
}

export default function TabLayout() {
  const token = useAppStore((s) => s.token);
  const pendingCount = useAppStore((s) => s.pendingCount);

  if (!token) {
    return <Redirect href="/(auth)/login" />;
  }

  return (
    <Tabs
      screenOptions={{
        headerShown: true,
        tabBarActiveTintColor: '#2563eb',
        tabBarStyle: { paddingBottom: 4 },
      }}
    >
      <Tabs.Screen
        name="index"
        options={{
          title: 'Inbox',
          tabBarIcon: ({ focused }) => <TabBarIcon name="inbox" focused={focused} />,
          tabBarBadge: pendingCount > 0 ? pendingCount : undefined,
        }}
      />
      <Tabs.Screen
        name="dashboard"
        options={{
          title: 'Dashboard',
          tabBarIcon: ({ focused }) => <TabBarIcon name="chart" focused={focused} />,
        }}
      />
      <Tabs.Screen
        name="settings"
        options={{
          title: 'Settings',
          tabBarIcon: ({ focused }) => <TabBarIcon name="gear" focused={focused} />,
        }}
      />
      <Tabs.Screen
        name="invoice/[id]"
        options={{
          href: null, // Hidden from tab bar - navigated to programmatically
        }}
      />
    </Tabs>
  );
}

const styles = StyleSheet.create({
  icon: { fontSize: 20 },
  iconFocused: { opacity: 1 },
});
