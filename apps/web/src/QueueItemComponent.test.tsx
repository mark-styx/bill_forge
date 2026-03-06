import React from 'react';
import { render, fireEvent } from '@testing-library/react';
import QueueItemComponent from './QueueItemComponent';

describe('QueueItemComponent', () => {
  it('renders an item and handles remove click', () => {
    const mockItem = { description: 'Item 1' };
    const handleRemoveClick = jest.fn();

    render(<QueueItemComponent item={mockItem} handleRemoveClick={handleRemoveClick} />);
    
    // Check if item is rendered
    expect(screen.getByText('Item 1')).toBeInTheDocument();

    // Check remove button click
    fireEvent.click(screen.getByText('Remove'));
    expect(handleRemoveClick).toHaveBeenCalled();
  });
});