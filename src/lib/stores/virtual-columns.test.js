import { describe, expect, it } from 'vitest'
import { get } from 'svelte/store'

const map = new Map()
globalThis.localStorage = /** @type {any} */ ({
  getItem: (k) => (map.has(k) ? map.get(k) : null),
  setItem: (k, v) => map.set(k, String(v)),
  removeItem: (k) => map.delete(k),
})
const { virtualColumnsStore } = await import('./virtual-columns.js')

describe('virtual column delete undo', () => {
  it('puts a deleted column back at its index, once', () => {
    for (const name of ['a', 'b', 'c']) virtualColumnsStore.add('public.t', { name, expression: '1', enabled: true })
    const names = () => (get(virtualColumnsStore)['public.t'] ?? []).map((c) => c.name)
    const mid = get(virtualColumnsStore)['public.t'][1].id
    const undo = virtualColumnsStore.remove('public.t', mid)
    expect(names()).toEqual(['a', 'c'])
    expect(undo?.()).toBe(true)
    expect(names()).toEqual(['a', 'b', 'c'])
    expect(undo?.()).toBe(false)
    expect(virtualColumnsStore.remove('public.t', 'missing')).toBe(null)
  })
})
