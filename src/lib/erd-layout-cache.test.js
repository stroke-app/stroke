import { describe, it, expect, beforeEach } from 'vitest'
import { layoutKey, loadLayout, saveLayout } from './erd-layout-cache.js'

/** A tiny in-memory localStorage for the node test environment. */
beforeEach(() => {
  const store = new Map()
  globalThis.localStorage = /** @type {any} */ ({
    getItem: (k) => (store.has(k) ? store.get(k) : null),
    setItem: (k, v) => { store.set(k, String(v)) },
    removeItem: (k) => { store.delete(k) },
  })
})

const graph = {
  connectionId: 'c1',
  schema: 'public',
  shape: 'all|full|normal|pills|false|0|false',
  nodes: [{ id: 'users', rows: 5 }, { id: 'orders', rows: 7 }],
  edges: [{ id: 'orders-users', source: 'orders', target: 'users' }],
}

describe('erd layout cache', () => {
  it('gives the same key whatever order the tables come in', () => {
    const a = layoutKey(graph)
    const b = layoutKey({ ...graph, nodes: [...graph.nodes].reverse() })
    expect(a).toBe(b)
  })

  it('changes the key when a table, a column count, a line or a setting changes', () => {
    const k = layoutKey(graph)
    expect(layoutKey({ ...graph, nodes: [...graph.nodes, { id: 'x', rows: 1 }] })).not.toBe(k)
    expect(layoutKey({ ...graph, nodes: [{ id: 'users', rows: 6 }, graph.nodes[1]] })).not.toBe(k)
    expect(layoutKey({ ...graph, edges: [] })).not.toBe(k)
    expect(layoutKey({ ...graph, shape: 'all|keys|normal|pills|false|0|false' })).not.toBe(k)
    expect(layoutKey({ ...graph, schema: 'other' })).not.toBe(k)
  })

  it('round-trips positions and line points', () => {
    const k = layoutKey(graph)
    const positions = new Map([['users', { x: 10.4, y: 20 }], ['orders', { x: 300, y: 40 }]])
    const hints = new Map([['orders-users', [{ x: 1, y: 2 }, { x: 3, y: 4 }]]])
    saveLayout(k, positions, hints)
    const got = loadLayout(k, ['users', 'orders'])
    expect(got?.positions.get('users')).toEqual({ x: 10, y: 20 })
    expect(got?.hints.get('orders-users')).toEqual([{ x: 1, y: 2 }, { x: 3, y: 4 }])
  })

  it('refuses a saved layout that does not place every card', () => {
    const k = layoutKey(graph)
    saveLayout(k, new Map([['users', { x: 0, y: 0 }]]), new Map())
    expect(loadLayout(k, ['users', 'orders'])).toBeNull()
  })

  it('keeps at most eight diagrams, dropping the oldest', () => {
    const keys = Array.from({ length: 9 }, (_, i) => layoutKey({ ...graph, schema: `s${i}` }))
    for (const k of keys) saveLayout(k, new Map([['users', { x: 0, y: 0 }]]), new Map())
    expect(loadLayout(keys[0], ['users'])).toBeNull()
    expect(loadLayout(keys[8], ['users'])).not.toBeNull()
  })
})

describe('erd layout cache rows', () => {
  it('misses when a column is renamed or moved, even with the same count', () => {
    const base = { ...graph, nodes: [{ id: 'users', rows: 'id,name' }, { id: 'orders', rows: 'id,user_id' }] }
    const k = layoutKey(base)
    expect(layoutKey({ ...base, nodes: [{ id: 'users', rows: 'name,id' }, base.nodes[1]] })).not.toBe(k)
    expect(layoutKey({ ...base, nodes: [{ id: 'users', rows: 'id,email' }, base.nodes[1]] })).not.toBe(k)
  })
})
