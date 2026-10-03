/**
 * SQL formatting preferences: the option set, its validation, and the live values.
 *
 * Deliberately separate from `format-sql.js`, which imports `sql-formatter`.
 * `stores/settings.js` needs the defaults and the validator on every load, and
 * pulling the formatter library in for that would put it on the app's startup
 * path for no benefit. The formatter reads `sqlFormatOptions()` when it actually
 * formats something.
 *
 * Option names are sql-formatter's own, so each one is exactly the knob the
 * library documents - no translation layer to drift out of step.
 */

/** @typedef {'preserve' | 'upper' | 'lower'} SqlCase */
/** @typedef {{ keywordCase: SqlCase, dataTypeCase: SqlCase, functionCase: SqlCase,
 *   identifierCase: SqlCase, tabWidth: number, useTabs: boolean,
 *   logicalOperatorNewline: 'before' | 'after', expressionWidth: number,
 *   linesBetweenQueries: number, compactClauses: boolean, lineWidth: number }} SqlFormatOptions */

/** @type {SqlFormatOptions} */
export const SQL_FORMAT_DEFAULTS = {
  keywordCase: 'upper',
  dataTypeCase: 'preserve',
  functionCase: 'preserve',
  // Identifiers are case-sensitive once quoted, so rewriting them can change what
  // a statement means. sql-formatter marks this experimental; preserve is the only
  // safe default.
  identifierCase: 'preserve',
  tabWidth: 2,
  useTabs: false,
  logicalOperatorNewline: 'before',
  expressionWidth: 50,
  linesBetweenQueries: 1,
  // Not sql-formatter options: format-sql.js applies these after it, because the
  // library always gives a clause's body its own line - `SELECT`, `  *`, `FROM`,
  // `  users` for a one-line query.
  compactClauses: true,
  lineWidth: 80,
}

export const SQL_CASE_OPTIONS = [
  { id: 'preserve', label: 'Preserve' },
  { id: 'upper', label: 'Upper' },
  { id: 'lower', label: 'Lower' },
]
export const SQL_TAB_WIDTHS = [2, 4, 8]

/** Fields for the settings UI, so the dialog and the formatter can't disagree. */
export const SQL_FORMAT_FIELDS = [
  { key: 'keywordCase', label: 'Keyword case', desc: 'Casing for SQL keywords (SELECT, FROM, WHERE…).', kind: 'case' },
  { key: 'dataTypeCase', label: 'Data type case', desc: 'Casing for data types (INT, VARCHAR…).', kind: 'case' },
  { key: 'functionCase', label: 'Function case', desc: 'Casing for function names (COUNT, SUM…).', kind: 'case' },
  { key: 'identifierCase', label: 'Identifier case', desc: 'Casing for identifiers. Experimental - a quoted identifier is case-sensitive, so changing this can change what a statement means.', kind: 'case' },
  { key: 'tabWidth', label: 'Tab width', desc: 'Spaces per indentation level.', kind: 'tabWidth' },
  { key: 'useTabs', label: 'Use tabs', desc: 'Indent with tab characters instead of spaces.', kind: 'bool' },
  { key: 'logicalOperatorNewline', label: 'Logical operator newline', desc: 'Put AND / OR before or after the line break.', kind: 'operatorNewline' },
  { key: 'expressionWidth', label: 'Expression width', desc: 'Characters allowed inside parentheses before wrapping.', kind: 'number', min: 20, max: 200, step: 10 },
  { key: 'linesBetweenQueries', label: 'Lines between queries', desc: 'Blank lines between separate statements.', kind: 'number', min: 0, max: 5, step: 1 },
  { key: 'compactClauses', label: 'Compact clauses', desc: 'Keep a clause on its keyword\'s line when it fits - SELECT * / FROM users, not four lines.', kind: 'bool' },
  { key: 'lineWidth', label: 'Line width', desc: 'Longest line a compact clause may make before it wraps.', kind: 'number', min: 40, max: 200, step: 10 },
]

const CASES = SQL_CASE_OPTIONS.map((c) => c.id)

/** @param {unknown} v @param {SqlCase} fallback @returns {SqlCase} */
const asCase = (v, fallback) => (CASES.includes(/** @type {string} */ (v)) ? /** @type {SqlCase} */ (v) : fallback)
/** @param {unknown} v @param {number} fallback @param {number} min @param {number} max */
const asInt = (v, fallback, min, max) => {
  const n = Math.round(Number(v))
  return Number.isFinite(n) ? Math.min(max, Math.max(min, n)) : fallback
}

/**
 * Coerce anything - persisted JSON from an older version, a partial patch - into
 * a complete, valid option set.
 * @param {Partial<SqlFormatOptions> | null | undefined} raw
 * @returns {SqlFormatOptions}
 */
export function normalizeSqlFormat(raw) {
  const r = raw ?? {}
  const tab = asInt(r.tabWidth, SQL_FORMAT_DEFAULTS.tabWidth, 1, 8)
  return {
    keywordCase: asCase(r.keywordCase, SQL_FORMAT_DEFAULTS.keywordCase),
    dataTypeCase: asCase(r.dataTypeCase, SQL_FORMAT_DEFAULTS.dataTypeCase),
    functionCase: asCase(r.functionCase, SQL_FORMAT_DEFAULTS.functionCase),
    identifierCase: asCase(r.identifierCase, SQL_FORMAT_DEFAULTS.identifierCase),
    tabWidth: SQL_TAB_WIDTHS.includes(tab) ? tab : SQL_FORMAT_DEFAULTS.tabWidth,
    useTabs: r.useTabs === true,
    logicalOperatorNewline: r.logicalOperatorNewline === 'after' ? 'after' : 'before',
    expressionWidth: asInt(r.expressionWidth, SQL_FORMAT_DEFAULTS.expressionWidth, 20, 200),
    linesBetweenQueries: asInt(r.linesBetweenQueries, SQL_FORMAT_DEFAULTS.linesBetweenQueries, 0, 5),
    compactClauses: r.compactClauses !== false,
    lineWidth: asInt(r.lineWidth, SQL_FORMAT_DEFAULTS.lineWidth, 40, 200),
  }
}

/** @type {SqlFormatOptions} */
let _current = { ...SQL_FORMAT_DEFAULTS }

/** Apply the user's preferences. Called by applySettings on load and on change. */
export function setSqlFormatOptions(/** @type {Partial<SqlFormatOptions> | null} */ next) {
  _current = normalizeSqlFormat(next)
}

/** The options in force right now (a copy - callers can't mutate the live set). */
export function sqlFormatOptions() {
  return { ..._current }
}
