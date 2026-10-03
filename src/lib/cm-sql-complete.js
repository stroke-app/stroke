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
 * Cost per keystroke: the candidates for a position are built once (analysis,
 * tiers, one object per option) and kept while only the word under the caret
 * changes; each keystroke then just filters and sorts them.
 *
 * Only the CodeEditor chunk imports this; none of it reaches startup.
 */
import { snippetCompletion, pickedCompletion, startCompletion } from '@codemirror/autocomplete'
import { statementAt } from '$lib/cm-sql-statements.js'
import { sqlCompletionContext } from '$lib/sql-complete-context.js'
import {
  PG_KEYWORDS, PG_FUNCTIONS, SQL_SNIPPETS, TABLE_CTX_KWS, COLUMN_CTX_KWS, SQL_KW_SET, analyzeQuery,
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

const isMysql = (/** @type {string} */ d) => d === 'mysql' || d === 'mariadb'

/**
 * Whether a name has to be quoted to mean itself. Postgres folds bare names to
 * lower case, so `userId` must be quoted there; MySQL does not fold.
 * @param {string} name @param {string} dialect
 */
function needsQuote(name, dialect) {
  if (SQL_KW_SET.has(name.toUpperCase())) return true
  return isMysql(dialect) ? !/^[A-Za-z_][\w$]*$/.test(name) : !/^[a-z_][a-z0-9_$]*$/.test(name)
}

/** @param {string} name @param {string} dialect */
function quoteName(name, dialect) {
  if (!needsQuote(name, dialect)) return name
  const q = isMysql(dialect) ? '`' : '"'
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
  const quote = c._quote ?? null
  const suffix = c._suffix ?? ''
  /** @type {{ from: number, to?: number, insert: string }[]} */
  let changes
  let end
  if (quote) {
    const closes = view.state.sliceDoc(to, to + 1) === quote
    const name = c.label.replaceAll(quote, quote + quote) + (closes ? '' : quote)
    changes = [{ from, to, insert: name }]
    if (suffix) changes.push({ from: closes ? to + 1 : to, insert: suffix })
    end = from + name.length + (closes ? 1 : 0) + suffix.length
  } else {
    const name = quoteName(c.label, c._dialect ?? 'postgres') + suffix
    changes = [{ from, to, insert: name }]
    end = from + name.length
  }
  view.dispatch({
    changes,
    selection: { anchor: end },
    annotations: pickedCompletion.of(c),
    userEvent: 'input.complete',
  })
  if (c._reopen) setTimeout(() => startCompletion(view))
}

// ── Templates ────────────────────────────────────────────────────────────────

/** Monaco snippet syntax → CodeMirror's: `$0` (final caret) becomes `${}`. */
const toSnippet = (/** @type {string} */ body) => body.replace(/\$0/g, '${}')
/** A signature without its tab stops, for the detail column. */
const plainSig = (/** @type {string} */ body) => body.replace(/\$\{\d+:?([^}]*)\}/g, '$1').replace(/\$\d+/g, '')

/** The SQL a snippet writes, beside the list - the name alone says little. */
const snippetPreview = (/** @type {string} */ body) => () => {
  const pre = document.createElement('pre')
  pre.className = 'cm-snippet-preview'
  pre.textContent = plainSig(body)
  return pre
}

/** @typedef {{ keywords: Completion[], functions: Completion[], snippets: Array<Completion & { aliases: string[] }> }} StaticTemplates */
/** @type {Map<boolean, StaticTemplates>} */
const staticCache = new Map()

/** @param {boolean} pg Postgres-family: include the Postgres-only snippets */
function staticTemplates(pg) {
  const hit = staticCache.get(pg)
  if (hit) return hit
  const built = {
    keywords: PG_KEYWORDS.map((label) => ({ label, type: 'keyword' })),
    functions: PG_FUNCTIONS.map((fn) =>
      snippetCompletion(toSnippet(fn.sig), { label: fn.label, type: 'function', detail: plainSig(fn.sig), info: fn.doc }),
    ),
    snippets: SQL_SNIPPETS.filter((s) => pg || !s.pg).map((s) => ({
      ...snippetCompletion(toSnippet(s.body), { label: s.name, type: 'snippet', detail: s.alias, info: snippetPreview(s.body) }),
      aliases: [s.alias],
    })),
  }
  staticCache.set(pg, built)
  return built
}

/**
 * @typedef {{ name: string, table: string, type: string }} ColumnHint
 * @typedef {{
 *   activeSchema: string,
 *   schemas: string[],
 *   tables: string[],
 *   tableSet: Set<string>,
 *   colsByTable: Map<string, ColumnHint[]>,
 *   enums: Completion[],
 *   userFns: Completion[],
 * }} HintTemplates
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
  const tables = hints.tables ?? []
  const built = {
    activeSchema: hints.activeSchema ?? 'public',
    schemas: hints.schemas ?? [],
    tables,
    tableSet: new Set(tables.map((t) => t.toLowerCase())),
    colsByTable,
    enums,
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
 * @returns {{ entries: Entry[], missing: string[] }} `missing`: named tables with no columns known
 */
function buildCandidates(ctx, H, S, dialect, statement) {
  const { aliasMap, referencedTables } = analyzeQuery(statement, H.tables)
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
  const refs = new Set([...referencedTables, ...ctx.tables.map((t) => t.toLowerCase())])
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

  /** @param {number} tier @param {Set<string> | null} only */
  function keywords(tier, only) {
    const next = new Set(ctx.kind === 'keywords' || ctx.afterExpr || ctx.kind === 'statement' ? ctx.next : [])
    for (const k of S.keywords) {
      if (next.has(k.label)) { add(k, NEXT_BOOST); continue }
      if (only && !only.has(k.label)) continue
      add(k, TIER[tier])
    }
  }

  if (ctx.kind === 'qualified') {
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
    // Past the table name: the clause keywords.
    keywords(0, typed ? null : TABLE_CTX_KWS)
    for (const t of H.tables) tableOption(t, 7)
  } else if (ctx.kind === 'columns') {
    // SELECT / WHERE / SET / ON ...
    columns(0, 1)
    for (const f of S.functions) add(f, TIER[2])
    for (const f of H.userFns) add(f, TIER[2])
    keywords(3, typed ? null : COLUMN_CTX_KWS)
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
    // name - just inside a quote, just after a dot. A snippet field stays quiet
    // until something is typed over it (`*` and `100` are often kept as they
    // are). Ctrl+Space always opens.
    if (!context.explicit && !ctx.prefix && !ctx.quote && ctx.kind !== 'qualified') return null

    const dialect = getDialect() || 'postgres'
    const S = staticTemplates(dialect === 'postgres' || dialect === 'duckdb')
    const wordFrom = start + ctx.from
    // What the candidates depend on: the statement minus the word being typed,
    // and the shape of the position. Same key → same candidates.
    const key = [
      ctx.kind, ctx.quote, ctx.qualifier, ctx.afterExpr, ctx.prefix !== '', dialect,
      state.sliceDoc(start, wordFrom), state.sliceDoc(to, end),
    ].join('\u0001')

    const statement = state.sliceDoc(start, end)
    /** @param {SqlSchemaHints} hints */
    const candidates = (hints) => {
      if (memo && memo.key === key && memo.hints === hints) return { entries: memo.entries, missing: [] }
      const built = buildCandidates(ctx, hintTemplates(hints), S, dialect, statement)
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
 * Filter and sort the candidates by what has been typed.
 * @param {Entry[]} entries @param {string} q @param {number} from @param {number} to
 * @returns {import('@codemirror/autocomplete').CompletionResult | null}
 */
function finish(entries, q, from, to) {
  /** @type {Array<{ e: Entry, score: number, at: number }>} */
  const hits = []
  for (const e of entries) {
    const m = matchLabel(e.c.label, e.lc, q, e.aliases)
    if (m) hits.push({ e, score: m.score, at: m.at })
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
