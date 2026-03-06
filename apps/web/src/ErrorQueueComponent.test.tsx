import React from 'react';
import { render, fireEvent } from '@testing-library/react';
import ErrorQueueComponent from './ErrorQueueComponent';

describe('ErrorQueueComponent', () => {
  it('renders items and handles add entry click', () => {
    const mockItems = [{ description: 'Item 1' }, { description: 'Item 2' }];
    const handleAddEntryClick = jest.fn();

    render(<ErrorQueueComponent items={mockItems} handleAddEntryClick={handleAddEntryClick} />);
    
    // Check if items are rendered
    expect(screen.getAllByText('Item 1')).toHaveLength(1);
    expect(screen.getAllByText('Item 2')).toHaveLength(1);

    // Check add entry button click
    fireEvent.click(screen.getByText('Add Entry to Error Queue'));
    expect(handleAddEntryClick).toHaveBeenCalled();
  });
});