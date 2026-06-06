'use client';

import { useState, useEffect } from 'react';
import { useRouter, useSearchParams } from 'next/navigation';
import Link from 'next/link';
import { useAuthStore, setupApiCallbacks } from '@/stores/auth';
import { api } from '@/lib/api';
import { GradientButton } from '@/components/ui/gradient-card';
import { FileText, ArrowRight, Check } from 'lucide-react';
import { useThemeStore } from '@/stores/theme';

export default function SignupPage() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const planId = searchParams.get('plan') || 'free';
  const { getCurrentColors } = useThemeStore();
  const colors = getCurrentColors();

  const [email, setEmail] = useState('');
  const [companyName, setCompanyName] = useState('');
  const [password, setPassword] = useState('');
  const [name, setName] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // If already authenticated, redirect to dashboard
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
  useEffect(() => {
    if (isAuthenticated) {
      router.replace('/dashboard');
    }
  }, [isAuthenticated, router]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setLoading(true);

    try {
      const apiBase = typeof window !== 'undefined' ? '' : (process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8080');
      const res = await fetch(`${apiBase}/api/public/signup`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          email,
          company_name: companyName,
          password,
          name: name || undefined,
          plan_id: planId,
        }),
      });

      if (!res.ok) {
        const body = await res.json().catch(() => null);
        const msg = body?.error?.message || `Signup failed (${res.status})`;
        throw new Error(msg);
      }

      const data = await res.json();

      // Store auth tokens via auth store
      api.setToken(data.access_token);
      api.setRefreshToken(data.refresh_token);
      setupApiCallbacks();

      useAuthStore.setState({
        user: {
          id: data.tenant_id,
          tenant_id: data.tenant_id,
          email,
          name: name || email,
          roles: ['tenant_admin'],
        },
        tenant: {
          id: data.tenant_id,
          name: companyName,
          enabled_modules: ['invoice_capture', 'invoice_processing', 'vendor_management', 'reporting'],
          settings: {
            company_name: companyName,
            timezone: 'UTC',
            default_currency: 'USD',
          },
        },
        accessToken: data.access_token,
        refreshToken: data.refresh_token,
        isAuthenticated: true,
        isLoading: false,
      });

      router.push('/dashboard?welcome=sandbox');
    } catch (err) {
      setError(err instanceof Error ? err.message : 'An unexpected error occurred');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="min-h-screen bg-background flex flex-col">
      {/* Header */}
      <header className="border-b border-border bg-background/95 backdrop-blur">
        <div className="container mx-auto flex h-16 items-center justify-between px-4">
          <Link href="/" className="flex items-center gap-2">
            <div
              className="flex h-9 w-9 items-center justify-center rounded-xl"
              style={{ background: `linear-gradient(135deg, hsl(${colors.primary}), hsl(${colors.accent}))` }}
            >
              <FileText className="h-5 w-5 text-white" />
            </div>
            <span className="text-xl font-bold text-foreground">BillForge</span>
          </Link>
          <Link
            href="/login"
            className="text-sm font-medium text-muted-foreground hover:text-foreground transition-colors"
          >
            Sign in
          </Link>
        </div>
      </header>

      {/* Form */}
      <main className="flex-1 flex items-center justify-center px-4 py-12">
        <div className="w-full max-w-md">
          <div className="text-center mb-8">
            <h1 className="text-2xl font-bold text-foreground">
              Create your free sandbox
            </h1>
            <p className="text-muted-foreground mt-2">
              Get instant access with sample data. No credit card required.
            </p>
          </div>

          <form onSubmit={handleSubmit} className="card p-6 space-y-4">
            {error && (
              <div className="rounded-lg border border-destructive/50 bg-destructive/5 p-3 text-sm text-destructive">
                {error}
              </div>
            )}

            <div>
              <label htmlFor="company" className="block text-sm font-medium text-foreground mb-1">
                Company name
              </label>
              <input
                id="company"
                type="text"
                required
                value={companyName}
                onChange={(e) => setCompanyName(e.target.value)}
                placeholder="Acme Corp"
                className="w-full rounded-lg border border-input bg-background px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary"
              />
            </div>

            <div>
              <label htmlFor="email" className="block text-sm font-medium text-foreground mb-1">
                Work email
              </label>
              <input
                id="email"
                type="email"
                required
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                placeholder="you@company.com"
                className="w-full rounded-lg border border-input bg-background px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary"
              />
            </div>

            <div>
              <label htmlFor="name" className="block text-sm font-medium text-foreground mb-1">
                Your name <span className="text-muted-foreground">(optional)</span>
              </label>
              <input
                id="name"
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="Jane Smith"
                className="w-full rounded-lg border border-input bg-background px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary"
              />
            </div>

            <div>
              <label htmlFor="password" className="block text-sm font-medium text-foreground mb-1">
                Password
              </label>
              <input
                id="password"
                type="password"
                required
                minLength={8}
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="Min 8 characters"
                className="w-full rounded-lg border border-input bg-background px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary"
              />
            </div>

            <div className="pt-2">
              <GradientButton
                type="submit"
                gradient="primary"
                className="w-full"
                disabled={loading}
              >
                {loading ? 'Creating sandbox...' : 'Create free sandbox'}
                {!loading && <ArrowRight className="w-4 h-4 ml-2" />}
              </GradientButton>
            </div>

            <div className="flex flex-col gap-2 pt-3 border-t border-border">
              <p className="text-xs text-muted-foreground text-center">Your sandbox includes:</p>
              <div className="grid grid-cols-2 gap-1.5">
                {[
                  'Sample vendors & invoices',
                  'Pre-configured workflows',
                  'All features unlocked',
                  'One-click upgrade path',
                ].map((item) => (
                  <div key={item} className="flex items-center gap-1.5 text-xs text-muted-foreground">
                    <Check className="w-3 h-3 text-green-500 flex-shrink-0" />
                    {item}
                  </div>
                ))}
              </div>
            </div>
          </form>

          <p className="text-center text-sm text-muted-foreground mt-6">
            Already have an account?{' '}
            <Link href="/login" className="text-primary hover:underline font-medium">
              Sign in
            </Link>
          </p>
        </div>
      </main>
    </div>
  );
}
