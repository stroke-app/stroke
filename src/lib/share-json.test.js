import { describe, it, expect, vi } from 'vitest'

vi.mock('$lib/api.js', () => ({ shareCreate: vi.fn() }))
vi.mock('$lib/stores/settings.js', () => ({ loadSettings: () => ({}), updateSettings: () => {} }))

const { buildPayload, jsonFields, rowsToJson } = await import('./share-flow.js')

describe('JSON shares', () => {
  const docs = [
    { _id: 1, name: 'Ana', email: 'ana@example.com', tags: ['a'] },
    { _id: 2, name: 'Luka', email: 'luka@example.com', plan: 'pro' },
  ]

  it('lists top-level fields first seen first, across array items', () => {
    expect(jsonFields(docs).columns.map((c) => c.name)).toEqual(['_id', 'name', 'email', 'tags', 'plan'])
    expect(jsonFields({ a: 1, b: 2 }).columns.map((c) => c.name)).toEqual(['a', 'b'])
    expect(jsonFields(42).columns).toEqual([])
  })

  it('drops fields left out and masks masked ones, keeping nested values', () => {
    const p = /** @type {any} */ (buildPayload({ kind: 'json', title: 'Users', value: docs }, { title: 'Users', keep: new Set([0, 1, 2, 3]), mask: new Set([2]) }))
    expect(p.kind).toBe('json')
    expect(p.value[0]).toEqual({ _id: 1, name: 'Ana', email: expect.not.stringContaining('ana@'), tags: ['a'] })
    expect(p.value[1]).not.toHaveProperty('plan')
    expect(p.masked).toEqual(['email'])
    expect(p.totalItems).toBe(2)
  })

  it('caps an array at the plan limit and keeps a lone object as is', () => {
    const many = Array.from({ length: 5 }, (_, i) => ({ i }))
    const p = /** @type {any} */ (buildPayload({ kind: 'json', title: 'x', value: many }, { title: 'x', rowCap: 3 }))
    expect(p.value).toHaveLength(3)
    expect(p.totalItems).toBe(5)
    const one = /** @type {any} */ (buildPayload({ kind: 'json', title: 'x', value: { a: [1, 2] } }, { title: 'x' }))
    expect(one.value).toEqual({ a: [1, 2] })
    expect(one.totalItems).toBeUndefined()
  })

  it('turns rows into one object per row', () => {
    expect(rowsToJson([{ name: 'id' }, { name: 'v' }], [[1, 'a'], [2, null]])).toEqual([{ id: 1, v: 'a' }, { id: 2, v: null }])
  })
})
