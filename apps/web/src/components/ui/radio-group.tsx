'use client';

import * as React from 'react';
import * as RadioGroupPrimitive from '@radix-ui/react-radio-group';
import { Circle } from 'lucide-react';
import { cn } from '@/lib/utils';

const RadioGroup = React.forwardRef<
  React.ElementRef<typeof RadioGroupPrimitive.Root>,
  React.ComponentPropsWithoutRef<typeof RadioGroupPrimitive.Root>
>(({ className, ...props }, ref) => {
  return (
    <RadioGroupPrimitive.Root
      className={cn('grid gap-3', className)}
      {...props}
      ref={ref}
    />
  );
});
RadioGroup.displayName = RadioGroupPrimitive.Root.displayName;

const RadioGroupItem = React.forwardRef<
  React.ElementRef<typeof RadioGroupPrimitive.Item>,
  React.ComponentPropsWithoutRef<typeof RadioGroupPrimitive.Item>
>(({ className, ...props }, ref) => {
  return (
    <RadioGroupPrimitive.Item
      ref={ref}
      className={cn(
        'aspect-square h-5 w-5 rounded-full border-2 border-input',
        'text-primary ring-offset-background',
        'focus:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2',
        'disabled:cursor-not-allowed disabled:opacity-50',
        'data-[state=checked]:border-primary data-[state=checked]:bg-primary/10',
        'transition-all duration-200',
        className
      )}
      {...props}
    >
      <RadioGroupPrimitive.Indicator className="flex items-center justify-center">
        <Circle className="h-2.5 w-2.5 fill-primary text-primary" />
      </RadioGroupPrimitive.Indicator>
    </RadioGroupPrimitive.Item>
  );
});
RadioGroupItem.displayName = RadioGroupPrimitive.Item.displayName;

// Radio with label
interface RadioWithLabelProps
  extends React.ComponentPropsWithoutRef<typeof RadioGroupPrimitive.Item> {
  label: string;
  description?: string;
}

const RadioWithLabel = React.forwardRef<
  React.ElementRef<typeof RadioGroupPrimitive.Item>,
  RadioWithLabelProps
>(({ label, description, id, ...props }, ref) => {
  const radioId = id || React.useId();

  return (
    <div className="flex items-start gap-3">
      <RadioGroupItem ref={ref} id={radioId} {...props} />
      <div className="grid gap-0.5 leading-none">
        <label
          htmlFor={radioId}
          className={cn(
            'text-sm font-medium text-foreground leading-none',
            'peer-disabled:cursor-not-allowed peer-disabled:opacity-70',
            'cursor-pointer'
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
RadioWithLabel.displayName = 'RadioWithLabel';

// Radio card for selection with visual
interface RadioCardProps
  extends React.ComponentPropsWithoutRef<typeof RadioGroupPrimitive.Item> {
  label: string;
  description?: string;
  icon?: React.ReactNode;
}

const RadioCard = React.forwardRef<
  React.ElementRef<typeof RadioGroupPrimitive.Item>,
  RadioCardProps
>(({ label, description, icon, className, ...props }, ref) => {
  return (
    <RadioGroupPrimitive.Item
      ref={ref}
      className={cn(
        'relative flex items-start gap-4 p-4 rounded-xl border-2 border-border',
        'ring-offset-background transition-all cursor-pointer',
        'focus:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2',
        'data-[state=checked]:border-primary data-[state=checked]:bg-primary/5',
        'hover:border-primary/50 hover:bg-secondary/30',
        'disabled:cursor-not-allowed disabled:opacity-50',
        className
      )}
      {...props}
    >
      {icon && (
        <div className="flex-shrink-0 w-10 h-10 rounded-lg bg-secondary flex items-center justify-center text-muted-foreground data-[state=checked]:bg-primary/10 data-[state=checked]:text-primary">
          {icon}
        </div>
      )}
      <div className="flex-1 min-w-0">
        <p className="font-medium text-foreground">{label}</p>
        {description && (
          <p className="text-sm text-muted-foreground mt-0.5">{description}</p>
        )}
      </div>
      <div className="flex-shrink-0 mt-0.5">
        <div
          className={cn(
            'h-5 w-5 rounded-full border-2 border-input',
            'data-[state=checked]:border-primary data-[state=checked]:bg-primary/10',
            'flex items-center justify-center transition-all'
          )}
        >
          <RadioGroupPrimitive.Indicator>
            <Circle className="h-2.5 w-2.5 fill-primary text-primary" />
          </RadioGroupPrimitive.Indicator>
        </div>
      </div>
    </RadioGroupPrimitive.Item>
  );
});
RadioCard.displayName = 'RadioCard';

// Compact radio group as buttons
interface RadioButtonGroupProps {
  options: Array<{ value: string; label: string; disabled?: boolean }>;
  value: string;
  onChange: (value: string) => void;
  className?: string;
  size?: 'sm' | 'md' | 'lg';
}

function RadioButtonGroup({
  options,
  value,
  onChange,
  className,
  size = 'md',
}: RadioButtonGroupProps) {
  const sizeClasses = {
    sm: 'px-3 py-1.5 text-xs',
    md: 'px-4 py-2 text-sm',
    lg: 'px-5 py-2.5 text-base',
  };

  return (
    <div
      className={cn(
        'inline-flex items-center rounded-lg border border-border bg-secondary/30 p-1',
        className
      )}
    >
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          disabled={option.disabled}
          onClick={() => onChange(option.value)}
          className={cn(
            'rounded-md font-medium transition-all',
            sizeClasses[size],
            value === option.value
              ? 'bg-background text-foreground shadow-sm'
              : 'text-muted-foreground hover:text-foreground',
            option.disabled && 'opacity-50 cursor-not-allowed'
          )}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

export {
  RadioGroup,
  RadioGroupItem,
  RadioWithLabel,
  RadioCard,
  RadioButtonGroup,
};
