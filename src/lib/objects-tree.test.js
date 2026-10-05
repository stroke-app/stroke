import { describe, expect, it } from 'vitest'
import { TREE_KINDS, buildGroups, flattenTree, nodeStartsOpen, rowIndex, typeAheadIndex } from './objects-tree.js'

const fn = (name, extra = {}) => ({ kind: 'function', name, args: '', argTypes: '', detail: '→ integer', subtype: '', ext: '', ...extra })
const OBJECTS = [
  { kind: 'view', name: 'active_users' },
  { kind: 'matview', name: 'order_totals', rowCount: 12 },
  fn('current_tenant_id', { detail: '→ uuid' }),
  fn('array_to_vector', { args: 'integer[], integer, boolean', argTypes: 'integer[], integer, boolean', ext: 'vector', detail: '→ vector' }),
  fn('array_to_vector', { args: 'real[], integer, boolean', argTypes: 'real[], integer, boolean', ext: 'vector', detail: '→ vector' }),
  fn('gtrgm_in', { ext: 'pg_trgm', detail: '→ gtrgm' }),
  fn('sum_amount', { subtype: 'aggregate', detail: 'aggregate → numeric' }),
  { kind: 'trigger', name: 'trg_orders', table: 'orders', detail: 'BEFORE UPDATE' },
  { kind: 'type', name: 'mood', subtype: 'enum', detail: 'enum · 3 values' },
]
const openAll = () => true
const openNone = () => false

describe('buildGroups', () => {
  it('files materialized views under Views and strings each row once', () => {
    const g = buildGroups(OBJECTS)
    expect(g.get('view')?.map((i) => [i.name, i.right])).toEqual([['active_users', ''], ['order_totals', 'materialized']])
    const fns = g.get('function') ?? []
    // The schema's own first, then extensions in name order; overloads are separate rows.
    expect(fns.map((i) => `${i.ext}:${i.name}`)).toEqual([':current_tenant_id', ':sum_amount', 'pg_trgm:gtrgm_in', 'vector:array_to_vector', 'vector:array_to_vector'])
    expect(new Set(fns.map((i) => i.key)).size).toBe(fns.length)
    // The return type moved off the row into the tooltip; the name keeps the width.
    expect(fns[0].right).toBe('')
    expect(fns[0].title).toContain('uuid')
    expect(fns[1].right).toBe('aggregate')
    expect(fns[3].args).toBe('integer[], integer, boolean')
    expect(fns[3].title).toContain('from the vector extension')
    expect(g.get('trigger')?.[0].right).toBe('on orders')
    expect(g.get('type')?.[0].right).toBe('enum')
  })
})

describe('flattenTree', () => {
  const groups = buildGroups(OBJECTS)

  it('leaves empty kinds out and puts extensions under their own nodes', () => {
    const { rows, total } = flattenTree(groups, TREE_KINDS, '', openAll)
    expect(rows.filter((r) => r.t === 'group').map((r) => r.key)).toEqual(['g:view', 'g:function', 'g:trigger', 'g:type'])
    expect(total).toBe(OBJECTS.length)
    const fnRows = rows.filter((r) => r.group === 'function').map((r) => (r.t === 'item' ? `${r.depth}:${r.item.name}` : `${r.t}:${r.label}:${r.count}`))
    expect(fnRows).toEqual(['group:Functions:5', '1:current_tenant_id', '1:sum_amount', 'ext:pg_trgm:1', '2:gtrgm_in', 'ext:vector:2', '2:array_to_vector', '2:array_to_vector'])
  })

  it('draws only headers for closed groups and closed extension nodes', () => {
    expect(flattenTree(groups, TREE_KINDS, '', openNone).rows.map((r) => r.t)).toEqual(['group', 'group', 'group', 'group'])
    const fnOpenOnly = flattenTree(groups, TREE_KINDS, '', (k) => k === 'g:function')
    expect(fnOpenOnly.rows.filter((r) => r.t === 'ext').map((r) => r.open)).toEqual([false, false])
    expect(fnOpenOnly.rows.some((r) => r.t === 'item' && r.depth === 2)).toBe(false)
  })

  it('a filter opens what matches, hides what does not, and counts matches', () => {
    const { rows, shown, total } = flattenTree(groups, TREE_KINDS, 'vector', openNone)
    expect(rows.map((r) => (r.t === 'item' ? r.item.name : `${r.t}:${r.label}`))).toEqual(['group:Functions', 'ext:vector', 'array_to_vector', 'array_to_vector'])
    expect(rows[0].t === 'group' && [rows[0].count, rows[0].total]).toEqual([2, 5])
    expect([shown, total]).toEqual([2, OBJECTS.length])
  })

  it('finds every row by key', () => {
    const { rows } = flattenTree(groups, TREE_KINDS, '', openAll)
    rows.forEach((r, n) => expect(rowIndex(rows, r.key)).toBe(n))
    expect(rowIndex(rows, 'nope')).toBe(-1)
  })

  it('flattens thousands of rows quickly', () => {
    const many = Array.from({ length: 10_000 }, (_, i) => fn(`f_${String(i).padStart(5, '0')}`, { ext: i % 3 ? '' : `ext${i % 7}` }))
    const g = buildGroups(many)
    const t0 = performance.now()
    for (let n = 0; n < 10; n++) flattenTree(g, TREE_KINDS, n % 2 ? 'f_01' : '', openAll)
    expect((performance.now() - t0) / 10).toBeLessThan(25)
  })
})

describe('typeAheadIndex', () => {
  it('finds the next row starting with the typed letters, wrapping', () => {
    const { rows } = flattenTree(buildGroups(OBJECTS), TREE_KINDS, '', openAll)
    const at = (name) => rows.findIndex((r) => r.t === 'item' && r.item.name === name)
    expect(rows[typeAheadIndex(rows, 0, 'c')].t === 'item' && rows[typeAheadIndex(rows, 0, 'c')]).toBeTruthy()
    expect(typeAheadIndex(rows, 0, 'cu')).toBe(at('current_tenant_id'))
    // The same letter again moves on to the next match.
    const first = typeAheadIndex(rows, -1, 'a')
    expect(typeAheadIndex(rows, first, 'a')).not.toBe(first)
    expect(typeAheadIndex(rows, 0, 'zz')).toBe(-1)
  })
})

describe('keys', () => {
  it('never repeats a key, even when the input does', () => {
    const items = buildGroups([fn('f', { args: 'int' }), fn('f', { args: 'int' })]).get('function') ?? []
    expect(new Set(items.map((i) => i.key)).size).toBe(2)
  })
})

describe('nodeStartsOpen', () => {
  it('opens Views and Functions by default and keeps what was remembered', () => {
    expect(nodeStartsOpen('g:view', undefined, true)).toBe(true)
    expect(nodeStartsOpen('g:function', {}, true)).toBe(true)
    expect(nodeStartsOpen('g:type', {}, true)).toBe(false)
    expect(nodeStartsOpen('x:function:vector', {}, true)).toBe(false)
    expect(nodeStartsOpen('g:type', { 'g:type': true }, true)).toBe(true)
    expect(nodeStartsOpen('g:view', { 'g:view': false }, true)).toBe(false)
    // With the setting off, the remembered state is ignored.
    expect(nodeStartsOpen('g:view', { 'g:view': false }, false)).toBe(true)
  })
})
