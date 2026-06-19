'use client';

import { useEffect, useState } from 'react';
import Link from 'next/link';
import { useThemeStore } from '@/stores/theme';
import { GradientButton } from '@/components/ui/gradient-card';
import { Check, ArrowRight } from 'lucide-react';
import { publicBillingApi, type PublicPlan } from '@/lib/api';

// Plans come from the backend (GET /api/public/plans -> Plan::all_public()).
// Enterprise has is_public=false in `backend/crates/billing/src/plans.rs`,
// so it is never returned here and never rendered on the public page.
//
// Plan-card feature copy is presentation only; the source of truth for
// prices and limits is the backend response (no hardcoded price values).
const PLAN_FEATURES: Record<string, string[]> = {
  free: [
    'Basic OCR',
    '5 vendors',
    '1 GB storage',
  ],
  starter: [
    'Up to 3 users',
    '50 vendors',
    '10 GB storage',
    'Vendor management module',
  ],
  professional: [
    'Up to 10 users',
    '500 vendors',
    '50 GB storage',
    'AI-powered OCR',
    'Custom workflows',
    'Priority support',
    'API access',
  ],
};

// u32::MAX from the backend, serialized as a JSON number.
const MAX_UINT32 = 4294967295;

function formatLimit(value: number, suffix: string): string {
  if (value >= MAX_UINT32) return `Unlimited ${suffix}`;
  return `${value} ${suffix}`;
}

function planFeatures(plan: PublicPlan): string[] {
  const base = PLAN_FEATURES[plan.id] ?? [];
  const limits: string[] = [
    formatLimit(plan.features.max_invoices_per_month, 'invoices/mo'),
    formatLimit(plan.features.max_vendors, 'vendors'),
  ];
  // Metered per-invoice price only matters on paid plans.
  if (plan.metered_invoice_unit_price_cents > 0) {
    const perInvoice = (plan.metered_invoice_unit_price_cents / 100).toFixed(2);
    limits.push(`$${perInvoice} per processed invoice`);
  }
  return [...limits, ...base];
}

export default function PricingPage() {
  const { getCurrentColors } = useThemeStore();
  const colors = getCurrentColors();

  const [plans, setPlans] = useState<PublicPlan[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    publicBillingApi
      .listPlans()
      .then((data) => {
        if (!cancelled) setPlans(data);
      })
      .catch((e: unknown) => {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : 'Failed to load plans');
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="min-h-screen bg-background">
      {/* Hero */}
      <section className="relative py-20 overflow-hidden">
        <div
          className="absolute inset-0 opacity-5"
          style={{
            background: `radial-gradient(circle at 30% 50%, hsl(${colors.primary}), transparent 50%), radial-gradient(circle at 70% 50%, hsl(${colors.accent}), transparent 50%)`,
          }}
        />
        <div className="container mx-auto px-4 text-center relative">
          <h1 className="text-4xl md:text-5xl font-bold text-foreground mb-4">
            Simple, transparent pricing
          </h1>
          <p className="text-lg text-muted-foreground max-w-2xl mx-auto">
            Start free, scale as you grow. No hidden fees. Cancel anytime.
          </p>
        </div>
      </section>

      {/* Plan Cards */}
      <section className="container mx-auto px-4 pb-20" data-testid="plans-section">
        {error && (
          <div
            className="max-w-lg mx-auto card p-6 text-center text-muted-foreground"
            data-testid="plans-error"
          >
            We couldn&apos;t load plans right now. Please try again later.
          </div>
        )}

        {!error && !plans && (
          <div
            className="max-w-lg mx-auto card p-6 text-center text-muted-foreground"
            data-testid="plans-loading"
          >
            Loading plans...
          </div>
        )}

        {!error && plans && plans.length > 0 && (
          <div className="grid grid-cols-1 md:grid-cols-3 gap-6 max-w-5xl mx-auto">
            {plans.map((plan) => {
              const monthlyDollars = plan.monthly_price_cents / 100;
              return (
                <div key={plan.id} className="card p-6 flex flex-col">
                  <div className="mb-4">
                    <h3 className="text-xl font-bold text-foreground">{plan.name}</h3>
                    <p className="text-sm text-muted-foreground mt-1">{plan.description}</p>
                  </div>
                  <div className="mb-4">
                    <span className="text-3xl font-bold text-foreground" data-testid={`plan-price-${plan.id}`}>
                      ${monthlyDollars}
                    </span>
                    {monthlyDollars > 0 ? (
                      <span className="text-sm text-muted-foreground">/mo</span>
                    ) : (
                      <span className="text-sm text-muted-foreground ml-1">forever</span>
                    )}
                  </div>
                  <ul className="space-y-2 flex-1 mb-6">
                    {planFeatures(plan).map((feature) => (
                      <li key={feature} className="flex items-start gap-2 text-sm">
                        <Check className="w-4 h-4 text-green-500 mt-0.5 flex-shrink-0" />
                        <span className="text-foreground">{feature}</span>
                      </li>
                    ))}
                  </ul>
                  <Link href={`/signup?plan=${plan.id}`} className="block" data-testid={`plan-cta-${plan.id}`}>
                    <GradientButton gradient="primary" className="w-full">
                      {monthlyDollars === 0 ? 'Get started free' : 'Start free trial'}
                      <ArrowRight className="w-4 h-4 ml-2" />
                    </GradientButton>
                  </Link>
                </div>
              );
            })}
          </div>
        )}
      </section>

      {/* Enterprise - not a public self-serve plan. Contact sales instead of
          showing a price or a free-trial CTA (mirrors is_public=false in the
          backend plan definitions). */}
      <section className="container mx-auto px-4 pb-24">
        <div
          className="max-w-3xl mx-auto card p-8 text-center"
          data-testid="enterprise-contact"
        >
          <h2 className="text-2xl font-bold text-foreground mb-2">Need Enterprise?</h2>
          <p className="text-muted-foreground mb-6">
            Custom volume pricing, SSO/SAML, dedicated support, and unlimited
            everything. Talk to our sales team for a tailored quote.
          </p>
          <a href="mailto:sales@billforge.com?subject=Enterprise%20pricing%20inquiry">
            <GradientButton gradient="primary">
              Contact sales
              <ArrowRight className="w-4 h-4 ml-2" />
            </GradientButton>
          </a>
        </div>
      </section>
    </div>
  );
}
