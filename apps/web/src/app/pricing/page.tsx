'use client';

import { useState, useMemo } from 'react';
import Link from 'next/link';
import { useThemeStore } from '@/stores/theme';
import { GradientButton } from '@/components/ui/gradient-card';
import { Check, ArrowRight, Calculator } from 'lucide-react';

// Plan definitions mirroring backend billforge-billing plans.rs
const PLANS = [
  {
    id: 'free',
    name: 'Free',
    description: 'Get started with basic invoice capture',
    monthlyPrice: 0,
    invoiceUnitPrice: 0,
    maxUsers: 1,
    maxInvoices: 25,
    maxVendors: 5,
    features: [
      'Up to 25 invoices/month',
      '1 user',
      'Basic OCR',
      '5 vendors',
    ],
    highlight: false,
  },
  {
    id: 'starter',
    name: 'Starter',
    description: 'Perfect for small businesses',
    monthlyPrice: 49,
    invoiceUnitPrice: 1.5,
    maxUsers: 3,
    maxInvoices: Infinity,
    maxVendors: 50,
    features: [
      'Unlimited invoices',
      'Up to 3 users',
      'Advanced OCR',
      '50 vendors',
      '$1.50 per processed invoice',
    ],
    highlight: false,
  },
  {
    id: 'professional',
    name: 'Professional',
    description: 'Full AP automation for growing teams',
    monthlyPrice: 149,
    invoiceUnitPrice: 1.0,
    maxUsers: 10,
    maxInvoices: Infinity,
    maxVendors: 500,
    features: [
      'Unlimited invoices',
      'Up to 10 users',
      'AI-powered OCR',
      '500 vendors',
      'Custom workflows',
      'Priority support',
      'API access',
      '$1.00 per processed invoice',
    ],
    highlight: true,
  },
  {
    id: 'enterprise',
    name: 'Enterprise',
    description: 'Custom solutions for large organizations',
    monthlyPrice: 499,
    invoiceUnitPrice: 0.65,
    maxUsers: Infinity,
    maxInvoices: Infinity,
    maxVendors: Infinity,
    features: [
      'Unlimited everything',
      'Unlimited users',
      'AI-powered OCR',
      'Unlimited vendors',
      'Custom workflows',
      'Priority support',
      'API access',
      'SSO/SAML',
      'Dedicated account manager',
      '$0.65 per processed invoice',
    ],
    highlight: false,
  },
] as const;

type PlanId = 'free' | 'starter' | 'professional' | 'enterprise';

export function recommendPlan(invoices: number, seats: number): PlanId {
  if (seats > 10 || invoices > 1000) return 'enterprise';
  if (seats > 3 || invoices > 100) return 'professional';
  if (invoices > 25 || seats > 1) return 'starter';
  return 'free';
}

export function estimateMonthlyCost(planId: PlanId, invoices: number): number {
  const plan = PLANS.find((p) => p.id === planId);
  if (!plan) return 0;
  return plan.monthlyPrice + Math.max(0, invoices) * plan.invoiceUnitPrice;
}

export { PLANS };

export default function PricingPage() {
  const { getCurrentColors } = useThemeStore();
  const colors = getCurrentColors();

  const [monthlyInvoices, setMonthlyInvoices] = useState(50);
  const [seats, setSeats] = useState(2);

  const recommendedPlan = useMemo(
    () => recommendPlan(monthlyInvoices, seats),
    [monthlyInvoices, seats]
  );

  const estimatedCost = useMemo(
    () => estimateMonthlyCost(recommendedPlan, monthlyInvoices),
    [recommendedPlan, monthlyInvoices]
  );

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
          <div className="inline-flex items-center gap-2 px-3 py-1 rounded-full bg-primary/10 text-primary text-sm font-medium mb-6">
            <Calculator className="w-4 h-4" />
            Pricing Calculator
          </div>
          <h1 className="text-4xl md:text-5xl font-bold text-foreground mb-4">
            Simple, transparent pricing
          </h1>
          <p className="text-lg text-muted-foreground max-w-2xl mx-auto">
            Start free, scale as you grow. No hidden fees. Cancel anytime.
          </p>
        </div>
      </section>

      {/* Calculator */}
      <section className="container mx-auto px-4 -mt-8 mb-16">
        <div className="max-w-lg mx-auto card p-6 space-y-6">
          <h2 className="text-lg font-semibold text-foreground text-center">
            Estimate your monthly cost
          </h2>

          <div>
            <label className="block text-sm font-medium text-muted-foreground mb-2">
              Monthly invoice volume: {monthlyInvoices}
            </label>
            <input
              type="range"
              min={0}
              max={2000}
              step={10}
              value={monthlyInvoices}
              onChange={(e) => setMonthlyInvoices(Number(e.target.value))}
              className="w-full accent-primary"
              data-testid="invoice-slider"
            />
            <div className="flex justify-between text-xs text-muted-foreground mt-1">
              <span>0</span>
              <span>500</span>
              <span>1,000</span>
              <span>2,000+</span>
            </div>
          </div>

          <div>
            <label className="block text-sm font-medium text-muted-foreground mb-2">
              Team seats: {seats}
            </label>
            <input
              type="range"
              min={1}
              max={50}
              step={1}
              value={seats}
              onChange={(e) => setSeats(Number(e.target.value))}
              className="w-full accent-primary"
              data-testid="seats-slider"
            />
            <div className="flex justify-between text-xs text-muted-foreground mt-1">
              <span>1</span>
              <span>10</span>
              <span>25</span>
              <span>50</span>
            </div>
          </div>

          <div
            className="rounded-xl p-4 text-center"
            style={{ background: `linear-gradient(135deg, hsl(${colors.primary} / 0.1), hsl(${colors.accent} / 0.1))` }}
          >
            <p className="text-sm text-muted-foreground mb-1">Recommended plan</p>
            <p className="text-2xl font-bold text-foreground" data-testid="recommended-plan">
              {PLANS.find((p) => p.id === recommendedPlan)?.name}
            </p>
            <p className="text-3xl font-bold mt-2" style={{ color: `hsl(${colors.primary})` }} data-testid="estimated-price">
              ${estimatedCost.toFixed(0)}
              <span className="text-sm font-normal text-muted-foreground">/mo</span>
            </p>
          </div>

          <Link href={`/signup?plan=${recommendedPlan}`} data-testid="signup-cta">
            <GradientButton gradient="primary" className="w-full">
              Start free sandbox
              <ArrowRight className="w-4 h-4 ml-2" />
            </GradientButton>
          </Link>
        </div>
      </section>

      {/* Plan Cards */}
      <section className="container mx-auto px-4 pb-20">
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6 max-w-6xl mx-auto">
          {PLANS.map((plan) => (
            <div
              key={plan.id}
              className={`card p-6 flex flex-col ${
                plan.id === recommendedPlan
                  ? 'ring-2 ring-primary'
                  : ''
              } ${plan.highlight ? 'relative' : ''}`}
            >
              {plan.highlight && (
                <div className="absolute -top-3 left-1/2 -translate-x-1/2 px-3 py-0.5 rounded-full bg-primary text-primary-foreground text-xs font-medium">
                  Most Popular
                </div>
              )}
              <div className="mb-4">
                <h3 className="text-xl font-bold text-foreground">{plan.name}</h3>
                <p className="text-sm text-muted-foreground mt-1">{plan.description}</p>
              </div>
              <div className="mb-4">
                <span className="text-3xl font-bold text-foreground">
                  ${plan.monthlyPrice}
                </span>
                {plan.monthlyPrice > 0 && (
                  <span className="text-sm text-muted-foreground">/mo</span>
                )}
                {plan.monthlyPrice === 0 && (
                  <span className="text-sm text-muted-foreground ml-1">forever</span>
                )}
              </div>
              <ul className="space-y-2 flex-1 mb-6">
                {plan.features.map((feature) => (
                  <li key={feature} className="flex items-start gap-2 text-sm">
                    <Check className="w-4 h-4 text-green-500 mt-0.5 flex-shrink-0" />
                    <span className="text-foreground">{feature}</span>
                  </li>
                ))}
              </ul>
              <Link href={`/signup?plan=${plan.id}`} className="block">
                <GradientButton
                  gradient={plan.highlight ? 'primary' : 'primary'}
                  className="w-full"
                >
                  {plan.monthlyPrice === 0 ? 'Get started free' : 'Start free trial'}
                </GradientButton>
              </Link>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}
