'use client';

import * as React from 'react';
import * as Popover from '@radix-ui/react-popover';
import { cn } from '@/lib/utils';
import { Check, ChevronDown, Search, X, Loader2 } from 'lucide-react';
import { Badge } from './badge';

export interface MultiSelectOption {
  value: string;
  label: string;
  description?: string;
  icon?: React.ReactNode;
  disabled?: boolean;
  group?: string;
}

export interface MultiSelectProps {
  options: MultiSelectOption[];
  value: string[];
  onChange: (value: string[]) => void;
  placeholder?: string;
  searchPlaceholder?: string;
  emptyText?: string;
  className?: string;
  disabled?: boolean;
  isLoading?: boolean;
  maxSelected?: number;
  showSelectedCount?: boolean;
  clearable?: boolean;
  searchable?: boolean;
  creatable?: boolean;
  onCreate?: (value: string) => void;
  renderOption?: (option: MultiSelectOption, selected: boolean) => React.ReactNode;
  renderSelectedItem?: (option: MultiSelectOption, onRemove: () => void) => React.ReactNode;
  groupBy?: boolean;
  variant?: 'default' | 'pills' | 'tags';
  size?: 'sm' | 'default' | 'lg';
}

export function MultiSelect({
  options,
  value,
  onChange,
  placeholder = 'Select options...',
  searchPlaceholder = 'Search...',
  emptyText = 'No options found',
  className,
  disabled = false,
  isLoading = false,
  maxSelected,
  showSelectedCount = false,
  clearable = true,
  searchable = true,
  creatable = false,
  onCreate,
  renderOption,
  renderSelectedItem,
  groupBy = false,
  variant = 'default',
  size = 'default',
}: MultiSelectProps) {
  const [open, setOpen] = React.useState(false);
  const [search, setSearch] = React.useState('');
  const inputRef = React.useRef<HTMLInputElement>(null);

  const selectedOptions = options.filter((opt) => value.includes(opt.value));

  const filteredOptions = React.useMemo(() => {
    let filtered = options;
    if (search) {
      const searchLower = search.toLowerCase();
      filtered = options.filter(
        (opt) =>
          opt.label.toLowerCase().includes(searchLower) ||
          opt.description?.toLowerCase().includes(searchLower)
      );
    }
    return filtered;
  }, [options, search]);

  const groupedOptions = React.useMemo(() => {
    if (!groupBy) return { '': filteredOptions };
    return filteredOptions.reduce((acc, opt) => {
      const group = opt.group || '';
      if (!acc[group]) acc[group] = [];
      acc[group].push(opt);
      return acc;
    }, {} as Record<string, MultiSelectOption[]>);
  }, [filteredOptions, groupBy]);

  const handleSelect = (optionValue: string) => {
    if (value.includes(optionValue)) {
      onChange(value.filter((v) => v !== optionValue));
    } else {
      if (maxSelected && value.length >= maxSelected) return;
      onChange([...value, optionValue]);
    }
  };

  const handleRemove = (optionValue: string) => {
    onChange(value.filter((v) => v !== optionValue));
  };

  const handleClear = (e: React.MouseEvent) => {
    e.stopPropagation();
    onChange([]);
  };

  const handleCreate = () => {
    if (!search.trim() || !creatable) return;
    onCreate?.(search.trim());
    setSearch('');
  };

  const canCreate =
    creatable &&
    search.trim() &&
    !options.some((opt) => opt.label.toLowerCase() === search.toLowerCase());

  const sizeClasses = {
    sm: 'min-h-8 text-sm',
    default: 'min-h-10',
    lg: 'min-h-12 text-base',
  };

  const badgeSize = size === 'sm' ? 'sm' : size === 'lg' ? 'lg' : 'default';

  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger asChild disabled={disabled}>
        <button
          type="button"
          role="combobox"
          aria-expanded={open}
          aria-haspopup="listbox"
          disabled={disabled}
          className={cn(
            'flex w-full items-center justify-between rounded-xl border border-input bg-background px-3 py-2 ring-offset-background transition-colors',
            'focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2',
            'disabled:cursor-not-allowed disabled:opacity-50',
            'hover:border-primary/50',
            open && 'border-primary ring-2 ring-ring ring-offset-2',
            sizeClasses[size],
            className
          )}
        >
          <div className="flex flex-wrap items-center gap-1.5 flex-1">
            {selectedOptions.length === 0 ? (
              <span className="text-muted-foreground">{placeholder}</span>
            ) : showSelectedCount ? (
              <span className="text-foreground">
                {selectedOptions.length} selected
              </span>
            ) : variant === 'pills' ? (
              selectedOptions.slice(0, 3).map((opt) =>
                renderSelectedItem ? (
                  <React.Fragment key={opt.value}>
                    {renderSelectedItem(opt, () => handleRemove(opt.value))}
                  </React.Fragment>
                ) : (
                  <Badge
                    key={opt.value}
                    variant="secondary"
                    className="gap-1 pr-1"
                  >
                    {opt.icon && <span className="w-3.5 h-3.5">{opt.icon}</span>}
                    {opt.label}
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        handleRemove(opt.value);
                      }}
                      className="ml-0.5 p-0.5 rounded hover:bg-secondary-foreground/20"
                    >
                      <X className="w-3 h-3" />
                    </button>
                  </Badge>
                )
              )
            ) : (
              <span className="text-foreground truncate">
                {selectedOptions.map((opt) => opt.label).join(', ')}
              </span>
            )}
            {variant === 'pills' && selectedOptions.length > 3 && (
              <Badge variant="secondary">+{selectedOptions.length - 3}</Badge>
            )}
          </div>

          <div className="flex items-center gap-1.5">
            {clearable && selectedOptions.length > 0 && (
              <button
                type="button"
                onClick={handleClear}
                className="p-0.5 rounded hover:bg-secondary"
              >
                <X className="w-4 h-4 text-muted-foreground" />
              </button>
            )}
            <ChevronDown
              className={cn(
                'w-4 h-4 text-muted-foreground transition-transform',
                open && 'rotate-180'
              )}
            />
          </div>
        </button>
      </Popover.Trigger>

      <Popover.Portal>
        <Popover.Content
          className="z-50 w-[var(--radix-popover-trigger-width)] min-w-[200px] max-h-[300px] overflow-hidden rounded-xl border border-border bg-popover shadow-lg animate-scale-in"
          sideOffset={4}
          align="start"
        >
          {/* Search */}
          {searchable && (
            <div className="p-2 border-b border-border">
              <div className="relative">
                <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
                <input
                  ref={inputRef}
                  type="text"
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  placeholder={searchPlaceholder}
                  className="w-full pl-8 pr-3 py-1.5 bg-secondary/50 border-0 rounded-lg text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/30"
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' && canCreate) {
                      e.preventDefault();
                      handleCreate();
                    }
                  }}
                />
              </div>
            </div>
          )}

          {/* Options */}
          <div className="max-h-[200px] overflow-y-auto p-1">
            {isLoading ? (
              <div className="flex items-center justify-center py-6 text-muted-foreground">
                <Loader2 className="w-5 h-5 animate-spin mr-2" />
                Loading...
              </div>
            ) : filteredOptions.length === 0 && !canCreate ? (
              <div className="py-6 text-center text-sm text-muted-foreground">
                {emptyText}
              </div>
            ) : (
              <>
                {Object.entries(groupedOptions).map(([group, opts]) => (
                  <div key={group || 'ungrouped'}>
                    {group && (
                      <div className="px-2 py-1.5 text-xs font-semibold text-muted-foreground uppercase tracking-wider">
                        {group}
                      </div>
                    )}
                    {opts.map((option) => {
                      const isSelected = value.includes(option.value);
                      const isDisabled =
                        option.disabled ||
                        (!isSelected && !!maxSelected && value.length >= maxSelected);

                      return (
                        <button
                          key={option.value}
                          type="button"
                          disabled={isDisabled}
                          onClick={() => handleSelect(option.value)}
                          className={cn(
                            'flex w-full items-center gap-2 px-2 py-2 rounded-lg text-sm transition-colors',
                            'hover:bg-secondary focus:bg-secondary focus:outline-none',
                            isSelected && 'bg-primary/10 text-primary',
                            isDisabled && 'opacity-50 cursor-not-allowed'
                          )}
                        >
                          <div
                            className={cn(
                              'flex items-center justify-center w-4 h-4 rounded border transition-colors',
                              isSelected
                                ? 'border-primary bg-primary text-primary-foreground'
                                : 'border-border'
                            )}
                          >
                            {isSelected && <Check className="w-3 h-3" />}
                          </div>
                          {renderOption ? (
                            renderOption(option, isSelected)
                          ) : (
                            <div className="flex-1 text-left">
                              <div className="flex items-center gap-2">
                                {option.icon && (
                                  <span className="w-4 h-4">{option.icon}</span>
                                )}
                                <span className="font-medium">{option.label}</span>
                              </div>
                              {option.description && (
                                <p className="text-xs text-muted-foreground mt-0.5">
                                  {option.description}
                                </p>
                              )}
                            </div>
                          )}
                        </button>
                      );
                    })}
                  </div>
                ))}

                {/* Create option */}
                {canCreate && (
                  <button
                    type="button"
                    onClick={handleCreate}
                    className="flex w-full items-center gap-2 px-2 py-2 rounded-lg text-sm transition-colors hover:bg-secondary focus:bg-secondary focus:outline-none text-primary"
                  >
                    <span className="w-4 h-4 flex items-center justify-center text-lg">
                      +
                    </span>
                    <span>Create "{search}"</span>
                  </button>
                )}
              </>
            )}
          </div>

          {/* Footer with selection info */}
          {maxSelected && (
            <div className="px-3 py-2 border-t border-border text-xs text-muted-foreground">
              {value.length} of {maxSelected} selected
            </div>
          )}
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

// Combobox variant for single select with search
export interface ComboboxProps
  extends Omit<MultiSelectProps, 'value' | 'onChange' | 'maxSelected' | 'showSelectedCount' | 'variant'> {
  value: string | null;
  onChange: (value: string | null) => void;
}

export function Combobox({
  options,
  value,
  onChange,
  placeholder = 'Select...',
  ...props
}: ComboboxProps) {
  const handleChange = (values: string[]) => {
    onChange(values[0] || null);
  };

  return (
    <MultiSelect
      {...props}
      options={options}
      value={value ? [value] : []}
      onChange={handleChange}
      placeholder={placeholder}
      maxSelected={1}
      showSelectedCount={false}
      variant="default"
    />
  );
}
