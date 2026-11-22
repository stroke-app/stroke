import { describe, it, expect, vi } from 'vitest'
import { toast } from './toast.svelte.js'

describe('toast store', () => {
  it('replaces a keyed toast in place, even one with an action', () => {
    toast.dismiss()
    const action = { label: 'Edit host', onClick: () => {} }
    const a = toast.error('Nothing answered at that address', { key: 'connect-failure', action })
    const b = toast.error('Nothing answered at that address', { key: 'connect-failure', action })
    expect(b).toBe(a)
    toast.error("You're offline", { key: 'connect-failure' })
    const up = toast.getActiveToasts()
    expect(up).toHaveLength(1)
    expect(up[0].title).toBe("You're offline")
    expect(up[0].action).toBeUndefined()
    toast.dismiss()
  })

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
