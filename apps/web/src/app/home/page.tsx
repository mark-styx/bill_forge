'use client';

import Link from 'next/link';
import { useThemeStore } from '@/stores/theme';
import { FeatureCard } from '@/components/ui/hero-section';
import {
  GradientCard,
  GradientText,
  GradientButton,
} from '@/components/ui/gradient-card';
import { Button } from '@/components/ui/button';
import {
  FileText,
  Zap,
  Shield,
  BarChart3,
  Check,
  ArrowRight,
  ScanLine,
  Workflow,
  CheckCircle,
  Lock,
  Clock,
  Puzzle,
  Eye,
  Download,
  Upload,
  Layers,
  CalendarCheck,
} from 'lucide-react';

const features = [
  {
    icon: <ScanLine className="w-6 h-6" />,
    title: 'Intelligent Capture',
    description:
      'OCR extracts line-item data from PDFs, scans, and email attachments without manual keying. Structured output ready for your GL.',
    color: 'capture' as const,
  },
  {
    icon: <Workflow className="w-6 h-6" />,
    title: 'Rule-Based Automation',
    description:
      'Route invoices by vendor, amount, cost center, or GL code. Build approval chains that mirror your org chart — no code required.',
    color: 'processing' as const,
  },
  {
    icon: <Eye className="w-6 h-6" />,
    title: 'Full Audit Visibility',
    description:
      'Every action timestamped and attributed. Know who approved what, when, and why. Audit readiness built in, not bolted on.',
    color: 'vendor' as const,
  },
  {
    icon: <BarChart3 className="w-6 h-6" />,
    title: 'AP Analytics',
    description:
      'Days payable outstanding, exception rates, and vendor spend trends at a glance. Aging reports you can actually act on.',
    color: 'reporting' as const,
  },
  {
    icon: <Puzzle className="w-6 h-6" />,
    title: 'Modular by Design',
    description:
      'Activate only the modules your team needs. Start with capture, add workflows and approvals as you scale — nothing locked behind tiers.',
    color: 'primary' as const,
  },
  {
    icon: <Shield className="w-6 h-6" />,
    title: 'Privacy & Control',
    description:
      'Your invoice data stays yours. Role-based access, data residency options, and no third-party data monetization.',
    color: 'vendor' as const,
  },
];

const steps = [
  {
    number: '01',
    icon: <Upload className="w-5 h-5" />,
    title: 'Capture',
    description:
      'Invoices arrive by email, upload, or API. OCR extracts vendor, amount, line items, and due date automatically.',
    color: 'capture' as const,
  },
  {
    number: '02',
    icon: <Layers className="w-5 h-5" />,
    title: 'Process',
    description:
      'Assignment rules match invoices to the right queue. Exceptions are flagged before they become problems.',
    color: 'processing' as const,
  },
  {
    number: '03',
    icon: <CalendarCheck className="w-5 h-5" />,
    title: 'Approve',
    description:
      'Approvers review on web or mobile. Multi-level routing, delegation, and automatic escalation on overdue items.',
    color: 'vendor' as const,
  },
  {
    number: '04',
    icon: <Download className="w-5 h-5" />,
    title: 'Export',
    description:
      'One-click export to QuickBooks, CSV, or your ERP. Payment files formatted exactly how your bank expects them.',
    color: 'reporting' as const,
  },
];

const pricingPlans = [
  {
    name: 'Starter',
    description: 'Teams processing up to 500 invoices/month',
    price: 299,
    period: 'month',
    features: [
      'Up to 500 invoices/month',
      'OCR capture (PDF + email)',
      '5 user seats',
      'Single-level approval workflows',
      'QuickBooks CSV export',
      'Standard audit log',
      'Email support',
    ],
    cta: 'Start Free Trial',
    popular: false,
    gradient: 'capture' as const,
  },
  {
    name: 'Growth',
    description: 'Growing AP teams, 500–2,000 invoices/month',
    price: 599,
    period: 'month',
    features: [
      'Up to 2,000 invoices/month',
      'Advanced OCR + GL coding',
      '15 user seats',
      'Multi-level approval routing',
      'Custom assignment rules',
      'Full AP analytics dashboard',
      'QuickBooks + direct ERP export',
      'Delegation & escalation rules',
      'Priority support',
    ],
    cta: 'Start Free Trial',
    popular: true,
    gradient: 'primary' as const,
  },
  {
    name: 'Scale',
    description: 'High-volume AP, 2,000–5,000 invoices/month',
    price: 1199,
    period: 'month',
    features: [
      'Up to 5,000 invoices/month',
      '50 user seats',
      'Multi-level approval routing',
      'Custom integrations via API',
      'Advanced analytics & reporting',
      'Dedicated onboarding',
      'SLA with uptime guarantee',
      'Priority support',
    ],
    cta: 'Start Free Trial',
    popular: false,
    gradient: 'vendor' as const,
  },
  {
    name: 'Enterprise',
    description: 'Custom solutions for complex requirements',
    price: null,
    period: 'month',
    features: [
      'Unlimited invoices',
      'Unlimited user seats',
      'SSO / SAML integration',
      'Custom integrations via API',
      'Data residency options',
      'Dedicated CSM',
      'SLA with uptime guarantee',
      'Custom contract',
    ],
    cta: 'Contact Sales',
    popular: false,
    gradient: 'reporting' as const,
  },
];

export default function LandingPage() {
  const { getCurrentColors } = useThemeStore();
  const colors = getCurrentColors();

  return (
    <div className="flex min-h-screen flex-col bg-background">

      {/* Sticky Header */}
      <header className="sticky top-0 z-50 border-b border-border/50 bg-background/80 backdrop-blur-lg">
        <div className="container mx-auto px-4 flex items-center justify-between h-16">
          <div className="flex items-center gap-2">
            <div
              className="flex h-9 w-9 items-center justify-center rounded-xl"
              style={{ background: `linear-gradient(135deg, hsl(${colors.primary}), hsl(${colors.accent}))` }}
            >
              <FileText className="h-5 w-5 text-white" />
            </div>
            <span className="text-xl font-bold text-foreground">BillForge</span>
          </div>
          <nav className="hidden md:flex items-center gap-6 text-sm text-muted-foreground">
            <Link href="#features" className="hover:text-foreground transition-colors">Features</Link>
            <Link href="#how-it-works" className="hover:text-foreground transition-colors">How It Works</Link>
            <Link href="#pricing" className="hover:text-foreground transition-colors">Pricing</Link>
          </nav>
          <div className="flex items-center gap-3">
            <Link href="/login">
              <Button variant="ghost" size="sm">Sign In</Button>
            </Link>
            <Link href="/login">
              <GradientButton gradient="primary" size="sm">
                Start Free Trial
              </GradientButton>
            </Link>
          </div>
        </div>
      </header>

      {/* Hero Section */}
      <section id="hero" className="relative overflow-hidden py-24 md:py-36">
        {/* Background */}
        <div className="absolute inset-0 overflow-hidden pointer-events-none">
          <div
            className="absolute -top-40 -right-40 w-96 h-96 rounded-full opacity-25 blur-3xl animate-pulse-soft"
            style={{ background: `hsl(${colors.primary})` }}
          />
          <div
            className="absolute -bottom-40 -left-40 w-96 h-96 rounded-full opacity-15 blur-3xl animate-pulse-soft"
            style={{ background: `hsl(${colors.accent})`, animationDelay: '1s' }}
          />
          <div
            className="absolute inset-0 bg-[linear-gradient(to_right,hsl(var(--border)/0.08)_1px,transparent_1px),linear-gradient(to_bottom,hsl(var(--border)/0.08)_1px,transparent_1px)] bg-[size:4rem_4rem]"
          />
        </div>

        <div className="container relative z-10 mx-auto px-4">
          <div className="mx-auto max-w-4xl text-center">
            {/* Eyebrow */}
            <div className="mb-6 inline-flex items-center rounded-full border border-primary/20 bg-primary/5 px-4 py-1.5 text-sm">
              <Zap className="mr-2 h-4 w-4 text-primary" />
              <span className="text-primary font-medium">AP Automation</span>
              <span className="mx-2 text-border">|</span>
              <span className="text-muted-foreground">Built for mid-market finance teams</span>
            </div>

            {/* Headline */}
            <h1 className="mb-6 text-4xl font-bold tracking-tight text-foreground md:text-6xl lg:text-7xl">
              Stop processing invoices.{' '}
              <GradientText gradient="primary" className="inline-block">
                Start managing AP.
              </GradientText>
            </h1>

            {/* Subheadline */}
            <p className="mb-8 text-lg text-muted-foreground md:text-xl max-w-2xl mx-auto leading-relaxed">
              BillForge automates the grind — capture, coding, routing, approval — so your AP team
              handles exceptions, not keystrokes. Enterprise-grade workflow without enterprise-grade
              complexity.
            </p>

            {/* CTAs */}
            <div className="flex flex-col items-center justify-center gap-4 sm:flex-row">
              <Link href="/login">
                <GradientButton gradient="primary" size="lg" className="min-w-[180px]">
                  Request a Demo
                  <ArrowRight className="ml-2 h-5 w-5" />
                </GradientButton>
              </Link>
              <Link href="#how-it-works">
                <Button variant="outline" size="lg" className="min-w-[180px]">
                  See How It Works
                </Button>
              </Link>
            </div>

            <p className="mt-4 text-sm text-muted-foreground">
              No credit card required for trial. Set up in under a day.
            </p>

            {/* Credibility bar — product facts, not fake social proof */}
            <div className="mt-14 grid grid-cols-1 sm:grid-cols-3 gap-6 max-w-2xl mx-auto">
              {[
                { value: '300–5,000', label: 'invoices/month supported' },
                { value: 'QuickBooks', label: 'native integration' },
                { value: 'Full', label: 'audit trail, every action' },
              ].map((item) => (
                <div key={item.label} className="text-center">
                  <div className="text-xl font-bold text-foreground">{item.value}</div>
                  <div className="text-sm text-muted-foreground mt-0.5">{item.label}</div>
                </div>
              ))}
            </div>
          </div>
        </div>
      </section>

      {/* Problem / Value Prop Band */}
      <section className="py-14 border-y border-border bg-secondary/20">
        <div className="container mx-auto px-4">
          <div className="mx-auto max-w-4xl">
            <p className="text-center text-sm font-medium text-muted-foreground uppercase tracking-widest mb-8">
              Why AP teams switch to BillForge
            </p>
            <div className="grid grid-cols-1 md:grid-cols-3 gap-8 text-center">
              {[
                {
                  icon: <Clock className="w-5 h-5 mx-auto mb-3 text-capture" />,
                  headline: 'Coupa and SAP are overkill',
                  body: 'Six-figure implementations, 12-month rollouts, and features you\'ll never use. BillForge deploys in days, not quarters.',
                },
                {
                  icon: <Layers className="w-5 h-5 mx-auto mb-3 text-processing" />,
                  headline: 'BILL is too simple',
                  body: 'Payment-first tools hit a wall at 200 invoices. You need real workflows, GL coding rules, and approval hierarchies.',
                },
                {
                  icon: <CheckCircle className="w-5 h-5 mx-auto mb-3 text-vendor" />,
                  headline: 'Spreadsheets don\'t scale',
                  body: 'Manual tracking creates risk: missed discounts, duplicate payments, and audit gaps that take days to untangle.',
                },
              ].map((item) => (
                <div key={item.headline} className="px-4">
                  {item.icon}
                  <h3 className="font-semibold text-foreground mb-2">{item.headline}</h3>
                  <p className="text-sm text-muted-foreground leading-relaxed">{item.body}</p>
                </div>
              ))}
            </div>
          </div>
        </div>
      </section>

      {/* Features Section */}
      <section id="features" className="py-20 md:py-32">
        <div className="container mx-auto px-4">
          <div className="mx-auto mb-16 max-w-2xl text-center">
            <div className="mb-4 inline-flex items-center rounded-full bg-capture/10 px-4 py-1.5 text-sm text-capture font-medium">
              <Zap className="mr-2 h-4 w-4" />
              Five Pillars
            </div>
            <h2 className="mb-4 text-3xl font-bold text-foreground md:text-4xl">
              Built around what AP managers actually need
            </h2>
            <p className="text-lg text-muted-foreground">
              Speed, automation, transparency, modularity, and privacy — every feature traces back to one of these.
            </p>
          </div>
          <div className="grid gap-6 md:grid-cols-2 lg:grid-cols-3">
            {features.map((feature) => (
              <FeatureCard
                key={feature.title}
                icon={feature.icon}
                title={feature.title}
                description={feature.description}
                color={feature.color}
              />
            ))}
          </div>
        </div>
      </section>

      {/* How It Works */}
      <section id="how-it-works" className="py-20 md:py-32 bg-gradient-to-b from-background to-secondary/20">
        <div className="container mx-auto px-4">
          <div className="mx-auto mb-16 max-w-2xl text-center">
            <div className="mb-4 inline-flex items-center rounded-full bg-processing/10 px-4 py-1.5 text-sm text-processing font-medium">
              <Workflow className="mr-2 h-4 w-4" />
              How It Works
            </div>
            <h2 className="mb-4 text-3xl font-bold text-foreground md:text-4xl">
              From inbox to export in four steps
            </h2>
            <p className="text-lg text-muted-foreground">
              A predictable, auditable flow every invoice follows. No exceptions to the exception process.
            </p>
          </div>

          <div className="mx-auto max-w-5xl">
            <div className="grid gap-6 md:grid-cols-2 lg:grid-cols-4">
              {steps.map((step, index) => (
                <div key={step.title} className="relative">
                  {/* Connector line */}
                  {index < steps.length - 1 && (
                    <div className="hidden lg:block absolute top-8 left-[calc(100%_-_12px)] w-6 h-px bg-border z-10" />
                  )}
                  <GradientCard hover className="h-full">
                    <div className="p-6">
                      <div className="flex items-center gap-3 mb-4">
                        <div
                          className="flex h-10 w-10 items-center justify-center rounded-xl text-white flex-shrink-0"
                          style={{
                            background: `linear-gradient(135deg, hsl(var(--${step.color})), hsl(var(--${step.color}) / 0.7))`,
                          }}
                        >
                          {step.icon}
                        </div>
                        <span className="text-xs font-mono text-muted-foreground font-bold tracking-widest">
                          {step.number}
                        </span>
                      </div>
                      <h3 className="text-lg font-semibold text-foreground mb-2">{step.title}</h3>
                      <p className="text-sm text-muted-foreground leading-relaxed">{step.description}</p>
                    </div>
                  </GradientCard>
                </div>
              ))}
            </div>
          </div>
        </div>
      </section>

      {/* Pricing Section */}
      <section id="pricing" className="py-20 md:py-32">
        <div className="container mx-auto px-4">
          <div className="mx-auto mb-16 max-w-2xl text-center">
            <div className="mb-4 inline-flex items-center rounded-full bg-vendor/10 px-4 py-1.5 text-sm text-vendor font-medium">
              <FileText className="mr-2 h-4 w-4" />
              Pricing
            </div>
            <h2 className="mb-4 text-3xl font-bold text-foreground md:text-4xl">
              Transparent pricing, no surprises
            </h2>
            <p className="text-lg text-muted-foreground">
              All plans include a 14-day free trial. No implementation fees. Cancel any time.
            </p>
          </div>

          <div className="mx-auto grid max-w-6xl gap-6 md:grid-cols-2 lg:grid-cols-4">
            {pricingPlans.map((plan) => (
              <GradientCard
                key={plan.name}
                gradient={plan.popular ? 'primary' : 'none'}
                gradientPosition={plan.popular ? 'border' : 'top'}
                hover
                className={plan.popular ? 'scale-105 shadow-lg' : ''}
              >
                <div className="p-6">
                  {plan.popular && (
                    <div className="mb-4 inline-flex items-center rounded-full bg-primary px-3 py-1 text-xs font-semibold text-white">
                      Most Popular
                    </div>
                  )}
                  <div className="mb-4">
                    <h3 className="text-xl font-semibold text-foreground">{plan.name}</h3>
                    <p className="text-sm text-muted-foreground mt-1">{plan.description}</p>
                  </div>
                  <div className="mb-6">
                    {plan.price !== null ? (
                      <>
                        <span className="text-4xl font-bold text-foreground">${plan.price.toLocaleString()}</span>
                        <span className="text-muted-foreground text-sm">/{plan.period}</span>
                      </>
                    ) : (
                      <span className="text-4xl font-bold text-foreground">Custom</span>
                    )}
                  </div>
                  <ul className="mb-6 space-y-3">
                    {plan.features.map((feature) => (
                      <li key={feature} className="flex items-start gap-2 text-sm text-muted-foreground">
                        <CheckCircle className="h-4 w-4 text-success flex-shrink-0 mt-0.5" />
                        {feature}
                      </li>
                    ))}
                  </ul>
                  <Link href="/login" className="block">
                    {plan.popular ? (
                      <GradientButton gradient="primary" className="w-full">
                        {plan.cta}
                      </GradientButton>
                    ) : (
                      <Button variant="outline" className="w-full">
                        {plan.cta}
                      </Button>
                    )}
                  </Link>
                </div>
              </GradientCard>
            ))}
          </div>

          <p className="text-center text-sm text-muted-foreground mt-8">
            All plans include a 14-day free trial. No credit card required.
          </p>
        </div>
      </section>

      {/* CTA Section */}
      <section id="demo" className="py-20">
        <div className="container mx-auto px-4">
          <GradientCard
            gradient="primary"
            gradientPosition="background"
            className="mx-auto max-w-4xl text-center"
          >
            <div
              className="p-10 md:p-16 rounded-2xl"
              style={{
                background: `linear-gradient(135deg, hsl(${colors.primary}), hsl(${colors.accent}))`,
              }}
            >
              <h2 className="mb-4 text-3xl md:text-4xl font-bold text-white">
                See BillForge with your own invoices
              </h2>
              <p className="mb-8 text-white/80 max-w-xl mx-auto text-lg leading-relaxed">
                We'll walk you through a live demo using your actual invoice volume and workflow
                requirements — no canned screenshots.
              </p>
              <div className="flex flex-col sm:flex-row gap-4 justify-center">
                <Link href="/login">
                  <Button size="lg" className="bg-white text-foreground hover:bg-white/90 min-w-[180px] font-semibold">
                    Request a Demo
                    <ArrowRight className="ml-2 h-5 w-5" />
                  </Button>
                </Link>
                <Link href="/login">
                  <Button
                    size="lg"
                    variant="outline"
                    className="border-white/30 text-white hover:bg-white/10 min-w-[180px]"
                  >
                    Start Free Trial
                  </Button>
                </Link>
              </div>
            </div>
          </GradientCard>
        </div>
      </section>

      {/* Footer */}
      <footer className="border-t border-border py-12 bg-card">
        <div className="container mx-auto px-4">
          <div className="grid gap-8 md:grid-cols-4">
            <div>
              <div className="mb-4 flex items-center gap-2">
                <div
                  className="flex h-9 w-9 items-center justify-center rounded-xl"
                  style={{ background: `linear-gradient(135deg, hsl(${colors.primary}), hsl(${colors.accent}))` }}
                >
                  <FileText className="h-5 w-5 text-white" />
                </div>
                <span className="text-xl font-bold text-foreground">BillForge</span>
              </div>
              <p className="text-sm text-muted-foreground leading-relaxed">
                Simplified invoice processing for the mid-market. Enterprise-grade AP automation without enterprise-grade overhead.
              </p>
            </div>
            <div>
              <h4 className="mb-4 font-semibold text-foreground">Product</h4>
              <ul className="space-y-2 text-sm text-muted-foreground">
                <li><Link href="#features" className="hover:text-foreground transition-colors">Features</Link></li>
                <li><Link href="#how-it-works" className="hover:text-foreground transition-colors">How It Works</Link></li>
                <li><Link href="#pricing" className="hover:text-foreground transition-colors">Pricing</Link></li>
                <li><Link href="/login" className="hover:text-foreground transition-colors">Sign In</Link></li>
              </ul>
            </div>
            <div>
              <h4 className="mb-4 font-semibold text-foreground">Use Cases</h4>
              <ul className="space-y-2 text-sm text-muted-foreground">
                <li><a href="#" className="hover:text-foreground transition-colors">QuickBooks Teams</a></li>
                <li><a href="#" className="hover:text-foreground transition-colors">High-Volume AP</a></li>
                <li><a href="#" className="hover:text-foreground transition-colors">Multi-Approver Workflows</a></li>
                <li><a href="#" className="hover:text-foreground transition-colors">Audit Readiness</a></li>
              </ul>
            </div>
            <div>
              <h4 className="mb-4 font-semibold text-foreground">Legal</h4>
              <ul className="space-y-2 text-sm text-muted-foreground">
                <li><a href="#" className="hover:text-foreground transition-colors">Privacy Policy</a></li>
                <li><a href="#" className="hover:text-foreground transition-colors">Terms of Service</a></li>
                <li><a href="#" className="hover:text-foreground transition-colors">Security</a></li>
                <li><a href="#" className="hover:text-foreground transition-colors">Compliance</a></li>
              </ul>
            </div>
          </div>
          <div className="mt-8 border-t border-border pt-8 flex flex-col sm:flex-row items-center justify-between gap-4 text-sm text-muted-foreground">
            <p>&copy; {new Date().getFullYear()} BillForge. All rights reserved.</p>
            <div className="flex items-center gap-2">
              <Lock className="w-4 h-4" />
              <span>SOC 2 Type II Compliant</span>
            </div>
          </div>
        </div>
      </footer>
    </div>
  );
}
