import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { WAKE_DELAY_MS, WAKE_MESSAGE, WakeNotice } from './WakeNotice'

function renderWithPendingQuery() {
  const queryClient = new QueryClient()
  void queryClient.prefetchQuery({ queryKey: ['lente'], queryFn: () => new Promise(() => {}) })
  render(
    <QueryClientProvider client={queryClient}>
      <WakeNotice />
    </QueryClientProvider>,
  )
}

describe('message de réveil du serveur', () => {
  beforeEach(() => vi.useFakeTimers())
  afterEach(() => vi.useRealTimers())

  it('apparaît quand une requête dure plus que le délai, pas avant', () => {
    renderWithPendingQuery()

    act(() => vi.advanceTimersByTime(WAKE_DELAY_MS - 1))
    expect(screen.queryByText(WAKE_MESSAGE)).toBeNull()

    act(() => vi.advanceTimersByTime(1))
    expect(screen.getByText(WAKE_MESSAGE)).toBeTruthy()
  })

  it('reste caché sans requête en cours', () => {
    render(
      <QueryClientProvider client={new QueryClient()}>
        <WakeNotice />
      </QueryClientProvider>,
    )
    act(() => vi.advanceTimersByTime(WAKE_DELAY_MS * 2))
    expect(screen.queryByText(WAKE_MESSAGE)).toBeNull()
  })
})
