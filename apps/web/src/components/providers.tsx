'use client';

import { QueryClient, QueryClientProvider, QueryCache, MutationCache } from '@tanstack/react-query';
import { useState, useEffect } from 'react';
import { ThemeProvider } from 'next-themes';
import { toast } from 'sonner';
import { ErrorBoundary } from '@/components/ui/error-boundary';
import { OrganizationThemeProvider } from '@/components/organization-theme-provider';

function parseError(error: unknown): string {
  if (error instanceof Error) {
    // Check if it's an API error with a specific message
    const message = error.message;
    if (message.includes('401') || message.includes('Unauthorized')) {
      return 'Your session has expired. Please log in again.';
    }
    if (message.includes('403') || message.includes('Forbidden')) {
      return 'You do not have permission to perform this action.';
    }
    if (message.includes('404') || message.includes('Not Found')) {
      return 'The requested resource was not found.';
    }
    if (message.includes('500') || message.includes('Internal Server')) {
      return 'A server error occurred. Please try again later.';
    }
    return message;
  }
  return 'An unexpected error occurred';
}

export function Providers({ children }: { children: React.ReactNode }) {
  const [queryClient] = useState(
    () =>
      new QueryClient({
        queryCache: new QueryCache({
          onError: (error, query) => {
            // Only show toast for background refetch errors
            if (query.state.data !== undefined) {
              toast.error(parseError(error));
            }
          },
        }),
        mutationCache: new MutationCache({
          onError: (error) => {
            toast.error(parseError(error));
          },
        }),
        defaultOptions: {
          queries: {
            staleTime: 60 * 1000, // 1 minute
            refetchOnWindowFocus: false,
            retry: (failureCount, error) => {
              // Don't retry on auth errors
              if (error instanceof Error) {
                if (error.message.includes('401') || error.message.includes('403')) {
                  return false;
                }
              }
              return failureCount < 3;
            },
          },
          mutations: {
            retry: false,
          },
        },
      })
  );

  return (
    <ErrorBoundary>
      <QueryClientProvider client={queryClient}>
        <ThemeProvider
          attribute="class"
          defaultTheme="light"
          enableSystem
          disableTransitionOnChange={false}
        >
          <OrganizationThemeProvider>
            {children}
          </OrganizationThemeProvider>
        </ThemeProvider>
      </QueryClientProvider>
    </ErrorBoundary>
  );
}
