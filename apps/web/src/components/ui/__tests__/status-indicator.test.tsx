import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import { StatusIndicator } from '@/components/ui/status-indicator';

describe('StatusIndicator', () => {
  it('supports invoice processing and capture statuses', () => {
    render(
      <div>
        <StatusIndicator status="on_hold" />
        <StatusIndicator status="ready_for_payment" />
        <StatusIndicator status="paid" />
        <StatusIndicator status="voided" />
        <StatusIndicator status="ready_for_review" />
        <StatusIndicator status="failed" />
      </div>,
    );

    expect(screen.getByText('On Hold')).toBeInTheDocument();
    expect(screen.getByText('Ready for Payment')).toBeInTheDocument();
    expect(screen.getByText('Paid')).toBeInTheDocument();
    expect(screen.getByText('Voided')).toBeInTheDocument();
    expect(screen.getByText('Ready for Review')).toBeInTheDocument();
    expect(screen.getByText('Failed')).toBeInTheDocument();
  });
});
