import { describe, it, expect, beforeEach } from 'vitest'

// A minimal localStorage (the test environment has none).
const mem = new Map()
globalThis.localStorage = /** @type {any} */ ({
  getItem: (k) => (mem.has(k) ? mem.get(k) : null),
  setItem: (k, v) => mem.set(k, String(v)),
  removeItem: (k) => mem.delete(k),
  clear: () => mem.clear(),
})
const { loadSqlDraft, saveSqlDraft, loadSqlTabs, saveSqlTabs } = await import('./sql-draft.js')

const tab = (sql) => ({ title: 'Query Editor', sql, savedQueryId: null, active: true })

describe('sql drafts per database', () => {
  beforeEach(() => localStorage.clear())

  it('keeps each database of one connection apart', () => {
    saveSqlTabs('conn::app', [tab('select * from users')])
    saveSqlTabs('conn::analytics', [tab('select * from events')])
    expect(loadSqlTabs('conn::app')[0].sql).toBe('select * from users')
    expect(loadSqlTabs('conn::analytics')[0].sql).toBe('select * from events')
    expect(loadSqlTabs('conn::other')).toEqual([])
  })

  it('hands an old connection-wide entry to the first database only', () => {
    saveSqlTabs('conn', [tab('select 1')])
    saveSqlDraft('conn', 'select 1')
    expect(loadSqlTabs('conn::app')[0].sql).toBe('select 1')
    expect(loadSqlDraft('conn::app')).toBe('select 1')
    // Moved, not copied: the next database starts empty.
    expect(loadSqlTabs('conn::analytics')).toEqual([])
    expect(loadSqlDraft('conn::analytics')).toBeNull()
    expect(loadSqlTabs('conn::app')[0].sql).toBe('select 1')
  })
})
