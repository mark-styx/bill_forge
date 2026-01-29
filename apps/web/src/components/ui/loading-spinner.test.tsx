import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import {
  LoadingSpinner,
  LoadingOverlay,
  LoadingButton,
  PageLoader,
  InlineLoader,
} from './loading-spinner'
import userEvent from '@testing-library/user-event'

// ============================================================================
// LoadingSpinner Tests
// ============================================================================
describe('LoadingSpinner', () => {
  it('renders with default size', () => {
    const { container } = render(<LoadingSpinner />)
    const spinner = container.querySelector('svg')
    expect(spinner).toBeInTheDocument()
    expect(spinner).toHaveClass('animate-spin')
  })

  it('renders with small size', () => {
    const { container } = render(<LoadingSpinner size="sm" />)
    const spinner = container.querySelector('svg')
    expect(spinner).toHaveClass('h-4', 'w-4')
  })

  it('renders with large size', () => {
    const { container } = render(<LoadingSpinner size="lg" />)
    const spinner = container.querySelector('svg')
    expect(spinner).toHaveClass('h-8', 'w-8')
  })

  it('accepts custom className', () => {
    const { container } = render(<LoadingSpinner className="custom-class" />)
    const spinner = container.querySelector('svg')
    expect(spinner).toHaveClass('custom-class')
  })
})

// ============================================================================
// LoadingOverlay Tests
// ============================================================================
describe('LoadingOverlay', () => {
  it('renders with default message', () => {
    render(<LoadingOverlay />)
    expect(screen.getByText('Loading...')).toBeInTheDocument()
  })

  it('renders with custom message', () => {
    render(<LoadingOverlay message="Please wait..." />)
    expect(screen.getByText('Please wait...')).toBeInTheDocument()
  })

  it('contains a spinner', () => {
    const { container } = render(<LoadingOverlay />)
    const spinner = container.querySelector('svg')
    expect(spinner).toBeInTheDocument()
    expect(spinner).toHaveClass('animate-spin')
  })
})

// ============================================================================
// LoadingButton Tests
// ============================================================================
describe('LoadingButton', () => {
  it('renders children when not loading', () => {
    render(<LoadingButton isLoading={false}>Submit</LoadingButton>)
    expect(screen.getByText('Submit')).toBeInTheDocument()
  })

  it('renders loading text when loading', () => {
    render(
      <LoadingButton isLoading={true} loadingText="Saving...">
        Submit
      </LoadingButton>
    )
    expect(screen.getByText('Saving...')).toBeInTheDocument()
    expect(screen.queryByText('Submit')).not.toBeInTheDocument()
  })

  it('renders default loading text when no loadingText provided', () => {
    render(<LoadingButton isLoading={true}>Submit</LoadingButton>)
    expect(screen.getByText('Loading...')).toBeInTheDocument()
  })

  it('is disabled when loading', () => {
    render(<LoadingButton isLoading={true}>Submit</LoadingButton>)
    const button = screen.getByRole('button')
    expect(button).toBeDisabled()
  })

  it('is disabled when disabled prop is true', () => {
    render(
      <LoadingButton isLoading={false} disabled={true}>
        Submit
      </LoadingButton>
    )
    const button = screen.getByRole('button')
    expect(button).toBeDisabled()
  })

  it('calls onClick when clicked and not loading', async () => {
    const handleClick = vi.fn()
    const user = userEvent.setup()

    render(
      <LoadingButton isLoading={false} onClick={handleClick}>
        Submit
      </LoadingButton>
    )

    await user.click(screen.getByRole('button'))
    expect(handleClick).toHaveBeenCalledTimes(1)
  })

  it('does not call onClick when loading', async () => {
    const handleClick = vi.fn()
    const user = userEvent.setup()

    render(
      <LoadingButton isLoading={true} onClick={handleClick}>
        Submit
      </LoadingButton>
    )

    await user.click(screen.getByRole('button'))
    expect(handleClick).not.toHaveBeenCalled()
  })

  it('has correct button type', () => {
    render(
      <LoadingButton isLoading={false} type="submit">
        Submit
      </LoadingButton>
    )
    const button = screen.getByRole('button')
    expect(button).toHaveAttribute('type', 'submit')
  })
})

// ============================================================================
// PageLoader Tests
// ============================================================================
describe('PageLoader', () => {
  it('renders without message', () => {
    const { container } = render(<PageLoader />)
    const spinner = container.querySelector('svg')
    expect(spinner).toBeInTheDocument()
  })

  it('renders with message', () => {
    render(<PageLoader message="Loading data..." />)
    expect(screen.getByText('Loading data...')).toBeInTheDocument()
  })
})

// ============================================================================
// InlineLoader Tests
// ============================================================================
describe('InlineLoader', () => {
  it('renders without message', () => {
    const { container } = render(<InlineLoader />)
    const spinner = container.querySelector('svg')
    expect(spinner).toBeInTheDocument()
  })

  it('renders with message', () => {
    render(<InlineLoader message="Processing..." />)
    expect(screen.getByText('Processing...')).toBeInTheDocument()
  })
})
