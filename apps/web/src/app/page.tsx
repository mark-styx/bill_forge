'use client';

import { useEffect } from 'react';
import { useRouter } from 'next/navigation';
import { useAuthStore } from '@/stores/auth';
import { FileText } from 'lucide-react';

export default function HomePage() {
  const router = useRouter();
  const isAuthenticated = useAuthStore((state) => state.isAuthenticated);
  const hasHydrated = useAuthStore((state) => state.hasHydrated);

  useEffect(() => {
    if (!hasHydrated) return;

    if (isAuthenticated) {
      router.push('/dashboard');
    } else {
      router.push('/home');
    }
  }, [isAuthenticated, hasHydrated, router]);

  return (
    <div className="min-h-screen flex items-center justify-center bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900">
      <div className="text-center">
        <div className="animate-pulse">
          <div className="inline-flex items-center justify-center w-16 h-16 rounded-2xl bg-blue-500/20 mb-4">
            <FileText className="w-8 h-8 text-blue-400" />
          </div>
          <div className="text-xl font-semibold text-white">BillForge</div>
        </div>
      </div>
    </div>
  );
}
