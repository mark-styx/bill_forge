'use client';

import * as React from 'react';
import { cn } from '@/lib/utils';
import {
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  ChevronUp,
  Columns3,
  Download,
  Filter,
  Loader2,
  MoreHorizontal,
  RefreshCw,
  Search,
  SlidersHorizontal,
  X,
} from 'lucide-react';
import { Button } from './button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  DropdownMenuCheckboxItem,
  DropdownMenuLabel,
} from './dropdown-menu';

export type SortDirection = 'asc' | 'desc' | null;

export interface ColumnDef<T> {
  id: string;
  header: string | React.ReactNode;
  accessorKey?: keyof T;
  cell?: (item: T, index: number) => React.ReactNode;
  sortable?: boolean;
  filterable?: boolean;
  width?: string | number;
  minWidth?: string | number;
  align?: 'left' | 'center' | 'right';
  hidden?: boolean;
  sticky?: 'left' | 'right';
}

export interface FilterValue {
  column: string;
  operator: 'equals' | 'contains' | 'startsWith' | 'endsWith' | 'gt' | 'lt' | 'gte' | 'lte';
  value: string;
}

export interface AdvancedDataTableProps<T> {
  columns: ColumnDef<T>[];
  data: T[];
  isLoading?: boolean;
  error?: string;
  emptyState?: React.ReactNode;
  emptySearchState?: React.ReactNode;

  // Row interactions
  onRowClick?: (item: T) => void;
  getRowKey?: (item: T) => string;
  getRowClassName?: (item: T, index: number) => string;
  rowActions?: (item: T) => React.ReactNode;

  // Selection
  selectable?: boolean;
  selectedRows?: string[];
  onSelectionChange?: (selectedIds: string[]) => void;

  // Sorting
  sortColumn?: string;
  sortDirection?: SortDirection;
  onSortChange?: (column: string, direction: SortDirection) => void;

  // Filtering
  searchValue?: string;
  onSearchChange?: (value: string) => void;
  searchPlaceholder?: string;
  filters?: FilterValue[];
  onFiltersChange?: (filters: FilterValue[]) => void;

  // Pagination
  pagination?: {
    page: number;
    perPage: number;
    totalItems: number;
    totalPages: number;
    onPageChange: (page: number) => void;
    onPerPageChange?: (perPage: number) => void;
    perPageOptions?: number[];
  };

  // Column visibility
  columnVisibility?: Record<string, boolean>;
  onColumnVisibilityChange?: (visibility: Record<string, boolean>) => void;

  // Toolbar
  showToolbar?: boolean;
  toolbarActions?: React.ReactNode;
  onRefresh?: () => void;
  onExport?: () => void;

  // Styling
  className?: string;
  stickyHeader?: boolean;
  striped?: boolean;
  bordered?: boolean;
  compact?: boolean;
  highlightOnHover?: boolean;
}

export function AdvancedDataTable<T extends Record<string, any>>({
  columns,
  data,
  isLoading,
  error,
  emptyState,
  emptySearchState,
  onRowClick,
  getRowKey,
  getRowClassName,
  rowActions,
  selectable,
  selectedRows = [],
  onSelectionChange,
  sortColumn,
  sortDirection,
  onSortChange,
  searchValue,
  onSearchChange,
  searchPlaceholder = 'Search...',
  filters = [],
  onFiltersChange,
  pagination,
  columnVisibility = {},
  onColumnVisibilityChange,
  showToolbar = true,
  toolbarActions,
  onRefresh,
  onExport,
  className,
  stickyHeader = false,
  striped = false,
  bordered = false,
  compact = false,
  highlightOnHover = true,
}: AdvancedDataTableProps<T>) {
  const [localSearch, setLocalSearch] = React.useState(searchValue || '');
  const [showFilters, setShowFilters] = React.useState(false);

  // Handle search with debounce
  React.useEffect(() => {
    const timer = setTimeout(() => {
      if (onSearchChange && localSearch !== searchValue) {
        onSearchChange(localSearch);
      }
    }, 300);
    return () => clearTimeout(timer);
  }, [localSearch, onSearchChange, searchValue]);

  const visibleColumns = columns.filter(
    (col) => !col.hidden && (columnVisibility[col.id] !== false)
  );

  const handleSelectAll = () => {
    if (!onSelectionChange || !getRowKey) return;
    if (selectedRows.length === data.length) {
      onSelectionChange([]);
    } else {
      onSelectionChange(data.map(getRowKey));
    }
  };

  const handleSelectRow = (item: T) => {
    if (!onSelectionChange || !getRowKey) return;
    const key = getRowKey(item);
    if (selectedRows.includes(key)) {
      onSelectionChange(selectedRows.filter((id) => id !== key));
    } else {
      onSelectionChange([...selectedRows, key]);
    }
  };

  const handleSort = (columnId: string) => {
    if (!onSortChange) return;
    const column = columns.find((col) => col.id === columnId);
    if (!column?.sortable) return;

    let newDirection: SortDirection = 'asc';
    if (sortColumn === columnId) {
      if (sortDirection === 'asc') newDirection = 'desc';
      else if (sortDirection === 'desc') newDirection = null;
    }
    onSortChange(columnId, newDirection);
  };

  const isAllSelected = data.length > 0 && selectedRows.length === data.length;
  const isSomeSelected = selectedRows.length > 0 && selectedRows.length < data.length;

  return (
    <div className={cn('space-y-4', className)}>
      {/* Toolbar */}
      {showToolbar && (
        <div className="flex flex-col sm:flex-row items-start sm:items-center justify-between gap-3">
          <div className="flex items-center gap-2 flex-1 w-full sm:w-auto">
            {/* Search */}
            {onSearchChange && (
              <div className="relative flex-1 sm:max-w-xs">
                <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
                <input
                  type="text"
                  value={localSearch}
                  onChange={(e) => setLocalSearch(e.target.value)}
                  placeholder={searchPlaceholder}
                  className="input pl-9 pr-8 h-9"
                />
                {localSearch && (
                  <button
                    onClick={() => {
                      setLocalSearch('');
                      onSearchChange('');
                    }}
                    className="absolute right-2 top-1/2 -translate-y-1/2 p-1 rounded hover:bg-secondary"
                  >
                    <X className="w-3 h-3 text-muted-foreground" />
                  </button>
                )}
              </div>
            )}

            {/* Filters */}
            {onFiltersChange && (
              <Button
                variant="outline"
                size="sm"
                onClick={() => setShowFilters(!showFilters)}
                className={cn(filters.length > 0 && 'border-primary text-primary')}
              >
                <Filter className="w-4 h-4 mr-1.5" />
                Filters
                {filters.length > 0 && (
                  <span className="ml-1.5 px-1.5 py-0.5 rounded-full bg-primary text-primary-foreground text-xs">
                    {filters.length}
                  </span>
                )}
              </Button>
            )}
          </div>

          <div className="flex items-center gap-2">
            {/* Selection info */}
            {selectable && selectedRows.length > 0 && (
              <span className="text-sm text-muted-foreground">
                {selectedRows.length} selected
              </span>
            )}

            {/* Column visibility */}
            {onColumnVisibilityChange && (
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button variant="outline" size="sm">
                    <Columns3 className="w-4 h-4 mr-1.5" />
                    Columns
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" className="w-48">
                  <DropdownMenuLabel>Toggle columns</DropdownMenuLabel>
                  <DropdownMenuSeparator />
                  {columns
                    .filter((col) => !col.hidden)
                    .map((column) => (
                      <DropdownMenuCheckboxItem
                        key={column.id}
                        checked={columnVisibility[column.id] !== false}
                        onCheckedChange={(checked) =>
                          onColumnVisibilityChange({
                            ...columnVisibility,
                            [column.id]: checked,
                          })
                        }
                      >
                        {typeof column.header === 'string' ? column.header : column.id}
                      </DropdownMenuCheckboxItem>
                    ))}
                </DropdownMenuContent>
              </DropdownMenu>
            )}

            {/* Refresh */}
            {onRefresh && (
              <Button variant="outline" size="sm" onClick={onRefresh} disabled={isLoading}>
                <RefreshCw className={cn('w-4 h-4', isLoading && 'animate-spin')} />
              </Button>
            )}

            {/* Export */}
            {onExport && (
              <Button variant="outline" size="sm" onClick={onExport}>
                <Download className="w-4 h-4 mr-1.5" />
                Export
              </Button>
            )}

            {/* Custom toolbar actions */}
            {toolbarActions}
          </div>
        </div>
      )}

      {/* Active Filters */}
      {filters.length > 0 && (
        <div className="flex flex-wrap items-center gap-2">
          {filters.map((filter, index) => (
            <span
              key={index}
              className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full bg-secondary text-sm"
            >
              <span className="font-medium">{filter.column}</span>
              <span className="text-muted-foreground">{filter.operator}</span>
              <span>{filter.value}</span>
              <button
                onClick={() =>
                  onFiltersChange?.(filters.filter((_, i) => i !== index))
                }
                className="p-0.5 rounded hover:bg-secondary-foreground/10"
              >
                <X className="w-3 h-3" />
              </button>
            </span>
          ))}
          <button
            onClick={() => onFiltersChange?.([])}
            className="text-sm text-muted-foreground hover:text-foreground"
          >
            Clear all
          </button>
        </div>
      )}

      {/* Error State */}
      {error && (
        <div className="p-4 rounded-xl bg-error/10 border border-error/20 text-error text-sm">
          {error}
        </div>
      )}

      {/* Table */}
      <div
        className={cn(
          'overflow-x-auto rounded-xl bg-card',
          bordered ? 'border border-border' : 'shadow-sm'
        )}
      >
        <table className="w-full text-sm">
          <thead
            className={cn(
              stickyHeader && 'sticky top-0 z-10 bg-card shadow-sm'
            )}
          >
            <tr className="border-b border-border">
              {/* Select all checkbox */}
              {selectable && (
                <th className="w-12 px-4 py-3">
                  <input
                    type="checkbox"
                    checked={isAllSelected}
                    ref={(el) => {
                      if (el) el.indeterminate = isSomeSelected;
                    }}
                    onChange={handleSelectAll}
                    className="rounded border-border text-primary focus:ring-primary"
                  />
                </th>
              )}
              {visibleColumns.map((column) => (
                <th
                  key={column.id}
                  className={cn(
                    'px-4 text-left text-xs font-semibold text-muted-foreground uppercase tracking-wider',
                    compact ? 'py-2' : 'py-3',
                    column.sortable && 'cursor-pointer select-none hover:text-foreground',
                    column.align === 'center' && 'text-center',
                    column.align === 'right' && 'text-right',
                    column.sticky === 'left' && 'sticky left-0 bg-card z-20',
                    column.sticky === 'right' && 'sticky right-0 bg-card z-20'
                  )}
                  style={{
                    width: column.width,
                    minWidth: column.minWidth,
                  }}
                  onClick={() => column.sortable && handleSort(column.id)}
                >
                  <div className="flex items-center gap-1.5">
                    {column.header}
                    {column.sortable && sortColumn === column.id && (
                      sortDirection === 'asc' ? (
                        <ChevronUp className="w-3.5 h-3.5" />
                      ) : sortDirection === 'desc' ? (
                        <ChevronDown className="w-3.5 h-3.5" />
                      ) : null
                    )}
                    {column.sortable && sortColumn !== column.id && (
                      <ChevronDown className="w-3.5 h-3.5 opacity-0 group-hover:opacity-30" />
                    )}
                  </div>
                </th>
              ))}
              {/* Row actions column */}
              {rowActions && <th className="w-12 px-4 py-3" />}
            </tr>
          </thead>
          <tbody className="divide-y divide-border">
            {isLoading ? (
              <tr>
                <td
                  colSpan={visibleColumns.length + (selectable ? 1 : 0) + (rowActions ? 1 : 0)}
                  className="py-12"
                >
                  <div className="flex flex-col items-center justify-center gap-2 text-muted-foreground">
                    <Loader2 className="w-6 h-6 animate-spin text-primary" />
                    <span>Loading data...</span>
                  </div>
                </td>
              </tr>
            ) : data.length === 0 ? (
              <tr>
                <td
                  colSpan={visibleColumns.length + (selectable ? 1 : 0) + (rowActions ? 1 : 0)}
                  className="py-12"
                >
                  {searchValue || filters.length > 0
                    ? emptySearchState || (
                        <div className="flex flex-col items-center justify-center text-center">
                          <Search className="w-10 h-10 text-muted-foreground/50 mb-3" />
                          <p className="font-medium text-foreground">No results found</p>
                          <p className="text-sm text-muted-foreground mt-1">
                            Try adjusting your search or filters
                          </p>
                        </div>
                      )
                    : emptyState || (
                        <div className="flex flex-col items-center justify-center text-center">
                          <p className="text-muted-foreground">No data available</p>
                        </div>
                      )}
                </td>
              </tr>
            ) : (
              data.map((item, index) => {
                const key = getRowKey ? getRowKey(item) : index.toString();
                const isSelected = selectedRows.includes(key);

                return (
                  <tr
                    key={key}
                    onClick={() => onRowClick?.(item)}
                    className={cn(
                      'transition-colors',
                      onRowClick && 'cursor-pointer',
                      highlightOnHover && 'hover:bg-secondary/50',
                      striped && index % 2 === 1 && 'bg-secondary/20',
                      isSelected && 'bg-primary/5',
                      getRowClassName?.(item, index)
                    )}
                  >
                    {/* Row checkbox */}
                    {selectable && (
                      <td className="w-12 px-4 py-3" onClick={(e) => e.stopPropagation()}>
                        <input
                          type="checkbox"
                          checked={isSelected}
                          onChange={() => handleSelectRow(item)}
                          className="rounded border-border text-primary focus:ring-primary"
                        />
                      </td>
                    )}
                    {visibleColumns.map((column) => {
                      const value = column.accessorKey
                        ? item[column.accessorKey]
                        : undefined;
                      const content = column.cell
                        ? column.cell(item, index)
                        : value;

                      return (
                        <td
                          key={column.id}
                          className={cn(
                            'px-4 text-foreground',
                            compact ? 'py-2' : 'py-3',
                            column.align === 'center' && 'text-center',
                            column.align === 'right' && 'text-right',
                            column.sticky === 'left' && 'sticky left-0 bg-card',
                            column.sticky === 'right' && 'sticky right-0 bg-card'
                          )}
                        >
                          {content}
                        </td>
                      );
                    })}
                    {/* Row actions */}
                    {rowActions && (
                      <td className="w-12 px-4 py-3" onClick={(e) => e.stopPropagation()}>
                        <DropdownMenu>
                          <DropdownMenuTrigger asChild>
                            <button className="p-1 rounded hover:bg-secondary">
                              <MoreHorizontal className="w-4 h-4 text-muted-foreground" />
                            </button>
                          </DropdownMenuTrigger>
                          <DropdownMenuContent align="end">
                            {rowActions(item)}
                          </DropdownMenuContent>
                        </DropdownMenu>
                      </td>
                    )}
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>

      {/* Pagination */}
      {pagination && pagination.totalPages > 0 && (
        <div className="flex flex-col sm:flex-row items-center justify-between gap-4 px-2">
          <div className="flex items-center gap-4 text-sm text-muted-foreground">
            <span>
              Showing {((pagination.page - 1) * pagination.perPage) + 1} to{' '}
              {Math.min(pagination.page * pagination.perPage, pagination.totalItems)} of{' '}
              {pagination.totalItems} results
            </span>
            {pagination.onPerPageChange && (
              <div className="flex items-center gap-2">
                <span>Rows per page:</span>
                <select
                  value={pagination.perPage}
                  onChange={(e) => pagination.onPerPageChange?.(Number(e.target.value))}
                  className="px-2 py-1 rounded-md border border-border bg-background text-foreground text-sm focus:outline-none focus:ring-2 focus:ring-primary/30"
                >
                  {(pagination.perPageOptions || [10, 25, 50, 100]).map((opt) => (
                    <option key={opt} value={opt}>
                      {opt}
                    </option>
                  ))}
                </select>
              </div>
            )}
          </div>

          <div className="flex items-center gap-1">
            <Button
              variant="outline"
              size="sm"
              onClick={() => pagination.onPageChange(1)}
              disabled={pagination.page === 1}
            >
              <ChevronsLeft className="w-4 h-4" />
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() => pagination.onPageChange(pagination.page - 1)}
              disabled={pagination.page === 1}
            >
              <ChevronLeft className="w-4 h-4" />
            </Button>

            <div className="flex items-center gap-1 px-2">
              {/* Page numbers */}
              {Array.from({ length: Math.min(5, pagination.totalPages) }, (_, i) => {
                let pageNum: number;
                if (pagination.totalPages <= 5) {
                  pageNum = i + 1;
                } else if (pagination.page <= 3) {
                  pageNum = i + 1;
                } else if (pagination.page >= pagination.totalPages - 2) {
                  pageNum = pagination.totalPages - 4 + i;
                } else {
                  pageNum = pagination.page - 2 + i;
                }

                return (
                  <Button
                    key={pageNum}
                    variant={pagination.page === pageNum ? 'default' : 'outline'}
                    size="sm"
                    onClick={() => pagination.onPageChange(pageNum)}
                    className="w-8"
                  >
                    {pageNum}
                  </Button>
                );
              })}
            </div>

            <Button
              variant="outline"
              size="sm"
              onClick={() => pagination.onPageChange(pagination.page + 1)}
              disabled={pagination.page >= pagination.totalPages}
            >
              <ChevronRight className="w-4 h-4" />
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() => pagination.onPageChange(pagination.totalPages)}
              disabled={pagination.page >= pagination.totalPages}
            >
              <ChevronsRight className="w-4 h-4" />
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}

// Export column helper for type safety
export function createColumnHelper<T>() {
  return {
    accessor: (
      accessorKey: keyof T,
      column: Omit<ColumnDef<T>, 'id' | 'accessorKey'>
    ): ColumnDef<T> => ({
      id: accessorKey as string,
      accessorKey,
      ...column,
    }),
    display: (column: Omit<ColumnDef<T>, 'accessorKey'>): ColumnDef<T> => ({
      ...column,
    }),
  };
}
