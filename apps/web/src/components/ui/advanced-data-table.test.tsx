import * as React from 'react'
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { AdvancedDataTable, ColumnDef } from './advanced-data-table'

interface Row {
  id: string
  name: string
  value: number
}

function makeRows(n: number): Row[] {
  return Array.from({ length: n }, (_, i) => ({
    id: `row-${i}`,
    name: `Item ${i}`,
    value: i * 10,
  }))
}

const columns: ColumnDef<Row>[] = [
  { id: 'name', header: 'Name', accessorKey: 'name', sortable: true },
  { id: 'value', header: 'Value', accessorKey: 'value' },
]

function headerCell(text: string) {
  return screen.getByText(text).closest('th') as HTMLTableCellElement
}

describe('AdvancedDataTable', () => {
  describe('row rendering', () => {
    it('renders all rows verbatim when data.length <= 50', () => {
      const rows = makeRows(25)
      render(<AdvancedDataTable columns={columns} data={rows} getRowKey={(r) => r.id} />)

      // Every row's name text is present in the DOM
      rows.forEach((r) => {
        expect(screen.getByText(r.name)).toBeInTheDocument()
      })

      // 25 data rows + 1 header row = 26 rows total
      expect(screen.getAllByRole('row').length).toBe(26)
    })

    it('virtualizes rows when data.length > 50 (only a subset is mounted)', () => {
      // jsdom reports clientHeight=0; give the scroll container a viewport so the
      // virtualizer can compute a window.
      Object.defineProperty(HTMLDivElement.prototype, 'clientHeight', {
        configurable: true,
        get() {
          return 500
        },
      })

      try {
        const rows = makeRows(200)
        render(
          <AdvancedDataTable columns={columns} data={rows} getRowKey={(r) => r.id} />
        )

        const bodyRows = screen.getAllByRole('row')
        // Header + spacer rows + visible window + overscan should be well under 200.
        expect(bodyRows.length).toBeGreaterThan(0)
        expect(bodyRows.length).toBeLessThan(30)
      } finally {
        // Restore by deleting the override; prototype falls back to native getter.
        // @ts-expect-error - delete is fine for a configurable property
        delete (HTMLDivElement.prototype as any).clientHeight
      }
    })
  })

  describe('sortable header accessibility', () => {
    let onSortChange: ReturnType<typeof vi.fn>

    function SortableTable() {
      const [sortColumn, setSortColumn] = React.useState<string | undefined>(undefined)
      const [sortDirection, setSortDirection] = React.useState<'asc' | 'desc' | null>(null)
      return (
        <AdvancedDataTable
          columns={columns}
          data={makeRows(3)}
          getRowKey={(r) => r.id}
          sortColumn={sortColumn}
          sortDirection={sortDirection}
          onSortChange={(col, dir) => {
            setSortColumn(dir ? col : undefined)
            setSortDirection(dir)
            onSortChange(col, dir)
          }}
        />
      )
    }

    beforeEach(() => {
      onSortChange = vi.fn()
    })

    afterEach(() => {
      vi.restoreAllMocks()
    })

    it('exposes aria-sort, scope=col, and a keyboard-activatable button', async () => {
      const user = userEvent.setup()
      render(<SortableTable />)

      const th = headerCell('Name')

      // Initial: no sort applied
      expect(th).toHaveAttribute('scope', 'col')
      expect(th).toHaveAttribute('aria-sort', 'none')

      // Inner control is a real <button> with a descriptive aria-label
      const button = th.querySelector('button')
      expect(button).not.toBeNull()
      expect(button).toHaveAttribute('aria-label')
      expect(button?.getAttribute('aria-label')).toMatch(/Sort by Name/i)

      // Click -> ascending
      await user.click(button!)
      expect(onSortChange).toHaveBeenLastCalledWith('name', 'asc')
      expect(headerCell('Name')).toHaveAttribute('aria-sort', 'ascending')

      // Click again -> descending
      await user.click(headerCell('Name').querySelector('button')!)
      expect(onSortChange).toHaveBeenLastCalledWith('name', 'desc')
      expect(headerCell('Name')).toHaveAttribute('aria-sort', 'descending')

      // Keyboard activation: pressing Enter on the focused button triggers a sort.
      // (Native <button> elements activate their click handler on Enter/Space.)
      const callsBefore = onSortChange.mock.calls.length
      headerCell('Name').querySelector('button')!.focus()
      await user.keyboard('{Enter}')
      expect(onSortChange.mock.calls.length).toBe(callsBefore + 1)
    })
  })
})
