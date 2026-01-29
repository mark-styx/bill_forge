import '@testing-library/jest-dom'
import { cleanup } from '@testing-library/react'
import { afterEach, beforeAll, afterAll } from 'vitest'

// Cleanup after each test
afterEach(() => {
  cleanup()
})

// Mock window.matchMedia
beforeAll(() => {
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    value: (query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    }),
  })
})

// Mock IntersectionObserver
beforeAll(() => {
  const mockIntersectionObserver = class {
    constructor() {}
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  window.IntersectionObserver = mockIntersectionObserver as unknown as typeof IntersectionObserver
})

// Mock ResizeObserver
beforeAll(() => {
  const mockResizeObserver = class {
    constructor() {}
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  window.ResizeObserver = mockResizeObserver as unknown as typeof ResizeObserver
})
