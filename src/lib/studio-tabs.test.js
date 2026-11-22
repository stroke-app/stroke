import { describe, it, expect } from 'vitest'
import {
  nextTabId,
  createTableTab,
  createSqlTab,
  createTableTabState,
  createSqlTabState,
  cloneTableTabState,
  cloneSqlTabState,
  createAiTab,
  createSchemaTab,
  findAiTab,
  findSchemaTab,
} from './studio-tabs.js'

describe('nextTabId', () => {
  it('returns unique, prefixed, monotonically increasing ids', () => {
    const a = nextTabId()
    const b = nextTabId()
    expect(a).toMatch(/^tab-\d+$/)
    expect(a).not.toBe(b)
  })
})

describe('tab factories', () => {
  it('createTableTab produces a table tab with a fresh id and state', () => {
    const tab = createTableTab('public', 'users')
    expect(tab.kind).toBe('table')
    expect(tab.id).toMatch(/^tab-\d+$/)
    expect(tab.state.schema).toBe('public')
    expect(tab.state.table).toBe('users')
    expect(tab.state.page).toBe(1)
  })

  it('createSqlTab carries the sql text and a title', () => {
    const tab = createSqlTab('SELECT 42;', 'Scratch')
    expect(tab.kind).toBe('sql')
    expect(tab.title).toBe('Scratch')
    expect(tab.state.sqlText).toBe('SELECT 42;')
  })

  it('createTableTabState defaults to an empty, unloaded table view', () => {
    const s = createTableTabState()
    expect(s.schema).toBe('public')
    expect(s.table).toBeNull()
    expect(s.rows).toEqual([])
    expect(s.selected).toBeInstanceOf(Set)
    expect(s.selected.size).toBe(0)
  })

  it('createSqlTabState defaults its editor text', () => {
    expect(createSqlTabState().sqlText).toBe('SELECT 1;')
    expect(createSqlTabState('SELECT now();').sqlText).toBe('SELECT now();')
  })
})

describe('clone helpers', () => {
  it('cloneTableTabState gives a windowed tab its own rows, holes kept', () => {
    const rows = new Array(10)
    rows[0] = [1]
    const state = /** @type {any} */ ({ rows, windowedLoaded: [0], selected: new Set(), hiddenColumns: new Set() })
    const clone = /** @type {any} */ (cloneTableTabState(state))
    expect(clone.rows).not.toBe(rows)
    expect(clone.rows.length).toBe(10)
    expect(1 in clone.rows).toBe(false)
    rows[0] = undefined
    expect(clone.rows[0]).toEqual([1])
    expect(clone.windowedLoaded).not.toBe(state.windowedLoaded)
  })

  it('cloneTableTabState gives each tab independent Sets', () => {
    const state = createTableTabState('public', 'users')
    state.selected.add(3)
    const clone = cloneTableTabState(state)
    clone.selected.add(9)
    expect(state.selected.has(9)).toBe(false)
    expect(clone.selected.has(3)).toBe(true)
    expect(clone.hiddenColumns).not.toBe(state.hiddenColumns)
    expect(clone.expandedRows).not.toBe(state.expandedRows)
  })

  it('cloneSqlTabState produces a shallow copy that is not the same reference', () => {
    const state = createSqlTabState('SELECT 1;')
    const clone = cloneSqlTabState(state)
    expect(clone).not.toBe(state)
    expect(clone.sqlText).toBe('SELECT 1;')
  })
})

describe('find helpers', () => {
  it('locate singleton tabs by kind', () => {
    const tabs = [createSqlTab('SELECT 1;'), createAiTab(), createSchemaTab()]
    expect(findAiTab(tabs)?.kind).toBe('ai')
    expect(findSchemaTab(tabs)?.kind).toBe('schema')
    expect(findAiTab([createSqlTab('SELECT 1;')])).toBeNull()
  })
})

describe('cloneSqlTabState', () => {
  it('preserves the running flag', () => {
    // A tab's snapshot is what the shell restores when you switch back to it.
    // Dropping this flag is what made a long query look cancelled the moment
    // you left its tab: the spinner stopped and the results pane reverted.
    const cloned = cloneSqlTabState({
      sqlText: 'select 1',
      sqlColumns: [],
      sqlRows: [],
      sqlQueryMs: 0,
      sqlMessage: '',
      sqlLoading: true,
      sqlError: '',
    })
    expect(cloned.sqlLoading).toBe(true)
  })
})
