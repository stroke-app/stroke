/**
 * SQL editor preferences (console and notebook cells): the option set, its
 * validation, and the fields the settings dialog renders. Kept apart from the
 * editor so the settings store can hold and check them without importing
 * CodeMirror - the same split as sql-format-options.js.
 */

/** @typedef {'small' | 'default' | 'large'} SqlEditorTextSize */
/** @typedef {{ textSize: SqlEditorTextSize, wrap: boolean, lineNumbers: boolean,
 *   folding: boolean, suggestWhileTyping: boolean, lint: boolean }} SqlEditorOptions */

/** @type {SqlEditorOptions} */
export const SQL_EDITOR_DEFAULTS = {
  textSize: 'default',
  wrap: true,
  lineNumbers: true,
  // Off: a query is a few lines, and an arrow beside every foldable one was
  // clutter in the gutter for a feature almost nobody reaches for there.
  folding: false,
  suggestWhileTyping: true,
  lint: true,
}

/** Text size → the type-scale step it follows, so it still scales with zoom. */
export const SQL_EDITOR_TEXT_SIZES = [
  { id: 'small', label: 'Small', css: 'var(--fs-2xs)' },
  { id: 'default', label: 'Default', css: 'var(--fs-xs)' },
  { id: 'large', label: 'Large', css: 'var(--fs-base)' },
]

/** Rows for the settings dialog, so it and the editor read one list. */
export const SQL_EDITOR_FIELDS = [
  { key: 'textSize', label: 'Text size', desc: 'Size of the text in the SQL editor. Follows the app zoom.', kind: 'textSize' },
  { key: 'wrap', label: 'Line wrap', desc: 'Wrap long lines instead of scrolling sideways.', kind: 'bool' },
  { key: 'lineNumbers', label: 'Line numbers', desc: 'Show line numbers in the gutter.', kind: 'bool' },
  { key: 'folding', label: 'Fold arrows', desc: 'Show arrows in the gutter to fold parenthesised blocks.', kind: 'bool' },
  { key: 'suggestWhileTyping', label: 'Suggestions while typing', desc: 'Open the completion list as you type. Ctrl+Space opens it either way.', kind: 'bool' },
  { key: 'lint', label: 'Problem markers', desc: 'Mark unterminated strings, unbalanced parentheses and statements missing a ;.', kind: 'bool' },
]

const SIZES = SQL_EDITOR_TEXT_SIZES.map((s) => s.id)
/** @param {unknown} v @param {boolean} fallback */
const asBool = (v, fallback) => (typeof v === 'boolean' ? v : fallback)

/**
 * Coerce anything - persisted JSON from an older build, a partial patch - into a
 * complete, valid option set.
 * @param {Partial<SqlEditorOptions> | null | undefined} raw
 * @returns {SqlEditorOptions}
 */
export function normalizeSqlEditor(raw) {
  const r = raw ?? {}
  const d = SQL_EDITOR_DEFAULTS
  return {
    textSize: SIZES.includes(/** @type {string} */ (r.textSize)) ? /** @type {SqlEditorTextSize} */ (r.textSize) : d.textSize,
    wrap: asBool(r.wrap, d.wrap),
    lineNumbers: asBool(r.lineNumbers, d.lineNumbers),
    folding: asBool(r.folding, d.folding),
    suggestWhileTyping: asBool(r.suggestWhileTyping, d.suggestWhileTyping),
    lint: asBool(r.lint, d.lint),
  }
}

/** @param {SqlEditorTextSize} id */
export function sqlEditorFontSize(id) {
  return (SQL_EDITOR_TEXT_SIZES.find((s) => s.id === id) ?? SQL_EDITOR_TEXT_SIZES[1]).css
}
