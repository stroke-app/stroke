import { beforeEach, describe, expect, it } from 'vitest'
import { clearRecentTabs, loadRecentTabs, pushRecentTab } from './recent-tabs.js'

describe('recent tabs clear undo', () => {
  beforeEach(() => {
    const map = new Map()
    globalThis.localStorage = /** @type {any} */ ({
      getItem: (k) => (map.has(k) ? map.get(k) : null),
      setItem: (k, v) => map.set(k, String(v)),
      removeItem: (k) => map.delete(k),
    })
  })
  it('puts the list back', () => {
    pushRecentTab('c', { schema: 'public', table: 'a', tableKind: 'table' })
    pushRecentTab('c', { schema: 'public', table: 'b', tableKind: 'table' })
    const undo = clearRecentTabs('c')
    expect(loadRecentTabs('c')).toEqual([])
    undo?.()
    expect(loadRecentTabs('c').map((r) => r.table)).toEqual(['b', 'a'])
    expect(clearRecentTabs('empty')).toBe(null)
  })
})
