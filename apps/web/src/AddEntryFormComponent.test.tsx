import React from 'react';
import { render, fireEvent } from '@testing-library/react';
import AddEntryFormComponent from './AddEntryFormComponent';

describe('AddEntryFormComponent', () => {
  it('handles input changes and form submission', () => {
    const mockQueueType = 'AP';
    const handleChange = jest.fn();
    const handleSubmit = jest.fn();

    render(<AddEntryFormComponent queueType={mockQueueType} />);

    // Check input change
    fireEvent.change(screen.getByLabelText('Description'), { target: { value: 'Test Item' } });
    expect(handleChange).toHaveBeenCalledWith(expect.any(Object));

    // Check form submission
    fireEvent.submit(screen.getByRole('form'));
    expect(handleSubmit).toHaveBeenCalled();
  });
});