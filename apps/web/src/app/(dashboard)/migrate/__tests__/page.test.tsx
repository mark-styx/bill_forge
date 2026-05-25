import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import MigratePage from '../page';
import { vendorsApi, quickbooksApi } from '@/lib/api';

// Mock the API module
vi.mock('@/lib/api', () => ({
  vendorsApi: {
    importCsv: vi.fn(),
  },
  quickbooksApi: {
    syncVendors: vi.fn(),
  },
}));

// Mock next/navigation
const mockPush = vi.fn();
vi.mock('next/navigation', () => ({
  useRouter: () => ({ push: mockPush }),
}));

// Mock next/link
vi.mock('next/link', () => ({
  default: ({ children, ...props }: { children: React.ReactNode; href: string }) => (
    <a {...props}>{children}</a>
  ),
}));

/** Click the stepper navigation Next button (not stepper step buttons). */
async function clickNext(user: ReturnType<typeof userEvent.setup>) {
  // The navigation Next button is the one containing "Next" text and a chevron-right SVG
  const nextButtons = screen.getAllByText('Next');
  // The navigation button is the last one rendered (in the footer area)
  const navNext = nextButtons[nextButtons.length - 1];
  await user.click(navNext);
}

describe('MigratePage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders step 1 with both source cards', () => {
    render(<MigratePage />);

    expect(screen.getByText('Import Vendors')).toBeInTheDocument();
    expect(screen.getByText('QuickBooks Online')).toBeInTheDocument();
    expect(screen.getByText('Spreadsheet (CSV)')).toBeInTheDocument();
  });

  it('selecting Spreadsheet then uploading a file calls vendorsApi.importCsv and renders counts on Review step', async () => {
    const user = userEvent.setup();

    vi.mocked(vendorsApi.importCsv).mockResolvedValueOnce({
      imported: 5,
      skipped: 2,
      errors: 1,
      error_details: ['Line 4: bad row'],
    });

    render(<MigratePage />);

    // Select Spreadsheet source
    await user.click(screen.getByText('Spreadsheet (CSV)').closest('button')!);

    // Advance to step 2
    await clickNext(user);

    // Should show file input area
    expect(screen.getByText(/Upload a CSV file/)).toBeInTheDocument();

    // Upload a file via the native input
    const file = new File(['name,email\nAcme,acme@test.com'], 'vendors.csv', { type: 'text/csv' });
    const fileInput = document.querySelector('input[type="file"]') as HTMLElement;
    expect(fileInput).toBeTruthy();
    await user.upload(fileInput, file);

    // Advance to step 3 (triggers import)
    await clickNext(user);

    await waitFor(() => {
      expect(vendorsApi.importCsv).toHaveBeenCalledTimes(1);
    });

    // Review step should show counts
    await waitFor(() => {
      expect(screen.getByText('Import Complete')).toBeInTheDocument();
    });
    expect(screen.getByText('5')).toBeInTheDocument();
    expect(screen.getByText('2')).toBeInTheDocument();
    expect(screen.getByText('1')).toBeInTheDocument();
    expect(screen.getByText('Line 4: bad row')).toBeInTheDocument();
  });

  it('selecting QuickBooks and advancing calls syncVendors and renders counts', async () => {
    const user = userEvent.setup();

    vi.mocked(quickbooksApi.syncVendors).mockResolvedValueOnce({
      synced: 10,
      created: 8,
      updated: 2,
      errors: ['Connection failed for vendor X'],
    });

    render(<MigratePage />);

    // Select QuickBooks source
    await user.click(screen.getByText('QuickBooks Online').closest('button')!);

    // Advance to step 2
    await clickNext(user);

    // Should show QuickBooks instructions
    expect(screen.getByText(/pull all vendors/)).toBeInTheDocument();

    // Advance to step 3 (triggers sync)
    await clickNext(user);

    await waitFor(() => {
      expect(quickbooksApi.syncVendors).toHaveBeenCalledTimes(1);
    });

    // Review step should show counts
    await waitFor(() => {
      expect(screen.getByText('Import Complete')).toBeInTheDocument();
    });
    expect(screen.getByText('8')).toBeInTheDocument();
    expect(screen.getByText('10')).toBeInTheDocument();
    expect(screen.getByText('1')).toBeInTheDocument();
    expect(screen.getByText('Connection failed for vendor X')).toBeInTheDocument();
  });

  it('shows error when no source is selected and Next is clicked', async () => {
    const user = userEvent.setup();

    render(<MigratePage />);

    await clickNext(user);

    expect(screen.getByText('Please select an import source')).toBeInTheDocument();
  });
});
