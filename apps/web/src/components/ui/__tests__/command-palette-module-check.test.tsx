import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'

// --- Mutable module list so each test can control the enabled state ---
let enabledModules: string[] = []

vi.mock('@/stores/auth', () => ({
  useAuthStore: () => ({
    hasModule: (mod: string) => enabledModules.includes(mod),
    logout: vi.fn(),
  }),
}))

vi.mock('@/stores/theme', () => ({
  useThemeStore: () => ({
    mode: 'light',
    setMode: vi.fn(),
    setPreset: vi.fn(),
    presetId: 'brilliant-blue',
  }),
  themePresets: [],
}))

const pushMock = vi.fn()
vi.mock('next/navigation', () => ({
  useRouter: () => ({ push: pushMock }),
}))

import { CommandPalette } from '../command-palette'

describe('CommandPalette module gate for Winston AI Assistant', () => {
  beforeEach(() => {
    enabledModules = []
    pushMock.mockClear()
  })

  function openPalette(container: HTMLElement) {
    fireEvent.keyDown(container.ownerDocument ?? document, {
      key: 'k',
      metaKey: true,
    })
  }

  it('hides Winston AI Assistant when ai_assistant module is disabled', () => {
    enabledModules = [] // no paid add-on

    const { container } = render(<CommandPalette />)
    openPalette(container)

    expect(screen.queryByText('Go to Winston AI Assistant')).not.toBeInTheDocument()
  })

  it('shows Winston AI Assistant when ai_assistant module is enabled', () => {
    enabledModules = ['ai_assistant']

    const { container } = render(<CommandPalette />)
    openPalette(container)

    expect(screen.getByText('Go to Winston AI Assistant')).toBeInTheDocument()
  })

  it('navigates to /ai-assistant when Winston command is selected', () => {
    enabledModules = ['ai_assistant']

    const { container } = render(<CommandPalette />)
    openPalette(container)

    const winstonItem = screen.getByText('Go to Winston AI Assistant')
    // The click target is the Command.Item wrapping the text node;
    // walk up to the closest element with an onSelect / click handler.
    fireEvent.click(winstonItem.closest('[cmdk-item]') ?? winstonItem)

    expect(pushMock).toHaveBeenCalledWith('/ai-assistant')
  })
})
