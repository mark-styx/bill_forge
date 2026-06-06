import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import PricingPage, { recommendPlan, estimateMonthlyCost } from '../page';

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

describe('PricingPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders the pricing calculator with default values', () => {
    render(<PricingPage />);
    expect(screen.getByText('Simple, transparent pricing')).toBeInTheDocument();
    expect(screen.getByTestId('recommended-plan')).toBeInTheDocument();
    expect(screen.getByTestId('estimated-price')).toBeInTheDocument();
  });

  it('shows Starter for default inputs (50 invoices, 2 seats)', () => {
    render(<PricingPage />);
    expect(screen.getByTestId('recommended-plan').textContent).toBe('Starter');
  });

  it('updates recommended plan when invoice slider changes', () => {
    render(<PricingPage />);
    const slider = screen.getByTestId('invoice-slider');
    fireEvent.change(slider, { target: { value: 500 } });
    expect(screen.getByTestId('recommended-plan').textContent).toBe('Professional');
  });

  it('shows Free for 0 invoices and 1 seat', () => {
    render(<PricingPage />);
    const invSlider = screen.getByTestId('invoice-slider');
    const seatSlider = screen.getByTestId('seats-slider');
    fireEvent.change(invSlider, { target: { value: 0 } });
    fireEvent.change(seatSlider, { target: { value: 1 } });
    expect(screen.getByTestId('recommended-plan').textContent).toBe('Free');
  });

  it('shows Enterprise for high volume', () => {
    render(<PricingPage />);
    const slider = screen.getByTestId('invoice-slider');
    fireEvent.change(slider, { target: { value: 2000 } });
    expect(screen.getByTestId('recommended-plan').textContent).toBe('Enterprise');
  });

  it('CTA link includes selected plan id', () => {
    render(<PricingPage />);
    const cta = screen.getByTestId('signup-cta');
    expect(cta.getAttribute('href')).toContain('/signup?plan=');
  });

  it('calculates estimated price correctly', () => {
    expect(estimateMonthlyCost('free', 10)).toBe(0);
    expect(estimateMonthlyCost('starter', 100)).toBe(49 + 100 * 1.5);
    expect(estimateMonthlyCost('professional', 100)).toBe(149 + 100 * 1.0);
    expect(estimateMonthlyCost('enterprise', 100)).toBe(499 + 100 * 0.65);
  });

  it('recommendPlan returns correct tiers', () => {
    expect(recommendPlan(0, 1)).toBe('free');
    expect(recommendPlan(50, 2)).toBe('starter');
    expect(recommendPlan(500, 5)).toBe('professional');
    expect(recommendPlan(2000, 5)).toBe('enterprise');
    expect(recommendPlan(100, 15)).toBe('enterprise');
  });
});
