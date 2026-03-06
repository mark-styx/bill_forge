import React from 'react';
import { render, screen } from '@testing-library/react';
import Dashboard from './Dashboard';

describe('Dashboard', () => {
  it('should display matched data and handle refresh', async () => {
    jest.spyOn(axios, 'get').mockResolvedValueOnce({
      data: [{ vendorDetails: {}, invoiceData: {} }],
    });
    render(<Dashboard matchedData={[{ vendorDetails: {}, invoiceData: {} }]} loading={false} error={null} />);
    expect(await screen.findByText('Vendor Details')).toBeInTheDocument();
    fireEvent.click(screen.getByText('Refresh'));
    expect(axios.get).toHaveBeenCalledWith('/api/matched-data');
  });

  it('should display loading and error states', () => {
    render(<Dashboard matchedData={null} loading={true} error={null} />);
    expect(screen.getByText('Loading...')).toBeInTheDocument();
    render(<Dashboard matchedData={null} loading={false} error="An error occurred" />);
    expect(screen.getByRole('alert')).toHaveTextContent('An error occurred');
  });
});