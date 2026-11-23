import { describe, it, expect, beforeEach } from 'vitest'
import { tabsToStore, loadOpenTabs, saveOpenTabs } from './open-tabs.js'

/** @type {Map<string, string>} */
let store
beforeEach(() => {
  store = new Map()
  globalThis.localStorage = /** @type {any} */ ({
    getItem: (/** @type {string} */ k) => store.get(k) ?? null,
    setItem: (/** @type {string} */ k, /** @type {string} */ v) => void store.set(k, v),
  })
})

const table = (id, name, extra = {}) => ({
  id, kind: 'table', title: name,
  state: { schema: 'public', table: name, tableKind: 'table', rows: [[1]], columns: [{ name: 'id' }], rowSort: null, rowFilters: [], rowSearch: '', pageSize: 50, ...extra },
})

describe('tabsToStore', () => {
  it('keeps what reopens a tab and drops rows and session-bound tabs', () => {
    const tabs = [
      { id: 'w', kind: 'welcome' },
      table('t1', 'orders', { rowSort: { column: 'id', direction: 'desc' }, rowSearch: 'x' }),
      { id: 's', kind: 'sql', title: 'Query Editor' },
      { id: 'd', kind: 'sql', title: 'DDL', draft: false },
      { id: 'term', kind: 'terminal' },
      { id: 'ai', kind: 'ai' },
      { id: 'e', kind: 'erd', pinned: true },
      { id: 'c', kind: 'docs', collection: 'shop.users', title: 'users' },
    ]
    const out = tabsToStore(tabs, 'e')
    expect(out).toEqual([
      { kind: 'table', schema: 'public', table: 'orders', tableKind: 'table', title: 'orders', view: { rowSort: { column: 'id', direction: 'desc' }, rowSearch: 'x', pageSize: 50 } },
      { kind: 'sql' },
      { kind: 'erd', pinned: true, active: true },
      { kind: 'docs', collection: 'shop.users', title: 'users' },
    ])
    expect(JSON.stringify(out)).not.toContain('rows')
  })

  it('skips a table tab with no table picked yet', () => {
    expect(tabsToStore([table('t', null)], null)).toEqual([])
  })
})

describe('loadOpenTabs / saveOpenTabs', () => {
  it('round-trips per database and forgets the oldest past the cap', () => {
    saveOpenTabs('a::db', [{ kind: 'erd' }])
    expect(loadOpenTabs('a::db')).toEqual([{ kind: 'erd' }])
    expect(loadOpenTabs('b::db')).toEqual([])
    for (let i = 0; i < 70; i++) saveOpenTabs(`k${i}`, [{ kind: 'sql' }])
    const all = JSON.parse(/** @type {string} */ (store.get('stroke:open-tabs')))
    expect(Object.keys(all).length).toBe(60)
  })

  it('reads a damaged entry as nothing', () => {
    store.set('stroke:open-tabs', '{"x::db":{"tabs":[null,{"nope":1},{"kind":"sql"}]}}')
    expect(loadOpenTabs('x::db')).toEqual([{ kind: 'sql' }])
    store.set('stroke:open-tabs', 'not json')
    expect(loadOpenTabs('x::db')).toEqual([])
  })
})
