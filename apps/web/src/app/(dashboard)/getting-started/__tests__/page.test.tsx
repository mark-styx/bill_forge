import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

// ---------------------------------------------------------------------------
// localStorage polyfill for Node 26 jsdom where window.localStorage is
// undefined unless --localstorage-file is passed. We provide a simple
// in-memory store and install it on window before any component imports.
// ---------------------------------------------------------------------------
class MemoryStorage implements Storage {
  private data = new Map<string, string>();
  get length() { return this.data.size; }
  key(index: number): string | null { return [...this.data.keys()][index] ?? null; }
  getItem(key: string): string | null { return this.data.get(key) ?? null; }
  setItem(key: string, value: string) { this.data.set(key, value); }
  removeItem(key: string) { this.data.delete(key); }
  clear() { this.data.clear(); }
}

const memoryStorage = new MemoryStorage();

beforeEach(() => {
  memoryStorage.clear();
  // Ensure every access path points at our polyfill
  (globalThis as Record<string, unknown>).localStorage = memoryStorage;
  if (typeof window !== 'undefined') {
    (window as unknown as Record<string, unknown>).localStorage = memoryStorage;
  }
});

// Mock auth store
vi.mock('@/stores/auth', () => ({
  useAuthStore: vi.fn(() => ({
    tenant: { id: 'test-tenant-1', name: 'Test Org' },
  })),
}));

// Mock next/link
vi.mock('next/link', () => ({
  default: ({ children, ...props }: { children: React.ReactNode; href: string }) => (
    <a {...props}>{children}</a>
  ),
}));

// Import page AFTER polyfill is set up
import GettingStartedPage from '../page';

describe('GettingStartedPage', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('renders the four phase headings and the Day counter', () => {
    render(<GettingStartedPage />);

    // Header
    expect(screen.getByText('Implementation Wizard')).toBeInTheDocument();
    expect(screen.getByTestId('day-number')).toBeInTheDocument();

    // Phase headings
    expect(screen.getByText('Connect your accounting system')).toBeInTheDocument();
    expect(screen.getByText('Choose an approval-chain template')).toBeInTheDocument();
    expect(screen.getByText('Validate OCR with 10 sample invoices')).toBeInTheDocument();
    expect(screen.getByText('Go-live checklist')).toBeInTheDocument();

    // All start as "Not started"
    const notStarted = screen.getAllByText('Not started');
    expect(notStarted.length).toBe(4);

    // Initial progress is 0%
    expect(screen.getByTestId('progress-percent')).toHaveTextContent('0% complete');
  });

  it('marks phase 1 complete when "Mark phase complete" is clicked, bumping progress to 25%', async () => {
    const user = userEvent.setup();
    render(<GettingStartedPage />);

    // Find the ERP phase card and click its "Mark phase complete" button
    const erpCard = screen.getByText('Connect your accounting system').closest('.border')!;
    const markBtn = within(erpCard as HTMLElement).getByText('Mark phase complete');
    await user.click(markBtn);

    // Status pill should now say "Complete"
    const completePills = within(erpCard as HTMLElement).getAllByText('Complete');
    expect(completePills.length).toBeGreaterThanOrEqual(1);

    // Progress bar should show 25%
    expect(screen.getByTestId('progress-percent')).toHaveTextContent('25% complete');
  });

  it('increments OCR counter and auto-completes at 10', async () => {
    const user = userEvent.setup();
    render(<GettingStartedPage />);

    // Find the +1 Sample button and click it 10 times
    const btn = screen.getByText('+1 Sample');
    for (let i = 0; i < 9; i++) {
      await user.click(btn);
    }

    // After 9 clicks count should be 9
    expect(screen.getByTestId('ocr-count')).toHaveTextContent('9 / 10 uploaded');

    // The button should still be present
    expect(screen.getByText('+1 Sample')).toBeInTheDocument();

    // 10th click
    await user.click(screen.getByText('+1 Sample'));

    // Should now show 10 and be complete
    expect(screen.getByTestId('ocr-count')).toHaveTextContent('10 / 10 uploaded');

    // The +1 button should be gone (replaced by completed state)
    expect(screen.queryByText('+1 Sample')).not.toBeInTheDocument();
  });

  it('persists state to localStorage', async () => {
    const user = userEvent.setup();
    render(<GettingStartedPage />);

    // Select an approval template
    await user.click(screen.getByText('By department'));

    // Check localStorage has the value
    const stored = memoryStorage.getItem('billforge.implementation.v1.test-tenant-1');
    expect(stored).toBeTruthy();
    const parsed = JSON.parse(stored!);
    expect(parsed.phases.approvals.template).toBe('department');
    expect(parsed.phases.approvals.status).toBe('complete');
  });
});
