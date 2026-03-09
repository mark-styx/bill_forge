'use client';

import * as React from 'react';
import { Slot } from '@radix-ui/react-slot';
import { cn } from '@/lib/utils';
import { cva, type VariantProps } from 'class-variance-authority';
import { Loader2 } from 'lucide-react';

const gradientButtonVariants = cva(
  'relative inline-flex items-center justify-center whitespace-nowrap font-medium transition-all duration-300 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50 overflow-hidden',
  {
    variants: {
      variant: {
        default: [
          'text-white',
          'bg-gradient-to-r from-primary to-accent',
          'hover:shadow-bright hover:scale-[1.02]',
          'active:scale-[0.98]',
        ],
        vivid: [
          'text-white',
          'bg-gradient-to-r from-blue-600 via-purple-600 to-pink-600',
          'hover:shadow-lg hover:scale-[1.02]',
          'active:scale-[0.98]',
        ],
        sunrise: [
          'text-white',
          'bg-gradient-to-r from-orange-500 via-red-500 to-pink-500',
          'hover:shadow-lg hover:scale-[1.02]',
        ],
        ocean: [
          'text-white',
          'bg-gradient-to-r from-cyan-500 via-blue-500 to-indigo-500',
          'hover:shadow-lg hover:scale-[1.02]',
        ],
        forest: [
          'text-white',
          'bg-gradient-to-r from-emerald-500 via-teal-500 to-cyan-500',
          'hover:shadow-lg hover:scale-[1.02]',
        ],
        outline: [
          'bg-transparent text-foreground',
          'border-2 border-transparent',
          'bg-clip-padding',
          'hover:bg-primary/5',
        ],
        ghost: [
          'bg-transparent',
          'text-primary',
          'hover:bg-gradient-to-r hover:from-primary/10 hover:to-accent/10',
        ],
        capture: [
          'text-white',
          'bg-gradient-to-r from-capture to-cyan-400',
          'hover:shadow-lg hover:scale-[1.02]',
        ],
        processing: [
          'text-white',
          'bg-gradient-to-r from-processing to-teal-400',
          'hover:shadow-lg hover:scale-[1.02]',
        ],
        vendor: [
          'text-white',
          'bg-gradient-to-r from-vendor to-purple-400',
          'hover:shadow-lg hover:scale-[1.02]',
        ],
        reporting: [
          'text-white',
          'bg-gradient-to-r from-reporting to-yellow-400',
          'hover:shadow-lg hover:scale-[1.02]',
        ],
      },
      size: {
        sm: 'h-8 px-4 text-sm rounded-lg',
        md: 'h-10 px-5 text-sm rounded-xl',
        lg: 'h-12 px-6 text-base rounded-xl',
        xl: 'h-14 px-8 text-lg rounded-2xl',
        icon: 'h-10 w-10 rounded-xl',
        'icon-sm': 'h-8 w-8 rounded-lg',
        'icon-lg': 'h-12 w-12 rounded-xl',
      },
      glow: {
        none: '',
        subtle: 'shadow-glow',
        medium: 'shadow-bright',
        strong: 'shadow-neon',
      },
      animated: {
        true: '',
        false: '',
      },
    },
    compoundVariants: [
      {
        variant: 'outline',
        className: [
          'before:absolute before:inset-0 before:rounded-[inherit] before:p-[2px]',
          'before:bg-gradient-to-r before:from-primary before:to-accent',
          'before:-z-10 before:content-[""]',
          'after:absolute after:inset-[2px] after:rounded-[calc(inherit-2px)] after:bg-background',
          'after:-z-10 after:content-[""]',
        ],
      },
    ],
    defaultVariants: {
      variant: 'default',
      size: 'md',
      glow: 'none',
      animated: false,
    },
  }
);

export interface GradientButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof gradientButtonVariants> {
  asChild?: boolean;
  loading?: boolean;
  leftIcon?: React.ReactNode;
  rightIcon?: React.ReactNode;
  shimmer?: boolean;
}

const GradientButton = React.forwardRef<HTMLButtonElement, GradientButtonProps>(
  (
    {
      className,
      variant,
      size,
      glow,
      animated,
      asChild = false,
      loading = false,
      leftIcon,
      rightIcon,
      shimmer = false,
      children,
      disabled,
      ...props
    },
    ref
  ) => {
    const Comp = asChild ? Slot : 'button';
    const isDisabled = disabled || loading;

    return (
      <Comp
        className={cn(
          gradientButtonVariants({ variant, size, glow, animated }),
          animated && 'animate-gradient-shift bg-[length:200%_200%]',
          className
        )}
        ref={ref}
        disabled={isDisabled}
        {...props}
      >
        {/* Shimmer effect overlay */}
        {shimmer && !loading && (
          <span className="absolute inset-0 overflow-hidden rounded-[inherit]">
            <span className="absolute inset-0 -translate-x-full animate-[shimmer_2s_infinite] bg-gradient-to-r from-transparent via-white/20 to-transparent" />
          </span>
        )}

        {/* Content */}
        <span className="relative flex items-center gap-2">
          {loading && <Loader2 className="h-4 w-4 animate-spin" />}
          {!loading && leftIcon && <span>{leftIcon}</span>}
          {children}
          {!loading && rightIcon && <span>{rightIcon}</span>}
        </span>
      </Comp>
    );
  }
);
GradientButton.displayName = 'GradientButton';

// Icon Button with gradient background
interface GradientIconButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    Pick<VariantProps<typeof gradientButtonVariants>, 'variant' | 'glow'> {
  icon: React.ReactNode;
  size?: 'sm' | 'md' | 'lg';
  tooltip?: string;
}

const GradientIconButton = React.forwardRef<HTMLButtonElement, GradientIconButtonProps>(
  ({ className, variant = 'default', glow, icon, size = 'md', tooltip, ...props }, ref) => {
    const sizeClasses = {
      sm: 'w-8 h-8 rounded-lg',
      md: 'w-10 h-10 rounded-xl',
      lg: 'w-12 h-12 rounded-xl',
    };

    const iconSizes = {
      sm: 'w-4 h-4',
      md: 'w-5 h-5',
      lg: 'w-6 h-6',
    };

    return (
      <button
        ref={ref}
        className={cn(
          'inline-flex items-center justify-center transition-all duration-300',
          'text-white hover:scale-105 active:scale-95',
          'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2',
          'disabled:pointer-events-none disabled:opacity-50',
          variant === 'default' && 'bg-gradient-to-r from-primary to-accent',
          variant === 'vivid' && 'bg-gradient-to-r from-blue-600 via-purple-600 to-pink-600',
          variant === 'ocean' && 'bg-gradient-to-r from-cyan-500 via-blue-500 to-indigo-500',
          variant === 'capture' && 'bg-gradient-to-r from-capture to-cyan-400',
          variant === 'processing' && 'bg-gradient-to-r from-processing to-teal-400',
          variant === 'vendor' && 'bg-gradient-to-r from-vendor to-purple-400',
          variant === 'reporting' && 'bg-gradient-to-r from-reporting to-yellow-400',
          glow === 'subtle' && 'shadow-glow',
          glow === 'medium' && 'shadow-bright',
          glow === 'strong' && 'shadow-neon',
          sizeClasses[size],
          className
        )}
        title={tooltip}
        {...props}
      >
        <span className={iconSizes[size]}>{icon}</span>
      </button>
    );
  }
);
GradientIconButton.displayName = 'GradientIconButton';

// Animated Gradient Border Button
const AnimatedBorderButton = React.forwardRef<
  HTMLButtonElement,
  React.ButtonHTMLAttributes<HTMLButtonElement> & {
    size?: 'sm' | 'md' | 'lg';
    gradientSpeed?: 'slow' | 'medium' | 'fast';
  }
>(({ className, size = 'md', gradientSpeed = 'medium', children, ...props }, ref) => {
  const sizeClasses = {
    sm: 'h-8 px-4 text-sm',
    md: 'h-10 px-5 text-sm',
    lg: 'h-12 px-6 text-base',
  };

  const speedClasses = {
    slow: 'animate-[gradient-shift_6s_ease_infinite]',
    medium: 'animate-[gradient-shift_4s_ease_infinite]',
    fast: 'animate-[gradient-shift_2s_ease_infinite]',
  };

  return (
    <button
      ref={ref}
      className={cn(
        'relative inline-flex items-center justify-center font-medium rounded-xl',
        'transition-all duration-300 hover:scale-[1.02] active:scale-[0.98]',
        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2',
        'disabled:pointer-events-none disabled:opacity-50',
        sizeClasses[size],
        className
      )}
      {...props}
    >
      {/* Animated gradient border */}
      <span
        className={cn(
          'absolute inset-0 rounded-xl p-[2px]',
          'bg-gradient-to-r from-primary via-accent via-capture to-primary',
          'bg-[length:300%_300%]',
          speedClasses[gradientSpeed]
        )}
      >
        <span className="absolute inset-[2px] rounded-[10px] bg-background" />
      </span>

      {/* Content */}
      <span className="relative text-foreground">{children}</span>
    </button>
  );
});
AnimatedBorderButton.displayName = 'AnimatedBorderButton';

// Renamed to avoid conflict with gradient-card.tsx exports
export {
  GradientButton as ModernGradientButton,
  GradientIconButton,
  AnimatedBorderButton,
  gradientButtonVariants as modernGradientButtonVariants,
};
