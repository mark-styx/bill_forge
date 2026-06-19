import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import PricingPage from '../page';

// Mock useThemeStore
vi.mock('@/stores/theme', () => ({
  useThemeStore: () => ({
    getCurrentColors: () => ({
      primary: '220 90% 56%',
      accent: '280 80% 60%',
    }),
  }),
}));

// Mock GradientButton
vi.mock('@/components/ui/gradient-card', () => ({
  GradientButton: ({ children, ...props }: any) => (
    <button {...props}>{children}</button>
  ),
}));

// Mock next/link
vi.mock('next/link', () => ({
  default: ({ children, ...props }: { children: React.ReactNode; href: string }) => (
    <a {...props}>{children}</a>
  ),
}));

// Mock the public billing API. Plan data must come from the backend
// (Plan::all_public()), which excludes Enterprise (is_public=false).
const mockListPublicPlans = vi.fn();
vi.mock('@/lib/api', () => ({
  publicBillingApi: {
    listPlans: (...args: unknown[]) => mockListPublicPlans(...args),
  },
}));

// Mirror of the PublicPlan shape returned by GET /api/public/plans.
// Prices are sourced from `backend/crates/billing/src/plans.rs`.
function publicPlansFixture() {
  return [
    {
      id: 'free',
      name: 'Free',
      description: 'Get started with basic invoice capture',
      monthly_price_cents: 0,
      annual_price_cents: 0,
      metered_invoice_unit_price_cents: 0,
      features: { max_users: 1, max_invoices_per_month: 25, max_vendors: 5 },
    },
    {
      id: 'starter',
      name: 'Starter',
      description: 'Perfect for small businesses',
      monthly_price_cents: 4900,
      annual_price_cents: 47000,
      metered_invoice_unit_price_cents: 150,
      features: { max_users: 3, max_invoices_per_month: 4294967295, max_vendors: 50 },
    },
    {
      id: 'professional',
      name: 'Professional',
      description: 'Full AP automation for growing teams',
      monthly_price_cents: 14900,
      annual_price_cents: 142800,
      metered_invoice_unit_price_cents: 100,
      features: { max_users: 10, max_invoices_per_month: 4294967295, max_vendors: 500 },
    },
  ];
}

describe('PricingPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockListPublicPlans.mockResolvedValue(publicPlansFixture());
  });

  it('renders the hero heading', async () => {
    render(<PricingPage />);
    expect(screen.getByText('Simple, transparent pricing')).toBeInTheDocument();
  });

  it('renders plans from the public billing API with backend prices', async () => {
    render(<PricingPage />);

    expect(mockListPublicPlans).toHaveBeenCalledTimes(1);

    // Prices flow from the mocked backend payload, not a hardcoded constant.
    await waitFor(() => {
      expect(screen.getByTestId('plan-price-free').textContent).toBe('$0');
    });
    expect(screen.getByTestId('plan-price-starter').textContent).toBe('$49');
    expect(screen.getByTestId('plan-price-professional').textContent).toBe('$149');
  });

  it('renders Free, Starter, and Professional plan names', async () => {
    render(<PricingPage />);
    await waitFor(() => {
      expect(screen.getByText('Free')).toBeInTheDocument();
    });
    expect(screen.getByText('Starter')).toBeInTheDocument();
    expect(screen.getByText('Professional')).toBeInTheDocument();
  });

  it('does NOT render the non-public Enterprise plan or its $499 price', async () => {
    render(<PricingPage />);
    await waitFor(() => {
      expect(screen.getByTestId('plan-price-professional')).toBeInTheDocument();
    });

    // No Enterprise plan card and no $499 price anywhere in the plans section.
    expect(screen.queryByTestId('plan-price-enterprise')).not.toBeInTheDocument();
    expect(screen.queryByText('$499')).not.toBeInTheDocument();
  });

  it('renders a separate contact-sales CTA (not a Start free trial CTA) for Enterprise', async () => {
    render(<PricingPage />);
    await waitFor(() => {
      expect(screen.getByTestId('enterprise-contact')).toBeInTheDocument();
    });

    // Non-trial CTA: "Contact sales", links to mailto (not /signup).
    const cta = screen.getByText('Contact sales');
    const link = cta.closest('a');
    expect(link).not.toBeNull();
    expect(link?.getAttribute('href')).toContain('mailto:');
    expect(link?.getAttribute('href')).not.toContain('/signup');
  });

  it('shows a loading state before plans resolve', async () => {
    // Never-resolving promise so the loading state stays mounted.
    mockListPublicPlans.mockReturnValue(new Promise(() => {}));

    render(<PricingPage />);
    expect(screen.getByTestId('plans-loading')).toBeInTheDocument();
  });

  it('shows an error fallback when the fetch fails', async () => {
    mockListPublicPlans.mockRejectedValue(new Error('network down'));

    render(<PricingPage />);
    await waitFor(() => {
      expect(screen.getByTestId('plans-error')).toBeInTheDocument();
    });
    // On error, no plan cards should be rendered.
    expect(screen.queryByTestId('plan-price-starter')).not.toBeInTheDocument();
  });
});
