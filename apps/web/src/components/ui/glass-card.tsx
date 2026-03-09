'use client';

import * as React from 'react';
import { cn } from '@/lib/utils';
import { cva, type VariantProps } from 'class-variance-authority';

const glassCardVariants = cva(
  'relative rounded-2xl overflow-hidden transition-all duration-300',
  {
    variants: {
      variant: {
        default:
          'bg-card/80 backdrop-blur-lg border border-border/50 shadow-soft',
        tinted:
          'bg-gradient-to-br from-primary/5 to-accent/5 backdrop-blur-xl border border-primary/10',
        frosted:
          'bg-white/60 dark:bg-card/60 backdrop-blur-2xl border border-white/20 dark:border-white/10 shadow-lg',
        neon:
          'bg-card/90 backdrop-blur-lg border border-primary/30 shadow-neon',
        gradient:
          'bg-gradient-to-br from-card via-card to-card/80 backdrop-blur-lg border border-border',
        subtle:
          'bg-card/50 backdrop-blur-sm border border-border/30',
      },
      hover: {
        none: '',
        lift: 'hover:-translate-y-1 hover:shadow-lg',
        glow: 'hover:shadow-glow hover:border-primary/40',
        scale: 'hover:scale-[1.02]',
        bright: 'hover:bg-card/95 hover:border-primary/30 hover:shadow-bright',
      },
      padding: {
        none: '',
        sm: 'p-4',
        md: 'p-5',
        lg: 'p-6',
        xl: 'p-8',
      },
    },
    defaultVariants: {
      variant: 'default',
      hover: 'lift',
      padding: 'md',
    },
  }
);

export interface GlassCardProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof glassCardVariants> {
  gradientBorder?: boolean;
  spotlight?: boolean;
  accentBar?: 'top' | 'left' | 'none';
  accentColor?: 'primary' | 'capture' | 'processing' | 'vendor' | 'reporting' | 'gradient';
}

const GlassCard = React.forwardRef<HTMLDivElement, GlassCardProps>(
  (
    {
      className,
      variant,
      hover,
      padding,
      gradientBorder = false,
      spotlight = false,
      accentBar = 'none',
      accentColor = 'primary',
      children,
      ...props
    },
    ref
  ) => {
    const accentGradients = {
      primary: 'linear-gradient(135deg, hsl(var(--primary)), hsl(var(--accent)))',
      capture: 'linear-gradient(135deg, hsl(var(--capture)), hsl(195 100% 55%))',
      processing: 'linear-gradient(135deg, hsl(var(--processing)), hsl(160 84% 55%))',
      vendor: 'linear-gradient(135deg, hsl(var(--vendor)), hsl(280 70% 60%))',
      reporting: 'linear-gradient(135deg, hsl(var(--reporting)), hsl(45 95% 55%))',
      gradient: 'linear-gradient(135deg, hsl(var(--primary)), hsl(var(--accent)), hsl(var(--capture)))',
    };

    return (
      <div
        ref={ref}
        className={cn(
          glassCardVariants({ variant, hover, padding }),
          spotlight && 'spotlight',
          gradientBorder && 'gradient-border',
          className
        )}
        {...props}
      >
        {/* Accent bar */}
        {accentBar === 'top' && (
          <div
            className="absolute top-0 left-0 right-0 h-1 rounded-t-2xl"
            style={{ background: accentGradients[accentColor] }}
          />
        )}
        {accentBar === 'left' && (
          <div
            className="absolute top-0 left-0 bottom-0 w-1 rounded-l-2xl"
            style={{ background: accentGradients[accentColor] }}
          />
        )}

        {/* Gradient border overlay */}
        {gradientBorder && (
          <div
            className="absolute inset-0 rounded-2xl pointer-events-none"
            style={{
              background: `linear-gradient(135deg, hsl(var(--primary) / 0.2), hsl(var(--accent) / 0.2))`,
              mask: 'linear-gradient(#fff 0 0) content-box, linear-gradient(#fff 0 0)',
              maskComposite: 'xor',
              padding: '1px',
            }}
          />
        )}

        {/* Content */}
        <div className="relative">{children}</div>
      </div>
    );
  }
);
GlassCard.displayName = 'GlassCard';

// Glass Card Header
interface GlassCardHeaderProps extends React.HTMLAttributes<HTMLDivElement> {
  gradient?: boolean;
}

const GlassCardHeader = React.forwardRef<HTMLDivElement, GlassCardHeaderProps>(
  ({ className, gradient = false, children, ...props }, ref) => (
    <div
      ref={ref}
      className={cn(
        'relative px-5 py-4 border-b border-border/50',
        gradient && 'bright-card-header',
        className
      )}
      {...props}
    >
      {children}
    </div>
  )
);
GlassCardHeader.displayName = 'GlassCardHeader';

// Glass Card Title
const GlassCardTitle = React.forwardRef<
  HTMLParagraphElement,
  React.HTMLAttributes<HTMLHeadingElement>
>(({ className, ...props }, ref) => (
  <h3
    ref={ref}
    className={cn('font-semibold text-foreground leading-none tracking-tight', className)}
    {...props}
  />
));
GlassCardTitle.displayName = 'GlassCardTitle';

// Glass Card Description
const GlassCardDescription = React.forwardRef<
  HTMLParagraphElement,
  React.HTMLAttributes<HTMLParagraphElement>
>(({ className, ...props }, ref) => (
  <p
    ref={ref}
    className={cn('text-sm text-muted-foreground mt-1', className)}
    {...props}
  />
));
GlassCardDescription.displayName = 'GlassCardDescription';

// Glass Card Content
const GlassCardContent = React.forwardRef<
  HTMLDivElement,
  React.HTMLAttributes<HTMLDivElement>
>(({ className, ...props }, ref) => (
  <div ref={ref} className={cn('', className)} {...props} />
));
GlassCardContent.displayName = 'GlassCardContent';

// Glass Card Footer
const GlassCardFooter = React.forwardRef<
  HTMLDivElement,
  React.HTMLAttributes<HTMLDivElement>
>(({ className, ...props }, ref) => (
  <div
    ref={ref}
    className={cn('flex items-center pt-4 border-t border-border/50 mt-4', className)}
    {...props}
  />
));
GlassCardFooter.displayName = 'GlassCardFooter';

// Spotlight Card - Enhanced glass card with mouse-following spotlight
interface SpotlightCardProps extends GlassCardProps {
  spotlightColor?: string;
}

const SpotlightCard = React.forwardRef<HTMLDivElement, SpotlightCardProps>(
  ({ className, spotlightColor, children, ...props }, ref) => {
    const containerRef = React.useRef<HTMLDivElement>(null);
    const [mousePosition, setMousePosition] = React.useState({ x: 0, y: 0 });
    const [isHovered, setIsHovered] = React.useState(false);

    const handleMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
      if (!containerRef.current) return;
      const rect = containerRef.current.getBoundingClientRect();
      setMousePosition({
        x: e.clientX - rect.left,
        y: e.clientY - rect.top,
      });
    };

    return (
      <GlassCard
        ref={ref}
        className={cn('overflow-hidden', className)}
        onMouseMove={handleMouseMove}
        onMouseEnter={() => setIsHovered(true)}
        onMouseLeave={() => setIsHovered(false)}
        {...props}
      >
        <div ref={containerRef} className="relative">
          {/* Spotlight effect */}
          {isHovered && (
            <div
              className="absolute pointer-events-none transition-opacity duration-300"
              style={{
                background: `radial-gradient(300px circle at ${mousePosition.x}px ${mousePosition.y}px, ${spotlightColor || 'hsl(var(--primary) / 0.1)'}, transparent 40%)`,
                inset: 0,
                opacity: isHovered ? 1 : 0,
              }}
            />
          )}
          {children}
        </div>
      </GlassCard>
    );
  }
);
SpotlightCard.displayName = 'SpotlightCard';

export {
  GlassCard,
  GlassCardHeader,
  GlassCardTitle,
  GlassCardDescription,
  GlassCardContent,
  GlassCardFooter,
  SpotlightCard,
  glassCardVariants,
};
