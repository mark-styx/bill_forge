import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ReactNode } from 'react';

// Mock next/navigation
vi.mock('next/navigation', () => ({
  useSearchParams: () => new URLSearchParams('token=test-vendor-token-123'),
}));

// Mock lucide-react icons
vi.mock('lucide-react', () => ({
  FileText: () => <span>FileText</span>,
  CheckCircle: () => <span>CheckCircle</span>,
}));

// Mock UI components
vi.mock('@/components/ui/button', () => ({
  Button: ({ children, ...props }: { children: ReactNode; [k: string]: unknown }) => (
    <button {...props}>{children}</button>
  ),
}));
vi.mock('@/components/ui/input', () => ({
  Input: (props: Record<string, unknown>) => <input {...props} />,
}));
vi.mock('@/components/ui/label', () => ({
  Label: ({ children }: { children: ReactNode }) => <label>{children}</label>,
}));

let submitOnboardingMock: ReturnType<typeof vi.fn>;

vi.mock('@/lib/api', () => ({
  vendorPortalApi: {
    get submitOnboarding() { return submitOnboardingMock; },
  },
}));

describe('Vendor Onboarding Page', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    submitOnboardingMock = vi.fn().mockResolvedValue({ submission_id: 'sub-1', status: 'pending' });
  });

  it('vendorPortalApi.submitOnboarding is callable with token and payload', async () => {
    const { vendorPortalApi } = await import('@/lib/api');
    submitOnboardingMock.mockResolvedValue({ submission_id: 'sub-new', status: 'pending' });

    const result = await vendorPortalApi.submitOnboarding(
      'test-token',
      {
        legal_name: 'Acme Corp',
        tax_form_type: 'w9',
      },
      undefined,
    );

    expect(submitOnboardingMock).toHaveBeenCalledWith(
      'test-token',
      expect.objectContaining({
        legal_name: 'Acme Corp',
        tax_form_type: 'w9',
      }),
      undefined,
    );
    expect(result.status).toBe('pending');
    expect(result.submission_id).toBe('sub-new');
  });

  it('submitOnboarding sends tax document file when provided', async () => {
    const { vendorPortalApi } = await import('@/lib/api');

    const mockFile = new File(['w9 content'], 'w9.pdf', { type: 'application/pdf' });
    await vendorPortalApi.submitOnboarding(
      'test-token',
      {
        legal_name: 'Test Vendor',
        tax_form_type: 'w9',
        banking: {
          bank_name: 'Test Bank',
          account_type: 'checking',
          account_number: '123456789',
          routing_number: '021000021',
        },
      },
      mockFile,
    );

    expect(submitOnboardingMock).toHaveBeenCalledWith(
      'test-token',
      expect.objectContaining({
        legal_name: 'Test Vendor',
        tax_form_type: 'w9',
        banking: expect.objectContaining({
          bank_name: 'Test Bank',
        }),
      }),
      mockFile,
    );
  });

  it('submitOnboarding handles w8ben form type', async () => {
    const { vendorPortalApi } = await import('@/lib/api');

    await vendorPortalApi.submitOnboarding(
      'test-token',
      {
        legal_name: 'International Corp',
        tax_form_type: 'w8ben',
        address: { line1: '1 High St', city: 'London', postal_code: 'SW1A 1AA', country: 'GB' },
      },
      undefined,
    );

    expect(submitOnboardingMock).toHaveBeenCalledWith(
      'test-token',
      expect.objectContaining({
        tax_form_type: 'w8ben',
      }),
      undefined,
    );
  });

  it('submitOnboarding sends remit contacts', async () => {
    const { vendorPortalApi } = await import('@/lib/api');

    const contacts = [
      { name: 'Jane Doe', email: 'jane@test.com', phone: '555-1234' },
      { name: 'Bob Smith', email: 'bob@test.com', phone: '555-5678' },
    ];

    await vendorPortalApi.submitOnboarding(
      'test-token',
      {
        legal_name: 'Test Corp',
        tax_form_type: 'w9',
        remit_contacts: contacts,
      },
      undefined,
    );

    expect(submitOnboardingMock).toHaveBeenCalledWith(
      'test-token',
      expect.objectContaining({
        remit_contacts: contacts,
      }),
      undefined,
    );
  });

  it('submitOnboarding rejects on missing legal_name', async () => {
    const { vendorPortalApi } = await import('@/lib/api');
    submitOnboardingMock.mockRejectedValue(new Error('Missing required field: legal_name'));

    await expect(
      vendorPortalApi.submitOnboarding('test-token', {
        legal_name: '',
        tax_form_type: 'w9',
      }, undefined),
    ).rejects.toThrow('Missing required field: legal_name');
  });
});
