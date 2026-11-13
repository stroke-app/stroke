/**
 * Completion for the grid's SQL filter: a WHERE condition, not a statement.
 *
 * The SQL editor's completer reads the whole statement to decide what fits
 * where (tables after FROM, columns of the tables it names). A filter like
 * `id > 200` has no FROM - the table is implicit - so that completer offers
 * statement keywords where a column belongs. This one offers what a condition
 * is made of: the table's columns first, then the engine's functions, then the
 * operators and literals that only make sense inside a WHERE.
 */
import { snippetCompletion } from '@codemirror/autocomplete'
import { PG_FUNCTIONS, DIALECT_FUNCTIONS, sqlFamily } from './sql-complete-data.js'
import { quoteName } from './sql-ident.js'

/** Words that continue or close a condition. */
export const FILTER_KEYWORDS = [
  'AND', 'OR', 'NOT', 'IS NULL', 'IS NOT NULL', 'IN', 'NOT IN', 'LIKE', 'NOT LIKE',
  'BETWEEN', 'EXISTS', 'TRUE', 'FALSE', 'NULL', 'CASE', 'WHEN', 'THEN', 'ELSE', 'END',
]

/** Aggregates can't appear in a WHERE, so a filter doesn't offer them. */
const AGGREGATES = new Set(['count', 'sum', 'avg', 'min', 'max', 'array_agg', 'string_agg', 'json_agg', 'jsonb_agg', 'group_concat', 'bool_and', 'bool_or', 'every'])

/**
 * @param {() => { name: string, type?: string }[]} getColumns the table's columns, read per query
 * @param {() => string} getDialect the app's Dialect id
 * @returns {import('@codemirror/autocomplete').CompletionSource}
 */
export function filterCompletionSource(getColumns, getDialect) {
  return (context) => {
    const before = context.state.sliceDoc(0, context.pos)
    // Inside a string literal there is nothing to suggest.
    if ((before.match(/'/g)?.length ?? 0) % 2 === 1) return null
    const word = context.matchBefore(/[A-Za-z_][\w$]*/)
    if (!word && !context.explicit) return null

    const dialect = getDialect()
    const family = sqlFamily(dialect)
    /** @type {import('@codemirror/autocomplete').Completion[]} */
    const options = []
    for (const c of getColumns()) {
      options.push({
        label: c.name,
        apply: quoteName(c.name, dialect, 'auto'),
        type: 'column',
        detail: c.type || undefined,
        boost: 20,
      })
    }
    const fns = [
      ...(family === 'postgres' ? PG_FUNCTIONS : []),
      ...DIALECT_FUNCTIONS.filter((f) => !f.only || f.only.includes(family)),
    ]
    const seen = new Set()
    for (const f of fns) {
      if (AGGREGATES.has(f.label) || seen.has(f.label)) continue
      seen.add(f.label)
      options.push(snippetCompletion(f.sig ?? `${f.label}()`, { label: f.label, type: 'function', info: f.doc, boost: -10 }))
    }
    for (const k of FILTER_KEYWORDS) options.push({ label: k, type: 'keyword', boost: 0 })
    return { from: word ? word.from : context.pos, options, validFor: /^[\w$]*$/ }
  }
}
