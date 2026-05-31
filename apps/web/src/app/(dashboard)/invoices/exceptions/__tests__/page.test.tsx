import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import OcrExceptionsPage from '../page';

// Mock next/navigation
vi.mock('next/navigation', () => ({
  useRouter: () => ({ push: vi.fn() }),
  usePathname: () => '/invoices/exceptions',
}));

// Mock the API
const mockListInvoices = vi.fn();

vi.mock('@/lib/api', () => ({
  invoicesApi: {
    list: (...args: unknown[]) => mockListInvoices(...args),
  },
}));

// Mock ConfidenceBadge
vi.mock('@/components/ConfidenceBadge', () => ({
  ConfidenceBadge: ({ confidence }: { confidence: number }) => (
    <span data-testid="confidence-badge">{Math.round(confidence * 100)}%</span>
  ),
}));

const mockInvoices = [
  {
    id: 'inv-001-abcdefgh',
    invoice_number: 'INV-001',
    vendor_name: 'Acme Corp',
    total_amount: { amount: 50000, currency: 'USD' },
    ocr_confidence: 0.72,
    invoice_date: '2026-05-28',
  },
  {
    id: 'inv-002-ijklmnop',
    invoice_number: 'INV-002',
    vendor_name: 'Beta LLC',
    total_amount: { amount: 25000, currency: 'USD' },
    ocr_confidence: 0.60,
    invoice_date: '2026-05-29',
  },
];

function renderExceptionsPage() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <OcrExceptionsPage />
    </QueryClientProvider>,
  );
}

describe('OcrExceptionsPage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockListInvoices.mockResolvedValue({
      data: mockInvoices,
      pagination: { page: 1, per_page: 25, total_items: 2, total_pages: 1 },
    });
  });

  it('defaults threshold to 0.85 and fetches with max_ocr_confidence', async () => {
    renderExceptionsPage();

    const input = screen.getByLabelText('OCR confidence threshold') as HTMLInputElement;
    expect(input.value).toBe('0.85');

    await waitFor(() => {
      expect(mockListInvoices).toHaveBeenCalledWith(
        expect.objectContaining({
          max_ocr_confidence: 0.85,
          page: 1,
          per_page: 25,
        }),
      );
    });
  });

  it('renders invoice rows with links to detail page', async () => {
    renderExceptionsPage();

    await waitFor(() => {
      expect(screen.getByText('INV-001')).toBeInTheDocument();
      expect(screen.getByText('INV-002')).toBeInTheDocument();
    });

    // Each invoice row links to /invoices/[id]
    const links = screen.getAllByRole('link');
    const hrefs = links.map((l) => l.getAttribute('href'));
    expect(hrefs).toContain('/invoices/inv-001-abcdefgh');
    expect(hrefs).toContain('/invoices/inv-002-ijklmnop');
  });

  it('refetches with new threshold when Apply is clicked', async () => {
    renderExceptionsPage();

    await waitFor(() => {
      expect(mockListInvoices).toHaveBeenCalledTimes(1);
    });

    const input = screen.getByLabelText('OCR confidence threshold');
    fireEvent.change(input, { target: { value: '0.70' } });

    fireEvent.click(screen.getByRole('button', { name: /apply/i }));

    await waitFor(() => {
      expect(mockListInvoices).toHaveBeenCalledWith(
        expect.objectContaining({
          max_ocr_confidence: 0.70,
          page: 1,
        }),
      );
    });
  });

  it('shows empty state when no invoices below threshold', async () => {
    mockListInvoices.mockResolvedValue({
      data: [],
      pagination: { page: 1, per_page: 25, total_items: 0, total_pages: 0 },
    });

    renderExceptionsPage();

    await waitFor(() => {
      expect(screen.getByText('No exceptions found')).toBeInTheDocument();
      expect(screen.getByText(/85% confidence threshold/)).toBeInTheDocument();
    });
  });

  it('shows vendor name and confidence badge for each row', async () => {
    renderExceptionsPage();

    await waitFor(() => {
      expect(screen.getByText('Acme Corp')).toBeInTheDocument();
      expect(screen.getByText('Beta LLC')).toBeInTheDocument();
    });

    const badges = screen.getAllByTestId('confidence-badge');
    expect(badges).toHaveLength(2);
    expect(badges[0]).toHaveTextContent('72%');
    expect(badges[1]).toHaveTextContent('60%');
  });
});
