import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import UploadInvoicePage from '../page';
import { invoicesApi } from '@/lib/api';

// Mock the API module
vi.mock('@/lib/api', () => ({
  invoicesApi: {
    upload: vi.fn(),
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

// Mock sonner toast
vi.mock('sonner', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

// Capture onDrop from useDropzone so tests can simulate file drops
let capturedOnDrop: (files: File[]) => void;
vi.mock('react-dropzone', () => ({
  useDropzone: (opts: { onDrop: (files: File[]) => void }) => {
    capturedOnDrop = opts.onDrop;
    return {
      getRootProps: () => ({ className: 'border-dashed' }),
      getInputProps: () => ({ type: 'file' }),
      isDragActive: false,
      isDragReject: false,
    };
  },
}));

function createFile(name: string, type: string, size = 1024): File {
  const file = new File(['x'.repeat(size)], name, { type });
  Object.defineProperty(file, 'size', { value: size });
  return file;
}

function dropFiles(files: File[]) {
  capturedOnDrop(files);
}

describe('UploadInvoicePage batch upload', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders and accepts multiple files', async () => {
    render(<UploadInvoicePage />);

    // Should show the dropzone with plural copy
    expect(screen.getByText('Drag & drop your invoices')).toBeInTheDocument();
    expect(screen.getByText(/up to 50 files/i)).toBeInTheDocument();

    // Simulate dropping 3 files
    const files = [
      createFile('invoice1.pdf', 'application/pdf'),
      createFile('invoice2.png', 'image/png'),
      createFile('invoice3.jpg', 'image/jpeg'),
    ];

    dropFiles(files);

    await waitFor(() => {
      expect(screen.getByText('invoice1.pdf')).toBeInTheDocument();
      expect(screen.getByText('invoice2.png')).toBeInTheDocument();
      expect(screen.getByText('invoice3.jpg')).toBeInTheDocument();
    });

    // Upload button should show count
    expect(screen.getByText(/Upload & Process \(3\)/)).toBeInTheDocument();
  });

  it('batch upload success path', async () => {
    const user = userEvent.setup();

    vi.mocked(invoicesApi.upload)
      .mockResolvedValueOnce({ invoice_id: 'id-1', document_id: 'd1', message: 'ok' })
      .mockResolvedValueOnce({ invoice_id: 'id-2', document_id: 'd2', message: 'ok' })
      .mockResolvedValueOnce({ invoice_id: 'id-3', document_id: 'd3', message: 'ok' });

    render(<UploadInvoicePage />);

    const files = [
      createFile('a.pdf', 'application/pdf'),
      createFile('b.png', 'image/png'),
      createFile('c.pdf', 'application/pdf'),
    ];

    dropFiles(files);

    await waitFor(() => {
      expect(screen.getByText('a.pdf')).toBeInTheDocument();
    });

    // Click upload button
    await user.click(screen.getByText(/Upload & Process/));

    // Wait for all uploads to complete
    await waitFor(() => {
      expect(screen.getByText(/3 of 3 uploaded/)).toBeInTheDocument();
    });

    // API should have been called 3 times
    expect(invoicesApi.upload).toHaveBeenCalledTimes(3);

    // No redirect for batch (more than 1 file)
    expect(mockPush).not.toHaveBeenCalled();

    // Toast success
    const { toast } = await import('sonner');
    expect(toast.success).toHaveBeenCalledWith('3 invoices uploaded');
  });

  it('partial failure shows errors for failed files', async () => {
    const user = userEvent.setup();

    vi.mocked(invoicesApi.upload)
      .mockResolvedValueOnce({ invoice_id: 'id-1', document_id: 'd1', message: 'ok' })
      .mockRejectedValueOnce(new Error('Server error'))
      .mockResolvedValueOnce({ invoice_id: 'id-3', document_id: 'd3', message: 'ok' });

    render(<UploadInvoicePage />);

    const files = [
      createFile('good1.pdf', 'application/pdf'),
      createFile('bad.pdf', 'application/pdf'),
      createFile('good2.pdf', 'application/pdf'),
    ];

    dropFiles(files);

    await waitFor(() => {
      expect(screen.getByText('good1.pdf')).toBeInTheDocument();
    });

    await user.click(screen.getByText(/Upload & Process/));

    // Wait for batch to finish
    await waitFor(() => {
      expect(screen.getByText(/2 of 3 uploaded, 1 failed/)).toBeInTheDocument();
    });

    // Error message for failed file should be visible
    expect(screen.getByText('Server error')).toBeInTheDocument();

    // No redirect
    expect(mockPush).not.toHaveBeenCalled();

    // Toast with partial failure info
    const { toast } = await import('sonner');
    expect(toast.error).toHaveBeenCalledWith('2 of 3 uploaded, 1 failed');
  });
});
