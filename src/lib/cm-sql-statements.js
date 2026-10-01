/**
 * The statements of an editor state, split once per document version.
 *
 * A CodeMirror document (Text) is immutable, so it keys the cache: caret moves
 * cost nothing, and the SQL editor (active-statement bar, run keys) and the
 * completion source share one split per edit instead of each making their own.
 */
import { splitSqlStatements, statementAtOffset } from '$lib/sql-statements.js'

/** @type {WeakMap<object, import('$lib/sql-statements.js').SqlStatement[]>} */
const byDoc = new WeakMap()

/** @param {import('@codemirror/state').EditorState} state */
export function statementsOf(state) {
  let s = byDoc.get(state.doc)
  if (!s) {
    s = splitSqlStatements(state.doc.toString())
    byDoc.set(state.doc, s)
  }
  return s
}

/** @param {import('@codemirror/state').EditorState} state @param {number} pos */
export function statementAt(state, pos) {
  return statementAtOffset(statementsOf(state), pos)
}
