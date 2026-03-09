'use client';

import Link from 'next/link';
import { useThemeStore, themePresets, generateGradient } from '@/stores/theme';
import {
  HeroSection,
  FeatureCard,
  StatsShowcase,
  TestimonialCard,
} from '@/components/ui/hero-section';
import {
  GradientCard,
  GradientText,
  GradientButton,
  GlowWrapper,
} from '@/components/ui/gradient-card';
import { Button } from '@/components/ui/button';
import {
  FileText,
  Zap,
  Shield,
  BarChart3,
  Users,
  Check,
  ArrowRight,
  PlayCircle,
  Sparkles,
  Star,
  Sun,
  Moon,
  Monitor,
  ScanLine,
  Workflow,
  CheckCircle,
  Globe,
  Lock,
  Clock,
  TrendingUp,
  ChevronRight,
  Mail,
} from 'lucide-react';

const features = [
  {
    icon: <ScanLine className="w-6 h-6" />,
    title: 'Smart Invoice Capture',
    description:
      'AI-powered OCR extracts data from invoices automatically with 99%+ accuracy. Support for PDF, images, and scanned documents.',
    color: 'capture' as const,
  },
  {
    icon: <Workflow className="w-6 h-6" />,
    title: 'Automated Workflows',
    description:
      'Custom approval workflows, automatic routing, and smart assignment rules that adapt to your business processes.',
    color: 'processing' as const,
  },
  {
    icon: <Users className="w-6 h-6" />,
    title: 'Vendor Management',
    description:
      'Centralized vendor portal with tax document management, payment tracking, and relationship insights.',
    color: 'vendor' as const,
  },
  {
    icon: <BarChart3 className="w-6 h-6" />,
    title: 'Real-time Analytics',
    description:
      'Comprehensive dashboards, aging reports, and cash flow insights to optimize your AP operations.',
    color: 'reporting' as const,
  },
  {
    icon: <Shield className="w-6 h-6" />,
    title: 'Enterprise Security',
    description:
      'SOC 2 compliant with complete audit trails, role-based access control, and data encryption at rest.',
    color: 'primary' as const,
  },
  {
    icon: <Globe className="w-6 h-6" />,
    title: 'Global Support',
    description:
      'Multi-currency support, localization for 50+ countries, and 24/7 support for enterprise customers.',
    color: 'capture' as const,
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
    gradient: 'capture' as const,
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
    gradient: 'primary' as const,
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
    gradient: 'vendor' as const,
  },
];

const testimonials = [
  {
    quote:
      "BillForge cut our invoice processing time by 75%. What used to take our team days now happens in hours.",
    author: 'Sarah Chen',
    role: 'AP Manager',
    company: 'TechCorp Inc.',
    rating: 5,
  },
  {
    quote:
      "The automation features are incredible. We've reduced manual data entry errors to nearly zero.",
    author: 'Michael Rodriguez',
    role: 'Controller',
    company: 'Global Logistics Co.',
    rating: 5,
  },
  {
    quote:
      "Finally an AP solution that actually fits how we work. The workflow customization is unmatched.",
    author: 'Jennifer Park',
    role: 'CFO',
    company: 'Innovate Health',
    rating: 5,
  },
];

const stats = [
  { value: '10x', label: 'Faster Processing', color: 'capture' as const },
  { value: '99%', suffix: '+', label: 'OCR Accuracy', color: 'processing' as const },
  { value: '75%', label: 'Cost Reduction', color: 'vendor' as const },
  { value: '10K', suffix: '+', label: 'Happy Customers', color: 'reporting' as const },
];

export default function LandingPage() {
  const { getCurrentColors, mode, setMode } = useThemeStore();
  const colors = getCurrentColors();

  return (
    <div className="flex min-h-screen flex-col bg-background">
      {/* Navigation */}
      <nav className="sticky top-0 z-50 border-b border-border bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
        <div className="container mx-auto flex h-16 items-center justify-between px-4">
          <div className="flex items-center gap-2">
            <div
              className="flex h-9 w-9 items-center justify-center rounded-xl"
              style={{ background: `linear-gradient(135deg, hsl(${colors.primary}), hsl(${colors.accent}))` }}
            >
              <FileText className="h-5 w-5 text-white" />
            </div>
            <span className="text-xl font-bold text-foreground">BillForge</span>
          </div>
          <div className="hidden items-center gap-8 md:flex">
            <Link href="#features" className="text-sm text-muted-foreground hover:text-foreground transition-colors">
              Features
            </Link>
            <Link href="#pricing" className="text-sm text-muted-foreground hover:text-foreground transition-colors">
              Pricing
            </Link>
            <Link href="#testimonials" className="text-sm text-muted-foreground hover:text-foreground transition-colors">
              Testimonials
            </Link>
          </div>
          <div className="flex items-center gap-3">
            {/* Theme Toggle */}
            <div className="flex items-center gap-1 p-1 bg-secondary/50 rounded-lg">
              {[
                { id: 'light', icon: Sun },
                { id: 'dark', icon: Moon },
                { id: 'system', icon: Monitor },
              ].map((option) => (
                <button
                  key={option.id}
                  onClick={() => setMode(option.id as 'light' | 'dark' | 'system')}
                  className={`p-1.5 rounded-md transition-all ${
                    mode === option.id
                      ? 'bg-background shadow-sm text-foreground'
                      : 'text-muted-foreground hover:text-foreground'
                  }`}
                >
                  <option.icon className="w-4 h-4" />
                </button>
              ))}
            </div>
            <Link
              href="/login"
              className="text-sm font-medium text-muted-foreground hover:text-foreground transition-colors"
            >
              Sign in
            </Link>
            <GradientButton gradient="primary" size="sm">
              Get Started
            </GradientButton>
          </div>
        </div>
      </nav>

      {/* Hero Section */}
      <section className="relative overflow-hidden py-20 md:py-32">
        {/* Animated background */}
        <div className="absolute inset-0 overflow-hidden pointer-events-none">
          <div
            className="absolute -top-40 -right-40 w-96 h-96 rounded-full opacity-30 blur-3xl animate-pulse-soft"
            style={{ background: `hsl(${colors.primary})` }}
          />
          <div
            className="absolute -bottom-40 -left-40 w-96 h-96 rounded-full opacity-20 blur-3xl animate-pulse-soft"
            style={{ background: `hsl(${colors.accent})`, animationDelay: '1s' }}
          />
          <div
            className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[600px] h-[600px] rounded-full opacity-10 blur-3xl animate-pulse-soft"
            style={{ background: `hsl(${colors.capture})`, animationDelay: '2s' }}
          />
          <div
            className="absolute inset-0 bg-[linear-gradient(to_right,hsl(var(--border)/0.1)_1px,transparent_1px),linear-gradient(to_bottom,hsl(var(--border)/0.1)_1px,transparent_1px)] bg-[size:4rem_4rem]"
          />
        </div>

        <div className="container relative z-10 mx-auto px-4">
          <div className="mx-auto max-w-4xl text-center">
            {/* Badge */}
            <div className="mb-6 inline-flex items-center rounded-full border border-primary/20 bg-primary/5 px-4 py-1.5 text-sm">
              <Sparkles className="mr-2 h-4 w-4 text-primary" />
              <span className="text-primary font-medium">New</span>
              <span className="mx-2 text-border">|</span>
              <span className="text-muted-foreground">AI-powered invoice processing is here</span>
            </div>

            {/* Title */}
            <h1 className="mb-6 text-4xl font-bold tracking-tight text-foreground md:text-6xl lg:text-7xl">
              Automate Your{' '}
              <GradientText gradient="primary" className="inline-block">
                Accounts Payable
              </GradientText>
            </h1>

            {/* Subtitle */}
            <p className="mb-8 text-lg text-muted-foreground md:text-xl max-w-2xl mx-auto">
              BillForge streamlines invoice processing with AI-powered capture, automated workflows,
              and real-time analytics. Process invoices 10x faster with fewer errors.
            </p>

            {/* CTAs */}
            <div className="flex flex-col items-center justify-center gap-4 sm:flex-row">
              <Link href="/login">
                <GradientButton gradient="primary" size="lg" className="min-w-[180px]">
                  Start Free Trial
                  <ArrowRight className="ml-2 h-5 w-5" />
                </GradientButton>
              </Link>
              <Button variant="outline" size="lg" className="min-w-[180px]">
                <PlayCircle className="mr-2 h-5 w-5" />
                Watch Demo
              </Button>
            </div>

            <p className="mt-4 text-sm text-muted-foreground">
              No credit card required. 14-day free trial.
            </p>

            {/* Trust indicators */}
            <div className="mt-12 flex flex-col sm:flex-row items-center justify-center gap-6">
              <div className="flex -space-x-2">
                {['A', 'B', 'C', 'D', 'E'].map((letter, i) => (
                  <div
                    key={i}
                    className="w-10 h-10 rounded-full border-2 border-background flex items-center justify-center text-white text-sm font-medium"
                    style={{
                      background: `hsl(${[colors.primary, colors.accent, colors.capture, colors.processing, colors.vendor][i]})`,
                    }}
                  >
                    {letter}
                  </div>
                ))}
              </div>
              <div className="text-sm text-left">
                <div className="flex items-center gap-1 text-foreground font-medium">
                  {[...Array(5)].map((_, i) => (
                    <Star key={i} className="w-4 h-4 fill-warning text-warning" />
                  ))}
                  <span className="ml-1">4.9/5</span>
                </div>
                <p className="text-muted-foreground">Trusted by 10,000+ businesses</p>
              </div>
            </div>
          </div>
        </div>
      </section>

      {/* Stats Section */}
      <section className="py-16 border-y border-border bg-secondary/20">
        <div className="container mx-auto px-4">
          <StatsShowcase stats={stats} />
        </div>
      </section>

      {/* Features Section */}
      <section id="features" className="py-20 md:py-32">
        <div className="container mx-auto px-4">
          <div className="mx-auto mb-16 max-w-2xl text-center">
            <div className="mb-4 inline-flex items-center rounded-full bg-capture/10 px-4 py-1.5 text-sm text-capture font-medium">
              <Zap className="mr-2 h-4 w-4" />
              Powerful Features
            </div>
            <h2 className="mb-4 text-3xl font-bold text-foreground md:text-5xl">
              Everything you need to manage AP
            </h2>
            <p className="text-lg text-muted-foreground">
              A complete platform for modern accounts payable teams
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

      {/* Theme Showcase Section */}
      <section className="py-20 bg-gradient-to-b from-background to-secondary/20">
        <div className="container mx-auto px-4">
          <div className="mx-auto mb-16 max-w-2xl text-center">
            <div className="mb-4 inline-flex items-center rounded-full bg-vendor/10 px-4 py-1.5 text-sm text-vendor font-medium">
              <Sparkles className="mr-2 h-4 w-4" />
              Customizable Themes
            </div>
            <h2 className="mb-4 text-3xl font-bold text-foreground md:text-5xl">
              Make it yours with{' '}
              <GradientText gradient="primary">custom themes</GradientText>
            </h2>
            <p className="text-lg text-muted-foreground">
              Choose from 30+ beautiful color themes or create your own to match your brand
            </p>
          </div>

          {/* Theme Preview Grid */}
          <div className="grid grid-cols-4 sm:grid-cols-6 md:grid-cols-8 lg:grid-cols-10 gap-3 max-w-4xl mx-auto">
            {themePresets.slice(0, 20).map((preset) => {
              const gradient = generateGradient(preset);
              return (
                <div
                  key={preset.id}
                  className="aspect-square rounded-xl shadow-sm hover:shadow-lg hover:scale-110 transition-all cursor-pointer"
                  style={{ background: gradient }}
                  title={preset.name}
                />
              );
            })}
          </div>

          <div className="text-center mt-8">
            <Link href="/login">
              <Button variant="outline" size="lg">
                Explore All Themes
                <ChevronRight className="ml-2 w-4 h-4" />
              </Button>
            </Link>
          </div>
        </div>
      </section>

      {/* Pricing Section */}
      <section id="pricing" className="py-20 md:py-32">
        <div className="container mx-auto px-4">
          <div className="mx-auto mb-16 max-w-2xl text-center">
            <div className="mb-4 inline-flex items-center rounded-full bg-processing/10 px-4 py-1.5 text-sm text-processing font-medium">
              <TrendingUp className="mr-2 h-4 w-4" />
              Pricing
            </div>
            <h2 className="mb-4 text-3xl font-bold text-foreground md:text-5xl">
              Simple, transparent pricing
            </h2>
            <p className="text-lg text-muted-foreground">
              Choose the plan that fits your team. All plans include a 14-day free trial.
            </p>
          </div>
          <div className="mx-auto grid max-w-5xl gap-8 lg:grid-cols-3">
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
                    <p className="text-sm text-muted-foreground">{plan.description}</p>
                  </div>
                  <div className="mb-6">
                    {plan.price !== null ? (
                      <>
                        <span className="text-4xl font-bold text-foreground">${plan.price}</span>
                        <span className="text-muted-foreground">/{plan.period}</span>
                      </>
                    ) : (
                      <span className="text-3xl font-bold text-foreground">Custom</span>
                    )}
                  </div>
                  <ul className="mb-6 space-y-3">
                    {plan.features.map((feature) => (
                      <li key={feature} className="flex items-center gap-2 text-sm text-muted-foreground">
                        <CheckCircle className="h-4 w-4 text-success flex-shrink-0" />
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
        </div>
      </section>

      {/* Testimonials Section */}
      <section id="testimonials" className="py-20 bg-secondary/20">
        <div className="container mx-auto px-4">
          <div className="mx-auto mb-16 max-w-2xl text-center">
            <div className="mb-4 inline-flex items-center rounded-full bg-reporting/10 px-4 py-1.5 text-sm text-reporting font-medium">
              <Star className="mr-2 h-4 w-4 fill-current" />
              Testimonials
            </div>
            <h2 className="mb-4 text-3xl font-bold text-foreground md:text-5xl">
              Loved by AP teams everywhere
            </h2>
            <p className="text-lg text-muted-foreground">
              See what our customers have to say about BillForge
            </p>
          </div>
          <div className="mx-auto grid max-w-5xl gap-6 md:grid-cols-3">
            {testimonials.map((testimonial) => (
              <TestimonialCard
                key={testimonial.author}
                quote={testimonial.quote}
                author={testimonial.author}
                role={testimonial.role}
                company={testimonial.company}
                rating={testimonial.rating}
              />
            ))}
          </div>
        </div>
      </section>

      {/* CTA Section */}
      <section className="py-20">
        <div className="container mx-auto px-4">
          <GradientCard
            gradient="primary"
            gradientPosition="background"
            className="mx-auto max-w-4xl text-center"
          >
            <div
              className="p-8 md:p-16 rounded-2xl"
              style={{
                background: `linear-gradient(135deg, hsl(${colors.primary}), hsl(${colors.accent}))`,
              }}
            >
              <h2 className="mb-4 text-3xl md:text-4xl font-bold text-white">
                Ready to transform your AP?
              </h2>
              <p className="mb-8 text-white/80 max-w-xl mx-auto">
                Join thousands of companies using BillForge to streamline their accounts payable operations
              </p>
              <div className="flex flex-col sm:flex-row gap-4 justify-center">
                <Link href="/login">
                  <Button size="lg" className="bg-white text-foreground hover:bg-white/90 min-w-[180px]">
                    Start Your Free Trial
                    <ArrowRight className="ml-2 h-5 w-5" />
                  </Button>
                </Link>
                <Button
                  size="lg"
                  variant="outline"
                  className="border-white/30 text-white hover:bg-white/10 min-w-[180px]"
                >
                  <Mail className="mr-2 h-5 w-5" />
                  Contact Sales
                </Button>
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
              <p className="text-sm text-muted-foreground">
                Modern accounts payable automation for growing businesses.
              </p>
              {/* Social icons placeholder */}
              <div className="flex gap-3 mt-4">
                {['Twitter', 'LinkedIn', 'GitHub'].map((social) => (
                  <a
                    key={social}
                    href="#"
                    className="w-8 h-8 rounded-lg bg-secondary flex items-center justify-center text-muted-foreground hover:text-foreground hover:bg-secondary/80 transition-colors"
                  >
                    <span className="text-xs font-bold">{social[0]}</span>
                  </a>
                ))}
              </div>
            </div>
            <div>
              <h4 className="mb-4 font-semibold text-foreground">Product</h4>
              <ul className="space-y-2 text-sm text-muted-foreground">
                <li><Link href="#features" className="hover:text-foreground transition-colors">Features</Link></li>
                <li><Link href="#pricing" className="hover:text-foreground transition-colors">Pricing</Link></li>
                <li><Link href="/login" className="hover:text-foreground transition-colors">Sign In</Link></li>
                <li><a href="#" className="hover:text-foreground transition-colors">API Docs</a></li>
              </ul>
            </div>
            <div>
              <h4 className="mb-4 font-semibold text-foreground">Company</h4>
              <ul className="space-y-2 text-sm text-muted-foreground">
                <li><a href="#" className="hover:text-foreground transition-colors">About</a></li>
                <li><a href="#" className="hover:text-foreground transition-colors">Blog</a></li>
                <li><a href="#" className="hover:text-foreground transition-colors">Careers</a></li>
                <li><a href="#" className="hover:text-foreground transition-colors">Contact</a></li>
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
              <span>SOC 2 Type II Certified</span>
            </div>
          </div>
        </div>
      </footer>
    </div>
  );
}
