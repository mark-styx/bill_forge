import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { LineItemsGrid } from '../line-items-grid'
import { InvoiceLineItem } from '@/lib/api'

function makeItem(overrides: Partial<InvoiceLineItem> = {}): InvoiceLineItem {
  return {
    id: 'item-1',
    line_number: 1,
    description: 'Test Item',
    quantity: 2,
    unit_price: { amount: 1000, currency: 'USD' },
    amount: { amount: 2000, currency: 'USD' },
    gl_code: '6000-100',
    department: 'Operations',
    project: 'PRJ-001',
    ...overrides,
  }
}

describe('LineItemsGrid', () => {
  it('renders line items in view mode with correct columns', () => {
    const items = [
      makeItem(),
      makeItem({ id: 'item-2', line_number: 2, description: 'Second Item', gl_code: '6000-200', department: 'Finance', project: 'PRJ-002', amount: { amount: 5000, currency: 'USD' } }),
    ]

    render(<LineItemsGrid items={items} isEditing={false} onChange={() => {}} />)

    expect(screen.getByText('Test Item')).toBeInTheDocument()
    expect(screen.getByText('Second Item')).toBeInTheDocument()
    expect(screen.getByText('6000-100')).toBeInTheDocument()
    expect(screen.getByText('Operations')).toBeInTheDocument()
    expect(screen.getByText('PRJ-001')).toBeInTheDocument()
    // $20.00 and $50.00
    expect(screen.getByText('$20.00')).toBeInTheDocument()
    expect(screen.getByText('$50.00')).toBeInTheDocument()
    // Total $70.00
    expect(screen.getByText('$70.00')).toBeInTheDocument()
  })

  it('shows GL code/department/project inputs in edit mode', () => {
    const items = [makeItem()]
    render(<LineItemsGrid items={items} isEditing={true} onChange={() => {}} />)

    const glInputs = screen.getAllByPlaceholderText('e.g. 6000-100')
    const deptInputs = screen.getAllByPlaceholderText('e.g. Operations')
    const projInputs = screen.getAllByPlaceholderText('e.g. PRJ-001')

    expect(glInputs.length).toBe(1)
    expect(deptInputs.length).toBe(1)
    expect(projInputs.length).toBe(1)
  })

  it('editing a GL code field calls onChange with updated items', async () => {
    const handleChange = vi.fn()
    const items = [makeItem()]

    render(<LineItemsGrid items={items} isEditing={true} onChange={handleChange} />)

    const glInput = screen.getByDisplayValue('6000-100')
    fireEvent.change(glInput, { target: { value: '7000-200' } })

    expect(handleChange).toHaveBeenCalled()
    const lastCall = handleChange.mock.calls[handleChange.mock.calls.length - 1][0]
    expect(lastCall[0].gl_code).toBe('7000-200')
  })

  it('Add Line button adds a new empty row', async () => {
    const handleChange = vi.fn()
    const items = [makeItem()]
    const user = userEvent.setup()

    render(<LineItemsGrid items={items} isEditing={true} onChange={handleChange} />)

    await user.click(screen.getByText('+ Add Line'))

    expect(handleChange).toHaveBeenCalledTimes(1)
    const newItems = handleChange.mock.calls[0][0]
    expect(newItems.length).toBe(2)
    expect(newItems[1].description).toBe('')
    expect(newItems[1].line_number).toBe(2)
  })

  it('delete button removes the correct row', async () => {
    const handleChange = vi.fn()
    const items = [makeItem(), makeItem({ id: 'item-2', line_number: 2, description: 'To Delete' })]
    const user = userEvent.setup()

    render(<LineItemsGrid items={items} isEditing={true} onChange={handleChange} />)

    // Click delete on second row (two clicks to confirm)
    const deleteButtons = screen.getAllByTitle('Delete line')
    await user.click(deleteButtons[1]) // first click: confirm
    expect(handleChange).not.toHaveBeenCalled()

    await user.click(deleteButtons[1]) // second click: delete
    expect(handleChange).toHaveBeenCalledTimes(1)
    const remaining = handleChange.mock.calls[0][0]
    expect(remaining.length).toBe(1)
    expect(remaining[0].id).toBe('item-1')
  })

  it('footer shows correct total amount', () => {
    const items = [
      makeItem({ amount: { amount: 1000, currency: 'USD' } }),
      makeItem({ id: 'item-2', line_number: 2, amount: { amount: 2500, currency: 'USD' } }),
    ]

    render(<LineItemsGrid items={items} isEditing={false} onChange={() => {}} />)

    // Total = 1000 + 2500 = 3500 cents = $35.00
    const totalCells = screen.getAllByText('$35.00')
    expect(totalCells.length).toBeGreaterThanOrEqual(1)
  })

  it('empty line_items array renders "No line items" message', () => {
    render(<LineItemsGrid items={[]} isEditing={false} onChange={() => {}} />)
    expect(screen.getByText('No line items')).toBeInTheDocument()
  })
})
