import type { Config } from 'tailwindcss';

const config: Config = {
  darkMode: ['class'],
  content: [
    './src/pages/**/*.{js,ts,jsx,tsx,mdx}',
    './src/components/**/*.{js,ts,jsx,tsx,mdx}',
    './src/app/**/*.{js,ts,jsx,tsx,mdx}',
  ],
  theme: {
    container: {
      center: true,
      padding: '2rem',
      screens: {
        '2xl': '1400px',
      },
    },
    extend: {
      colors: {
        border: 'hsl(var(--border))',
        input: 'hsl(var(--input))',
        ring: 'hsl(var(--ring))',
        background: 'hsl(var(--background))',
        foreground: 'hsl(var(--foreground))',
        primary: {
          DEFAULT: 'hsl(var(--primary))',
          foreground: 'hsl(var(--primary-foreground))',
          50: 'hsl(var(--primary) / 0.05)',
          100: 'hsl(var(--primary) / 0.1)',
          200: 'hsl(var(--primary) / 0.2)',
          300: 'hsl(var(--primary) / 0.3)',
          400: 'hsl(var(--primary) / 0.4)',
          500: 'hsl(var(--primary) / 0.5)',
        },
        secondary: {
          DEFAULT: 'hsl(var(--secondary))',
          foreground: 'hsl(var(--secondary-foreground))',
        },
        destructive: {
          DEFAULT: 'hsl(var(--destructive))',
          foreground: 'hsl(var(--destructive-foreground))',
        },
        muted: {
          DEFAULT: 'hsl(var(--muted))',
          foreground: 'hsl(var(--muted-foreground))',
        },
        accent: {
          DEFAULT: 'hsl(var(--accent))',
          foreground: 'hsl(var(--accent-foreground))',
          50: 'hsl(var(--accent) / 0.05)',
          100: 'hsl(var(--accent) / 0.1)',
          200: 'hsl(var(--accent) / 0.2)',
        },
        popover: {
          DEFAULT: 'hsl(var(--popover))',
          foreground: 'hsl(var(--popover-foreground))',
        },
        card: {
          DEFAULT: 'hsl(var(--card))',
          foreground: 'hsl(var(--card-foreground))',
        },
        // Surfaces for layering
        surface: {
          1: 'hsl(var(--surface-1))',
          2: 'hsl(var(--surface-2))',
          3: 'hsl(var(--surface-3))',
        },
        // Module-specific colors with variants
        capture: {
          DEFAULT: 'hsl(var(--capture))',
          foreground: 'hsl(var(--capture-foreground))',
          50: 'hsl(var(--capture) / 0.05)',
          100: 'hsl(var(--capture) / 0.1)',
          200: 'hsl(var(--capture) / 0.2)',
        },
        processing: {
          DEFAULT: 'hsl(var(--processing))',
          foreground: 'hsl(var(--processing-foreground))',
          50: 'hsl(var(--processing) / 0.05)',
          100: 'hsl(var(--processing) / 0.1)',
          200: 'hsl(var(--processing) / 0.2)',
        },
        vendor: {
          DEFAULT: 'hsl(var(--vendor))',
          foreground: 'hsl(var(--vendor-foreground))',
          50: 'hsl(var(--vendor) / 0.05)',
          100: 'hsl(var(--vendor) / 0.1)',
          200: 'hsl(var(--vendor) / 0.2)',
        },
        reporting: {
          DEFAULT: 'hsl(var(--reporting))',
          foreground: 'hsl(var(--reporting-foreground))',
          50: 'hsl(var(--reporting) / 0.05)',
          100: 'hsl(var(--reporting) / 0.1)',
          200: 'hsl(var(--reporting) / 0.2)',
        },
        // State colors with variants
        success: {
          DEFAULT: 'hsl(var(--success))',
          foreground: 'hsl(var(--success-foreground))',
          50: 'hsl(var(--success) / 0.05)',
          100: 'hsl(var(--success) / 0.1)',
        },
        warning: {
          DEFAULT: 'hsl(var(--warning))',
          foreground: 'hsl(var(--warning-foreground))',
          50: 'hsl(var(--warning) / 0.05)',
          100: 'hsl(var(--warning) / 0.1)',
        },
        error: {
          DEFAULT: 'hsl(var(--error))',
          foreground: 'hsl(var(--error-foreground))',
          50: 'hsl(var(--error) / 0.05)',
          100: 'hsl(var(--error) / 0.1)',
        },
        // Bright accent colors
        bright: {
          blue: 'hsl(210 100% 55%)',
          cyan: 'hsl(190 100% 50%)',
          emerald: 'hsl(160 90% 45%)',
          violet: 'hsl(270 85% 60%)',
          orange: 'hsl(35 100% 55%)',
          pink: 'hsl(330 90% 60%)',
        },
      },
      borderRadius: {
        '2xl': 'calc(var(--radius) + 8px)',
        xl: 'calc(var(--radius) + 4px)',
        lg: 'var(--radius)',
        md: 'calc(var(--radius) - 2px)',
        sm: 'calc(var(--radius) - 4px)',
      },
      fontFamily: {
        sans: ['var(--font-geist-sans)', 'Inter', 'system-ui', 'sans-serif'],
        mono: ['var(--font-geist-mono)', 'monospace'],
      },
      boxShadow: {
        'soft': '0 2px 8px -2px rgba(0, 0, 0, 0.05), 0 4px 16px -4px rgba(0, 0, 0, 0.08)',
        'soft-lg': '0 4px 12px -2px rgba(0, 0, 0, 0.06), 0 8px 24px -4px rgba(0, 0, 0, 0.1)',
        'soft-xl': '0 8px 24px -4px rgba(0, 0, 0, 0.08), 0 16px 48px -8px rgba(0, 0, 0, 0.12)',
        'glow': '0 0 20px -5px hsl(var(--primary) / 0.3)',
        'glow-lg': '0 0 30px -5px hsl(var(--primary) / 0.4)',
        'glow-accent': '0 0 20px -5px hsl(var(--accent) / 0.3)',
        'inner-glow': 'inset 0 1px 0 0 rgba(255, 255, 255, 0.1)',
        'bright': '0 4px 20px -4px hsl(var(--primary) / 0.25), 0 8px 32px -8px hsl(var(--primary) / 0.15)',
        'card-hover': '0 8px 30px -12px rgba(0, 0, 0, 0.15), 0 4px 12px -4px rgba(0, 0, 0, 0.08)',
      },
      backgroundImage: {
        'gradient-radial': 'radial-gradient(var(--tw-gradient-stops))',
        'gradient-conic': 'conic-gradient(from 180deg at 50% 50%, var(--tw-gradient-stops))',
        'shimmer': 'linear-gradient(90deg, transparent 0%, rgba(255,255,255,0.1) 50%, transparent 100%)',
        'mesh-gradient': 'radial-gradient(at 40% 20%, hsl(var(--primary) / 0.15) 0px, transparent 50%), radial-gradient(at 80% 0%, hsl(var(--accent) / 0.1) 0px, transparent 50%)',
      },
      keyframes: {
        'accordion-down': {
          from: { height: '0' },
          to: { height: 'var(--radix-accordion-content-height)' },
        },
        'accordion-up': {
          from: { height: 'var(--radix-accordion-content-height)' },
          to: { height: '0' },
        },
        'fade-in': {
          from: { opacity: '0' },
          to: { opacity: '1' },
        },
        'fade-in-up': {
          from: { opacity: '0', transform: 'translateY(10px)' },
          to: { opacity: '1', transform: 'translateY(0)' },
        },
        'slide-in-from-right': {
          from: { transform: 'translateX(100%)' },
          to: { transform: 'translateX(0)' },
        },
        'slide-up': {
          from: { transform: 'translateY(8px)', opacity: '0' },
          to: { transform: 'translateY(0)', opacity: '1' },
        },
        'scale-in': {
          from: { transform: 'scale(0.95)', opacity: '0' },
          to: { transform: 'scale(1)', opacity: '1' },
        },
        'pulse-soft': {
          '0%, 100%': { opacity: '1' },
          '50%': { opacity: '0.7' },
        },
        'shimmer': {
          '0%': { backgroundPosition: '-200% 0' },
          '100%': { backgroundPosition: '200% 0' },
        },
        'glow-pulse': {
          '0%, 100%': { boxShadow: '0 0 0 0 hsl(var(--primary) / 0.4)' },
          '50%': { boxShadow: '0 0 20px 4px hsl(var(--primary) / 0.2)' },
        },
        'float': {
          '0%, 100%': { transform: 'translateY(0)' },
          '50%': { transform: 'translateY(-6px)' },
        },
        'gradient-shift': {
          '0%, 100%': { backgroundPosition: '0% 50%' },
          '50%': { backgroundPosition: '100% 50%' },
        },
        'border-dance': {
          '0%, 100%': { borderColor: 'hsl(var(--primary) / 0.3)' },
          '50%': { borderColor: 'hsl(var(--accent) / 0.3)' },
        },
        'spin-slow': {
          from: { transform: 'rotate(0deg)' },
          to: { transform: 'rotate(360deg)' },
        },
      },
      animation: {
        'accordion-down': 'accordion-down 0.2s ease-out',
        'accordion-up': 'accordion-up 0.2s ease-out',
        'fade-in': 'fade-in 0.2s ease-out',
        'fade-in-up': 'fade-in-up 0.3s ease-out',
        'slide-in': 'slide-in-from-right 0.3s ease-out',
        'slide-up': 'slide-up 0.3s ease-out',
        'scale-in': 'scale-in 0.2s ease-out',
        'pulse-soft': 'pulse-soft 2s ease-in-out infinite',
        'shimmer': 'shimmer 2s linear infinite',
        'glow-pulse': 'glow-pulse 2s ease-in-out infinite',
        'float': 'float 3s ease-in-out infinite',
        'gradient-shift': 'gradient-shift 4s ease infinite',
        'border-dance': 'border-dance 2s ease-in-out infinite',
        'spin-slow': 'spin-slow 8s linear infinite',
      },
      transitionDuration: {
        '400': '400ms',
      },
      transitionTimingFunction: {
        'bounce-in': 'cubic-bezier(0.68, -0.55, 0.265, 1.55)',
        'smooth': 'cubic-bezier(0.4, 0, 0.2, 1)',
      },
    },
  },
  plugins: [require('tailwindcss-animate')],
};

export default config;
