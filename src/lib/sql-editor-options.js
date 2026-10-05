/**
 * SQL editor preferences (console and notebook cells): the option set, its
 * validation, and the fields the settings dialog renders. Kept apart from the
 * editor so the settings store can hold and check them without importing
 * CodeMirror - the same split as sql-format-options.js.
 */

/** @typedef {'small' | 'default' | 'large'} SqlEditorTextSize */
/** @typedef {'off' | 'current' | 'all'} SqlCodeLens */
/** @typedef {import('./sql-ident.js').QuoteMode} SqlQuoteNames */
/** @typedef {{ textSize: SqlEditorTextSize, wrap: boolean, lineNumbers: boolean,
 *   folding: boolean, suggestWhileTyping: boolean, endHint: boolean, lint: boolean,
 *   tabSize: 2 | 4, indentTabs: boolean, smartIndent: boolean, autoClose: boolean,
 *   acceptOnEnter: boolean, activeLine: boolean, whitespace: boolean, cursorBlink: boolean,
 *   resultsBeside: boolean, revealResultsOnRun: boolean, exactRowCount: boolean,
 *   highlightBlock: boolean, codeLens: SqlCodeLens, variables: boolean,
 *   rememberVariables: boolean, autoLimit: number, quoteNames: SqlQuoteNames,
 *   qualifySchema: boolean }} SqlEditorOptions */

/** @type {SqlEditorOptions} */
export const SQL_EDITOR_DEFAULTS = {
  textSize: 'default',
  wrap: true,
  lineNumbers: true,
  // On: the arrows show only while the pointer is over the gutter, so they
  // cost nothing at rest, and a long script folds statement by statement.
  folding: true,
  suggestWhileTyping: true,
  // A faint `;` after a finished statement; Tab writes it. Only shown where
  // a statement visibly ends, so it never sits in the way of typing.
  endHint: true,
  lint: true,
  // Editing basics, VS Code's defaults: a new line keeps the indentation of
  // the line above, brackets and quotes close themselves, Enter or Tab takes
  // a suggestion, the caret blinks.
  tabSize: 2,
  indentTabs: false,
  smartIndent: false,
  autoClose: true,
  acceptOnEnter: true,
  activeLine: true,
  whitespace: false,
  cursorBlink: true,
  // Results: under the editor by default, the layout the console has always had.
  resultsBeside: false,
  // A run opens a collapsed results pane: what it returned is why it ran.
  revealResultsOnRun: true,
  // Off: a big unfiltered table shows the planner's estimate, which is instant.
  exactRowCount: false,
  highlightBlock: true,
  // Above the statement under the caret only: above every statement, a long
  // script is half action rows.
  codeLens: 'current',
  variables: true,
  rememberVariables: true,
  // Off: millions of rows are what the result grid is built for, so a cap is
  // opted into, never put in the way.
  autoLimit: 0,
  quoteNames: 'auto',
  qualifySchema: true,
}

export const SQL_CODE_LENS_OPTIONS = [
  { value: 'off', label: 'Off' },
  { value: 'current', label: 'At caret' },
  { value: 'all', label: 'Every statement' },
]
export const SQL_AUTO_LIMIT_OPTIONS = [
  { value: 0, label: 'Off' },
  { value: 100, label: '100' },
  { value: 500, label: '500' },
  { value: 1000, label: '1,000' },
  { value: 5000, label: '5,000' },
]
export const SQL_TAB_SIZE_OPTIONS = [
  { value: 2, label: '2' },
  { value: 4, label: '4' },
]
export const SQL_QUOTE_NAMES_OPTIONS = [
  { value: 'auto', label: 'When needed' },
  { value: 'always', label: 'Always' },
  { value: 'never', label: 'Never' },
]

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
  { key: 'folding', label: 'Fold arrows', desc: 'Arrows beside the line numbers to fold a statement or comment. Ctrl+Shift+[ and ] fold either way.', kind: 'bool' },
  { key: 'suggestWhileTyping', label: 'Suggestions while typing', desc: 'Open the completion list as you type. Ctrl+Space opens it either way.', kind: 'bool' },
  { key: 'endHint', label: 'Suggest ; at statement end', desc: 'A faint ; after the caret when the statement there looks finished; Tab adds it.', kind: 'bool' },
  // Editing basics: set once, so they stay out of the editor's quick menu.
  { key: 'tabSize', label: 'Tab size', desc: 'Columns a tab and one indent level take.', kind: 'choice', options: SQL_TAB_SIZE_OPTIONS, menu: false },
  { key: 'indentTabs', label: 'Indent with tabs', desc: 'Indent with a tab character instead of spaces.', kind: 'bool', menu: false },
  { key: 'smartIndent', label: 'Indent continuation lines', desc: 'Enter indents the next line of an unfinished statement one level deeper. Off, a new line keeps the indentation of the line above, as in VS Code.', kind: 'bool', menu: false },
  { key: 'autoClose', label: 'Close brackets and quotes', desc: 'Typing ( [ \' " or ` adds its closing pair.', kind: 'bool', menu: false },
  { key: 'acceptOnEnter', label: 'Enter accepts a suggestion', desc: 'Enter takes the highlighted suggestion. Off, only Tab does and Enter always breaks the line.', kind: 'bool', menu: false },
  { key: 'activeLine', label: 'Highlight the current line', desc: 'A faint band behind the line the caret is on.', kind: 'bool', menu: false },
  { key: 'whitespace', label: 'Show whitespace', desc: 'Draw spaces and tabs as faint dots and arrows.', kind: 'bool', menu: false },
  { key: 'cursorBlink', label: 'Blinking caret', desc: 'Off, the caret stays solid.', kind: 'bool', menu: false },
  { key: 'lint', label: 'Problem markers', desc: 'Mark unterminated strings, unbalanced parentheses and statements missing a ;.', kind: 'bool' },
  { key: 'resultsBeside', label: 'Results beside the editor', desc: 'Show the results to the right of the editor instead of under it. Suits a wide window.', kind: 'bool' },
  { key: 'revealResultsOnRun', label: 'Show results when a query runs', desc: 'Open the results pane when a run starts, if it was collapsed (Ctrl+J).', kind: 'bool' },
  // Not an editor setting, so not in the editor's own menu: it is about the
  // table grid's total.
  { key: 'exactRowCount', label: 'Exact row count for table data', desc: 'Count every row of a large table for the grid\'s total. Off shows the planner\'s estimate above 100,000 rows, which is instant; a filtered table is always counted exactly. PostgreSQL.', kind: 'bool', menu: false },
  { key: 'highlightBlock', label: 'Highlight statement at caret', desc: 'A faint band behind the statement Ctrl+R would run, when the editor holds more than one.', kind: 'bool' },
  { key: 'codeLens', label: 'Statement actions', desc: 'At caret: Run and a ⋯ menu (Select, New tab, JSON, Variables, Ask AI) at the right of the statement under the caret; lines keep room for it, so it never covers text. Every statement: the full row above each statement.', kind: 'choice', options: SQL_CODE_LENS_OPTIONS },
  // The rest change what runs or what the app writes, not how the editor
  // looks, so they stay out of the editor's own menu too.
  { key: 'variables', label: 'SQL variables', desc: 'Read :name, $name and ${name} as variables: a run asks for their values and inlines them as escaped literals.', kind: 'bool', menu: false },
  { key: 'rememberVariables', label: 'Remember variable values', desc: 'Keep each editor tab\'s variable values across restarts. Off, they last until the app closes.', kind: 'bool', menu: false },
  { key: 'autoLimit', label: 'Add LIMIT to SELECT', desc: 'Append LIMIT to a SELECT that has none before it runs. Other statements, and SQL Server, are never touched.', kind: 'choice', options: SQL_AUTO_LIMIT_OPTIONS },
  { key: 'quoteNames', label: 'Quote object names', desc: 'In SQL the app writes for you, such as Open in SQL editor: quote table and column names only when needed, always, or never.', kind: 'choice', options: SQL_QUOTE_NAMES_OPTIONS },
  { key: 'qualifySchema', label: 'Qualify tables with their schema', desc: 'Write schema.table in SQL the app writes for you. Off, the default schema (public, dbo, main) is left out; any other is kept.', kind: 'bool', menu: false },
]

const SIZES = SQL_EDITOR_TEXT_SIZES.map((s) => s.id)
/** @param {unknown} v @param {boolean} fallback */
const asBool = (v, fallback) => (typeof v === 'boolean' ? v : fallback)
/**
 * @template T
 * @param {unknown} v @param {{ value: T }[]} options @param {T} fallback
 * @returns {T}
 */
const oneOf = (v, options, fallback) => (options.some((o) => o.value === v) ? /** @type {T} */ (v) : fallback)

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
    endHint: asBool(r.endHint, d.endHint),
    tabSize: oneOf(r.tabSize, SQL_TAB_SIZE_OPTIONS, d.tabSize),
    indentTabs: asBool(r.indentTabs, d.indentTabs),
    smartIndent: asBool(r.smartIndent, d.smartIndent),
    autoClose: asBool(r.autoClose, d.autoClose),
    acceptOnEnter: asBool(r.acceptOnEnter, d.acceptOnEnter),
    activeLine: asBool(r.activeLine, d.activeLine),
    whitespace: asBool(r.whitespace, d.whitespace),
    cursorBlink: asBool(r.cursorBlink, d.cursorBlink),
    lint: asBool(r.lint, d.lint),
    resultsBeside: asBool(r.resultsBeside, d.resultsBeside),
    revealResultsOnRun: asBool(r.revealResultsOnRun, d.revealResultsOnRun),
    exactRowCount: asBool(r.exactRowCount, d.exactRowCount),
    highlightBlock: asBool(r.highlightBlock, d.highlightBlock),
    codeLens: oneOf(r.codeLens, SQL_CODE_LENS_OPTIONS, d.codeLens),
    variables: asBool(r.variables, d.variables),
    rememberVariables: asBool(r.rememberVariables, d.rememberVariables),
    autoLimit: oneOf(r.autoLimit, SQL_AUTO_LIMIT_OPTIONS, d.autoLimit),
    quoteNames: oneOf(r.quoteNames, SQL_QUOTE_NAMES_OPTIONS, d.quoteNames),
    qualifySchema: asBool(r.qualifySchema, d.qualifySchema),
  }
}

/** @param {SqlEditorTextSize} id */
export function sqlEditorFontSize(id) {
  return (SQL_EDITOR_TEXT_SIZES.find((s) => s.id === id) ?? SQL_EDITOR_TEXT_SIZES[1]).css
}
