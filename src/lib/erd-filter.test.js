import { describe, it, expect } from 'vitest'
import { relatedTo, linkedTables, visibleTables, visibleRels, mergeParallelEdges, hubTables } from './erd-filter.js'

/** @param {[string,string][]} pairs */
const rels = (pairs) => pairs.map(([source, target]) => ({ source, target }))

const TABLES = ['users', 'orders', 'order_items', 'products', 'audit_log']
const FKS = rels([
  ['orders', 'users'],
  ['order_items', 'orders'],
  ['order_items', 'products'],
])

describe('relatedTo', () => {
  it('takes exactly one hop, in either direction', () => {
    expect([...relatedTo(['orders'], FKS)].sort()).toEqual(['order_items', 'orders', 'users'])
  })

  it('does not walk the whole component, whatever order the edges arrive in', () => {
    const forward = relatedTo(['users'], FKS)
    const reversed = relatedTo(['users'], [...FKS].reverse())
    expect([...forward].sort()).toEqual(['orders', 'users'])
    expect([...reversed].sort()).toEqual([...forward].sort())
  })
})

describe('linkedTables', () => {
  it('is every table on either end of a foreign key', () => {
    expect([...linkedTables(FKS)].sort()).toEqual(['order_items', 'orders', 'products', 'users'])
  })
})

describe('visibleTables', () => {
  it('shows the whole schema by default', () => {
    expect(visibleTables({ tables: TABLES, rels: FKS }).size).toBe(5)
  })

  it('drops the tables with no foreign key when asked', () => {
    const out = visibleTables({ tables: TABLES, rels: FKS, connectedOnly: true })
    expect(out.has('audit_log')).toBe(false)
    expect(out.size).toBe(4)
  })

  it('scopes to one table, or to one table and its neighbours', () => {
    const self = visibleTables({ tables: TABLES, rels: FKS, focusTable: 'orders', scope: 'self' })
    expect([...self]).toEqual(['orders'])
    const related = visibleTables({ tables: TABLES, rels: FKS, focusTable: 'orders', scope: 'related' })
    expect([...related].sort()).toEqual(['order_items', 'orders', 'users'])
  })

  it('draws exactly the picked tables', () => {
    const out = visibleTables({ tables: TABLES, rels: FKS, picked: new Set(['users', 'products']) })
    expect([...out].sort()).toEqual(['products', 'users'])
  })

  it('ignores a picked name that is not in the schema', () => {
    const out = visibleTables({ tables: TABLES, rels: FKS, picked: new Set(['users', 'ghost']) })
    expect([...out]).toEqual(['users'])
  })

  it('keeps the focused table on its own diagram even when the pick omits it', () => {
    const out = visibleTables({
      tables: TABLES, rels: FKS, focusTable: 'orders', scope: 'related',
      picked: new Set(['products']),
    })
    expect([...out].sort()).toEqual(['orders', 'products'])
  })

  it('lets a pick override the scope and the linked-only toggle', () => {
    const out = visibleTables({
      tables: TABLES, rels: FKS, scope: 'related', connectedOnly: true,
      picked: new Set(['audit_log']),
    })
    expect([...out]).toEqual(['audit_log'])
  })
})

describe('visibleRels', () => {
  it('keeps only relations with both ends on the diagram', () => {
    const visible = new Set(['orders', 'users'])
    expect(visibleRels(FKS, visible)).toEqual(rels([['orders', 'users']]))
  })

  it('in a neighbours diagram, only the focused table’s own relations', () => {
    const visible = visibleTables({ tables: TABLES, rels: FKS, focusTable: 'orders', scope: 'related' })
    const out = visibleRels(FKS, visible, { focusTable: 'orders', scope: 'related' })
    expect(out).toEqual(rels([['orders', 'users'], ['order_items', 'orders']]))
  })

  it('in a picked diagram, every relation among the picked tables', () => {
    const picked = new Set(['orders', 'users', 'order_items'])
    const visible = visibleTables({ tables: TABLES, rels: FKS, focusTable: 'orders', scope: 'related', picked })
    const out = visibleRels(FKS, visible, { focusTable: 'orders', scope: 'related', picked })
    expect(out).toEqual(rels([['orders', 'users'], ['order_items', 'orders']]))
  })
})

describe('mergeParallelEdges', () => {
  const fk = (id, source, target, extra = {}) =>
    ({ id, source, target, many: true, optional: false, ...extra })

  it('draws one line for two foreign keys pointing the same way', () => {
    const out = mergeParallelEdges([
      fk('posts__created_by__users', 'posts', 'users'),
      fk('posts__updated_by__users', 'posts', 'users'),
    ])
    expect(out).toHaveLength(1)
    expect(out[0].mergedCount).toBe(2)
    expect(out[0].mergedIds).toEqual(['posts__created_by__users', 'posts__updated_by__users'])
  })

  it('keeps the first of the group, so the line still anchors on a real column', () => {
    const out = mergeParallelEdges([
      fk('a', 'posts', 'users', { sourceHandle: 'src-created_by' }),
      fk('b', 'posts', 'users', { sourceHandle: 'src-updated_by' }),
    ])
    expect(out[0].id).toBe('a')
    expect(out[0].sourceHandle).toBe('src-created_by')
  })

  it('treats the two directions as different facts', () => {
    const out = mergeParallelEdges([fk('a', 'x', 'y'), fk('b', 'y', 'x')])
    expect(out).toHaveLength(2)
    for (const e of out) expect(e.mergedCount).toBe(1)
  })

  it('leaves a lone relationship exactly as it was, plus a count of one', () => {
    const only = fk('a', 'x', 'y')
    const out = mergeParallelEdges([only])
    expect(out[0]).toMatchObject({ ...only, mergedCount: 1 })
  })

  it('is many when any of the keys can repeat', () => {
    const out = mergeParallelEdges([
      fk('a', 'x', 'y', { many: false }),
      fk('b', 'x', 'y', { many: true }),
    ])
    expect(out[0].many).toBe(true)
  })

  it('is optional only when every one of the keys is nullable', () => {
    const allNull = mergeParallelEdges([
      fk('a', 'x', 'y', { optional: true }),
      fk('b', 'x', 'y', { optional: true }),
    ])
    expect(allNull[0].optional).toBe(true)

    const oneRequired = mergeParallelEdges([
      fk('a', 'x', 'y', { optional: true }),
      fk('b', 'x', 'y', { optional: false }),
    ])
    expect(oneRequired[0].optional).toBe(false)
  })

  it('keeps the input order of the surviving lines', () => {
    const out = mergeParallelEdges([
      fk('a', 'x', 'y'), fk('b', 'p', 'q'), fk('c', 'x', 'y'), fk('d', 'm', 'n'),
    ])
    expect(out.map((e) => e.id)).toEqual(['a', 'b', 'd'])
  })

  it('does not touch what it was given', () => {
    const input = [fk('a', 'x', 'y'), fk('b', 'x', 'y')]
    const copy = JSON.parse(JSON.stringify(input))
    mergeParallelEdges(input)
    expect(input).toEqual(copy)
  })

  it('has nothing to merge in an empty schema', () => {
    expect(mergeParallelEdges([])).toEqual([])
  })
})

describe('hubTables', () => {
  it('names the table most of the diagram points at, counting source tables', () => {
    const edges = []
    for (let i = 0; i < 20; i++) edges.push({ source: `t${i}`, target: 'tenants' })
    for (let i = 0; i < 3; i++) edges.push({ source: `t${i}`, target: 'users' })
    // Two keys from one table into `users` is still one table.
    edges.push({ source: 't0', target: 'users' })
    const hubs = hubTables(edges, 24)
    expect(hubs.has('tenants')).toBe(true)
    expect(hubs.has('users')).toBe(false)
  })
  it('needs the floor and the share, unless the count alone is a fan', () => {
    const refs = (/** @type {number} */ n) => Array.from({ length: n }, (_, i) => ({ source: `a${i}`, target: 'x' }))
    // Six referrers on a hundred tables (15% is fifteen): not a hub...
    expect(hubTables(refs(6), 100).has('x')).toBe(false)
    // ...on forty it is (15% is six).
    expect(hubTables(refs(6), 40).has('x')).toBe(true)
    // Five never is, however small the page.
    expect(hubTables(refs(5), 8).has('x')).toBe(false)
    // Twenty-five is, however large the page.
    expect(hubTables(refs(25), 500).has('x')).toBe(true)
  })
})
