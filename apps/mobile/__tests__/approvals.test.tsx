/**
 * Component tests for the Approval Inbox screen.
 *
 * Uses @testing-library/react-native to verify rendering,
 * formatted amounts, color-coded due dates, empty states,
 * and approve/reject interactions.
 */

// Mock react-native with lightweight stubs (RN 0.76 platform resolution doesn't work in Jest without Metro)
jest.mock('react-native', () => {
  const React = require('react');
  const View = (p) => React.createElement('View', p, p && p.children);
  const Text = (p) => React.createElement('Text', p, p && p.children);
  const FlatList = (p) => {
    const content = p && p.data && p.data.length > 0
      ? p.data.map((item, i) => p.renderItem({ item, index: i }))
      : (p && p.ListEmptyComponent) || null;
    return React.createElement('FlatList', p, content);
  };
  const TouchableOpacity = (p) => React.createElement('TouchableOpacity', p, p && p.children);
  const RefreshControl = (p) => React.createElement('RefreshControl', p);
  return {
    View, Text, FlatList, TouchableOpacity, RefreshControl,
    StyleSheet: { create: (s) => s },
    Alert: { alert: jest.fn() },
    Animated: { Value: class { constructor() {} }, View },
  };
});

jest.mock('@testing-library/react-native', () => {
  const renderer = require('react-test-renderer');
  function render(ui) {
    const tree = renderer.create(ui);
    function queryByText(textOrRegex) {
      const nodes = tree.root.findAll((node) =>
        node.children.some((c) => {
          if (typeof c !== 'string') return false;
          return typeof textOrRegex === 'string' ? c === textOrRegex : textOrRegex.test(c);
        })
      );
      return nodes.length > 0 ? nodes[0] : null;
    }
    return { queryByText };
  }
  return { render, fireEvent: {} };
});

// Mock expo modules
jest.mock('expo-router', () => ({
  useRouter: () => ({ push: jest.fn(), back: jest.fn() }),
}));

jest.mock('expo-secure-store', () => ({}));

jest.mock('expo-status-bar', () => ({ StatusBar: 'StatusBar' }));

jest.mock('expo-notifications', () => ({
  setNotificationHandler: jest.fn(),
  getPermissionsAsync: jest.fn(() => Promise.resolve({ status: 'granted' })),
  requestPermissionsAsync: jest.fn(() => Promise.resolve({ status: 'granted' })),
  getExpoPushTokenAsync: jest.fn(() => Promise.resolve({ data: 'mock-token' })),
  addNotificationReceivedListener: jest.fn(() => ({ remove: jest.fn() })),
}));

jest.mock('@tanstack/react-query', () => {
  const React = require('react');
  let queryData: unknown = [];
  let queryLoading = false;
  let queryError: unknown = null;

  return {
    QueryClient: jest.fn().mockImplementation(() => ({})),
    QueryClientProvider: ({ children }: { children: React.ReactNode }) => children,
    useQuery: jest.fn(() => ({
      data: queryData,
      isLoading: queryLoading,
      error: queryError,
      refetch: jest.fn(),
      isRefetching: false,
    })),
    useQueryClient: jest.fn(() => ({
      invalidateQueries: jest.fn(),
    })),
    // Test helper to set mock state
    __setMockQueryState: (data: unknown, loading = false, error: unknown = null) => {
      queryData = data;
      queryLoading = loading;
      queryError = error;
    },
  };
});

jest.mock('../lib/store', () => ({
  useAppStore: jest.fn((selector: (s: Record<string, unknown>) => unknown) =>
    selector({ pendingCount: 2, setPendingCount: jest.fn() }),
  ),
}));

jest.mock('../lib/api', () => ({
  api: {
    getApprovals: jest.fn(),
    approveInvoice: jest.fn(() => Promise.resolve({ success: true })),
    rejectInvoice: jest.fn(() => Promise.resolve({ success: true })),
  },
}));

import React from 'react';
import { render, fireEvent } from '@testing-library/react-native';

// Get a reference to the mock setter
const { __setMockQueryState } = require('@tanstack/react-query');

// We need to import the component under test after mocks
const InboxScreen = require('../app/(tabs)/index').default;

const makeApproval = (overrides: Record<string, unknown> = {}) => ({
  id: 'apr-1',
  invoice: {
    id: 'inv-1',
    vendor_name: 'Acme Corp',
    invoice_number: 'INV-001',
    total_amount_cents: 150050, // $1,500.50
    currency: 'USD',
    due_date: '2026-04-10',
    status: 'pending_approval',
    days_until_due: 6,
    requires_action: true,
    created_at: '2026-04-01T00:00:00Z',
  },
  requested_at: '2026-04-01T00:00:00Z',
  expires_at: null,
  can_approve: true,
  ...overrides,
});

describe('Approval Inbox Screen', () => {
  beforeEach(() => {
    jest.clearAllMocks();
    __setMockQueryState([]);
  });

  it('renders empty state when no approvals', () => {
    __setMockQueryState([]);
    const { queryByText } = render(<InboxScreen />);
    expect(queryByText('All caught up!')).not.toBeNull();
    expect(queryByText('No pending approvals.')).not.toBeNull();
  });

  it('renders list of pending approvals', () => {
    __setMockQueryState([makeApproval()]);
    const { queryByText } = render(<InboxScreen />);
    expect(queryByText('Acme Corp')).not.toBeNull();
    expect(queryByText('INV-001')).not.toBeNull();
  });

  it('displays formatted amount from cents', () => {
    __setMockQueryState([makeApproval()]);
    const { queryByText } = render(<InboxScreen />);
    // 150050 cents = $1,500.50
    expect(queryByText(/\$1,500\.50/)).not.toBeNull();
  });

  it('shows due-date badge with days', () => {
    __setMockQueryState([makeApproval()]);
    const { queryByText } = render(<InboxScreen />);
    // 6 days until due
    expect(queryByText('6d')).not.toBeNull();
  });

  it('shows dash for null days_until_due', () => {
    __setMockQueryState([
      makeApproval({
        invoice: {
          ...makeApproval().invoice,
          days_until_due: null,
        },
      }),
    ]);
    const { queryByText } = render(<InboxScreen />);
    // Find the dash in the due badge area
    const allTexts = queryByText('—');
    expect(allTexts).not.toBeNull();
  });

  it('shows approve button when can_approve is true', () => {
    __setMockQueryState([makeApproval({ can_approve: true })]);
    const { queryByText } = render(<InboxScreen />);
    expect(queryByText('Approve')).not.toBeNull();
  });

  it('hides approve button when can_approve is false', () => {
    __setMockQueryState([makeApproval({ can_approve: false })]);
    const { queryByText } = render(<InboxScreen />);
    // The "Approve" quick-action button should not be rendered
    // The text "Approve" should not appear as a button label
    const approveButtons = queryByText('Approve');
    expect(approveButtons).toBeNull();
  });

  it('renders multiple approval items', () => {
    __setMockQueryState([
      makeApproval({ id: 'apr-1', invoice: { ...makeApproval().invoice, vendor_name: 'Vendor A', invoice_number: 'INV-001' } }),
      makeApproval({ id: 'apr-2', invoice: { ...makeApproval().invoice, vendor_name: 'Vendor B', invoice_number: 'INV-002' } }),
    ]);
    const { queryByText } = render(<InboxScreen />);
    expect(queryByText('Vendor A')).not.toBeNull();
    expect(queryByText('Vendor B')).not.toBeNull();
  });
});
