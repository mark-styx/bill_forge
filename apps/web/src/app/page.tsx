'use client';

import { useEffect } from 'react';
import { useRouter } from 'next/navigation';
import { useAuthStore } from '@/stores/auth';

export default function HomePage() {
  const router = useRouter();
  const isAuthenticated = useAuthStore((state) => state.isAuthenticated);
  const hasHydrated = useAuthStore((state) => state.hasHydrated);

  useEffect(() => {
    if (!hasHydrated) return;

    if (isAuthenticated) {
      router.replace('/dashboard');
    } else {
      router.replace('/home');
    }
  }, [isAuthenticated, hasHydrated, router]);

  return null;
}
