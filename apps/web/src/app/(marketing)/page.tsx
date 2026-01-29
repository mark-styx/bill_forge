'use client';

import Link from 'next/link';
import {
  FileText,
  Zap,
  Shield,
  BarChart3,
  Users,
  Check,
  ArrowRight,
  PlayCircle,
} from 'lucide-react';

const features = [
  {
    icon: FileText,
    title: 'Smart Invoice Capture',
    description:
      'AI-powered OCR extracts data from invoices automatically with 99%+ accuracy. Support for PDF, images, and scanned documents.',
  },
  {
    icon: Zap,
    title: 'Automated Workflows',
    description:
      'Custom approval workflows, automatic routing, and smart assignment rules that adapt to your business processes.',
  },
  {
    icon: Users,
    title: 'Vendor Management',
    description:
      'Centralized vendor portal with tax document management, payment tracking, and relationship insights.',
  },
  {
    icon: BarChart3,
    title: 'Real-time Analytics',
    description:
      'Comprehensive dashboards, aging reports, and cash flow insights to optimize your AP operations.',
  },
  {
    icon: Shield,
    title: 'Enterprise Security',
    description:
      'SOC 2 compliant with complete audit trails, role-based access control, and data encryption at rest.',
  },
];

const pricingPlans = [
  {
    name: 'Starter',
    description: 'For small teams getting started',
    price: 99,
    period: 'per month',
    features: [
      'Up to 500 invoices/month',
      'Basic OCR capture',
      '3 user seats',
      'Email support',
      'Standard integrations',
    ],
    cta: 'Start Free Trial',
    popular: false,
  },
  {
    name: 'Professional',
    description: 'For growing AP teams',
    price: 299,
    period: 'per month',
    features: [
      'Up to 2,500 invoices/month',
      'Advanced OCR with AI',
      '10 user seats',
      'Priority support',
      'Custom workflows',
      'Approval routing',
      'Analytics dashboard',
    ],
    cta: 'Start Free Trial',
    popular: true,
  },
  {
    name: 'Enterprise',
    description: 'For large organizations',
    price: null,
    period: 'custom pricing',
    features: [
      'Unlimited invoices',
      'Unlimited users',
      'Dedicated support',
      'SSO integration',
      'Custom integrations',
      'On-premise option',
      'SLA guarantee',
      'Custom training',
    ],
    cta: 'Contact Sales',
    popular: false,
  },
];

const testimonials = [
  {
    quote:
      "BillForge cut our invoice processing time by 75%. What used to take our team days now happens in hours.",
    author: 'Sarah Chen',
    role: 'AP Manager',
    company: 'TechCorp Inc.',
  },
  {
    quote:
      "The automation features are incredible. We've reduced manual data entry errors to nearly zero.",
    author: 'Michael Rodriguez',
    role: 'Controller',
    company: 'Global Logistics Co.',
  },
  {
    quote:
      "Finally an AP solution that actually fits how we work. The workflow customization is unmatched.",
    author: 'Jennifer Park',
    role: 'CFO',
    company: 'Innovate Health',
  },
];

export default function LandingPage() {
  return (
    <div className="flex min-h-screen flex-col">
      {/* Navigation */}
      <nav className="sticky top-0 z-50 border-b bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
        <div className="container mx-auto flex h-16 items-center justify-between px-4">
          <div className="flex items-center gap-2">
            <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-primary">
              <FileText className="h-5 w-5 text-primary-foreground" />
            </div>
            <span className="text-xl font-bold">BillForge</span>
          </div>
          <div className="hidden items-center gap-8 md:flex">
            <Link href="#features" className="text-sm text-muted-foreground hover:text-foreground">
              Features
            </Link>
            <Link href="#pricing" className="text-sm text-muted-foreground hover:text-foreground">
              Pricing
            </Link>
            <Link href="#testimonials" className="text-sm text-muted-foreground hover:text-foreground">
              Testimonials
            </Link>
          </div>
          <div className="flex items-center gap-4">
            <Link
              href="/login"
              className="text-sm font-medium text-muted-foreground hover:text-foreground"
            >
              Sign in
            </Link>
            <Link
              href="/login"
              className="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90"
            >
              Get Started
            </Link>
          </div>
        </div>
      </nav>

      {/* Hero Section */}
      <section className="relative overflow-hidden py-20 md:py-32">
        <div className="absolute inset-0 -z-10 bg-[linear-gradient(to_right,#8080800a_1px,transparent_1px),linear-gradient(to_bottom,#8080800a_1px,transparent_1px)] bg-[size:24px_24px]" />
        <div className="container mx-auto px-4">
          <div className="mx-auto max-w-3xl text-center">
            <div className="mb-6 inline-flex items-center rounded-full border bg-muted/50 px-4 py-1.5 text-sm">
              <span className="mr-2 rounded-full bg-primary px-2 py-0.5 text-xs font-semibold text-primary-foreground">
                New
              </span>
              AI-powered invoice processing is here
            </div>
            <h1 className="mb-6 text-4xl font-bold tracking-tight md:text-6xl">
              Automate Your{' '}
              <span className="bg-gradient-to-r from-primary to-blue-600 bg-clip-text text-transparent">
                Accounts Payable
              </span>
            </h1>
            <p className="mb-8 text-lg text-muted-foreground md:text-xl">
              BillForge streamlines invoice processing with AI-powered capture, automated workflows,
              and real-time analytics. Process invoices 10x faster with fewer errors.
            </p>
            <div className="flex flex-col items-center justify-center gap-4 sm:flex-row">
              <Link
                href="/login"
                className="inline-flex items-center gap-2 rounded-md bg-primary px-6 py-3 text-sm font-medium text-primary-foreground hover:bg-primary/90"
              >
                Start Free Trial
                <ArrowRight className="h-4 w-4" />
              </Link>
              <button className="inline-flex items-center gap-2 rounded-md border px-6 py-3 text-sm font-medium hover:bg-muted">
                <PlayCircle className="h-4 w-4" />
                Watch Demo
              </button>
            </div>
            <p className="mt-4 text-xs text-muted-foreground">
              No credit card required. 14-day free trial.
            </p>
          </div>
        </div>
      </section>

      {/* Features Section */}
      <section id="features" className="border-t bg-muted/30 py-20">
        <div className="container mx-auto px-4">
          <div className="mx-auto mb-16 max-w-2xl text-center">
            <h2 className="mb-4 text-3xl font-bold md:text-4xl">
              Everything you need to manage AP
            </h2>
            <p className="text-lg text-muted-foreground">
              A complete platform for modern accounts payable teams
            </p>
          </div>
          <div className="grid gap-8 md:grid-cols-2 lg:grid-cols-3">
            {features.map((feature) => (
              <div
                key={feature.title}
                className="rounded-xl border bg-background p-6 shadow-sm transition-shadow hover:shadow-md"
              >
                <div className="mb-4 flex h-12 w-12 items-center justify-center rounded-lg bg-primary/10">
                  <feature.icon className="h-6 w-6 text-primary" />
                </div>
                <h3 className="mb-2 text-lg font-semibold">{feature.title}</h3>
                <p className="text-sm text-muted-foreground">{feature.description}</p>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* Pricing Section */}
      <section id="pricing" className="py-20">
        <div className="container mx-auto px-4">
          <div className="mx-auto mb-16 max-w-2xl text-center">
            <h2 className="mb-4 text-3xl font-bold md:text-4xl">
              Simple, transparent pricing
            </h2>
            <p className="text-lg text-muted-foreground">
              Choose the plan that fits your team
            </p>
          </div>
          <div className="mx-auto grid max-w-5xl gap-8 md:grid-cols-3">
            {pricingPlans.map((plan) => (
              <div
                key={plan.name}
                className={`relative rounded-xl border p-6 ${
                  plan.popular ? 'border-primary shadow-lg' : ''
                }`}
              >
                {plan.popular && (
                  <div className="absolute -top-3 left-1/2 -translate-x-1/2">
                    <span className="rounded-full bg-primary px-3 py-1 text-xs font-semibold text-primary-foreground">
                      Most Popular
                    </span>
                  </div>
                )}
                <div className="mb-4">
                  <h3 className="text-lg font-semibold">{plan.name}</h3>
                  <p className="text-sm text-muted-foreground">{plan.description}</p>
                </div>
                <div className="mb-6">
                  {plan.price !== null ? (
                    <>
                      <span className="text-4xl font-bold">${plan.price}</span>
                      <span className="text-muted-foreground">/{plan.period}</span>
                    </>
                  ) : (
                    <span className="text-2xl font-bold">Custom</span>
                  )}
                </div>
                <ul className="mb-6 space-y-3">
                  {plan.features.map((feature) => (
                    <li key={feature} className="flex items-center gap-2 text-sm">
                      <Check className="h-4 w-4 text-primary" />
                      {feature}
                    </li>
                  ))}
                </ul>
                <Link
                  href="/login"
                  className={`block w-full rounded-md px-4 py-2 text-center text-sm font-medium ${
                    plan.popular
                      ? 'bg-primary text-primary-foreground hover:bg-primary/90'
                      : 'border hover:bg-muted'
                  }`}
                >
                  {plan.cta}
                </Link>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* Testimonials Section */}
      <section id="testimonials" className="border-t bg-muted/30 py-20">
        <div className="container mx-auto px-4">
          <div className="mx-auto mb-16 max-w-2xl text-center">
            <h2 className="mb-4 text-3xl font-bold md:text-4xl">
              Loved by AP teams everywhere
            </h2>
            <p className="text-lg text-muted-foreground">
              See what our customers have to say
            </p>
          </div>
          <div className="mx-auto grid max-w-5xl gap-8 md:grid-cols-3">
            {testimonials.map((testimonial) => (
              <div key={testimonial.author} className="rounded-xl border bg-background p-6">
                <p className="mb-4 text-sm italic text-muted-foreground">
                  "{testimonial.quote}"
                </p>
                <div>
                  <p className="font-semibold">{testimonial.author}</p>
                  <p className="text-xs text-muted-foreground">
                    {testimonial.role}, {testimonial.company}
                  </p>
                </div>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* CTA Section */}
      <section className="py-20">
        <div className="container mx-auto px-4">
          <div className="mx-auto max-w-3xl rounded-2xl bg-primary p-8 text-center text-primary-foreground md:p-12">
            <h2 className="mb-4 text-3xl font-bold">Ready to transform your AP?</h2>
            <p className="mb-8 text-primary-foreground/80">
              Join hundreds of companies using BillForge to streamline their accounts payable
            </p>
            <Link
              href="/login"
              className="inline-flex items-center gap-2 rounded-md bg-background px-6 py-3 text-sm font-medium text-foreground hover:bg-background/90"
            >
              Start Your Free Trial
              <ArrowRight className="h-4 w-4" />
            </Link>
          </div>
        </div>
      </section>

      {/* Footer */}
      <footer className="border-t py-12">
        <div className="container mx-auto px-4">
          <div className="grid gap-8 md:grid-cols-4">
            <div>
              <div className="mb-4 flex items-center gap-2">
                <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-primary">
                  <FileText className="h-5 w-5 text-primary-foreground" />
                </div>
                <span className="text-xl font-bold">BillForge</span>
              </div>
              <p className="text-sm text-muted-foreground">
                Modern accounts payable automation for growing businesses.
              </p>
            </div>
            <div>
              <h4 className="mb-4 font-semibold">Product</h4>
              <ul className="space-y-2 text-sm text-muted-foreground">
                <li><Link href="#features" className="hover:text-foreground">Features</Link></li>
                <li><Link href="#pricing" className="hover:text-foreground">Pricing</Link></li>
                <li><Link href="/login" className="hover:text-foreground">Sign In</Link></li>
              </ul>
            </div>
            <div>
              <h4 className="mb-4 font-semibold">Company</h4>
              <ul className="space-y-2 text-sm text-muted-foreground">
                <li><a href="#" className="hover:text-foreground">About</a></li>
                <li><a href="#" className="hover:text-foreground">Blog</a></li>
                <li><a href="#" className="hover:text-foreground">Careers</a></li>
              </ul>
            </div>
            <div>
              <h4 className="mb-4 font-semibold">Legal</h4>
              <ul className="space-y-2 text-sm text-muted-foreground">
                <li><a href="#" className="hover:text-foreground">Privacy Policy</a></li>
                <li><a href="#" className="hover:text-foreground">Terms of Service</a></li>
                <li><a href="#" className="hover:text-foreground">Security</a></li>
              </ul>
            </div>
          </div>
          <div className="mt-8 border-t pt-8 text-center text-sm text-muted-foreground">
            <p>&copy; {new Date().getFullYear()} BillForge. All rights reserved.</p>
          </div>
        </div>
      </footer>
    </div>
  );
}
