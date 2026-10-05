import { describe, it, expect, beforeEach } from 'vitest'
import { planSave, savedQueryFor, sqlTabsToStore, sameSql } from '$lib/sql-saving.js'
import { loadSqlTabs, saveSqlTabs, loadSqlDraft, saveSqlDraft } from '$lib/stores/sql-draft.js'

const q = (id, sql, name = id) => ({ id, connectionId: 'c1', name, sql, createdAt: 1, updatedAt: 1 })

describe('planSave', () => {
  const saved = [q('a', 'SELECT 1'), q('b', 'SELECT * FROM users')]

  it('writes a linked tab in place when its text changed', () => {
    expect(planSave('SELECT 2', 'a', saved)).toEqual({ kind: 'update', query: saved[0] })
  })

  it('does nothing for a linked tab whose text is the saved text', () => {
    expect(planSave('  SELECT 1\n', 'a', saved)).toEqual({ kind: 'unchanged', query: saved[0] })
  })

  it('links an unlinked tab to the query that already holds its text instead of copying it', () => {
    expect(planSave('SELECT * FROM users', null, saved)).toEqual({ kind: 'link', query: saved[1] })
  })

  it('falls back to the text when the linked query was deleted', () => {
    expect(planSave('SELECT 1', 'gone', saved)).toEqual({ kind: 'link', query: saved[0] })
  })

  it('asks for a name only for text nobody saved', () => {
    expect(planSave('SELECT 3', null, saved)).toEqual({ kind: 'ask' })
    expect(planSave('SELECT 3', 'gone', saved)).toEqual({ kind: 'ask' })
  })
})

describe('savedQueryFor / sameSql', () => {
  it('matches on trimmed text and never on a blank buffer', () => {
    const saved = [q('a', 'SELECT 1')]
    expect(savedQueryFor(' SELECT 1 ', saved)?.id).toBe('a')
    expect(savedQueryFor('   ', [q('blank', '')])).toBeNull()
    expect(sameSql('SELECT 1', 'select 1')).toBe(false)
  })
})

describe('sqlTabsToStore', () => {
  it('keeps the editor tabs in order, takes the live text for the tab in front, skips DDL viewers', () => {
    const tabs = [
      { id: 't1', kind: 'sql', title: 'Query Editor', state: { sqlText: 'SELECT 1' } },
      { id: 't2', kind: 'table', title: 'users', state: {} },
      { id: 't3', kind: 'sql', title: 'Revenue', savedQueryId: 'a', state: { sqlText: 'stale' } },
      { id: 't4', kind: 'sql', title: 'users.sql', draft: false, state: { sqlText: 'CREATE TABLE users ()' } },
    ]
    expect(sqlTabsToStore(tabs, 't3', 'SELECT 2')).toEqual([
      { title: 'Query Editor', sql: 'SELECT 1', savedQueryId: null, active: false },
      { title: 'Revenue', sql: 'SELECT 2', savedQueryId: 'a', active: true },
    ])
  })
})

describe('sql-draft tab store', () => {
  beforeEach(() => {
    const map = new Map()
    globalThis.localStorage = /** @type {any} */ ({
      getItem: (k) => (map.has(k) ? map.get(k) : null),
      setItem: (k, v) => { map.set(k, String(v)) },
      removeItem: (k) => { map.delete(k) },
    })
  })

  it('round-trips each connection separately', () => {
    const one = [
      { title: 'Query Editor', sql: 'SELECT 1', savedQueryId: null, active: false },
      { title: 'Revenue', sql: 'SELECT 2', savedQueryId: 'a', active: true },
    ]
    saveSqlTabs('c1', one)
    saveSqlTabs('c2', [{ title: 'Other', sql: 'SELECT 3', savedQueryId: null, active: true }])
    expect(loadSqlTabs('c1')).toEqual(one)
    expect(loadSqlTabs('c2')).toHaveLength(1)
    expect(loadSqlTabs('c3')).toEqual([])
  })

  it('forgets a connection when its list empties, and leaves the single draft alone', () => {
    saveSqlDraft('c1', 'SELECT draft')
    saveSqlTabs('c1', [{ title: 'Query Editor', sql: 'x', savedQueryId: null, active: true }])
    saveSqlTabs('c1', [])
    expect(loadSqlTabs('c1')).toEqual([])
    expect(loadSqlDraft('c1')).toBe('SELECT draft')
  })

  it('drops malformed entries instead of restoring them half-built', () => {
    localStorage.setItem('stroke:sql-tabs', JSON.stringify({ c1: [null, { title: 3 }, { sql: 'SELECT 1', title: '', savedQueryId: 7 }] }))
    expect(loadSqlTabs('c1')).toEqual([{ title: 'Query Editor', sql: 'SELECT 1', savedQueryId: null, active: false }])
    localStorage.setItem('stroke:sql-tabs', 'not json')
    expect(loadSqlTabs('c1')).toEqual([])
  })
})
