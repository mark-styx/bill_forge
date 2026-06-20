import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import ApMigrationBundlePage from '../page';
import { apMigrationApi } from '@/lib/api/apMigration';

vi.mock('@/lib/api/apMigration', () => ({
  apMigrationApi: {
    uploadBundle: vi.fn(),
    getPreview: vi.fn(),
    commit: vi.fn(),
    cancel: vi.fn(),
  },
}));

const mockPush = vi.fn();
let mockSearchParams = new URLSearchParams();
vi.mock('next/navigation', () => ({
  useRouter: () => ({ push: mockPush }),
  useSearchParams: () => mockSearchParams,
}));

vi.mock('next/link', () => ({
  default: ({ children, ...props }: { children: React.ReactNode; href: string }) => (
    <a {...props}>{children}</a>
  ),
}));

describe('ApMigrationBundlePage', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockSearchParams = new URLSearchParams();
  });

  it('renders the upload state and POSTs the file', async () => {
    const user = userEvent.setup();
    vi.mocked(apMigrationApi.uploadBundle).mockResolvedValueOnce({
      bundle_id: 'bundle-123',
      source: 'bill',
      status: 'parsed',
      parse_errors: [],
    });

    render(<ApMigrationBundlePage />);

    expect(screen.getByText('Import from BILL.com or Coupa')).toBeInTheDocument();

    const file = new File(['fake zip bytes'], 'export.zip', { type: 'application/zip' });
    const fileInput = document.querySelector('input[type="file"]') as HTMLElement;
    expect(fileInput).toBeTruthy();
    await user.upload(fileInput, file);

    await user.click(screen.getByRole('button', { name: /Upload and Preview/i }));

    await waitFor(() => {
      expect(apMigrationApi.uploadBundle).toHaveBeenCalledTimes(1);
    });
    expect(mockPush).toHaveBeenCalledWith('/migrate/bundle?id=bundle-123');
  });

  it('renders the side-by-side preview with per-section counts and commits', async () => {
    const user = userEvent.setup();
    mockSearchParams = new URLSearchParams('id=bundle-abc');
    vi.mocked(apMigrationApi.getPreview).mockResolvedValueOnce({
      bundle: {
        id: 'bundle-abc',
        source: 'bill',
        status: 'parsed',
        original_filename: 'export.zip',
        uploaded_at: '2026-06-20T00:00:00Z',
        error_text: null,
      },
      entities: {
        vendors: [
          {
            id: 'r1',
            entity_type: 'vendor',
            source_payload: { name: 'Acme', tax_id: '11-2222222' },
            target_action: 'create',
            target_match_id: null,
            conflict_reason: null,
          },
          {
            id: 'r2',
            entity_type: 'vendor',
            source_payload: { name: 'Globex', tax_id: '99-0000000' },
            target_action: 'update',
            target_match_id: 'vendor-uuid',
            conflict_reason: null,
          },
        ],
        invoices: [
          {
            id: 'inv1',
            entity_type: 'invoice',
            source_payload: { invoice_number: 'INV-1', amount: '100.00' },
            target_action: 'create',
            target_match_id: null,
            conflict_reason: null,
          },
        ],
        approval_workflows: [],
        gl_mappings: [],
        approvers: [],
        documents: [],
      },
    });
    vi.mocked(apMigrationApi.commit).mockResolvedValueOnce({
      bundle_id: 'bundle-abc',
      status: 'committed',
      vendors_created: 1,
      vendors_updated: 1,
      invoices_created: 1,
      invoices_updated: 0,
      approval_workflows_created: 0,
      gl_mappings_created: 0,
      gl_mappings_updated: 0,
      approvers_created: 0,
      approvers_updated: 0,
      documents_created: 0,
      skipped: 0,
    });

    render(<ApMigrationBundlePage />);

    await waitFor(() => {
      expect(screen.getByText('Migration Preview')).toBeInTheDocument();
    });

    expect(screen.getByText('Vendors')).toBeInTheDocument();
    expect(screen.getByText('Open Invoices')).toBeInTheDocument();
    expect(screen.getByText(/2 rows.*1 create.*1 update/)).toBeInTheDocument();
    expect(screen.getAllByText('Source (BILL)').length).toBeGreaterThan(0);
    expect(screen.getAllByText('Target (BillForge)').length).toBeGreaterThan(0);

    await user.click(screen.getByRole('button', { name: /Commit Migration/i }));

    await waitFor(() => {
      expect(apMigrationApi.commit).toHaveBeenCalledWith('bundle-abc');
    });

    await waitFor(() => {
      expect(screen.getByText('Migration Complete')).toBeInTheDocument();
    });
    expect(screen.getByText('Vendors created')).toBeInTheDocument();
  });
});
