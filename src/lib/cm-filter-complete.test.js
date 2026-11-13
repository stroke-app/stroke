import { describe, expect, it } from 'vitest'
import { EditorState } from '@codemirror/state'
import { CompletionContext } from '@codemirror/autocomplete'
import { filterCompletionSource } from './cm-filter-complete.js'

const cols = [{ name: 'id', type: 'int4' }, { name: 'created at', type: 'timestamptz' }]

/** @param {string} doc @param {string} [dialect] @param {boolean} [explicit] */
function complete(doc, dialect = 'postgres', explicit = false) {
  const state = EditorState.create({ doc })
  const src = filterCompletionSource(() => cols, () => dialect)
  return /** @type {any} */ (src(new CompletionContext(state, doc.length, explicit)))
}

describe('filterCompletionSource', () => {
  it('offers the table columns first, quoting names that need it', () => {
    const r = complete('i')
    expect(r.from).toBe(0)
    const col = r.options.filter((o) => o.type === 'column')
    expect(col.map((o) => o.label)).toEqual(['id', 'created at'])
    expect(col[1].apply).toBe('"created at"')
    expect(Math.min(...col.map((o) => o.boost))).toBeGreaterThan(Math.max(...r.options.filter((o) => o.type !== 'column').map((o) => o.boost)))
  })

  it('uses backticks on MySQL and leaves aggregates out', () => {
    const r = complete('cre', 'mysql')
    expect(r.options.find((o) => o.label === 'created at').apply).toBe('`created at`')
    expect(r.options.some((o) => o.label === 'count')).toBe(false)
  })

  it('is quiet inside a string and with nothing typed', () => {
    expect(complete("name = 'ab")).toBeNull()
    expect(complete('id > ')).toBeNull()
    expect(complete('id > ', 'postgres', true)).not.toBeNull()
  })
})
