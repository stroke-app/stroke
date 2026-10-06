/**
 * SQL completion for CodeMirror - the SQL console, notebook cells and the
 * review dock all use this one source.
 *
 * A port of the Monaco provider this editor used to have: the same curated
 * keywords, function signatures (as snippets with tab stops), snippets, enum
 * values, user functions, schema/table/column hints and alias resolution, and
 * the same context tiers (tables after FROM, the referenced tables' columns
 * first in SELECT/WHERE, clause keywords after a table name). On top of it,
 * sql-complete-context.js reads the caret position with a tokenizer, so names
 * complete inside quotes and nothing pops up inside strings or comments.
 *
 * The grammar layer (followAt in sql-complete-context.js) adds what follows
 * at each point as whole phrases: DROP TABLE | offers IF EXISTS, IF | offers
 * EXISTS, ALTER TABLE t | its actions, ORDER | BY. Where the next word is
 * certain the list opens after a space by itself.
 *
 * Cost per keystroke: the candidates for a position are built once (analysis,
 * tiers, one object per option) and kept while only the word under the caret
 * changes; each keystroke then just filters and sorts them.
 *
 * Only the CodeEditor chunk imports this; none of it reaches startup.
 */
import {
  snippet, snippetCompletion, pickedCompletion, startCompletion, completionStatus, selectedCompletion, closeCompletion,
} from '@codemirror/autocomplete'
import { EditorSelection } from '@codemirror/state'
import { isStatementSnippet, snippetEndsStatement } from './sql-terminator.js'
import { statementAt } from '$lib/cm-sql-statements.js'
import { sqlCompletionContext } from '$lib/sql-complete-context.js'
import {
  PG_KEYWORDS, DDL_KEYWORDS, DIALECT_KEYWORDS, PG_FUNCTIONS, PG_FUNCTION_FAMILIES, DIALECT_FUNCTIONS,
  SQL_SNIPPETS, SQL_TYPES, TABLE_CTX_KWS, COLUMN_CTX_KWS, SQL_KW_SET, analyzeQuery, sqlFamily,
} from '$lib/sql-complete-data.js'

/** @typedef {import('$lib/sql-complete-data.js').SqlSchemaHints} SqlSchemaHints */
/** @typedef {import('@codemirror/autocomplete').Completion} Completion */
/** @typedef {import('@codemirror/view').EditorView} EditorView */
/**
 * A candidate: the completion itself (boost included) plus what matching needs.
 * @typedef {{ c: Completion, lc: string, aliases?: string[], order: number }} Entry
 */

/** Monaco's sort tiers ('0_' best ... '9_' worst), as CodeMirror boosts. */
const TIER = [50, 40, 30, 20, 10, 0, -10, -20, -30, -40]
/** The clause's likely next word, above every tier. */
const NEXT_BOOST = 60
/** Scanned back from the caret when the statement start is not known. */
const LOOKBEHIND = 4000
/** Options handed to CodeMirror per keystroke; it shows 50, and sorting more is waste. */
const MAX_OPTIONS = 300
/** How long a first suggestion waits for columns being fetched. */
const COLUMN_WAIT_MS = 1500

// ── Identifier insertion ─────────────────────────────────────────────────────

/**
 * Whether a name has to be quoted to mean itself. Postgres folds bare names to
 * lower case, so `userId` must be quoted there. The other engines keep a bare
 * name's case (or compare without it), and SQL Server reads "x" as a string
 * when QUOTED_IDENTIFIER is off, so a mixed-case name stays bare there.
 * @param {string} name @param {string} dialect
 */
function needsQuote(name, dialect) {
  if (SQL_KW_SET.has(name.toUpperCase())) return true
  return sqlFamily(dialect) === 'postgres' ? !/^[a-z_][a-z0-9_$]*$/.test(name) : !/^[A-Za-z_][\w$]*$/.test(name)
}

/** @param {string} name @param {string} dialect */
function quoteName(name, dialect) {
  if (!needsQuote(name, dialect)) return name
  const q = sqlFamily(dialect) === 'mysql' ? '`' : '"'
  return q + name.replaceAll(q, q + q) + q
}

/**
 * Insert a name, reading how from the completion itself (one function for
 * every name option, not a closure each). Inside an open quote: the bare name,
 * stepping over the closing quote or adding it. Outside: quoted only when it
 * has to be. `_suffix` follows the name ('.' after a schema); `_reopen` opens
 * the list again (a schema's tables).
 * @param {EditorView} view @param {Completion & { _quote?: string | null, _dialect?: string, _suffix?: string, _reopen?: boolean }} c
 * @param {number} from @param {number} to
 */
function applyName(view, c, from, to) {
  const { state } = view
  const quote = c._quote ?? null
  const suffix = c._suffix ?? ''
  const name = quote ? c.label.replaceAll(quote, quote + quote) : quoteName(c.label, c._dialect ?? 'postgres')
  // At every cursor that holds the same text, as CodeMirror's own completion
  // does. A template field used twice (`ON ${4:table}` and `UPDATE ${4:table}`
  // in a trigger) is two cursors: written at one only, the other kept the
  // placeholder, and the trigger was created pointing at `table_name`.
  const { main } = state.selection
  const fromOff = from - main.from
  const toOff = to - main.from
  const replaced = state.sliceDoc(from, to)
  view.dispatch({
    ...state.changeByRange((range) => {
      const rFrom = range.from + fromOff
      const rTo = to === main.from ? range.to : range.from + toOff
      if (range !== main && from !== to && state.sliceDoc(rFrom, rTo) !== replaced) return { range }
      if (!quote) {
        return { changes: { from: rFrom, to: rTo, insert: name + suffix }, range: EditorSelection.cursor(rFrom + name.length + suffix.length) }
      }
      const closes = state.sliceDoc(rTo, rTo + 1) === quote
      const changes = [{ from: rFrom, to: rTo, insert: name + (closes ? '' : quote) }]
      if (suffix) changes.push({ from: closes ? rTo + 1 : rTo, to: closes ? rTo + 1 : rTo, insert: suffix })
      return { changes, range: EditorSelection.cursor(rFrom + name.length + 1 + suffix.length) }
    }),
    annotations: pickedCompletion.of(c),
    userEvent: 'input.complete',
  })
  if (c._reopen) setTimeout(() => startCompletion(view))
}

/**
 * Take a phrase that names follow (IF EXISTS, DROP COLUMN, LEFT JOIN): write
 * it with a space after it and open the list again at the names.
 * @param {EditorView} view @param {Completion} c @param {number} from @param {number} to
 */
function applyPhrase(view, c, from, to) {
  const spaced = view.state.sliceDoc(to, to + 1) === ' '
  view.dispatch({
    changes: { from, to, insert: spaced ? c.label : `${c.label} ` },
    selection: { anchor: from + c.label.length + 1 },
    annotations: pickedCompletion.of(c),
    userEvent: 'input.complete',
  })
  setTimeout(() => startCompletion(view))
}

/** A phrase written in lower case when that is how the statement is typed. @param {string} prefix @param {boolean} statementLower */
const phraseLower = (prefix, statementLower) => (prefix ? /[a-z]/.test(prefix) && prefix === prefix.toLowerCase() : statementLower)

// ── Templates ────────────────────────────────────────────────────────────────

/** Monaco snippet syntax → CodeMirror's: `$0` (final caret) becomes `${}`. */
const toSnippet = (/** @type {string} */ body) => body.replace(/\$0/g, '${}')
/** A signature without its tab stops, for the detail column. */
const plainSig = (/** @type {string} */ body) => body.replace(/\$\{\d+:?([^}]*)\}/g, '$1').replace(/\$\d+/g, '')

/**
 * A snippet completion that closes the statement with `;` when it writes a
 * whole statement and nothing follows it on its line (sql-terminator.js).
 * Clause snippets (JOIN, ORDER BY) and a statement typed into a bracket or in
 * front of more SQL go in as written.
 * @param {string} body @param {Omit<Completion, 'apply'>} info
 */
function statementSnippet(body, info) {
  const plain = snippetCompletion(toSnippet(body), info)
  if (!isStatementSnippet(body)) return plain
  const closed = snippet(toSnippet(`${body};`))
  return {
    ...plain,
    apply: (/** @type {import('@codemirror/view').EditorView} */ view, /** @type {Completion} */ c, /** @type {number} */ from, /** @type {number} */ to) => {
      const rest = view.state.doc.sliceString(to, view.state.doc.lineAt(to).to)
      if (snippetEndsStatement(rest)) closed(view, c, from, to)
      else /** @type {any} */ (plain.apply)(view, c, from, to)
    },
  }
}

/** The SQL a snippet writes, beside the list - the name alone says little. */
const snippetPreview = (/** @type {string} */ body) => () => {
  const pre = document.createElement('pre')
  pre.className = 'cm-snippet-preview'
  pre.textContent = plainSig(body)
  return pre
}

/**
 * @typedef {{ c: Completion, common: boolean }} TypeOption
 * @typedef {{
 *   keywords: Completion[],
 *   functions: Completion[],
 *   functionNames: Set<string>,
 *   snippets: Array<Completion & { aliases: string[] }>,
 *   types: TypeOption[],
 *   typesUpper: TypeOption[],
 * }} StaticTemplates
 * `typesUpper`: the same types written in capitals, for when that is what is
 * being typed (`VARC` → `VARCHAR(255)`). ClickHouse types keep their case.
 */
/** @type {Map<string, StaticTemplates>} */
const staticCache = new Map()
/** DDL words that are not query keywords as well (SET, DROP stay everywhere). */
const DDL_KEYWORD_SET = new Set(DDL_KEYWORDS.filter((k) => !PG_KEYWORDS.includes(k)))

/** @param {{ label: string, sig?: string, common?: boolean }} t @param {boolean} upper */
function typeOption(t, upper) {
  const label = upper ? t.label.toUpperCase() : t.label
  // Only the name goes up: `VARCHAR(${1:255})`, the placeholder stays as written.
  const sig = t.sig && (upper ? t.sig.replace(/^[^(]+/, (name) => name.toUpperCase()) : t.sig)
  const c = sig
    ? snippetCompletion(toSnippet(sig), { label, type: 'type', detail: plainSig(sig) })
    : { label, type: 'type' }
  return { c, common: !!t.common }
}

/**
 * The fixed vocabulary for one engine family: its keywords, the functions and
 * snippets that run on it, and its column types.
 * @param {ReturnType<typeof sqlFamily>} family
 */
function staticTemplates(family) {
  const hit = staticCache.get(family)
  if (hit) return hit
  const keywordLabels = [...new Set([...PG_KEYWORDS, ...DDL_KEYWORDS, ...(DIALECT_KEYWORDS[family] ?? [])])]
  const fnDefs = [
    ...PG_FUNCTIONS.filter((fn) => PG_FUNCTION_FAMILIES[fn.label]?.includes(family) ?? true),
    ...DIALECT_FUNCTIONS.filter((fn) => fn.only.includes(family)),
  ]
  const typeDefs = SQL_TYPES[family] ?? SQL_TYPES.postgres
  const caseSensitiveTypes = family === 'clickhouse'
  const built = {
    keywords: keywordLabels.map((label) => ({ label, type: 'keyword' })),
    functions: fnDefs.map((fn) =>
      snippetCompletion(toSnippet(fn.sig), { label: fn.label, type: 'function', detail: plainSig(fn.sig), info: fn.doc }),
    ),
    functionNames: new Set(fnDefs.map((fn) => fn.label.toLowerCase())),
    snippets: SQL_SNIPPETS.filter((s) => !s.only || s.only.includes(family)).map((s) => ({
      ...statementSnippet(s.body, { label: s.name, type: 'snippet', detail: s.alias, info: snippetPreview(s.body) }),
      aliases: [s.alias],
    })),
    types: typeDefs.map((t) => typeOption(t, false)),
    typesUpper: caseSensitiveTypes ? typeDefs.map((t) => typeOption(t, false)) : typeDefs.map((t) => typeOption(t, true)),
  }
  staticCache.set(family, built)
  return built
}

/** Capitals typed (`VARC`, `I`): write the type in capitals too. @param {string} prefix */
const typedInCapitals = (prefix) => /[A-Z]/.test(prefix) && prefix === prefix.toUpperCase()

/**
 * @typedef {{ name: string, table: string, type: string }} ColumnHint
 * @typedef {{
 *   activeSchema: string,
 *   schemas: string[],
 *   tables: string[],
 *   tableSet: Set<string>,
 *   colsByTable: Map<string, ColumnHint[]>,
 *   enums: Completion[],
 *   enumTypes: Completion[],
 *   userFns: Completion[],
 * }} HintTemplates
 * `enumTypes`: the user's enum types by name, offered where a type goes.
 */

/** @type {WeakMap<object, HintTemplates>} */
const hintsCache = new WeakMap()

/** Built once per hints object - a big schema is thousands of entries. @param {SqlSchemaHints} hints */
function hintTemplates(hints) {
  const hit = hintsCache.get(hints)
  if (hit) return hit
  // Columns grouped (and deduped) by short table name: schema-qualified keys
  // like "public.users" collapse into "users". Typed entries come first in the
  // hints, so where a table appears twice its types win.
  /** @type {Map<string, ColumnHint[]>} */
  const colsByTable = new Map()
  for (const [key, cols] of Object.entries(hints.columnsByTable ?? {})) {
    if (key === '__result__') continue
    const short = (key.includes('.') ? key.split('.').pop() ?? key : key).toLowerCase()
    let bucket = colsByTable.get(short)
    if (!bucket) { bucket = []; colsByTable.set(short, bucket) }
    const seen = new Map(bucket.map((c) => [c.name, c]))
    for (const col of cols ?? []) {
      const name = typeof col === 'string' ? col : col?.name
      if (!name) continue
      const type = typeof col === 'string' ? '' : col.type ?? ''
      const prev = seen.get(name)
      if (prev) { if (!prev.type && type) prev.type = type; continue }
      const hint = { name, table: short, type }
      seen.set(name, hint)
      bucket.push(hint)
    }
  }
  /** @type {Completion[]} */
  const enums = []
  for (const [enumName, values] of Object.entries(hints.enumValues ?? {})) {
    for (const val of values ?? []) {
      enums.push({ label: val, type: 'enum', detail: enumName, apply: `'${val.replaceAll("'", "''")}'` })
    }
  }
  const userFns = (hints.userFunctions ?? []).map((f) =>
    snippetCompletion(`${f.name}(\${})`, { label: f.name, type: 'function', detail: `→ ${f.returnType}`, info: f.signature }),
  )
  const enumTypes = Object.keys(hints.enumValues ?? {}).map((name) => ({ label: name, type: 'type', detail: 'enum' }))
  const tables = hints.tables ?? []
  const built = {
    activeSchema: hints.activeSchema ?? 'public',
    schemas: hints.schemas ?? [],
    tables,
    tableSet: new Set(tables.map((t) => t.toLowerCase())),
    colsByTable,
    enums,
    enumTypes,
    userFns,
  }
  hintsCache.set(hints, built)
  return built
}

// ── Matching ─────────────────────────────────────────────────────────────────
// Our own, not CodeMirror's fuzzy matcher: that one matches letters scattered
// anywhere, so `sel` offered u·se·rs_tab·l·e. Here a label matches by prefix,
// then at a word start (`email` finds `user_email`, `id` finds `authorId`),
// then - for two or more letters - anywhere as one run. Better matches sort
// first; within a match kind, the context tier decides.

/**
 * @param {string} label @param {string} l the label lower-cased (precomputed)
 * @param {string} q lower-cased typed text @param {string[] | undefined} aliases
 * @returns {{ score: number, at: number } | null} `at`: where the match starts (-1: via alias)
 */
function matchLabel(label, l, q, aliases) {
  if (!q) return { score: 1, at: 0 }
  if (l === q) return { score: 6, at: 0 }
  if (l.startsWith(q)) return { score: 5, at: 0 }
  if (aliases?.some((a) => a.startsWith(q))) return { score: 5, at: -1 }
  for (let i = 1; i < label.length; i++) {
    const prev = label[i - 1]
    const wordStart = prev === '_' || prev === '.' || prev === ' ' || prev === '(' ||
      (label[i] >= 'A' && label[i] <= 'Z' && prev >= 'a' && prev <= 'z')
    if (wordStart && l.startsWith(q, i)) return { score: 4, at: i }
  }
  if (q.length >= 2) {
    const i = l.indexOf(q)
    if (i > 0) return { score: 2, at: i }
  }
  return null
}

/** A blank line ends a query even without a `;`: queries above stay out of it. */
function afterBlankLine(/** @type {string} */ text) {
  const re = /\n[ \t]*\r?\n/g
  let cut = 0
  for (let m; (m = re.exec(text)); ) cut = m.index + m[0].length
  return cut
}
function beforeBlankLine(/** @type {string} */ text) {
  const m = /\n[ \t]*\r?\n/.exec(text)
  return m ? m.index : text.length
}

/**
 * The statement around `pos`. Aliases and tables are read from all of it: in
 * `SELECT p.| FROM posts p` the FROM comes after the caret.
 * @param {import('@codemirror/state').EditorState} state @param {number} pos
 */
function statementRange(state, pos) {
  const stmt = statementAt(state, pos)
  if (stmt && stmt.start <= pos && pos <= stmt.end) return { start: stmt.start, end: stmt.end }
  return { start: Math.max(0, pos - LOOKBEHIND), end: pos }
}

// ── Candidates ───────────────────────────────────────────────────────────────

/**
 * Every candidate for one position, before the typed word filters them.
 * @param {import('$lib/sql-complete-context.js').SqlCompletionContext} ctx
 * @param {HintTemplates} H @param {StaticTemplates} S
 * @param {string} dialect @param {string} statement the whole statement, for aliases
 * @param {string | null} rowTable the table a trigger's NEW / OLD rows belong to
 * @returns {{ entries: Entry[], missing: string[] }} `missing`: named tables with no columns known
 */
function buildCandidates(ctx, H, S, dialect, statement, rowTable) {
  const { aliasMap, referencedTables } = analyzeQuery(statement, H.tables)
  // In a trigger, NEW and OLD (SQL Server: inserted / deleted) are rows of the
  // table it is on.
  if (rowTable) for (const row of ROW_ALIASES) aliasMap[row] ??= rowTable.toLowerCase()
  const typed = ctx.prefix !== ''
  /** @type {Entry[]} */
  const entries = []
  const add = (/** @type {Completion} */ c, /** @type {number} */ boost, /** @type {string[] | undefined} */ aliases) =>
    entries.push({ c: { ...c, boost }, lc: c.label.toLowerCase(), aliases, order: entries.length })
  const name = (/** @type {object} */ extra) => ({ apply: applyName, _quote: ctx.quote, _dialect: dialect, ...extra })

  const schemaOption = (/** @type {string} */ s, /** @type {number} */ tier) =>
    add(/** @type {Completion} */ (name({ label: s, type: 'schema', detail: 'schema', _suffix: '.', _reopen: true })), TIER[tier])
  const tableOption = (/** @type {string} */ t, /** @type {number} */ tier) =>
    add(/** @type {Completion} */ (name({ label: t, type: 'table', detail: H.activeSchema })), TIER[tier])
  const columnOption = (/** @type {ColumnHint} */ c, /** @type {number} */ tier) =>
    add(/** @type {Completion} */ (name({ label: c.name, type: 'column', detail: c.type ? `${c.type} · ${c.table}` : c.table })), TIER[tier])

  // Tables the statement names (by name or alias) - their columns rank first.
  const refs = new Set([...referencedTables, ...ctx.tables.map((t) => t.toLowerCase()), ...(rowTable ? [rowTable.toLowerCase()] : [])])
  /** @type {Set<string>} */
  const missing = new Set()
  const wantColumnsOf = (/** @type {string} */ t) => {
    if (!H.colsByTable.has(t) && H.tableSet.has(t)) missing.add(t)
    return H.colsByTable.get(t) ?? []
  }

  /** Referenced tables' columns first, then every other table's, deduped by name. */
  function columns(/** @type {number} */ refTier, /** @type {number} */ otherTier) {
    const seen = new Set()
    for (const t of refs) {
      for (const c of wantColumnsOf(t)) { columnOption(c, refTier); seen.add(c.name) }
    }
    for (const [t, cols] of H.colsByTable) {
      if (refs.has(t)) continue
      for (const c of cols) {
        if (seen.has(c.name)) continue
        seen.add(c.name)
        columnOption(c, otherTier)
      }
    }
  }

  // What the grammar says comes next, first: whole phrases, in the case being typed.
  const family = sqlFamily(dialect)
  const lower = phraseLower(ctx.prefix, ctx.lower)
  /** @type {Set<string>} */
  const phraseWords = new Set()
  /** First words of the phrases (IF of IF EXISTS): not offered alone unless they stand alone here. */
  const phraseStarts = new Set()
  for (const ph of ctx.phrases) {
    if (ph.only && !ph.only.includes(family)) continue
    const label = lower ? ph.text.toLowerCase() : ph.text
    phraseWords.add(ph.text)
    if (ph.text.includes(' ')) phraseStarts.add(ph.text.slice(0, ph.text.indexOf(' ')))
    add(ph.reopen ? { label, type: 'keyword', apply: applyPhrase } : { label, type: 'keyword' }, NEXT_BOOST)
  }

  const nextFirst = ctx.kind === 'keywords' || ctx.kind === 'statement' || ctx.kind === 'ddl' || ctx.kind === 'types' || ctx.afterExpr
  // DDL's own words (COLUMN, RENAME, TEMP ...) have no place in a query's clauses.
  const ddlWords = ctx.kind === 'statement' || ctx.kind === 'ddl' || ctx.kind === 'types'
  /**
   * @param {number} tier @param {Set<string> | null} only
   * @param {boolean} [besideFunctions] functions are in the list too: COALESCE
   *   and CAST are offered once, as the function with its signature
   */
  function keywords(tier, only, besideFunctions = false) {
    const next = new Set(nextFirst ? ctx.next : [])
    for (const k of S.keywords) {
      if (phraseWords.has(k.label)) continue
      if (phraseStarts.has(k.label) && !next.has(k.label) && !ctx.next.includes(k.label)) continue
      if (!ddlWords && DDL_KEYWORD_SET.has(k.label) && !next.has(k.label)) continue
      if (besideFunctions && S.functionNames.has(k.label.toLowerCase())) continue
      if (next.has(k.label)) { add(k, NEXT_BOOST); continue }
      if (only && !only.has(k.label)) continue
      add(k, TIER[tier])
    }
  }

  if (ctx.columnsOf && ctx.kind !== 'qualified') {
    // ALTER TABLE t DROP COLUMN |, INSERT INTO t (|: that table's columns only.
    for (const c of wantColumnsOf(ctx.columnsOf.toLowerCase())) columnOption(c, 0)
    if (typed && !ctx.quote) keywords(3, null)
  } else if (ctx.names === 'schemas' && !ctx.quote) {
    // DROP SCHEMA |: the schemas themselves.
    for (const sc of H.schemas) add(/** @type {Completion} */ (name({ label: sc, type: 'schema', detail: 'schema' })), TIER[0])
    if (typed) keywords(3, null)
  } else if (ctx.kind === 'qualified') {
    // After a dot: schema → its tables, table or alias → its columns.
    const left = /** @type {string} */ (ctx.qualifier).toLowerCase()
    if (H.schemas.some((s) => s.toLowerCase() === left)) {
      for (const t of H.tables) tableOption(t, 0)
    } else {
      for (const c of wantColumnsOf(aliasMap[left] ?? left)) columnOption(c, 0)
    }
  } else if (ctx.quote) {
    // Inside a quote: only names.
    if (ctx.kind === 'tables') {
      for (const t of H.tables) tableOption(t, 0)
      for (const s of H.schemas) schemaOption(s, 1)
    } else {
      columns(0, 1)
    }
  } else if (ctx.kind === 'tables') {
    // Right after FROM / JOIN / UPDATE / INTO.
    for (const t of H.tables) tableOption(t, 0)
    for (const s of H.schemas) schemaOption(s, 1)
    if (typed) keywords(2, null) // `FROM (SEL` → SELECT
  } else if (ctx.kind === 'keywords') {
    // Past the table name: the clause keywords. Opened by itself (the next
    // word is certain), only what follows.
    keywords(0, typed ? null : ctx.eager ? new Set() : TABLE_CTX_KWS)
    if (typed || !ctx.eager) for (const t of H.tables) tableOption(t, 7)
  } else if (ctx.kind === 'types') {
    // A column definition, ALTER ... TYPE, CAST(x AS ...), x::...
    keywords(0, new Set(ctx.next))
    // ALTER COLUMN c |: a bare type only on SQL Server; elsewhere TYPE comes first.
    if (!ctx.typesFor || ctx.typesFor.includes(family)) {
      for (const t of typedInCapitals(ctx.prefix) ? S.typesUpper : S.types) add(t.c, TIER[t.common ? 0 : 1])
      if (family === 'postgres') for (const e of H.enumTypes) add(e, TIER[1])
    }
  } else if (ctx.kind === 'ddl') {
    // Only the statement's own words go here: a new name is not one to pick.
    keywords(3, typed ? null : new Set(ctx.next))
  } else if (ctx.kind === 'columns') {
    // SELECT / WHERE / SET / ON ...
    columns(0, 1)
    for (const f of S.functions) add(f, TIER[2])
    for (const f of H.userFns) add(f, TIER[2])
    keywords(3, typed ? null : COLUMN_CTX_KWS, true)
    for (const e of H.enums) add(e, TIER[6])
    for (const t of H.tables) tableOption(t, 7)
    for (const s of H.schemas) schemaOption(s, 8)
    if (typed) for (const s of S.snippets) add(s, TIER[6], s.aliases)
  } else {
    // Start of a statement.
    for (const s of S.snippets) add(s, TIER[0], s.aliases)
    keywords(1, null)
  }
  return { entries, missing: [...missing] }
}

// ── Source ───────────────────────────────────────────────────────────────────

const EMPTY_HINTS = /** @type {SqlSchemaHints} */ ({})
/** What a trigger body calls its rows. */
const ROW_ALIASES = ['new', 'old', 'inserted', 'deleted']

/**
 * Postgres: the table a trigger function serves, from the CREATE TRIGGER in
 * the document that EXECUTEs it.
 * @param {string} doc @param {string} fn
 */
function tableForTriggerFunction(doc, fn) {
  const name = fn.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const re = new RegExp(
    `create\\s+(?:or\\s+replace\\s+)?(?:constraint\\s+)?trigger\\b[^;]*?\\bon\\s+(?:[\\w$]+\\.|"[^"]+"\\.)?("[^"]+"|[\\w$]+)[^;]*?\\bexecute\\s+(?:function|procedure)\\s+(?:[\\w$]+\\.|"[^"]+"\\.)?"?${name}"?\\s*\\(`,
    'i',
  )
  const m = re.exec(doc)
  return m ? m[1].replace(/^"|"$/g, '') : null
}
/** Tables already waited for, per table list - a table with no columns waits once. */
/** @type {WeakMap<object, Set<string>>} */
const waitedFor = new WeakMap()

/**
 * @param {() => SqlSchemaHints | null | undefined} getHints read per query, so new hints need no reconfigure
 * @param {() => string} getDialect the app's Dialect id ('postgres', 'mysql', ...)
 * @returns {import('@codemirror/autocomplete').CompletionSource}
 */
export function sqlCompletionSource(getHints, getDialect) {
  /** @type {{ key: string, hints: SqlSchemaHints, entries: Entry[] } | null} */
  let memo = null

  return (context) => {
    const { state } = context
    // A selected snippet field (`${column}`) is a slot: complete for where it
    // starts, and replace the placeholder text.
    const sel = state.selection.main
    const pos = sel.empty ? context.pos : sel.from
    const to = sel.empty ? context.pos : sel.to

    const range = statementRange(state, pos)
    const head = state.sliceDoc(range.start, pos)
    const start = range.start + afterBlankLine(head)
    const end = to + beforeBlankLine(state.sliceDoc(to, Math.max(to, range.end)))
    const ctx = sqlCompletionContext(state.sliceDoc(start, pos))
    if (!ctx) return null
    // Nothing typed: open by itself only where the next token is certainly a
    // name or a type - just inside a quote, just after a dot or a `::`. A
    // snippet field stays quiet until something is typed over it (`*` and
    // `100` are often kept as they are). Ctrl+Space always opens.
    const afterCast = ctx.kind === 'types' && state.sliceDoc(pos - 2, pos) === '::'
    if (!context.explicit && !ctx.prefix && !ctx.quote && ctx.kind !== 'qualified' && !afterCast && !ctx.eager) return null

    const dialect = getDialect() || 'postgres'
    const S = staticTemplates(sqlFamily(dialect))
    const wordFrom = start + ctx.from
    // What the candidates depend on: the statement minus the word being typed,
    // and the shape of the position. Same key → same candidates.
    const key = [
      ctx.kind, ctx.quote, ctx.qualifier, ctx.afterExpr, ctx.prefix !== '', typedInCapitals(ctx.prefix),
      phraseLower(ctx.prefix, ctx.lower), dialect,
      state.sliceDoc(start, wordFrom), state.sliceDoc(to, end),
    ].join('\u0001')

    const statement = state.sliceDoc(start, end)
    // A Postgres trigger function's rows belong to the table of the trigger
    // that runs it, further down the document.
    const rowTable = ctx.rowTable ?? (ctx.routine ? tableForTriggerFunction(state.doc.toString(), ctx.routine) : null)
    /** @param {SqlSchemaHints} hints */
    const candidates = (hints) => {
      if (memo && memo.key === key && memo.hints === hints) return { entries: memo.entries, missing: [] }
      const built = buildCandidates(ctx, hintTemplates(hints), S, dialect, statement, rowTable)
      memo = { key, hints, entries: built.entries }
      return built
    }
    /** @param {Entry[]} entries */
    const result = (entries) => finish(entries, ctx.prefix.toLowerCase(), wordFrom, to)

    const hints = getHints() ?? EMPTY_HINTS
    const built = candidates(hints)
    // The statement names a table whose columns are not loaded yet: wait for
    // them once (briefly), then answer from the fresh hints.
    const tablesKey = hints.tables ?? EMPTY_HINTS
    let waited = waitedFor.get(tablesKey)
    if (!waited) { waited = new Set(); waitedFor.set(tablesKey, waited) }
    const fresh = built.missing.filter((t) => !waited.has(t))
    if (!fresh.length || !hints.loadColumns) return result(built.entries)
    for (const t of fresh) waited.add(t)
    return Promise.race([hints.loadColumns(fresh), new Promise((r) => setTimeout(r, COLUMN_WAIT_MS))])
      .catch(() => {})
      .then(() => (context.aborted ? null : result(candidates(getHints() ?? EMPTY_HINTS).entries)))
  }
}

/**
 * Whether taking `c` would leave the text as it is: the word before the caret
 * already is that keyword or name. A snippet, a schema (it adds a dot), a name
 * that needs quoting and a value all still write something.
 * @param {import('@codemirror/state').EditorState} state @param {Completion} c
 */
export function completionIsTypedOut(state, c) {
  const head = state.selection.main.head
  const line = state.doc.lineAt(head)
  const word = /[\w$]*$/.exec(state.sliceDoc(line.from, head))?.[0] ?? ''
  if (!word) return false
  if (c.apply === applyName) {
    const n = /** @type {Completion & { _quote?: string | null, _dialect?: string, _suffix?: string }} */ (c)
    if (n._quote || n._suffix || needsQuote(c.label, n._dialect ?? 'postgres')) return false
    return word === c.label
  }
  // A one-word phrase typed in full (FROM after DELETE): Enter breaks the line.
  if (c.apply === applyPhrase) return !c.label.includes(' ') && word.toLowerCase() === c.label.toLowerCase()
  if (c.apply) return false
  // A keyword typed in another case: taking it would only change the case.
  return word.toLowerCase() === c.label.toLowerCase()
}

/**
 * Enter with the list open on a word already typed out (`FROM users` and
 * `users` on top) closes the list and lets Enter break the line. Taking the
 * suggestion would change nothing and cost the keystroke. Bound above the
 * completion keymap, so it runs first.
 * @param {EditorView} view
 */
export function enterPastTypedWord(view) {
  if (completionStatus(view.state) !== 'active') return false
  const c = selectedCompletion(view.state)
  if (!c || !completionIsTypedOut(view.state, c)) return false
  closeCompletion(view)
  return false
}

/**
 * Filter and sort the candidates by what has been typed.
 * @param {Entry[]} entries @param {string} q @param {number} from @param {number} to
 * @returns {import('@codemirror/autocomplete').CompletionResult | null}
 */
function finish(entries, q, from, to) {
  /** @type {Array<{ e: Entry, score: number, at: number }>} */
  const hits = []
  for (const e of entries) {
    const m = matchLabel(e.c.label, e.lc, q, e.aliases)
    // A keyword is typed from its start: `em` finds user_email, never TEMP. A
    // snippet from the start of one of its words: `na` is not EXPLAIN ANALYZE.
    if (m && (!q || (e.c.type === 'keyword' ? m.score >= 5 : e.c.type === 'snippet' ? m.score >= 4 : true))) {
      hits.push({ e, score: m.score, at: m.at })
    }
  }
  if (!hits.length) return null
  // Match kind, then context tier, then (while typing) the shorter name, then
  // the order they were listed in - a table's columns stay in table order.
  hits.sort((a, b) =>
    b.score - a.score ||
    (b.e.c.boost ?? 0) - (a.e.c.boost ?? 0) ||
    (q ? a.e.c.label.length - b.e.c.label.length : 0) ||
    a.e.order - b.e.order,
  )
  if (hits.length > MAX_OPTIONS) hits.length = MAX_OPTIONS
  /** @type {Map<Completion, number>} */
  const at = new Map()
  const options = hits.map((h) => { at.set(h.e.c, h.at); return h.e.c })
  return {
    from,
    to,
    options,
    // Filtered and sorted here; asked again on every keystroke.
    filter: false,
    getMatch: (c) => {
      const i = at.get(c) ?? -1
      return i >= 0 && q ? [i, i + q.length] : []
    },
  }
}
