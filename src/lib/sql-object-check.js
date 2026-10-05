/**
 * Schema checks for the objects the editor creates: triggers, functions,
 * procedures, views and events.
 *
 * The engines accept these with a body that cannot run. SQLite and MySQL parse
 * a trigger or routine body without looking its tables up, and PL/pgSQL checks
 * only the syntax, so `UPDATE table_name …` left in from a template is created
 * without a word and fails the first time the trigger fires - on every write to
 * the table it is on. This reads each CREATE against the schema the editor
 * knows and reports the names that are not in it, before the statement is sent.
 *
 * Only what can be known is reported: a name in another schema, a function
 * call, a CTE, a table the script creates first, or columns not loaded yet are
 * all left alone.
 */
import { splitSqlStatements } from './sql-statements.js'

/** @typedef {import('./sql-statements.js').SqlDiagnostic} SqlDiagnostic */
/** @typedef {import('./sql-complete-data.js').SqlSchemaHints} SqlSchemaHints */
/** @typedef {{ t: 'word' | 'qid' | 'str' | 'num' | 'var' | 'punct', v: string, start: number, end: number }} Tok */

/** The statement defines one of these, and so has a body to check. */
const OBJECT_HEAD =
  /^(?:create|alter)\s+(?:or\s+(?:replace|alter)\s+)?(?:definer\s*=\s*\S+\s+)?(?:algorithm\s*=\s*\w+\s+)?(?:sql\s+security\s+\w+\s+)?(?:temp(?:orary)?\s+)?(?:constraint\s+)?(?:materialized\s+)?(?:recursive\s+)?(trigger|function|procedure|proc|view|event)\b/i
/** A table or view the script itself creates, for the bodies after it. */
const CREATES_TABLE =
  /^create\s+(?:or\s+replace\s+)?(?:global\s+|local\s+)?(?:temp(?:orary)?\s+|unlogged\s+)?(?:table|view|materialized\s+view)\s+(?:if\s+not\s+exists\s+)?((?:[\w$]+|"[^"]+"|`[^`]+`|\[[^\]]+\])(?:\s*\.\s*(?:[\w$]+|"[^"]+"|`[^`]+`|\[[^\]]+\]))*)/i

/** Rows a trigger body reads without a table of that name. */
const PSEUDO_TABLES = new Set(['new', 'old', 'inserted', 'deleted', 'excluded', 'dual'])
/** Columns every SQLite row has without declaring them. */
const SQLITE_ROW_IDS = new Set(['rowid', 'oid', '_rowid_'])
/** Functions whose argument list has a FROM of its own: EXTRACT(YEAR FROM ts). */
const FROM_FUNCTIONS = new Set(['EXTRACT', 'SUBSTRING', 'SUBSTR', 'TRIM', 'OVERLAY', 'POSITION'])
/** Words that can follow FROM / JOIN / UPDATE without being the table. */
const NOT_A_TABLE = new Set([
  'SELECT', 'LATERAL', 'ONLY', 'VALUES', 'UNNEST', 'STATISTICS', 'DUAL', 'WHERE', 'SET', 'ON', 'OF', 'OR',
])
/** Where a SET list ends. */
const SET_LIST_ENDS = new Set(['WHERE', 'FROM', 'RETURNING', 'LIMIT', 'ORDER', 'OUTPUT', 'END'])

/** @param {string | undefined | null} dialect */
function family(dialect) {
  const d = String(dialect ?? '').toLowerCase()
  if (d === 'sqlite' || d === 'd1' || d === 'libsql') return 'sqlite'
  if (d === 'mysql' || d === 'mariadb' || d === 'tidb' || d === 'planetscale') return 'mysql'
  if (d === 'mssql' || d === 'sqlserver') return 'mssql'
  return 'postgres'
}

/**
 * Tokens with their offsets. A `$$` / `$tag$` is a token of its own, so a
 * Postgres function body is read as the SQL it is.
 * @param {string} text @param {number} base offset of `text` in the document
 * @param {string} fam
 * @returns {Tok[]}
 */
function tokenize(text, base, fam) {
  /** @type {Tok[]} */
  const out = []
  const n = text.length
  let i = 0
  const push = (/** @type {Tok['t']} */ t, /** @type {string} */ v, /** @type {number} */ s, /** @type {number} */ e) =>
    out.push({ t, v, start: base + s, end: base + e })
  while (i < n) {
    const c = text[i]
    if (c === ' ' || c === '\t' || c === '\n' || c === '\r') { i++; continue }
    if ((c === '-' && text[i + 1] === '-') || (c === '#' && fam === 'mysql')) {
      const nl = text.indexOf('\n', i)
      i = nl === -1 ? n : nl + 1
      continue
    }
    if (c === '/' && text[i + 1] === '*') { const e = text.indexOf('*/', i + 2); i = e === -1 ? n : e + 2; continue }
    if (c === '$') {
      const m = /^\$(?:[A-Za-z_]\w*)?\$/.exec(text.slice(i, i + 64))
      if (m) { push('punct', m[0], i, i + m[0].length); i += m[0].length; continue }
    }
    if (c === "'") {
      let j = i + 1
      while (j < n) {
        if (text[j] === '\\' && fam === 'mysql') { j += 2; continue }
        if (text[j] === "'") { if (text[j + 1] === "'") { j += 2; continue } break }
        j++
      }
      push('str', '', i, Math.min(n, j + 1))
      i = j + 1
      continue
    }
    if (c === '"' || c === '`' || (c === '[' && fam === 'mssql')) {
      const close = c === '[' ? ']' : c
      let j = i + 1
      while (j < n) { if (text[j] === close) { if (text[j + 1] === close) { j += 2; continue } break } j++ }
      push('qid', text.slice(i + 1, j).replaceAll(close + close, close), i, Math.min(n, j + 1))
      i = j + 1
      continue
    }
    if (c === '@' || (c === '#' && fam === 'mssql') || (c === ':' && /[A-Za-z_]/.test(text[i + 1] ?? ''))) {
      // @variable, #temp_table, :bind - never a name in the schema.
      let j = i + 1
      while (j < n && /[@#\w$]/.test(text[j])) j++
      push('var', text.slice(i, j), i, j)
      i = j
      continue
    }
    if (/[A-Za-z_]/.test(c)) {
      let j = i + 1
      while (j < n && /[\w$]/.test(text[j])) j++
      push('word', text.slice(i, j), i, j)
      i = j
      continue
    }
    if (/[0-9]/.test(c)) {
      let j = i + 1
      while (j < n && /[\w.]/.test(text[j])) j++
      push('num', text.slice(i, j), i, j)
      i = j
      continue
    }
    push('punct', c, i, i + 1)
    i++
  }
  return out
}

/** @param {Tok | undefined} tok */
const kw = (tok) => (tok?.t === 'word' ? tok.v.toUpperCase() : '')
/** @param {Tok | undefined} tok */
const isName = (tok) => tok?.t === 'word' || tok?.t === 'qid'
/** @param {Tok | undefined} tok @param {string} p */
const isPunct = (tok, p) => tok?.t === 'punct' && tok.v === p

/**
 * `[db.][schema.]name` starting at `i`.
 * @param {Tok[]} toks @param {number} i
 * @returns {{ schema: string | null, name: string, tok: Tok, end: number } | null}
 */
function dottedName(toks, i) {
  if (!isName(toks[i])) return null
  const parts = [toks[i]]
  let j = i + 1
  while (isPunct(toks[j], '.') && isName(toks[j + 1])) { parts.push(toks[j + 1]); j += 2 }
  const tok = /** @type {Tok} */ (parts.at(-1))
  return { schema: parts.length > 1 ? parts.at(-2)?.v ?? null : null, name: tok.v, tok, end: j }
}

/**
 * Columns per short, lower-cased table name, for the tables whose columns the
 * hints carry. A table missing from this map has not been loaded yet.
 * @param {SqlSchemaHints} hints
 */
function columnsOf(hints) {
  /** @type {Map<string, Set<string>>} */
  const out = new Map()
  for (const [key, cols] of Object.entries(hints.columnsByTable ?? {})) {
    if (key === '__result__' || !Array.isArray(cols)) continue
    const short = (key.split('.').pop() ?? key).toLowerCase()
    const set = out.get(short) ?? new Set()
    for (const c of cols) {
      const name = typeof c === 'string' ? c : c?.name
      if (name) set.add(name.toLowerCase())
    }
    out.set(short, set)
  }
  return out
}

/** @param {string} text */
const stripLead = (text) => text.replace(/^(?:\s+|--[^\n]*(?:\n|$)|\/\*[\s\S]*?\*\/)*/, '')

/**
 * Index of the word starting the object's name and the kind, past CREATE ... KIND.
 * @param {Tok[]} toks @param {string} kind
 */
function afterKind(toks, kind) {
  const want = kind === 'proc' ? 'PROC' : kind.toUpperCase()
  const at = toks.findIndex((t) => kw(t) === want)
  if (at < 0) return -1
  let i = at + 1
  if (kw(toks[i]) === 'IF') i += kw(toks[i + 1]) === 'NOT' ? 3 : 2
  return i
}

/**
 * The table a trigger fires on: the name after its ON (SQL Server: right after
 * the trigger's name). Null for a database or server trigger.
 * @param {Tok[]} toks
 */
function triggerTarget(toks) {
  const start = afterKind(toks, 'trigger')
  if (start < 0) return null
  const own = dottedName(toks, start)
  for (let i = own ? own.end : start; i < toks.length; i++) {
    const k = kw(toks[i])
    if (k === 'BEGIN' || k === 'AS' || k === 'EXECUTE' || k === 'FOR' && kw(toks[i + 1]) === 'EACH') break
    if (k !== 'ON') continue
    if (['DATABASE', 'ALL', 'SCHEMA'].includes(kw(toks[i + 1]))) return null
    const ref = dottedName(toks, i + 1)
    return ref ? { ...ref, onIndex: i } : null
  }
  return null
}

/**
 * Postgres: the table each trigger function serves, from the CREATE TRIGGERs
 * in the script that EXECUTE it.
 * @param {{ toks: Tok[], kind: string }[]} objects
 */
function triggerFunctionTables(objects) {
  /** @type {Map<string, string>} */
  const out = new Map()
  for (const o of objects) {
    if (o.kind !== 'trigger') continue
    const target = triggerTarget(o.toks)
    const exec = o.toks.findIndex((t, i) => kw(t) === 'EXECUTE' && ['FUNCTION', 'PROCEDURE'].includes(kw(o.toks[i + 1])))
    const fn = exec >= 0 ? dottedName(o.toks, exec + 2) : null
    if (target && fn) out.set(fn.name.toLowerCase(), target.name)
  }
  return out
}

/**
 * Problems in the CREATE TRIGGER / FUNCTION / PROCEDURE / VIEW / EVENT
 * statements of `text`: tables and views the schema does not have, NEW / OLD
 * and SET columns the table does not have, a BEGIN with no END.
 * @param {string} text
 * @param {SqlSchemaHints | null | undefined} hints
 * @param {string} [dialect]
 * @returns {{ diags: SqlDiagnostic[], missing: string[] }} `missing`: tables
 *   whose columns would be checked once loaded
 */
export function checkObjectSql(text, hints, dialect) {
  /** @type {SqlDiagnostic[]} */
  const diags = []
  /** @type {Set<string>} */
  const missing = new Set()
  const tableList = hints?.tables ?? []
  // No table list (not loaded, no connection): nothing can be said about names.
  if (!hints || !tableList.length) return { diags, missing: [] }
  const fam = family(dialect)
  const known = new Set(tableList.map((t) => t.toLowerCase()))
  const cols = columnsOf(hints)
  const active = String(hints.activeSchema ?? '').toLowerCase()

  const statements = splitSqlStatements(text)
  /** Tables the script creates before a body uses them. */
  const created = new Set()
  /** @type {{ stmt: import('./sql-statements.js').SqlStatement, toks: Tok[], kind: string, createdBefore: Set<string> }[]} */
  const objects = []
  for (const stmt of statements) {
    const body = stripLead(stmt.text)
    const head = OBJECT_HEAD.exec(body)
    if (head) {
      const kind = head[1].toLowerCase()
      objects.push({ stmt, kind, toks: tokenize(stmt.text, stmt.start, fam), createdBefore: new Set(created) })
      if (kind === 'view') {
        const toks = objects.at(-1)?.toks ?? []
        const own = dottedName(toks, afterKind(toks, 'view'))
        if (own) created.add(own.name.toLowerCase())
      }
    }
    const ct = CREATES_TABLE.exec(body)
    if (ct) created.add(ct[1].split('.').pop()?.trim().replace(/^["`[]|["`\]]$/g, '').toLowerCase() ?? '')
  }
  if (!objects.length) return { diags, missing: [] }
  const fnTables = fam === 'postgres' ? triggerFunctionTables(objects) : new Map()

  /** @param {Tok} tok @param {string} message */
  const error = (tok, message) => diags.push({ message, severity: 'error', start: tok.start, end: tok.end })

  for (const { toks, kind, createdBefore } of objects) {
    // A body in another language (plv8, plpython ...) is not SQL to read.
    const lang = toks.findIndex((t) => kw(t) === 'LANGUAGE')
    const langName = lang >= 0 ? (toks[lang + 1]?.v ?? '').toLowerCase() : ''
    const sqlBody = !langName || langName === 'sql' || langName === 'plpgsql'

    // CTE names: WITH name AS ( ... ), name AS ( ... ).
    const ctes = new Set()
    for (let i = 0; i < toks.length; i++) {
      if ((kw(toks[i]) === 'WITH' || isPunct(toks[i], ',')) && isName(toks[i + 1])) {
        let j = i + 2
        if (isPunct(toks[j], '(')) { while (j < toks.length && !isPunct(toks[j], ')')) j++; j++ }
        if (kw(toks[j]) === 'AS') ctes.add(toks[i + 1].v.toLowerCase())
      }
    }

    /** @param {{ schema: string | null, name: string, tok: Tok }} ref */
    const tableKnown = (ref) => {
      const name = ref.name.toLowerCase()
      const schema = ref.schema?.toLowerCase() ?? null
      // Another schema's tables are not in the hints: nothing to say.
      if (schema && schema !== active && !(fam === 'sqlite' && (schema === 'main' || schema === 'temp'))) return true
      if (known.has(name) || createdBefore.has(name) || ctes.has(name) || PSEUDO_TABLES.has(name)) return true
      return name.startsWith('sqlite_') || name.startsWith('pg_') || schema === 'information_schema'
    }
    /** @param {{ schema: string | null, name: string, tok: Tok }} ref */
    const reportTable = (ref) => {
      if (tableKnown(ref)) return
      const where = hints.activeSchema ? ` in ${hints.activeSchema}` : ''
      error(
        ref.tok,
        ref.name.toLowerCase() === 'table_name'
          ? '"table_name" is the template\'s placeholder: put the table here'
          : `No table or view named "${ref.name}"${where}`,
      )
    }
    /** Columns of `table`, or null while they are not loaded. @param {string} table */
    const columnsFor = (table) => {
      const set = cols.get(table.toLowerCase())
      if (!set && known.has(table.toLowerCase())) missing.add(table)
      return set ?? null
    }

    // ── The table a trigger is on ────────────────────────────────────────
    /** @type {string | null} */
    let rowTable = null
    if (kind === 'trigger') {
      const target = triggerTarget(toks)
      if (target) {
        reportTable(target)
        if (tableKnown(target)) rowTable = target.name
      }
    } else if (kind === 'function') {
      const own = dottedName(toks, afterKind(toks, 'function'))
      rowTable = own ? fnTables.get(own.name.toLowerCase()) ?? null : null
    }

    if (sqlBody) {
      // ── Tables the body reads and writes ───────────────────────────────
      /** The word before each open `(`, innermost last. @type {string[]} */
      const parens = []
      for (let i = 0; i < toks.length; i++) {
        const tok = toks[i]
        if (isPunct(tok, '(')) { parens.push(kw(toks[i - 1])); continue }
        if (isPunct(tok, ')')) { parens.pop(); continue }
        const k = kw(tok)
        let at = -1
        if (k === 'FROM') {
          if (kw(toks[i - 1]) === 'DISTINCT' || FROM_FUNCTIONS.has(parens.at(-1) ?? '')) continue
          at = i + 1
        } else if (k === 'JOIN') {
          at = i + 1
        } else if (k === 'INTO') {
          // INSERT / MERGE / REPLACE INTO a table; SELECT ... INTO a variable.
          const back = toks.slice(Math.max(0, i - 4), i).map(kw)
          if (!back.some((w) => w === 'INSERT' || w === 'MERGE' || w === 'REPLACE')) continue
          at = i + 1
        } else if (k === 'UPDATE') {
          // AFTER UPDATE ON, UPDATE OF col, INSERT OR UPDATE, FOR UPDATE, ON UPDATE CASCADE: events and options.
          const prev = kw(toks[i - 1])
          if (prev === 'FOR' || prev === 'ON' || prev === 'OR' || prev === 'KEY') continue
          if (['ON', 'OF', 'OR'].includes(kw(toks[i + 1])) || isPunct(toks[i + 1], ',')) continue
          at = i + 1
        } else if (k === 'TRUNCATE') {
          at = kw(toks[i + 1]) === 'TABLE' ? i + 2 : i + 1
        }
        if (at < 0) continue
        if (kw(toks[at]) === 'ONLY') at++
        if (NOT_A_TABLE.has(kw(toks[at]))) continue
        const ref = dottedName(toks, at)
        // A call (generate_series(...), json_each(...)) is a function, not a table.
        if (!ref || isPunct(toks[ref.end], '(')) continue
        reportTable(ref)

        // UPDATE t [alias] SET a = ..., b = ...: the columns must be t's.
        if (k === 'UPDATE' && tableKnown(ref) && !PSEUDO_TABLES.has(ref.name.toLowerCase())) {
          let s = ref.end
          if (kw(toks[s]) === 'AS') s++
          if (isName(toks[s]) && kw(toks[s]) !== 'SET') s++
          if (kw(toks[s]) !== 'SET' || isPunct(toks[s + 1], '(')) continue
          const set = columnsFor(ref.name)
          if (!set) continue
          let depth = 0
          let expectName = true
          for (let j = s + 1; j < toks.length; j++) {
            const t = toks[j]
            if (isPunct(t, '(')) depth++
            else if (isPunct(t, ')')) { if (depth === 0) break; depth-- }
            else if (depth === 0 && (isPunct(t, ';') || SET_LIST_ENDS.has(kw(t)))) break
            else if (depth === 0 && isPunct(t, ',')) { expectName = true; continue }
            if (!expectName || depth > 0) continue
            expectName = false
            if (!isName(t)) continue
            // alias.col or col
            const colTok = isPunct(toks[j + 1], '.') && isName(toks[j + 2]) ? toks[j + 2] : t
            const name = colTok.v.toLowerCase()
            if (set.has(name) || (fam === 'sqlite' && SQLITE_ROW_IDS.has(name))) continue
            error(colTok, `${ref.name} has no column "${colTok.v}"`)
          }
        }
      }

      // ── NEW.col / OLD.col: the trigger's table must have them ───────────
      const rowCols = rowTable ? columnsFor(rowTable) : null
      if (rowCols) {
        for (let i = 0; i + 2 < toks.length; i++) {
          const q = kw(toks[i])
          if ((q !== 'NEW' && q !== 'OLD') || !isPunct(toks[i + 1], '.') || !isName(toks[i + 2])) continue
          if (isPunct(toks[i - 1], '.')) continue
          const name = toks[i + 2].v.toLowerCase()
          if (rowCols.has(name) || (fam === 'sqlite' && SQLITE_ROW_IDS.has(name))) continue
          error(toks[i + 2], `${rowTable} has no column "${toks[i + 2].v}"`)
        }
      }
    }

    // ── BEGIN ... END: a body left open swallows the rest of the script ─
    if (kind !== 'view' && !toks.some((t) => t.t === 'punct' && t.v.startsWith('$'))) {
      /** @type {Tok[]} */
      const open = []
      for (let i = 0; i < toks.length; i++) {
        const k = kw(toks[i])
        if (k === 'BEGIN') {
          if (!['TRAN', 'TRANSACTION', 'WORK', 'DISTRIBUTED'].includes(kw(toks[i + 1]))) open.push(toks[i])
        } else if (k === 'CASE') {
          if (kw(toks[i - 1]) !== 'END') open.push(toks[i])
        } else if (k === 'END') {
          if (!['IF', 'LOOP', 'WHILE', 'REPEAT'].includes(kw(toks[i + 1]))) open.pop()
        }
      }
      for (const tok of open) {
        if (kw(tok) === 'BEGIN') error(tok, 'This BEGIN has no END: the body runs to the end of the script')
      }
    }
  }
  return { diags, missing: [...missing] }
}

/**
 * The first problem that should stop a CREATE from being run, as a sentence.
 * @param {SqlDiagnostic[]} diags
 */
export function objectProblemSummary(diags) {
  const errors = diags.filter((d) => d.severity === 'error')
  if (!errors.length) return ''
  const more = errors.length > 1 ? ` (and ${errors.length - 1} more)` : ''
  return `${errors[0].message}${more}`
}
