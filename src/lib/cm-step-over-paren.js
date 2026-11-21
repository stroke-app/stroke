/**
 * Typing `)` right before a `)` steps over it when the line's parentheses are
 * already balanced.
 *
 * A function completion is a snippet, `st_astext(${})`, and CodeMirror only
 * steps over the brackets its own closeBrackets inserted: finishing the call
 * by typing `geom)` left `st_astext(geom))`, an extra `)` that broke the
 * statement. Balance is the test so a nested call still gets the `)` it
 * needs: in `f(g(x|)` the line is one short and the typed `)` is inserted.
 */
import { EditorView } from '@codemirror/view'

/**
 * Whether a `)` typed at `pos` in `line` should step over the `)` already there.
 * Parentheses inside '...' and "..." are not counted.
 * @param {string} line @param {number} pos offset into `line`
 */
export function stepsOverParen(line, pos) {
  if (line[pos] !== ')') return false
  let depth = 0
  /** @type {string | null} */
  let quote = null
  for (let i = 0; i < line.length; i++) {
    const ch = line[i]
    if (quote) { if (ch === quote) quote = null; continue }
    if (ch === "'" || ch === '"') { quote = ch; continue }
    if (ch === '(') depth++
    else if (ch === ')') depth--
  }
  return depth <= 0
}

/** The CodeMirror side: an input handler for `)`. */
export const stepOverParen = EditorView.inputHandler.of((view, from, to, text) => {
  if (text !== ')' || from !== to) return false
  const line = view.state.doc.lineAt(from)
  if (!stepsOverParen(line.text, from - line.from)) return false
  view.dispatch({ selection: { anchor: from + 1 }, scrollIntoView: true, userEvent: 'input.type' })
  return true
})
