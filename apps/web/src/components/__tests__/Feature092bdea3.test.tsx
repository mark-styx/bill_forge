import React from 'react';
import { render, fireEvent, screen } from '@testing-library/react';
import InvoiceUploader from './InvoiceUploader';

describe('InvoiceUploader', () => {
  it('should handle file change and submit with progress indicator', async () => {
    const mockChange = jest.fn();
    render(<InvoiceUploader onChange={mockChange} />);
    fireEvent.change(screen.getByLabelText(/upload invoice file/i), {
      target: { files: [{ name: 'test.pdf' }] },
    });
    expect(mockChange).toHaveBeenCalledWith(expect.any(File));
    expect(screen.getByText('Selected File')).toBeInTheDocument();
    expect(screen.getByTestId('progress-indicator')).toBeInTheDocument();
  });

  it('should handle error', async () => {
    jest.spyOn(axios, 'post').mockRejectedValueOnce(new Error('An error occurred'));
    render(<InvoiceUploader onChange={() => {}} />);
    fireEvent.change(screen.getByLabelText(/upload invoice file/i), {
      target: { files: [{ name: 'test.pdf' }] },
    });
    expect(await screen.findByRole('alert')).toHaveTextContent('An error occurred');
  });
});