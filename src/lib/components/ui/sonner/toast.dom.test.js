import { describe, it, expect, vi } from 'vitest'
import { toast } from './toast.svelte.js'

describe('toast store', () => {
  it('shows a repeated message once, and different ones apart', () => {
    vi.useFakeTimers()
    toast.dismiss()
    const a = toast.error('Cannot edit', { description: 'This table has no primary key.' })
    const b = toast.error('Cannot edit', { description: 'This table has no primary key.' })
    expect(b).toBe(a)
    expect(toast.toasts).toHaveLength(1)
    toast.error('Cannot edit', { description: 'Read-only.' })
    toast.success('Cannot edit', { description: 'This table has no primary key.' })
    expect(toast.toasts).toHaveLength(3)
    toast.dismiss()
    vi.useRealTimers()
  })
})
