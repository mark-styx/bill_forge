import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { DocumentQAPanel } from '../DocumentQAPanel';

vi.mock('@/lib/documentQa', () => ({
  askDocument: vi.fn(),
}));

import { askDocument } from '@/lib/documentQa';
const mockedAskDocument = vi.mocked(askDocument);

describe('DocumentQAPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders the panel with input and sends a question', async () => {
    mockedAskDocument.mockResolvedValueOnce({
      answer: 'The total is $5,000.',
      citations: [
        { id: 1, page: 1, bbox: [0, 400, 612, 500] as [number, number, number, number], quote: 'Total: $5,000' },
      ],
    });

    render(<DocumentQAPanel documentId="doc-123" />);

    const textarea = screen.getByPlaceholderText('Ask a question...');
    expect(textarea).toBeInTheDocument();

    await userEvent.type(textarea, 'What is the total?');
    fireEvent.click(screen.getByText('Send'));

    await waitFor(() => {
      expect(mockedAskDocument).toHaveBeenCalledWith('doc-123', 'What is the total?');
    });

    await waitFor(() => {
      expect(screen.getByText(/The total is \$5,000/)).toBeInTheDocument();
    });
  });

  it('renders citation chips and fires onCitationClick when clicked', async () => {
    const citation = {
      id: 2,
      page: 3,
      bbox: [10, 200, 400, 300] as [number, number, number, number],
      quote: 'Payment due in 30 days',
    };
    mockedAskDocument.mockResolvedValueOnce({
      answer: 'Payment is due in 30 days.',
      citations: [citation],
    });

    const handleClick = vi.fn();
    render(<DocumentQAPanel documentId="doc-456" onCitationClick={handleClick} />);

    const textarea = screen.getByPlaceholderText('Ask a question...');
    await userEvent.type(textarea, 'When is payment due?');
    fireEvent.click(screen.getByText('Send'));

    await waitFor(() => {
      expect(screen.getByText('[#2]')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText('[#2]'));
    expect(handleClick).toHaveBeenCalledWith(citation);
  });

  it('shows disabled state when no documentId', () => {
    render(<DocumentQAPanel documentId={undefined} />);

    const textarea = screen.getByPlaceholderText('Ask a question...');
    expect(textarea).toBeDisabled();
    expect(screen.getByText('Send')).toBeDisabled();
  });
});
