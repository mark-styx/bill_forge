import { useEffect } from 'react';
import { Stack } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import * as Notifications from 'expo-notifications';
import { useAppStore } from '../lib/store';
import { api } from '../lib/api';
import { Platform } from 'react-native';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      retry: 2,
    },
  },
});

// Configure how notifications appear when the app is foregrounded
Notifications.setNotificationHandler({
  handleNotification: async () => ({
    shouldShowAlert: true,
    shouldPlaySound: true,
    shouldSetBadge: true,
  }),
});

export default function RootLayout() {
  const hydrated = useAppStore((s) => s.hydrated);
  const token = useAppStore((s) => s.token);
  const hydrateAuth = useAppStore((s) => s.hydrateAuth);
  const hydrateSync = useAppStore((s) => s.hydrateSync);
  const setPendingCount = useAppStore((s) => s.setPendingCount);

  // Hydrate auth + sync state from persisted storage
  useEffect(() => {
    hydrateAuth();
    hydrateSync();
  }, []);

  // Register for push notifications when authenticated
  useEffect(() => {
    if (!token) return;

    let mounted = true;

    async function registerPush() {
      try {
        const { status: existingStatus } = await Notifications.getPermissionsAsync();
        let finalStatus = existingStatus;
        if (existingStatus !== 'granted') {
          const { status } = await Notifications.requestPermissionsAsync();
          finalStatus = status;
        }
        if (finalStatus !== 'granted') return;

        const pushToken = (await Notifications.getExpoPushTokenAsync()).data;

        if (!mounted) return;

        await api.registerDevice({
          device_id: pushToken.slice(-36), // unique per install
          platform: Platform.OS === 'ios' ? 'ios' : 'android',
          token: pushToken,
        });
      } catch {
        // Push registration failure is non-fatal
      }
    }

    registerPush();
    return () => {
      mounted = false;
    };
  }, [token]);

  // Listen for incoming notifications to update badge count
  useEffect(() => {
    if (!token) return;

    const subscription = Notifications.addNotificationReceivedListener((notification) => {
      const count = notification.request.content.badge;
      if (typeof count === 'number') {
        setPendingCount(count);
      }
    });

    return () => subscription.remove();
  }, [token, setPendingCount]);

  // Wait for auth hydration before rendering
  if (!hydrated) {
    return null;
  }

  return (
    <QueryClientProvider client={queryClient}>
      <StatusBar style="auto" />
      <Stack screenOptions={{ headerShown: false }}>
        <Stack.Screen name="(auth)/login" />
        <Stack.Screen name="(tabs)" />
      </Stack>
    </QueryClientProvider>
  );
}
