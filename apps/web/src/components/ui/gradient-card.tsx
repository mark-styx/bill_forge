'use client';

import * as React from 'react';
import { cn } from '@/lib/utils';
import { cva, type VariantProps } from 'class-variance-authority';

const gradientCardVariants = cva(
  'relative overflow-hidden rounded-xl border border-border bg-card text-card-foreground',
  {
    variants: {
      variant: {
        default: '',
        glow: 'shadow-glow',
        elevated: 'shadow-soft-lg',
        glass: 'bg-card/60 backdrop-blur-xl border-white/10',
      },
      gradient: {
        none: '',
        primary: '',
        capture: '',
        processing: '',
        vendor: '',
        reporting: '',
        rainbow: '',
        aurora: '',
        sunset: '',
        ocean: '',
        custom: '',
      },
      gradientPosition: {
        top: '',
        bottom: '',
        left: '',
        right: '',
        background: '',
        border: '',
      },
      hover: {
        true: 'transition-all duration-300 hover:shadow-lg hover:-translate-y-0.5 cursor-pointer',
        false: '',
      },
      animated: {
        true: '',
        false: '',
      },
    },
    defaultVariants: {
      variant: 'default',
      gradient: 'none',
      gradientPosition: 'top',
      hover: false,
      animated: false,
    },
  }
);

interface GradientCardProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof gradientCardVariants> {
  gradientColors?: {
    from: string;
    via?: string;
    to: string;
    angle?: number;
  };
}

const getGradientStyle = (
  gradient: string | null | undefined,
  gradientColors?: GradientCardProps['gradientColors']
): string => {
  if (gradientColors) {
    const { from, via, to, angle = 135 } = gradientColors;
    if (via) {
      return `linear-gradient(${angle}deg, hsl(${from}), hsl(${via}), hsl(${to}))`;
    }
    return `linear-gradient(${angle}deg, hsl(${from}), hsl(${to}))`;
  }

  const gradients: Record<string, string> = {
    primary: 'linear-gradient(135deg, hsl(var(--primary)), hsl(var(--accent)))',
    capture: 'linear-gradient(135deg, hsl(var(--capture)), hsl(195 100% 55%))',
    processing: 'linear-gradient(135deg, hsl(var(--processing)), hsl(160 84% 55%))',
    vendor: 'linear-gradient(135deg, hsl(var(--vendor)), hsl(280 70% 60%))',
    reporting: 'linear-gradient(135deg, hsl(var(--reporting)), hsl(45 95% 55%))',
    rainbow: 'linear-gradient(135deg, hsl(0 100% 55%), hsl(60 100% 55%), hsl(120 100% 45%), hsl(180 100% 50%), hsl(240 100% 60%), hsl(300 100% 55%))',
    aurora: 'linear-gradient(135deg, hsl(150 80% 50%), hsl(190 100% 50%), hsl(280 80% 55%))',
    sunset: 'linear-gradient(135deg, hsl(25 95% 55%), hsl(340 80% 55%))',
    ocean: 'linear-gradient(135deg, hsl(210 100% 50%), hsl(190 95% 45%))',
  };

  return gradients[gradient || ''] || '';
};

const GradientCard = React.forwardRef<HTMLDivElement, GradientCardProps>(
  (
    {
      className,
      variant,
      gradient,
      gradientPosition,
      hover,
      animated,
      gradientColors,
      children,
      style,
      ...props
    },
    ref
  ) => {
    const gradientStyle = getGradientStyle(gradient, gradientColors);
    const hasGradient = gradient && gradient !== 'none';

    return (
      <div
        ref={ref}
        className={cn(gradientCardVariants({ variant, gradient, gradientPosition, hover, animated }), className)}
        style={style}
        {...props}
      >
        {/* Gradient Bar/Background */}
        {hasGradient && gradientPosition === 'top' && (
          <div
            className={cn('h-1', animated && 'animate-pulse-soft')}
            style={{ background: gradientStyle }}
          />
        )}
        {hasGradient && gradientPosition === 'bottom' && (
          <div
            className={cn('absolute bottom-0 left-0 right-0 h-1', animated && 'animate-pulse-soft')}
            style={{ background: gradientStyle }}
          />
        )}
        {hasGradient && gradientPosition === 'left' && (
          <div
            className={cn('absolute top-0 bottom-0 left-0 w-1', animated && 'animate-pulse-soft')}
            style={{ background: gradientStyle }}
          />
        )}
        {hasGradient && gradientPosition === 'right' && (
          <div
            className={cn('absolute top-0 bottom-0 right-0 w-1', animated && 'animate-pulse-soft')}
            style={{ background: gradientStyle }}
          />
        )}
        {hasGradient && gradientPosition === 'background' && (
          <div
            className={cn(
              'absolute inset-0 opacity-10',
              animated && 'animate-pulse-soft'
            )}
            style={{ background: gradientStyle }}
          />
        )}
        {hasGradient && gradientPosition === 'border' && (
          <div
            className="absolute inset-0 rounded-xl p-[1px]"
            style={{ background: gradientStyle }}
          >
            <div className="h-full w-full rounded-xl bg-card" />
          </div>
        )}

        {/* Content */}
        <div className={cn('relative', gradientPosition === 'border' && 'z-10')}>
          {children}
        </div>
      </div>
    );
  }
);
GradientCard.displayName = 'GradientCard';

// Animated gradient border component
interface AnimatedGradientBorderProps extends React.HTMLAttributes<HTMLDivElement> {
  gradientColors?: string[];
  borderWidth?: number;
  animationDuration?: number;
}

const AnimatedGradientBorder = React.forwardRef<HTMLDivElement, AnimatedGradientBorderProps>(
  (
    {
      className,
      gradientColors = ['hsl(var(--primary))', 'hsl(var(--accent))', 'hsl(var(--capture))', 'hsl(var(--processing))'],
      borderWidth = 2,
      animationDuration = 4,
      children,
      ...props
    },
    ref
  ) => {
    const gradientString = gradientColors.join(', ');

    return (
      <div
        ref={ref}
        className={cn('relative rounded-xl', className)}
        {...props}
      >
        <div
          className="absolute inset-0 rounded-xl"
          style={{
            background: `linear-gradient(90deg, ${gradientString}, ${gradientColors[0]})`,
            backgroundSize: '200% 100%',
            animation: `shimmer ${animationDuration}s linear infinite`,
            padding: borderWidth,
          }}
        >
          <div className="h-full w-full rounded-xl bg-card" />
        </div>
        <div className="relative z-10">{children}</div>
      </div>
    );
  }
);
AnimatedGradientBorder.displayName = 'AnimatedGradientBorder';

// Gradient text component
interface GradientTextProps extends React.HTMLAttributes<HTMLSpanElement> {
  gradient?: 'primary' | 'capture' | 'processing' | 'vendor' | 'reporting' | 'rainbow' | 'custom';
  gradientColors?: {
    from: string;
    via?: string;
    to: string;
    angle?: number;
  };
  animated?: boolean;
}

const GradientText = React.forwardRef<HTMLSpanElement, GradientTextProps>(
  ({ className, gradient = 'primary', gradientColors, animated = false, children, style, ...props }, ref) => {
    const gradientStyle = getGradientStyle(gradient, gradientColors);

    return (
      <span
        ref={ref}
        className={cn(
          'bg-clip-text text-transparent',
          animated && 'animate-pulse-soft',
          className
        )}
        style={{
          backgroundImage: gradientStyle,
          ...style,
        }}
        {...props}
      >
        {children}
      </span>
    );
  }
);
GradientText.displayName = 'GradientText';

// Gradient button component
interface GradientButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  gradient?: 'primary' | 'capture' | 'processing' | 'vendor' | 'reporting' | 'rainbow' | 'custom';
  gradientColors?: {
    from: string;
    via?: string;
    to: string;
    angle?: number;
  };
  size?: 'sm' | 'default' | 'lg';
  loading?: boolean;
}

const GradientButton = React.forwardRef<HTMLButtonElement, GradientButtonProps>(
  (
    {
      className,
      gradient = 'primary',
      gradientColors,
      size = 'default',
      loading = false,
      disabled,
      children,
      ...props
    },
    ref
  ) => {
    const gradientStyle = getGradientStyle(gradient, gradientColors);
    const isDisabled = disabled || loading;

    const sizeClasses = {
      sm: 'h-8 px-3 text-sm',
      default: 'h-10 px-4',
      lg: 'h-12 px-6 text-lg',
    };

    return (
      <button
        ref={ref}
        disabled={isDisabled}
        className={cn(
          'relative inline-flex items-center justify-center font-medium text-white rounded-lg transition-all duration-200',
          'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2',
          'disabled:opacity-50 disabled:cursor-not-allowed',
          'hover:shadow-lg hover:brightness-110 active:scale-[0.98]',
          sizeClasses[size],
          className
        )}
        style={{ background: gradientStyle }}
        {...props}
      >
        {loading && (
          <svg
            className="animate-spin -ml-1 mr-2 h-4 w-4"
            fill="none"
            viewBox="0 0 24 24"
          >
            <circle
              className="opacity-25"
              cx="12"
              cy="12"
              r="10"
              stroke="currentColor"
              strokeWidth="4"
            />
            <path
              className="opacity-75"
              fill="currentColor"
              d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
            />
          </svg>
        )}
        {children}
      </button>
    );
  }
);
GradientButton.displayName = 'GradientButton';

// Glow effect wrapper
interface GlowWrapperProps extends React.HTMLAttributes<HTMLDivElement> {
  color?: 'primary' | 'capture' | 'processing' | 'vendor' | 'reporting' | 'success' | 'warning' | 'error';
  intensity?: 'low' | 'medium' | 'high';
  animated?: boolean;
}

const GlowWrapper = React.forwardRef<HTMLDivElement, GlowWrapperProps>(
  ({ className, color = 'primary', intensity = 'medium', animated = false, children, ...props }, ref) => {
    const colorMap: Record<string, string> = {
      primary: 'var(--primary)',
      capture: 'var(--capture)',
      processing: 'var(--processing)',
      vendor: 'var(--vendor)',
      reporting: 'var(--reporting)',
      success: 'var(--success)',
      warning: 'var(--warning)',
      error: 'var(--error)',
    };

    const intensityMap: Record<string, string> = {
      low: '0 0 15px -5px',
      medium: '0 0 25px -5px',
      high: '0 0 40px -5px',
    };

    return (
      <div
        ref={ref}
        className={cn(
          'relative',
          animated && 'animate-pulse-glow',
          className
        )}
        style={{
          boxShadow: `${intensityMap[intensity]} hsl(${colorMap[color]} / 0.5)`,
        }}
        {...props}
      >
        {children}
      </div>
    );
  }
);
GlowWrapper.displayName = 'GlowWrapper';

export {
  GradientCard,
  AnimatedGradientBorder,
  GradientText,
  GradientButton,
  GlowWrapper,
  gradientCardVariants,
};
