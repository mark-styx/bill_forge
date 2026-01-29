import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import {
  cn,
  formatCurrency,
  formatDate,
  formatRelativeTime,
  truncate,
  capitalize,
  toTitleCase,
  generateId,
  debounce,
  isEmpty,
  getInitials,
  parseApiError,
} from './utils'

// ============================================================================
// cn (class name merge) tests
// ============================================================================
describe('cn', () => {
  it('merges class names correctly', () => {
    expect(cn('foo', 'bar')).toBe('foo bar')
  })

  it('handles conditional classes', () => {
    expect(cn('foo', false && 'bar', 'baz')).toBe('foo baz')
    expect(cn('foo', true && 'bar', 'baz')).toBe('foo bar baz')
  })

  it('deduplicates tailwind classes', () => {
    expect(cn('p-4', 'p-2')).toBe('p-2')
  })

  it('handles arrays', () => {
    expect(cn(['foo', 'bar'])).toBe('foo bar')
  })

  it('handles objects', () => {
    expect(cn({ foo: true, bar: false })).toBe('foo')
  })
})

// ============================================================================
// formatCurrency tests
// ============================================================================
describe('formatCurrency', () => {
  it('formats USD correctly', () => {
    expect(formatCurrency(12345)).toBe('$123.45')
    expect(formatCurrency(100)).toBe('$1.00')
    expect(formatCurrency(0)).toBe('$0.00')
  })

  it('formats negative amounts', () => {
    expect(formatCurrency(-5000)).toBe('-$50.00')
  })

  it('formats EUR correctly', () => {
    const result = formatCurrency(12345, 'EUR')
    expect(result).toContain('123.45')
  })

  it('handles different locales', () => {
    const result = formatCurrency(12345, 'EUR', 'de-DE')
    expect(result).toContain('123,45') // German uses comma as decimal separator
  })
})

// ============================================================================
// formatDate tests
// ============================================================================
describe('formatDate', () => {
  it('formats date string correctly', () => {
    // Use ISO format with time to avoid timezone issues
    const result = formatDate('2024-01-15T12:00:00')
    expect(result).toContain('Jan')
    expect(result).toContain('15')
    expect(result).toContain('2024')
  })

  it('formats Date object correctly', () => {
    const date = new Date(2024, 0, 15) // January 15, 2024
    const result = formatDate(date)
    expect(result).toContain('Jan')
    expect(result).toContain('15')
    expect(result).toContain('2024')
  })

  it('accepts custom options', () => {
    const result = formatDate('2024-01-15', { month: 'long', year: 'numeric' })
    expect(result).toContain('January')
    expect(result).toContain('2024')
  })
})

// ============================================================================
// formatRelativeTime tests
// ============================================================================
describe('formatRelativeTime', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2024-01-15T12:00:00'))
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('returns "Just now" for recent times', () => {
    const result = formatRelativeTime(new Date('2024-01-15T11:59:30'))
    expect(result).toBe('Just now')
  })

  it('returns minutes ago', () => {
    const result = formatRelativeTime(new Date('2024-01-15T11:55:00'))
    expect(result).toBe('5 minutes ago')
  })

  it('returns singular minute', () => {
    const result = formatRelativeTime(new Date('2024-01-15T11:59:00'))
    expect(result).toBe('1 minute ago')
  })

  it('returns hours ago', () => {
    const result = formatRelativeTime(new Date('2024-01-15T10:00:00'))
    expect(result).toBe('2 hours ago')
  })

  it('returns singular hour', () => {
    const result = formatRelativeTime(new Date('2024-01-15T11:00:00'))
    expect(result).toBe('1 hour ago')
  })

  it('returns days ago', () => {
    const result = formatRelativeTime(new Date('2024-01-13T12:00:00'))
    expect(result).toBe('2 days ago')
  })

  it('returns formatted date for old dates', () => {
    const result = formatRelativeTime(new Date('2023-11-15T12:00:00'))
    expect(result).toContain('Nov')
    expect(result).toContain('15')
  })
})

// ============================================================================
// truncate tests
// ============================================================================
describe('truncate', () => {
  it('does not truncate short strings', () => {
    expect(truncate('hello', 10)).toBe('hello')
  })

  it('truncates long strings', () => {
    expect(truncate('hello world', 5)).toBe('hello...')
  })

  it('handles exact length', () => {
    expect(truncate('hello', 5)).toBe('hello')
  })

  it('handles empty string', () => {
    expect(truncate('', 5)).toBe('')
  })
})

// ============================================================================
// capitalize tests
// ============================================================================
describe('capitalize', () => {
  it('capitalizes first letter', () => {
    expect(capitalize('hello')).toBe('Hello')
  })

  it('keeps already capitalized', () => {
    expect(capitalize('Hello')).toBe('Hello')
  })

  it('handles single character', () => {
    expect(capitalize('h')).toBe('H')
  })

  it('handles empty string', () => {
    expect(capitalize('')).toBe('')
  })

  it('handles all caps', () => {
    expect(capitalize('HELLO')).toBe('HELLO')
  })
})

// ============================================================================
// toTitleCase tests
// ============================================================================
describe('toTitleCase', () => {
  it('converts snake_case', () => {
    expect(toTitleCase('hello_world')).toBe('Hello World')
  })

  it('converts kebab-case', () => {
    expect(toTitleCase('hello-world')).toBe('Hello World')
  })

  it('converts mixed case', () => {
    expect(toTitleCase('hello_world-test')).toBe('Hello World Test')
  })

  it('handles single word', () => {
    expect(toTitleCase('hello')).toBe('Hello')
  })
})

// ============================================================================
// generateId tests
// ============================================================================
describe('generateId', () => {
  it('generates unique IDs', () => {
    const id1 = generateId()
    const id2 = generateId()
    expect(id1).not.toBe(id2)
  })

  it('generates string IDs', () => {
    const id = generateId()
    expect(typeof id).toBe('string')
  })

  it('generates IDs with alphanumeric characters', () => {
    const id = generateId()
    expect(id).toMatch(/^[a-z0-9]+$/)
  })
})

// ============================================================================
// debounce tests
// ============================================================================
describe('debounce', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('delays function execution', () => {
    const fn = vi.fn()
    const debounced = debounce(fn, 100)

    debounced()
    expect(fn).not.toHaveBeenCalled()

    vi.advanceTimersByTime(100)
    expect(fn).toHaveBeenCalledTimes(1)
  })

  it('only calls once for rapid calls', () => {
    const fn = vi.fn()
    const debounced = debounce(fn, 100)

    debounced()
    debounced()
    debounced()

    vi.advanceTimersByTime(100)
    expect(fn).toHaveBeenCalledTimes(1)
  })

  it('passes arguments to the function', () => {
    const fn = vi.fn()
    const debounced = debounce(fn, 100)

    debounced('arg1', 'arg2')
    vi.advanceTimersByTime(100)

    expect(fn).toHaveBeenCalledWith('arg1', 'arg2')
  })
})

// ============================================================================
// isEmpty tests
// ============================================================================
describe('isEmpty', () => {
  it('returns true for null', () => {
    expect(isEmpty(null)).toBe(true)
  })

  it('returns true for undefined', () => {
    expect(isEmpty(undefined)).toBe(true)
  })

  it('returns true for empty string', () => {
    expect(isEmpty('')).toBe(true)
    expect(isEmpty('   ')).toBe(true)
  })

  it('returns true for empty array', () => {
    expect(isEmpty([])).toBe(true)
  })

  it('returns true for empty object', () => {
    expect(isEmpty({})).toBe(true)
  })

  it('returns false for non-empty values', () => {
    expect(isEmpty('hello')).toBe(false)
    expect(isEmpty([1, 2, 3])).toBe(false)
    expect(isEmpty({ a: 1 })).toBe(false)
  })

  it('returns false for numbers', () => {
    expect(isEmpty(0)).toBe(false)
    expect(isEmpty(42)).toBe(false)
  })
})

// ============================================================================
// getInitials tests
// ============================================================================
describe('getInitials', () => {
  it('returns initials for two names', () => {
    expect(getInitials('John Doe')).toBe('JD')
  })

  it('returns initials for single name', () => {
    expect(getInitials('John')).toBe('J')
  })

  it('handles multiple names', () => {
    expect(getInitials('John Michael Doe')).toBe('JM')
  })

  it('returns uppercase initials', () => {
    expect(getInitials('john doe')).toBe('JD')
  })
})

// ============================================================================
// parseApiError tests
// ============================================================================
describe('parseApiError', () => {
  it('returns error message from Error', () => {
    const error = new Error('Something went wrong')
    expect(parseApiError(error)).toBe('Something went wrong')
  })

  it('returns string errors directly', () => {
    expect(parseApiError('Error message')).toBe('Error message')
  })

  it('extracts message from object', () => {
    const error = { message: 'Object error' }
    expect(parseApiError(error)).toBe('Object error')
  })

  it('returns default message for unknown errors', () => {
    expect(parseApiError(null)).toBe('An unexpected error occurred')
    expect(parseApiError(42)).toBe('An unexpected error occurred')
  })
})
