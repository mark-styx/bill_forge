'use client';

import * as React from 'react';
import * as CheckboxPrimitive from '@radix-ui/react-checkbox';
import { Check, Minus } from 'lucide-react';
import { cn } from '@/lib/utils';

interface CheckboxProps
  extends React.ComponentPropsWithoutRef<typeof CheckboxPrimitive.Root> {
  indeterminate?: boolean;
}

const Checkbox = React.forwardRef<
  React.ElementRef<typeof CheckboxPrimitive.Root>,
  CheckboxProps
>(({ className, indeterminate, ...props }, ref) => (
  <CheckboxPrimitive.Root
    ref={ref}
    className={cn(
      'peer h-5 w-5 shrink-0 rounded-md border-2 border-input',
      'ring-offset-background',
      'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2',
      'disabled:cursor-not-allowed disabled:opacity-50',
      'data-[state=checked]:bg-primary data-[state=checked]:border-primary data-[state=checked]:text-primary-foreground',
      'data-[state=indeterminate]:bg-primary data-[state=indeterminate]:border-primary data-[state=indeterminate]:text-primary-foreground',
      'transition-all duration-200',
      className
    )}
    {...props}
  >
    <CheckboxPrimitive.Indicator
      className={cn('flex items-center justify-center text-current')}
    >
      {indeterminate ? (
        <Minus className="h-3.5 w-3.5" />
      ) : (
        <Check className="h-3.5 w-3.5" />
      )}
    </CheckboxPrimitive.Indicator>
  </CheckboxPrimitive.Root>
));
Checkbox.displayName = CheckboxPrimitive.Root.displayName;

// Checkbox with label
interface CheckboxWithLabelProps extends CheckboxProps {
  label: string;
  description?: string;
  labelClassName?: string;
}

const CheckboxWithLabel = React.forwardRef<
  React.ElementRef<typeof CheckboxPrimitive.Root>,
  CheckboxWithLabelProps
>(({ label, description, labelClassName, id, ...props }, ref) => {
  const checkboxId = id || React.useId();

  return (
    <div className="flex items-start gap-3">
      <Checkbox ref={ref} id={checkboxId} {...props} />
      <div className="grid gap-0.5 leading-none">
        <label
          htmlFor={checkboxId}
          className={cn(
            'text-sm font-medium text-foreground leading-none',
            'peer-disabled:cursor-not-allowed peer-disabled:opacity-70',
            'cursor-pointer',
            labelClassName
          )}
        >
          {label}
        </label>
        {description && (
          <p className="text-sm text-muted-foreground">{description}</p>
        )}
      </div>
    </div>
  );
});
CheckboxWithLabel.displayName = 'CheckboxWithLabel';

// Checkbox group
interface CheckboxGroupProps {
  options: Array<{
    value: string;
    label: string;
    description?: string;
    disabled?: boolean;
  }>;
  value: string[];
  onChange: (value: string[]) => void;
  className?: string;
  direction?: 'horizontal' | 'vertical';
}

function CheckboxGroup({
  options,
  value,
  onChange,
  className,
  direction = 'vertical',
}: CheckboxGroupProps) {
  const handleChange = (optionValue: string, checked: boolean) => {
    if (checked) {
      onChange([...value, optionValue]);
    } else {
      onChange(value.filter((v) => v !== optionValue));
    }
  };

  return (
    <div
      className={cn(
        direction === 'horizontal'
          ? 'flex flex-wrap gap-4'
          : 'flex flex-col gap-3',
        className
      )}
    >
      {options.map((option) => (
        <CheckboxWithLabel
          key={option.value}
          label={option.label}
          description={option.description}
          checked={value.includes(option.value)}
          disabled={option.disabled}
          onCheckedChange={(checked) =>
            handleChange(option.value, checked === true)
          }
        />
      ))}
    </div>
  );
}

export { Checkbox, CheckboxWithLabel, CheckboxGroup };
