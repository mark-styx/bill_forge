'use client';

import * as React from 'react';
import { cn } from '@/lib/utils';
import { useThemeStore } from '@/stores/theme';
import { Button } from './button';
import { GradientText, GradientButton } from './gradient-card';
import { ArrowRight, Play, Sparkles, Check, Star } from 'lucide-react';

interface HeroSectionProps {
  title: string;
  titleHighlight?: string;
  subtitle: string;
  description?: string;
  primaryCta?: {
    text: string;
    href: string;
    icon?: React.ReactNode;
  };
  secondaryCta?: {
    text: string;
    href: string;
    icon?: React.ReactNode;
  };
  features?: string[];
  badges?: {
    text: string;
    icon?: React.ReactNode;
  }[];
  image?: React.ReactNode;
  variant?: 'default' | 'centered' | 'split';
  className?: string;
}

export function HeroSection({
  title,
  titleHighlight,
  subtitle,
  description,
  primaryCta,
  secondaryCta,
  features,
  badges,
  image,
  variant = 'default',
  className,
}: HeroSectionProps) {
  const { getCurrentColors } = useThemeStore();
  const colors = getCurrentColors();

  return (
    <section
      className={cn(
        'relative overflow-hidden',
        variant === 'centered' && 'text-center',
        className
      )}
    >
      {/* Animated background gradients */}
      <div className="absolute inset-0 overflow-hidden pointer-events-none">
        {/* Primary gradient blob */}
        <div
          className="absolute -top-40 -right-40 w-80 h-80 rounded-full opacity-30 blur-3xl animate-pulse-soft"
          style={{ background: `hsl(${colors.primary})` }}
        />
        {/* Accent gradient blob */}
        <div
          className="absolute -bottom-40 -left-40 w-80 h-80 rounded-full opacity-20 blur-3xl animate-pulse-soft"
          style={{ background: `hsl(${colors.accent})`, animationDelay: '1s' }}
        />
        {/* Capture color blob */}
        <div
          className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-96 h-96 rounded-full opacity-10 blur-3xl animate-pulse-soft"
          style={{ background: `hsl(${colors.capture})`, animationDelay: '2s' }}
        />
        {/* Grid pattern */}
        <div
          className="absolute inset-0 bg-[linear-gradient(to_right,hsl(var(--border)/0.1)_1px,transparent_1px),linear-gradient(to_bottom,hsl(var(--border)/0.1)_1px,transparent_1px)] bg-[size:4rem_4rem]"
        />
      </div>

      <div className={cn(
        'relative z-10 container mx-auto px-4',
        variant === 'split' ? 'grid lg:grid-cols-2 gap-12 items-center' : ''
      )}>
        <div className={cn(variant === 'centered' && 'max-w-3xl mx-auto')}>
          {/* Badges */}
          {badges && badges.length > 0 && (
            <div className={cn(
              'flex flex-wrap gap-2 mb-6',
              variant === 'centered' && 'justify-center'
            )}>
              {badges.map((badge, i) => (
                <span
                  key={i}
                  className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-xs font-medium bg-primary/10 text-primary border border-primary/20"
                >
                  {badge.icon || <Sparkles className="w-3 h-3" />}
                  {badge.text}
                </span>
              ))}
            </div>
          )}

          {/* Title */}
          <h1 className={cn(
            'text-4xl sm:text-5xl lg:text-6xl font-bold text-foreground tracking-tight',
            variant === 'centered' && 'mx-auto'
          )}>
            {title}{' '}
            {titleHighlight && (
              <GradientText gradient="primary" className="block sm:inline">
                {titleHighlight}
              </GradientText>
            )}
          </h1>

          {/* Subtitle */}
          <p className={cn(
            'mt-4 text-lg sm:text-xl text-muted-foreground',
            variant === 'centered' && 'mx-auto max-w-2xl'
          )}>
            {subtitle}
          </p>

          {/* Description */}
          {description && (
            <p className={cn(
              'mt-3 text-base text-muted-foreground/80',
              variant === 'centered' && 'mx-auto max-w-xl'
            )}>
              {description}
            </p>
          )}

          {/* Features list */}
          {features && features.length > 0 && (
            <div className={cn(
              'mt-6 flex flex-wrap gap-x-6 gap-y-2',
              variant === 'centered' && 'justify-center'
            )}>
              {features.map((feature, i) => (
                <div key={i} className="flex items-center gap-2 text-sm text-muted-foreground">
                  <Check className="w-4 h-4 text-success" />
                  {feature}
                </div>
              ))}
            </div>
          )}

          {/* CTAs */}
          {(primaryCta || secondaryCta) && (
            <div className={cn(
              'mt-8 flex flex-wrap gap-4',
              variant === 'centered' && 'justify-center'
            )}>
              {primaryCta && (
                <GradientButton
                  gradient="primary"
                  size="lg"
                  onClick={() => window.location.href = primaryCta.href}
                >
                  {primaryCta.text}
                  {primaryCta.icon || <ArrowRight className="ml-2 w-5 h-5" />}
                </GradientButton>
              )}
              {secondaryCta && (
                <Button
                  variant="outline"
                  size="lg"
                  onClick={() => window.location.href = secondaryCta.href}
                >
                  {secondaryCta.icon || <Play className="mr-2 w-4 h-4" />}
                  {secondaryCta.text}
                </Button>
              )}
            </div>
          )}

          {/* Trust indicators */}
          <div className={cn(
            'mt-10 flex items-center gap-6',
            variant === 'centered' && 'justify-center'
          )}>
            <div className="flex -space-x-2">
              {[...Array(5)].map((_, i) => (
                <div
                  key={i}
                  className="w-8 h-8 rounded-full border-2 border-background flex items-center justify-center text-white text-xs font-medium"
                  style={{
                    background: `hsl(${[colors.primary, colors.accent, colors.capture, colors.processing, colors.vendor][i]})`,
                  }}
                >
                  {String.fromCharCode(65 + i)}
                </div>
              ))}
            </div>
            <div className="text-sm">
              <div className="flex items-center gap-1 text-foreground font-medium">
                <Star className="w-4 h-4 fill-warning text-warning" />
                <Star className="w-4 h-4 fill-warning text-warning" />
                <Star className="w-4 h-4 fill-warning text-warning" />
                <Star className="w-4 h-4 fill-warning text-warning" />
                <Star className="w-4 h-4 fill-warning text-warning" />
                <span className="ml-1">4.9/5</span>
              </div>
              <p className="text-muted-foreground">Trusted by 10,000+ businesses</p>
            </div>
          </div>
        </div>

        {/* Image/Illustration */}
        {image && variant === 'split' && (
          <div className="relative">
            <div className="relative z-10">{image}</div>
            {/* Decorative elements */}
            <div
              className="absolute -top-4 -right-4 w-24 h-24 rounded-2xl opacity-20 blur-xl"
              style={{ background: `hsl(${colors.primary})` }}
            />
            <div
              className="absolute -bottom-4 -left-4 w-32 h-32 rounded-2xl opacity-20 blur-xl"
              style={{ background: `hsl(${colors.accent})` }}
            />
          </div>
        )}
      </div>
    </section>
  );
}

// Feature showcase component
interface FeatureCardProps {
  icon: React.ReactNode;
  title: string;
  description: string;
  color?: 'primary' | 'capture' | 'processing' | 'vendor' | 'reporting';
  href?: string;
}

export function FeatureCard({
  icon,
  title,
  description,
  color = 'primary',
  href,
}: FeatureCardProps) {
  const colorClasses: Record<string, { bg: string; text: string }> = {
    primary: { bg: 'bg-primary/10', text: 'text-primary' },
    capture: { bg: 'bg-capture/10', text: 'text-capture' },
    processing: { bg: 'bg-processing/10', text: 'text-processing' },
    vendor: { bg: 'bg-vendor/10', text: 'text-vendor' },
    reporting: { bg: 'bg-reporting/10', text: 'text-reporting' },
  };

  const Wrapper = href ? 'a' : 'div';
  const wrapperProps = href ? { href } : {};

  return (
    <Wrapper
      {...wrapperProps}
      className={cn(
        'group relative p-6 rounded-2xl bg-card border border-border transition-all duration-300',
        href && 'hover:shadow-lg hover:-translate-y-1 cursor-pointer'
      )}
    >
      <div className={cn('w-12 h-12 rounded-xl flex items-center justify-center mb-4', colorClasses[color].bg)}>
        <div className={colorClasses[color].text}>{icon}</div>
      </div>
      <h3 className="text-lg font-semibold text-foreground mb-2">{title}</h3>
      <p className="text-sm text-muted-foreground">{description}</p>
      {href && (
        <ArrowRight className="absolute bottom-6 right-6 w-4 h-4 text-muted-foreground opacity-0 group-hover:opacity-100 group-hover:translate-x-1 transition-all" />
      )}
    </Wrapper>
  );
}

// Stats showcase component
interface StatItemProps {
  value: string;
  label: string;
  suffix?: string;
  color?: 'primary' | 'capture' | 'processing' | 'vendor' | 'reporting';
}

export function StatItem({ value, label, suffix, color = 'primary' }: StatItemProps) {
  const { getCurrentColors } = useThemeStore();
  const colors = getCurrentColors();

  const colorMap: Record<string, string> = {
    primary: colors.primary,
    capture: colors.capture,
    processing: colors.processing,
    vendor: colors.vendor,
    reporting: colors.reporting,
  };

  return (
    <div className="text-center p-6">
      <div className="text-4xl sm:text-5xl font-bold mb-2">
        <GradientText
          gradientColors={{ from: colorMap[color], to: colors.accent }}
        >
          {value}
          {suffix && <span className="text-2xl">{suffix}</span>}
        </GradientText>
      </div>
      <p className="text-sm text-muted-foreground">{label}</p>
    </div>
  );
}

interface StatsShowcaseProps {
  stats: StatItemProps[];
  className?: string;
}

export function StatsShowcase({ stats, className }: StatsShowcaseProps) {
  return (
    <div className={cn('grid grid-cols-2 md:grid-cols-4 gap-4', className)}>
      {stats.map((stat, i) => (
        <StatItem key={i} {...stat} />
      ))}
    </div>
  );
}

// Testimonial card
interface TestimonialCardProps {
  quote: string;
  author: string;
  role: string;
  company: string;
  avatar?: string;
  rating?: number;
}

export function TestimonialCard({
  quote,
  author,
  role,
  company,
  avatar,
  rating = 5,
}: TestimonialCardProps) {
  const { getCurrentColors } = useThemeStore();
  const colors = getCurrentColors();

  return (
    <div className="p-6 rounded-2xl bg-card border border-border">
      {/* Rating */}
      <div className="flex gap-1 mb-4">
        {[...Array(5)].map((_, i) => (
          <Star
            key={i}
            className={cn(
              'w-4 h-4',
              i < rating ? 'fill-warning text-warning' : 'text-muted-foreground/30'
            )}
          />
        ))}
      </div>

      {/* Quote */}
      <blockquote className="text-foreground mb-6">&ldquo;{quote}&rdquo;</blockquote>

      {/* Author */}
      <div className="flex items-center gap-3">
        {avatar ? (
          <img src={avatar} alt={author} className="w-10 h-10 rounded-full" />
        ) : (
          <div
            className="w-10 h-10 rounded-full flex items-center justify-center text-white font-medium"
            style={{ background: `linear-gradient(135deg, hsl(${colors.primary}), hsl(${colors.accent}))` }}
          >
            {author[0]}
          </div>
        )}
        <div>
          <p className="font-medium text-foreground">{author}</p>
          <p className="text-sm text-muted-foreground">
            {role} at {company}
          </p>
        </div>
      </div>
    </div>
  );
}
