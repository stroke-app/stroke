/**
 * Revert for a write run in the SQL console.
 *
 * A single UPDATE, DELETE or INSERT run from the console keeps what it takes to
 * put things back: the rows it is about to change are read in the same
 * transaction as the write, and the statements that reverse it are written
 * there and then. Revert runs them in a transaction of its own.
 *
 * Every value is captured as a SQL literal the database writes itself
 * (`quote_nullable` on Postgres, `quote` on SQLite, hex on MySQL), never as
 * JSON. A bigint above 2^53, a
 * numeric with forty digits, a bytea, an array, a timestamp with microseconds:
 * each comes back exactly as it was, and nothing a cell contains can end up
 * parsed as SQL, because the database quoted it for its own parser.
 *
 * Schema changes have a revert too, where one exists: a CREATE is undone by
 * dropping what it made (never CASCADE, so a drop that would take something
 * else with it fails instead), ADD COLUMN by dropping the column, a RENAME by
 * renaming back. DROP and CREATE OR REPLACE have none: what they removed or
 * replaced is gone.
 *
 * What a revert can't undo it says so instead: the work of triggers, and rows
 * in other tables an ON DELETE rule changed. Rows that changed again after the
 * run are left alone (an UPDATE's revert only touches rows still holding the
 * values it wrote), and a DELETE's revert skips keys that are taken again.
 */
import { splitSqlStatements } from './sql-statements.js'

/** A write that touches more rows than this runs without an undo copy. */
export const UNDO_MAX_ROWS = 10_000
/** Or more data than this (Postgres measures it before reading anything). */
export const UNDO_MAX_BYTES = 64 * 1024 * 1024
/** Rows per revert statement. */
const CHUNK = 500

// ── Lexer ────────────────────────────────────────────────────────────────────

/**
 * @typedef {{
 *   t: 'word' | 'ident' | 'string' | 'num' | 'param' | 'punct',
 *   text: string,
 *   up: string,
 *   start: number,
 *   end: number,
 *   depth: number,
 * }} Tok
 * `ident` is a quoted identifier with its quotes removed (`text` is the name);
 * `depth` is the parenthesis depth the token sits at.
 */

/**
 * Tokens of one statement, comments and whitespace dropped. Enough of SQL to
 * find clause boundaries: strings (with E'' escapes), dollar quotes, quoted
 * identifiers and nested block comments never end a clause early.
 * @param {string} sql
 * @returns {Tok[]}
 */
export function lex(sql) {
  /** @type {Tok[]} */
  const out = []
  const n = sql.length
  let i = 0
  let depth = 0
  /** @param {Tok['t']} t @param {string} text @param {number} start @param {number} end */
  const push = (t, text, start, end) => out.push({ t, text, up: t === 'word' ? text.toUpperCase() : text, start, end, depth })
  while (i < n) {
    const c = sql[i]
    if (/\s/.test(c)) { i++; continue }
    if (c === '-' && sql[i + 1] === '-') {
      while (i < n && sql[i] !== '\n') i++
      continue
    }
    if (c === '/' && sql[i + 1] === '*') {
      let level = 0
      while (i < n) {
        if (sql[i] === '/' && sql[i + 1] === '*') { level++; i += 2; continue }
        if (sql[i] === '*' && sql[i + 1] === '/') { level--; i += 2; if (!level) break; continue }
        i++
      }
      continue
    }
    const start = i
    if (c === "'" || ((c === 'E' || c === 'e') && sql[i + 1] === "'")) {
      const escapes = c !== "'"
      i += escapes ? 2 : 1
      while (i < n) {
        if (escapes && sql[i] === '\\') { i += 2; continue }
        if (sql[i] === "'") {
          if (sql[i + 1] === "'") { i += 2; continue }
          i++
          break
        }
        i++
      }
      push('string', sql.slice(start, i), start, i)
      continue
    }
    if (c === '"' || c === '`') {
      let name = ''
      i++
      while (i < n) {
        if (sql[i] === c) {
          if (sql[i + 1] === c) { name += c; i += 2; continue }
          i++
          break
        }
        name += sql[i++]
      }
      push('ident', name, start, i)
      continue
    }
    if (c === '$') {
      const tag = /^\$([A-Za-z_][\w]*)?\$/.exec(sql.slice(i))
      if (tag) {
        const close = sql.indexOf(tag[0], i + tag[0].length)
        i = close === -1 ? n : close + tag[0].length
        push('string', sql.slice(start, i), start, i)
        continue
      }
      const param = /^\$\d+/.exec(sql.slice(i))
      if (param) { i += param[0].length; push('param', param[0], start, i); continue }
    }
    if (/[A-Za-z_\u0080-\uffff]/.test(c)) {
      while (i < n && /[\w$\u0080-\uffff]/.test(sql[i])) i++
      push('word', sql.slice(start, i), start, i)
      continue
    }
    if (/\d/.test(c) || (c === '.' && /\d/.test(sql[i + 1] ?? ''))) {
      while (i < n && /[\d.eE]/.test(sql[i])) i++
      push('num', sql.slice(start, i), start, i)
      continue
    }
    if (c === ')') depth = Math.max(0, depth - 1)
    push('punct', c, start, i + 1)
    if (c === '(') depth++
    i++
  }
  return out
}

// ── Parsing the write ────────────────────────────────────────────────────────

/**
 * @typedef {{ name: string, quoted: boolean }} NameRef
 * @typedef {{
 *   kind: 'update' | 'delete' | 'insert',
 *   body: string,
 *   table: { text: string, parts: NameRef[] },
 *   only: boolean,
 *   alias: string,
 *   rowRef: string,
 *   setColumns: NameRef[],
 *   tail: string,
 *   insertRows: number,
 * }} WritePlan
 * `body` is the statement without its `;`. `rowRef` is how the statement's
 * columns are qualified (its alias, else the table name as written). `tail` is
 * what picks the rows: WHERE, plus ORDER BY / LIMIT where the engine has them.
 */

const isIdent = (/** @type {Tok | undefined} */ t) => !!t && (t.t === 'ident' || t.t === 'word')

/**
 * A table name at `i`: one to three dot-separated parts.
 * @param {Tok[]} toks @param {number} i @param {string} sql
 * @returns {{ text: string, parts: NameRef[], end: number, last: string } | null}
 */
function readName(toks, i, sql) {
  if (!isIdent(toks[i])) return null
  const first = toks[i]
  /** @type {NameRef[]} */
  const parts = []
  let j = i
  for (;;) {
    const t = toks[j]
    parts.push({ name: t.text, quoted: t.t === 'ident' })
    if (toks[j + 1]?.text === '.' && isIdent(toks[j + 2]) && parts.length < 3) { j += 2; continue }
    break
  }
  return { text: sql.slice(first.start, toks[j].end), parts, end: j + 1, last: sql.slice(toks[j].start, toks[j].end) }
}

/** Keywords that end a table reference's alias position. */
const AFTER_TABLE = new Set(['SET', 'WHERE', 'RETURNING', 'USING', 'ORDER', 'LIMIT', 'VALUES', 'DEFAULT', 'SELECT', 'WITH', 'ON', 'OVERRIDING', 'FROM'])

/**
 * Optional `[AS] alias` at `i`.
 * @param {Tok[]} toks @param {number} i @param {string} sql
 * @returns {{ alias: string, end: number }}
 */
function readAlias(toks, i, sql) {
  if (toks[i]?.up === 'AS' && isIdent(toks[i + 1])) return { alias: sql.slice(toks[i + 1].start, toks[i + 1].end), end: i + 2 }
  const t = toks[i]
  if (t && t.depth === 0 && (t.t === 'ident' || (t.t === 'word' && !AFTER_TABLE.has(t.up)))) {
    return { alias: sql.slice(t.start, t.end), end: i + 1 }
  }
  return { alias: '', end: i }
}

/** @param {Tok[]} toks @param {number} from @param {Set<string>} words */
function findTop(toks, from, words) {
  for (let i = from; i < toks.length; i++) if (toks[i].depth === 0 && toks[i].t === 'word' && words.has(toks[i].up)) return i
  return -1
}

/**
 * The columns a SET list assigns, `a = …, (b, c) = …` style.
 * @param {Tok[]} toks @param {number} from @param {number} to exclusive
 * @returns {NameRef[] | null}
 */
function setColumnsOf(toks, from, to) {
  /** @type {NameRef[]} */
  const cols = []
  let expectTarget = true
  for (let i = from; i < to; i++) {
    const t = toks[i]
    if (t.depth === 0 && t.text === ',') { expectTarget = true; continue }
    if (!expectTarget) continue
    if (t.text === '(') {
      for (i++; i < to && toks[i].text !== ')'; i++) {
        if (isIdent(toks[i])) cols.push({ name: toks[i].text, quoted: toks[i].t === 'ident' })
      }
      expectTarget = false
      continue
    }
    if (!isIdent(t)) return null
    // `col`, or `t.col` on MySQL: the last part names the column.
    let k = i
    while (toks[k + 1]?.text === '.' && isIdent(toks[k + 2])) k += 2
    cols.push({ name: toks[k].text, quoted: toks[k].t === 'ident' })
    i = k
    expectTarget = false
  }
  return cols.length ? cols : null
}

/**
 * The write in `sql`, when it is one this module can capture: a single
 * UPDATE / DELETE / INSERT … VALUES on one table. Joins (UPDATE … FROM,
 * DELETE … USING), CTEs, INSERT … SELECT, upserts and statements that already
 * RETURN something are run as they are, without an undo copy.
 * @param {string} sql
 * @returns {WritePlan | DdlPlan | null}
 */
export function parseWrite(sql) {
  const stmts = splitSqlStatements(String(sql ?? ''))
  if (stmts.length !== 1) return null
  const body = stmts[0].text.trim().replace(/;\s*$/, '').trim()
  const toks = lex(body)
  if (!toks.length) return null
  const head = toks[0].up
  if (head === 'UPDATE') return parseUpdate(body, toks)
  if (head === 'DELETE') return parseDelete(body, toks)
  if (head === 'INSERT') return parseInsert(body, toks)
  if (head === 'CREATE') return parseCreate(body, toks)
  if (head === 'ALTER') return parseAlter(body, toks)
  return null
}

/** @param {string} body @param {Tok[]} toks @returns {WritePlan | null} */
function parseUpdate(body, toks) {
  let i = 1
  if (toks[i]?.up === 'OR') i += 2 // SQLite: UPDATE OR IGNORE
  const only = toks[i]?.up === 'ONLY'
  if (only) i++
  const name = readName(toks, i, body)
  if (!name) return null
  i = name.end
  if (toks[i]?.text === '*') i++
  const { alias, end } = readAlias(toks, i, body)
  i = end
  if (toks[i]?.up !== 'SET') return null
  const stop = findTop(toks, i + 1, new Set(['FROM', 'WHERE', 'RETURNING', 'ORDER', 'LIMIT']))
  if (stop !== -1 && toks[stop].up === 'FROM') return null
  const setColumns = setColumnsOf(toks, i + 1, stop === -1 ? toks.length : stop)
  if (!setColumns) return null
  const tail = tailOf(body, toks, stop)
  if (tail === null) return null
  return { kind: 'update', body, table: { text: name.text, parts: name.parts }, only, alias, rowRef: alias || name.last, setColumns, tail, insertRows: 0 }
}

/** @param {string} body @param {Tok[]} toks @returns {WritePlan | null} */
function parseDelete(body, toks) {
  if (toks[1]?.up !== 'FROM') return null
  let i = 2
  const only = toks[i]?.up === 'ONLY'
  if (only) i++
  const name = readName(toks, i, body)
  if (!name) return null
  i = name.end
  if (toks[i]?.text === '*') i++
  const { alias, end } = readAlias(toks, i, body)
  i = end
  if (i < toks.length && !['WHERE', 'RETURNING', 'ORDER', 'LIMIT'].includes(toks[i].up)) return null
  const tail = tailOf(body, toks, i < toks.length ? i : -1)
  if (tail === null) return null
  return { kind: 'delete', body, table: { text: name.text, parts: name.parts }, only, alias, rowRef: alias || name.last, setColumns: [], tail, insertRows: 0 }
}

/** @param {string} body @param {Tok[]} toks @returns {WritePlan | null} */
function parseInsert(body, toks) {
  let i = 1
  if (toks[i]?.up === 'OR') {
    // INSERT OR REPLACE deletes the rows it replaces: nothing to put back.
    if (toks[i + 1]?.up !== 'IGNORE' && toks[i + 1]?.up !== 'ABORT' && toks[i + 1]?.up !== 'FAIL') return null
    i += 2
  }
  if (toks[i]?.up !== 'INTO') return null
  const name = readName(toks, i + 1, body)
  if (!name) return null
  i = name.end
  const { alias, end } = readAlias(toks, i, body)
  i = end
  if (toks[i]?.text === '(') {
    while (i < toks.length && !(toks[i].text === ')' && toks[i].depth === 0)) i++
    i++
  }
  if (toks[i]?.up === 'OVERRIDING') i += 3
  let rows = 0
  if (toks[i]?.up === 'DEFAULT' && toks[i + 1]?.up === 'VALUES') {
    rows = 1
    i += 2
  } else if (toks[i]?.up === 'VALUES') {
    for (i++; i < toks.length; i++) {
      const t = toks[i]
      if (t.depth !== 0) continue
      if (t.text === '(') { rows++; continue }
      if (t.text === ',' || t.text === ')') continue
      break
    }
  } else {
    return null
  }
  // What follows the rows: nothing, or ON CONFLICT … DO NOTHING.
  if (i < toks.length) {
    if (toks[i].up !== 'ON' || toks[i + 1]?.up !== 'CONFLICT') return null
    const doAt = findTop(toks, i + 2, new Set(['DO']))
    if (doAt === -1 || toks[doAt + 1]?.up !== 'NOTHING' || doAt + 2 !== toks.length) return null
  }
  if (!rows) return null
  return { kind: 'insert', body, table: { text: name.text, parts: name.parts }, only: false, alias, rowRef: alias || name.last, setColumns: [], tail: '', insertRows: rows }
}

/**
 * WHERE / ORDER BY / LIMIT up to RETURNING, or null when the statement returns
 * rows of its own (the capture adds its own RETURNING, and only one fits) or
 * updates through a cursor.
 * @param {string} body @param {Tok[]} toks @param {number} from index of the first tail token, -1 when none
 */
function tailOf(body, toks, from) {
  if (from === -1) return ''
  const ret = findTop(toks, from, new Set(['RETURNING']))
  if (ret !== -1) return null
  for (let k = from; k < toks.length - 1; k++) if (toks[k].up === 'CURRENT' && toks[k + 1].up === 'OF') return null
  return body.slice(toks[from].start).trim()
}

/**
 * @typedef {{
 *   kind: 'ddl',
 *   ddl: 'create' | 'add-column' | 'rename-table' | 'rename-column',
 *   objectType: 'TABLE' | 'VIEW' | 'MATERIALIZED VIEW' | 'SEQUENCE' | 'INDEX' | 'SCHEMA',
 *   body: string,
 *   name: { text: string, parts: NameRef[] },
 *   table: { text: string, parts: NameRef[] } | null,
 *   column: NameRef | null,
 *   to: NameRef | null,
 * }} DdlPlan
 * `name` is what a CREATE makes; `table` is an index's table or the table an
 * ALTER changes; `column` is the column added or renamed, `to` its (or the
 * table's) new name.
 */

const CREATABLE = new Set(['TABLE', 'VIEW', 'SEQUENCE', 'INDEX', 'SCHEMA'])

/** @param {string} body @param {Tok[]} toks @returns {DdlPlan | null} */
function parseCreate(body, toks) {
  let i = 1
  // OR REPLACE: the definition it replaced is gone. TEMP: it lives on one
  // pooled connection, and the revert would run on another.
  if (toks[i]?.up === 'OR') return null
  if (['TEMP', 'TEMPORARY', 'GLOBAL', 'LOCAL'].includes(toks[i]?.up)) return null
  if (toks[i]?.up === 'UNLOGGED') i++
  const unique = toks[i]?.up === 'UNIQUE'
  if (unique) i++
  /** @type {DdlPlan['objectType'] | ''} */
  let type = ''
  if (toks[i]?.up === 'MATERIALIZED' && toks[i + 1]?.up === 'VIEW') { type = 'MATERIALIZED VIEW'; i += 2 }
  else if (CREATABLE.has(toks[i]?.up)) { type = /** @type {DdlPlan['objectType']} */ (toks[i].up); i++ }
  if (!type || (unique && type !== 'INDEX')) return null
  // CONCURRENTLY can't run inside the transaction the capture uses.
  if (type === 'INDEX' && toks[i]?.up === 'CONCURRENTLY') return null
  if (toks[i]?.up === 'IF' && toks[i + 1]?.up === 'NOT' && toks[i + 2]?.up === 'EXISTS') i += 3
  // An unnamed index gets a generated name; CREATE SCHEMA AUTHORIZATION takes the role's.
  if (toks[i]?.up === 'ON' || toks[i]?.up === 'AUTHORIZATION') return null
  const name = readName(toks, i, body)
  if (!name) return null
  /** @type {DdlPlan['table']} */
  let table = null
  if (type === 'INDEX') {
    let j = name.end
    if (toks[j]?.up !== 'ON') return null
    j++
    if (toks[j]?.up === 'ONLY') j++
    const t = readName(toks, j, body)
    if (!t) return null
    table = { text: t.text, parts: t.parts }
  }
  return { kind: 'ddl', ddl: 'create', objectType: type, body, name: { text: name.text, parts: name.parts }, table, column: null, to: null }
}

/** @param {Tok | undefined} t @returns {NameRef | null} */
const refOf = (t) => (isIdent(t) ? { name: /** @type {Tok} */ (t).text, quoted: /** @type {Tok} */ (t).t === 'ident' } : null)

/** @param {string} body @param {Tok[]} toks @returns {DdlPlan | null} */
function parseAlter(body, toks) {
  if (toks[1]?.up !== 'TABLE') return null
  let i = 2
  if (toks[i]?.up === 'IF' && toks[i + 1]?.up === 'EXISTS') i += 2
  if (toks[i]?.up === 'ONLY') i++
  const table = readName(toks, i, body)
  if (!table) return null
  i = table.end
  if (toks[i]?.text === '*') i++
  // One action: `ADD a int, ADD b int` would need one revert per action.
  for (let k = i; k < toks.length; k++) if (toks[k].depth === 0 && toks[k].text === ',') return null
  const base = { kind: /** @type {const} */ ('ddl'), objectType: /** @type {const} */ ('TABLE'), body, name: { text: table.text, parts: table.parts }, table: { text: table.text, parts: table.parts } }
  if (toks[i]?.up === 'ADD') {
    i++
    const saidColumn = toks[i]?.up === 'COLUMN'
    if (saidColumn) i++
    if (toks[i]?.up === 'IF' && toks[i + 1]?.up === 'NOT' && toks[i + 2]?.up === 'EXISTS') i += 3
    // ADD CONSTRAINT / PRIMARY KEY / UNIQUE / FOREIGN KEY / CHECK: not a column.
    if (!saidColumn && ['CONSTRAINT', 'PRIMARY', 'UNIQUE', 'FOREIGN', 'CHECK', 'EXCLUDE'].includes(toks[i]?.up)) return null
    const column = refOf(toks[i])
    return column ? { ...base, ddl: 'add-column', column, to: null } : null
  }
  if (toks[i]?.up === 'RENAME') {
    i++
    if (toks[i]?.up === 'TO') {
      const to = refOf(toks[i + 1])
      return to && i + 2 === toks.length ? { ...base, ddl: 'rename-table', column: null, to } : null
    }
    if (toks[i]?.up === 'CONSTRAINT') return null
    if (toks[i]?.up === 'COLUMN') i++
    const column = refOf(toks[i])
    const to = refOf(toks[i + 2])
    return column && to && toks[i + 1]?.up === 'TO' && i + 3 === toks.length ? { ...base, ddl: 'rename-column', column, to } : null
  }
  return null
}

// ── Dialects ─────────────────────────────────────────────────────────────────

/**
 * @typedef {{
 *   name: string,
 *   type: string,
 *   pk: boolean,
 *   generated: boolean,
 *   identityAlways: boolean,
 *   dataType?: string,
 * }} ColumnMeta
 * `dataType` is MySQL's bare type (`varchar`, `blob`), which decides how a
 * value is captured.
 * @typedef {{
 *   qualified: string,
 *   columns: ColumnMeta[],
 *   pk: ColumnMeta[],
 *   triggers: boolean,
 *   cascades: boolean,
 * }} TableMeta
 */

/** @param {string} name */
const dq = (name) => `"${String(name).replace(/"/g, '""')}"`
/** @param {string} name */
const bq = (name) => `\`${String(name).replace(/`/g, '``')}\``
/** An identifier quoted for the dialect: MySQL reads "x" as a string. @param {string} name @param {'postgres' | 'sqlite' | 'mysql'} dialect */
const qi = (name, dialect) => (dialect === 'mysql' ? bq(name) : dq(name))

/**
 * MySQL text the database can't misread whatever its sql_mode: the UTF-8 bytes
 * in hex, as a utf8mb4 string. QUOTE() would be shorter, but it escapes with
 * backslashes, which NO_BACKSLASH_ESCAPES turns into literal characters.
 * @param {string} s
 */
function mysqlText(s) {
  const hex = [...new TextEncoder().encode(s)].map((b) => b.toString(16).padStart(2, '0')).join('')
  return `CONVERT(X'${hex}' USING utf8mb4)`
}

/**
 * A string literal for SQL text this module wrote itself (a table name as the
 * statement spelled it). Dollar-quoted on Postgres, so no setting changes how
 * it reads.
 * @param {string} s @param {'postgres' | 'sqlite' | 'mysql'} dialect
 */
function textLiteral(s, dialect) {
  if (dialect === 'mysql') return mysqlText(s)
  if (dialect === 'sqlite') return `'${s.replace(/'/g, "''")}'`
  let tag = '$q$'
  for (let k = 0; s.includes(tag); k++) tag = `$q${k}$`
  return `${tag}${s}${tag}`
}

/**
 * The metadata column a statement's name refers to: exact when quoted, folded
 * otherwise (Postgres lowercases, SQLite ignores case).
 * @param {ColumnMeta[]} cols @param {NameRef} ref @param {'postgres' | 'sqlite' | 'mysql'} dialect
 */
function findColumn(cols, ref, dialect) {
  if (ref.quoted) return cols.find((c) => c.name === ref.name) ?? null
  if (dialect === 'postgres') return cols.find((c) => c.name === ref.name.toLowerCase()) ?? null
  // SQLite and MySQL column names ignore case.
  return cols.find((c) => c.name.toLowerCase() === ref.name.toLowerCase()) ?? null
}

/** @param {WritePlan} plan */
const fromClause = (plan) => `${plan.only ? 'ONLY ' : ''}${plan.table.text}${plan.alias ? ` AS ${plan.alias}` : ''}`

/**
 * Catalog query for the table the write names, resolved exactly as the
 * statement resolves it (search_path, quoting).
 * @param {WritePlan} plan
 */
export function pgMetaSql(plan) {
  return `SELECT a.attname::text, format_type(a.atttypid, a.atttypmod), (a.attgenerated <> '')::text,
  (a.attidentity = 'a')::text, COALESCE(a.attnum = ANY (i.indkey), false)::text,
  EXISTS (SELECT 1 FROM pg_trigger t WHERE t.tgrelid = c.oid AND NOT t.tgisinternal)::text,
  EXISTS (SELECT 1 FROM pg_constraint f WHERE f.confrelid = c.oid AND f.contype = 'f' AND f.confdeltype IN ('c', 'n', 'd'))::text,
  c.relkind::text, format('%I.%I', n.nspname, c.relname)
FROM pg_class c
JOIN pg_namespace n ON n.oid = c.relnamespace
JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum > 0 AND NOT a.attisdropped
LEFT JOIN pg_index i ON i.indrelid = c.oid AND i.indisprimary
WHERE c.oid = to_regclass(${textLiteral(plan.table.text, 'postgres')})
ORDER BY a.attnum`
}

/**
 * @param {unknown[][]} rows pgMetaSql's
 * @returns {TableMeta | string} the table, or why it can't be reverted
 */
export function pgReadMeta(rows) {
  if (!rows.length) return ''
  const kind = String(rows[0][7])
  if (kind !== 'r' && kind !== 'p') return 'only tables can be reverted'
  /** @type {ColumnMeta[]} */
  const columns = rows.map((r) => ({
    name: String(r[0]),
    type: String(r[1]),
    generated: r[2] === 'true',
    identityAlways: r[3] === 'true',
    pk: r[4] === 'true',
  }))
  return {
    qualified: String(rows[0][8]),
    columns,
    pk: columns.filter((c) => c.pk),
    triggers: rows[0][5] === 'true',
    cascades: rows[0][6] === 'true',
  }
}

/**
 * The SQLite equivalents: a table's columns (generated ones are hidden 2 or 3,
 * the key is pk > 0), whether it is a table at all, and its triggers.
 * @param {WritePlan} plan
 */
export function sqliteMetaSqls(plan) {
  const parts = plan.table.parts
  const table = parts[parts.length - 1].name
  const schema = parts.length > 1 ? dq(parts[parts.length - 2].name) : 'main'
  const lit = textLiteral(table, 'sqlite')
  return [
    `SELECT name, type, pk, hidden FROM pragma_table_xinfo(${lit}, ${textLiteral(schema.replace(/^"|"$/g, ''), 'sqlite')}) ORDER BY cid`,
    `SELECT type, (SELECT count(*) FROM ${schema}.sqlite_master WHERE type = 'trigger' AND tbl_name = ${lit} COLLATE NOCASE) FROM ${schema}.sqlite_master WHERE name = ${lit} COLLATE NOCASE AND type IN ('table', 'view')`,
  ]
}

/**
 * @param {unknown[][]} columnRows @param {unknown[][]} tableRows
 * @param {WritePlan} plan
 * @returns {TableMeta | string}
 */
export function sqliteReadMeta(columnRows, tableRows, plan) {
  if (!tableRows.length || !columnRows.length) return ''
  if (String(tableRows[0][0]) !== 'table') return 'only tables can be reverted'
  /** @type {ColumnMeta[]} */
  const columns = columnRows.map((r) => ({
    name: String(r[0]),
    type: String(r[1] ?? ''),
    pk: Number(r[2]) > 0,
    generated: Number(r[3]) === 2 || Number(r[3]) === 3,
    identityAlways: false,
  }))
  /** @type {ColumnMeta[]} */
  let pk = columnRows
    .filter((r) => Number(r[2]) > 0)
    .sort((a, b) => Number(a[2]) - Number(b[2]))
    .map((r) => /** @type {ColumnMeta} */ (columns.find((c) => c.name === String(r[0]))))
  // No declared key: the rowid addresses the row (a WITHOUT ROWID table always
  // declares one).
  if (!pk.length) pk = [{ name: 'rowid', type: 'integer', pk: true, generated: false, identityAlways: false }]
  return { qualified: plan.table.text, columns, pk, triggers: Number(tableRows[0][1]) > 0, cascades: false }
}

/** The schema a MySQL name lives in: its own qualifier, else the connection's database. @param {{ parts: NameRef[] }} name */
function mysqlSchema(name) {
  return name.parts.length > 1 ? mysqlText(name.parts[name.parts.length - 2].name) : 'DATABASE()'
}

/**
 * MySQL's catalog for the table: columns with their bare type and key, plus
 * the table's kind and engine (only InnoDB rolls back), its triggers and the
 * ON DELETE rules that point at it.
 * @param {WritePlan} plan
 */
export function mysqlMetaSql(plan) {
  const name = mysqlText(plan.table.parts[plan.table.parts.length - 1].name)
  return `SELECT c.COLUMN_NAME, c.COLUMN_TYPE, LOWER(c.DATA_TYPE), c.COLUMN_KEY, c.EXTRA, t.TABLE_TYPE, t.ENGINE,
  (SELECT COUNT(*) FROM information_schema.TRIGGERS g WHERE g.EVENT_OBJECT_SCHEMA = t.TABLE_SCHEMA AND g.EVENT_OBJECT_TABLE = t.TABLE_NAME),
  (SELECT COUNT(*) FROM information_schema.REFERENTIAL_CONSTRAINTS r WHERE r.UNIQUE_CONSTRAINT_SCHEMA = t.TABLE_SCHEMA AND r.REFERENCED_TABLE_NAME = t.TABLE_NAME AND r.DELETE_RULE IN ('CASCADE', 'SET NULL', 'SET DEFAULT')),
  CONCAT('\`', REPLACE(t.TABLE_SCHEMA, '\`', '\`\`'), '\`.\`', REPLACE(t.TABLE_NAME, '\`', '\`\`'), '\`')
FROM information_schema.TABLES t
JOIN information_schema.COLUMNS c ON c.TABLE_SCHEMA = t.TABLE_SCHEMA AND c.TABLE_NAME = t.TABLE_NAME
WHERE t.TABLE_SCHEMA = ${mysqlSchema(plan.table)} AND t.TABLE_NAME = ${name}
ORDER BY c.ORDINAL_POSITION`
}

/** Types a MySQL value can't be restored from text for (no exact literal). */
const MYSQL_SPATIAL = new Set(['geometry', 'point', 'linestring', 'polygon', 'multipoint', 'multilinestring', 'multipolygon', 'geometrycollection', 'geomcollection'])

/**
 * @param {unknown[][]} rows mysqlMetaSql's
 * @returns {TableMeta | string}
 */
export function mysqlReadMeta(rows) {
  if (!rows.length) return ''
  if (String(rows[0][5]) !== 'BASE TABLE') return 'only tables can be reverted'
  if (String(rows[0][6]).toLowerCase() !== 'innodb') return `only InnoDB tables can be reverted, and this one is ${rows[0][6]}`
  /** @type {ColumnMeta[]} */
  const columns = rows.map((r) => ({
    name: String(r[0]),
    type: String(r[1]),
    dataType: String(r[2]),
    pk: String(r[3]) === 'PRI',
    generated: /GENERATED/i.test(String(r[4] ?? '')),
    identityAlways: false,
  }))
  return {
    qualified: String(rows[0][9]),
    columns,
    pk: columns.filter((c) => c.pk),
    triggers: Number(rows[0][7]) > 0,
    cascades: Number(rows[0][8]) > 0,
  }
}

const MYSQL_NUMERIC = new Set(['tinyint', 'smallint', 'mediumint', 'int', 'integer', 'bigint', 'decimal', 'numeric', 'float', 'double', 'real', 'year'])
const MYSQL_BINARY = new Set(['binary', 'varbinary', 'tinyblob', 'blob', 'mediumblob', 'longblob'])

/**
 * The SQL that reads a column as a literal of itself.
 * @param {ColumnMeta} c @param {string} ref how the query names the column
 * @param {'postgres' | 'sqlite' | 'mysql'} dialect
 */
function literalOf(c, ref, dialect) {
  if (dialect === 'postgres') return `quote_nullable(${ref})`
  if (dialect === 'sqlite') return `quote(${ref})`
  const t = c.dataType ?? ''
  if (MYSQL_NUMERIC.has(t)) return `IF(${ref} IS NULL, 'NULL', CAST(${ref} AS CHAR))`
  if (t === 'bit') return `IF(${ref} IS NULL, 'NULL', CAST(${ref} + 0 AS CHAR))`
  if (MYSQL_BINARY.has(t)) return `IF(${ref} IS NULL, 'NULL', CONCAT('X''', HEX(${ref}), ''''))`
  return `IF(${ref} IS NULL, 'NULL', CONCAT('CONVERT(X''', HEX(CAST(${ref} AS CHAR)), ''' USING utf8mb4)'))`
}


/**
 * How many rows the write will touch, and (Postgres) how much the captured
 * columns weigh, before anything is read.
 * @param {WritePlan} plan @param {TableMeta} meta @param {ColumnMeta[]} captured @param {'postgres' | 'sqlite' | 'mysql'} dialect
 */
export function sizeSql(plan, meta, captured, dialect) {
  if (dialect === 'sqlite') return `SELECT count(*) FROM (SELECT 1 FROM ${fromClause(plan)} ${plan.tail})`
  if (dialect === 'mysql') {
    // A derived table, so an UPDATE's ORDER BY … LIMIT counts what it touches.
    const len = captured.map((c) => `COALESCE(LENGTH(${plan.rowRef}.${bq(c.name)}), 0)`).join(' + ') || '0'
    return `SELECT COUNT(*), COALESCE(SUM(__b), 0) FROM (SELECT ${len} AS __b FROM ${fromClause(plan)} ${plan.tail}) AS __s`
  }
  const bytes = plan.kind === 'delete'
    ? `pg_column_size(${plan.rowRef}.*)`
    : captured.map((c) => `COALESCE(pg_column_size(${plan.rowRef}.${dq(c.name)}), 0)`).join(' + ')
  return `SELECT count(*)::text, COALESCE(sum(${bytes}), 0)::text FROM ${fromClause(plan)} ${plan.tail}`.trim()
}

/**
 * The rows as they are before the write: each captured column as a literal,
 * locked on Postgres so the write changes exactly these.
 * @param {WritePlan} plan @param {ColumnMeta[]} captured @param {'postgres' | 'sqlite' | 'mysql'} dialect
 */
export function captureSql(plan, captured, dialect) {
  const cols = captured.map((c) => literalOf(c, `${plan.rowRef}.${colRef(c, dialect)}`, dialect)).join(', ')
  const lock = dialect === 'sqlite' ? '' : ' FOR UPDATE'
  return `SELECT ${cols} FROM ${fromClause(plan)} ${plan.tail}`.trim() + lock
}

/**
 * The INSERT with the new rows' keys returned as literals. Inside a CTE, so
 * the transaction path (which reads rows from SELECT and WITH) returns them.
 * Postgres only: SQLite has no INSERT inside WITH.
 * @param {WritePlan} plan @param {TableMeta} meta
 */
export function insertReturningSql(plan, meta) {
  const keys = meta.pk.map((c, k) => `quote_nullable(${dq(c.name)}) AS k${k}`).join(', ')
  return `WITH __ins AS (
${plan.body}
RETURNING ${keys}
) SELECT ${meta.pk.map((_, k) => `k${k}`).join(', ')} FROM __ins`
}

/**
 * The updated rows as they are after the write, by key, for the revert's guard.
 * @param {TableMeta} meta @param {ColumnMeta[]} captured @param {string[][]} keys literal tuples
 * @param {'postgres' | 'sqlite' | 'mysql'} dialect
 */
export function afterSql(meta, captured, keys, dialect) {
  const cols = captured.map((c) => literalOf(c, colRef(c, dialect), dialect)).join(', ')
  return `SELECT ${cols} FROM ${meta.qualified} WHERE ${keyIn(meta.pk, keys, dialect)}`
}

/** @param {ColumnMeta} c @param {'postgres' | 'sqlite' | 'mysql'} dialect */
const colRef = (c, dialect) => (c.name === 'rowid' && dialect === 'sqlite' ? 'rowid' : qi(c.name, dialect))

/**
 * `key IN (…)` for literal key tuples.
 * @param {ColumnMeta[]} pk @param {string[][]} keys @param {'postgres' | 'sqlite' | 'mysql'} dialect
 */
function keyIn(pk, keys, dialect) {
  if (pk.length === 1) return `${colRef(pk[0], dialect)} IN (${keys.map((k) => k[0]).join(', ')})`
  const cols = `(${pk.map((c) => colRef(c, dialect)).join(', ')})`
  const tuples = keys.map((k) => `(${k.join(', ')})`).join(', ')
  return dialect === 'sqlite' ? `${cols} IN (VALUES ${tuples})` : `${cols} IN (${tuples})`
}

/** @param {unknown[]} a @param {number} size */
function chunks(a, size) {
  const out = []
  for (let i = 0; i < a.length; i += size) out.push(a.slice(i, i + size))
  return out
}

/**
 * Revert of an UPDATE: put back the old values of the columns it set, only in
 * rows that still hold what it wrote.
 * @param {TableMeta} meta @param {ColumnMeta[]} setCols
 * @param {{ key: string[], before: string[], after: string[] }[]} rows literals
 * @param {'postgres' | 'sqlite' | 'mysql'} dialect
 * @returns {string[]}
 */
export function revertUpdateSql(meta, setCols, rows, dialect) {
  if (dialect === 'mysql') {
    // One UPDATE per row; the guard compares each value's bytes as text, so a
    // case-insensitive collation can't call a changed value unchanged.
    return chunks(rows, CHUNK).map((part) => part.map((r) => {
      const set = setCols.map((c, k) => `${bq(c.name)} = ${r.before[k]}`).join(', ')
      const where = [
        ...meta.pk.map((c, k) => `${bq(c.name)} = ${r.key[k]}`),
        ...setCols.map((c, k) => (c.dataType === 'bit'
          // A BIT reads back as raw bytes; its literal is the number.
          ? `${bq(c.name)} + 0 <=> ${r.after[k]}`
          : `HEX(CAST(${bq(c.name)} AS CHAR)) <=> HEX(CAST(${r.after[k]} AS CHAR))`)),
      ].join(' AND ')
      return `UPDATE ${meta.qualified} SET ${set} WHERE ${where};`
    }).join('\n'))
  }
  if (dialect === 'sqlite') {
    return chunks(rows, CHUNK).map((part) => part.map((r) => {
      const set = setCols.map((c, k) => `${dq(c.name)} = ${r.before[k]}`).join(', ')
      const where = [
        ...meta.pk.map((c, k) => `${colRef(c, dialect)} = ${r.key[k]}`),
        ...setCols.map((c, k) => `${dq(c.name)} IS ${r.after[k]}`),
      ].join(' AND ')
      return `UPDATE ${meta.qualified} SET ${set} WHERE ${where};`
    }).join('\n'))
  }
  // One statement per chunk: the old values ride in a VALUES list (text) and
  // are cast to each column's type; the guard compares text forms, which works
  // for every type, json included (it has no = operator).
  const keyNames = meta.pk.map((_, k) => `k${k}`)
  const oldNames = setCols.map((_, k) => `o${k}`)
  const newNames = setCols.map((_, k) => `a${k}`)
  return chunks(rows, CHUNK).map((part) => {
    const values = part.map((r) => `(${[...r.key, ...r.before, ...r.after].join(', ')})`).join(',\n  ')
    const set = setCols.map((c, k) => `${dq(c.name)} = __v.${oldNames[k]}::${c.type}`).join(', ')
    const where = [
      ...meta.pk.map((c, k) => `__t.${dq(c.name)} = __v.${keyNames[k]}::${c.type}`),
      ...setCols.map((c, k) => `__t.${dq(c.name)}::text IS NOT DISTINCT FROM __v.${newNames[k]}`),
    ].join('\n  AND ')
    return `UPDATE ${meta.qualified} AS __t SET ${set}\nFROM (VALUES\n  ${values}\n) AS __v(${[...keyNames, ...oldNames, ...newNames].join(', ')})\nWHERE ${where};`
  })
}

/**
 * Revert of a DELETE: insert the rows back, skipping keys that are taken again.
 * @param {TableMeta} meta @param {ColumnMeta[]} cols @param {string[][]} rows literals
 * @param {'postgres' | 'sqlite' | 'mysql'} dialect
 * @returns {string[]}
 */
export function revertDeleteSql(meta, cols, rows, dialect) {
  const names = cols.map((c) => colRef(c, dialect)).join(', ')
  const overriding = dialect === 'postgres' && cols.some((c) => c.identityAlways) ? ' OVERRIDING SYSTEM VALUE' : ''
  return chunks(rows, CHUNK).map((part) => {
    const values = part.map((r) => `(${r.join(', ')})`).join(',\n  ')
    if (dialect === 'mysql') {
      // A taken key updates nothing (and reports nothing): MySQL's DO NOTHING.
      // Not INSERT IGNORE, which also turns bad values into defaults.
      const noop = bq((meta.pk[0] ?? cols[0]).name)
      return `INSERT INTO ${meta.qualified} (${names}) VALUES\n  ${values}\nON DUPLICATE KEY UPDATE ${noop} = ${noop};`
    }
    return dialect === 'sqlite'
      ? `INSERT OR IGNORE INTO ${meta.qualified} (${names}) VALUES\n  ${values};`
      : `INSERT INTO ${meta.qualified} (${names})${overriding} VALUES\n  ${values}\nON CONFLICT DO NOTHING;`
  })
}

/**
 * Revert of an INSERT: delete the rows it added, by key.
 * @param {TableMeta} meta @param {string[][]} keys literals
 * @param {'postgres' | 'sqlite' | 'mysql'} dialect
 * @returns {string[]}
 */
export function revertInsertSql(meta, keys, dialect) {
  return chunks(keys, CHUNK).map((part) => `DELETE FROM ${meta.qualified} WHERE ${keyIn(meta.pk, /** @type {string[][]} */ (part), dialect)};`)
}

// ── Running a write with an undo copy ────────────────────────────────────────

/**
 * @typedef {{
 *   columns?: unknown[],
 *   rows?: unknown[][],
 *   rowCount?: number | null,
 *   message?: string | null,
 *   queryMs?: number,
 *   sql?: string,
 * }} SqlResult
 * @typedef {{
 *   inspect: (sql: string) => Promise<SqlResult>,
 *   begin: () => Promise<unknown>,
 *   exec: (sql: string) => Promise<SqlResult>,
 *   run: (sql: string) => Promise<SqlResult>,
 *   commit: () => Promise<unknown>,
 *   rollback: () => Promise<unknown>,
 * }} UndoIo
 * `inspect` reads outside any transaction (and can be stopped); `exec` reads
 * inside it; `run` is the user's statement, inside it.
 * @typedef {{
 *   id: string,
 *   dialect: 'postgres' | 'sqlite' | 'mysql',
 *   kind: 'update' | 'delete' | 'insert' | 'ddl',
 *   table: string,
 *   rows: number,
 *   columns: string[],
 *   statements: string[],
 *   warnings: string[],
 *   sql: string,
 *   at: number,
 *   connection?: string,
 *   words?: { title: string, body: string, note: string, done: string, action: string, destructive: boolean },
 *   precheck?: { sql: string, says: string },
 * }} UndoRecord
 * A schema change carries its own `words` (the dialog's and the toast's) and
 * may carry a `precheck`: a count read when the dialog opens, said as `says`
 * with `{rows}` in it (how many rows a dropped table still holds).
 * @typedef {{ result: SqlResult, undo: UndoRecord | null, note: string } | { fallback: true, note: string }} UndoOutcome
 * `fallback`: run the statement the ordinary way; `note` says why there is no
 * undo copy, when there is a reason worth telling ('' otherwise).
 */

/** @param {string} dialect @returns {'postgres' | 'sqlite' | 'mysql' | null} */
export function undoDialect(dialect) {
  if (dialect === 'postgres') return 'postgres'
  if (dialect === 'sqlite') return 'sqlite'
  if (dialect === 'mysql' || dialect === 'mariadb') return 'mysql'
  return null
}

let seq = 0

/** The name a statement means: Postgres folds unquoted names to lowercase. @param {NameRef} ref @param {'postgres' | 'sqlite' | 'mysql'} dialect */
const folded = (ref, dialect) => (ref.quoted || dialect !== 'postgres' ? ref.name : ref.name.toLowerCase())

const DROP_NOUN = /** @type {const} */ ({ TABLE: 'table', VIEW: 'view', 'MATERIALIZED VIEW': 'materialized view', SEQUENCE: 'sequence', INDEX: 'index', SCHEMA: 'schema' })

/**
 * The first value of the first row, as text.
 * @param {SqlResult} r
 */
const cell = (r) => {
  const v = r.rows?.[0]?.[0]
  return v === null || v === undefined ? null : String(v)
}

/**
 * A schema change with its revert: what it made is dropped, what it renamed is
 * renamed back. Checked and run in one transaction (DDL is transactional on
 * Postgres and SQLite), so the object it reports is the one it made.
 * @param {DdlPlan} plan @param {'postgres' | 'sqlite' | 'mysql'} dialect @param {UndoIo} io
 * @returns {Promise<UndoOutcome>}
 */
async function runDdlWithUndo(plan, dialect, io) {
  if (dialect !== 'postgres' && (plan.objectType === 'MATERIALIZED VIEW' || plan.objectType === 'SEQUENCE' || plan.objectType === 'SCHEMA')) {
    return { fallback: true, note: '' }
  }
  try {
    await io.begin()
  } catch {
    return { fallback: true, note: '' }
  }
  /** @type {SqlResult} */
  let result
  /** @type {{ revert: string, words: NonNullable<UndoRecord['words']>, table: string, precheck?: UndoRecord['precheck'] } | null} */
  let made = null
  let note = ''
  try {
    if (dialect === 'postgres') await io.exec("SET LOCAL lock_timeout = '5s'")
    const step = dialect === 'postgres' ? pgDdlSteps(plan) : dialect === 'mysql' ? mysqlDdlSteps(plan) : sqliteDdlSteps(plan)
    const before = step.before ? await io.exec(step.before) : null
    if (step.existed(before)) {
      // IF NOT EXISTS met an existing object: the statement makes nothing.
      result = await io.run(plan.body)
      note = 'it already existed'
    } else {
      result = await io.run(plan.body)
      const after = step.after ? await io.exec(step.after(before)) : null
      made = step.made(before, after)
    }
    await io.commit()
  } catch {
    try { await io.rollback() } catch { /* already closed */ }
    return { fallback: true, note: '' }
  }
  return {
    result: { ...result, rowCount: null, message: null },
    undo: made
      ? {
          id: `undo-${Date.now().toString(36)}-${++seq}`,
          dialect,
          kind: 'ddl',
          table: made.table,
          rows: 0,
          columns: [],
          statements: [made.revert],
          warnings: [],
          sql: plan.body,
          at: Date.now(),
          words: made.words,
          precheck: made.precheck,
        }
      : null,
    note,
  }
}

/**
 * @typedef {{
 *   before: string | null,
 *   existed: (before: SqlResult | null) => boolean,
 *   after: ((before: SqlResult | null) => string) | null,
 *   made: (before: SqlResult | null, after: SqlResult | null) => { revert: string, words: NonNullable<UndoRecord['words']>, table: string, precheck?: UndoRecord['precheck'] } | null,
 * }} DdlSteps
 * `before` runs ahead of the statement, `after` behind it; `made` turns what
 * they read into the revert, or null when there is nothing to undo.
 */

/** @param {DdlPlan} plan @returns {DdlSteps} */
function pgDdlSteps(plan) {
  const lit = (/** @type {string} */ s) => textLiteral(s, 'postgres')
  /** An oid read back from the catalog: digits only, or nothing. @param {string | null} v */
  const oid = (v) => (v && /^\d+$/.test(v) ? v : null)
  const relOf = (/** @type {string} */ id) => `SELECT format('%I.%I', n.nspname, c.relname) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE c.oid = ${id}`
  const tableText = plan.table?.text ?? ''

  if (plan.ddl === 'create') {
    const type = plan.objectType
    const last = folded(plan.name.parts[plan.name.parts.length - 1], 'postgres')
    // An index lives in its table's schema, whatever the search path says.
    const indexSchema = `(SELECT n.nspname FROM pg_class t JOIN pg_namespace n ON n.oid = t.relnamespace WHERE t.oid = to_regclass(${lit(tableText)}))`
    const existsSql = type === 'SCHEMA'
      ? `SELECT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = ${lit(last)})::text`
      : type === 'INDEX'
        ? `SELECT (to_regclass(format('%I.%I', ${indexSchema}, ${lit(last)})) IS NOT NULL)::text`
        : `SELECT (to_regclass(${lit(plan.name.text)}) IS NOT NULL)::text`
    const nameSql = type === 'SCHEMA'
      ? `SELECT quote_ident(${lit(last)})`
      : type === 'INDEX'
        ? `SELECT format('%I.%I', ${indexSchema}, ${lit(last)})`
        : `SELECT format('%I.%I', n.nspname, c.relname) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE c.oid = to_regclass(${lit(plan.name.text)})`
    return {
      before: existsSql,
      existed: (b) => cell(/** @type {SqlResult} */ (b)) === 'true',
      after: () => nameSql,
      made: (_b, a) => {
        const q = a ? cell(a) : null
        if (!q) return null
        return createWords(type, q, `DROP ${type} ${q};`, type === 'TABLE' || type === 'MATERIALIZED VIEW' ? `SELECT count(*)::text FROM ${q}` : '')
      },
    }
  }

  if (plan.ddl === 'add-column') {
    const col = folded(/** @type {NameRef} */ (plan.column), 'postgres')
    return {
      before: `SELECT c.oid::text, EXISTS (SELECT 1 FROM pg_attribute a WHERE a.attrelid = c.oid AND a.attname = ${lit(col)} AND NOT a.attisdropped)::text FROM pg_class c WHERE c.oid = to_regclass(${lit(tableText)})`,
      existed: (b) => b?.rows?.[0]?.[1] === 'true',
      after: (b) => `SELECT q.name, quote_ident(${lit(col)}) FROM (${relOf(oid(cell(/** @type {SqlResult} */ (b))) ?? 'NULL')}) AS q(name)`,
      made: (_b, a) => {
        const q = a?.rows?.[0]?.[0]
        const c = a?.rows?.[0]?.[1]
        if (!q || !c) return null
        return {
          revert: `ALTER TABLE ${q} DROP COLUMN ${c};`,
          table: String(q),
          words: { title: 'Revert ADD COLUMN', body: `Drops the column ${c} it added to ${q}.`, note: 'Values written to it since are lost with it.', done: `Dropped column ${c} from ${q}`, action: 'Drop column', destructive: true },
        }
      },
    }
  }

  if (plan.ddl === 'rename-table') {
    return {
      before: `SELECT c.oid::text, quote_ident(c.relname) FROM pg_class c WHERE c.oid = to_regclass(${lit(tableText)})`,
      existed: () => false,
      after: (b) => relOf(oid(cell(/** @type {SqlResult} */ (b))) ?? 'NULL'),
      made: (b, a) => {
        const was = b?.rows?.[0]?.[1]
        const now = a ? cell(a) : null
        if (!was || !now) return null
        return {
          revert: `ALTER TABLE ${now} RENAME TO ${was};`,
          table: now,
          words: { title: 'Revert RENAME', body: `Renames ${now} back to ${was}.`, note: '', done: `Renamed back to ${was}`, action: 'Rename back', destructive: false },
        }
      },
    }
  }

  // rename-column
  const from = folded(/** @type {NameRef} */ (plan.column), 'postgres')
  return {
    before: `SELECT c.oid::text, a.attnum::text, quote_ident(a.attname) FROM pg_class c JOIN pg_attribute a ON a.attrelid = c.oid WHERE c.oid = to_regclass(${lit(tableText)}) AND a.attname = ${lit(from)} AND NOT a.attisdropped`,
    existed: () => false,
    after: (b) => {
      const id = oid(String(b?.rows?.[0]?.[0] ?? '')) ?? 'NULL'
      const num = oid(String(b?.rows?.[0]?.[1] ?? '')) ?? 'NULL'
      return `SELECT format('%I.%I', n.nspname, c.relname), quote_ident(a.attname) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace JOIN pg_attribute a ON a.attrelid = c.oid WHERE c.oid = ${id} AND a.attnum = ${num}`
    },
    made: (b, a) => {
      const was = b?.rows?.[0]?.[2]
      const q = a?.rows?.[0]?.[0]
      const now = a?.rows?.[0]?.[1]
      if (!was || !q || !now) return null
      return {
        revert: `ALTER TABLE ${q} RENAME COLUMN ${now} TO ${was};`,
        table: String(q),
        words: { title: 'Revert RENAME COLUMN', body: `Renames the column ${now} of ${q} back to ${was}.`, note: '', done: `Renamed column ${now} back to ${was}`, action: 'Rename back', destructive: false },
      }
    },
  }
}

/** @param {DdlPlan} plan @returns {DdlSteps} */
function sqliteDdlSteps(plan) {
  const lit = (/** @type {string} */ s) => textLiteral(s, 'sqlite')
  const parts = (plan.table ?? plan.name).parts
  const schemaRef = parts.length > 1 ? dq(parts[parts.length - 2].name) : 'main'
  const schemaName = parts.length > 1 ? parts[parts.length - 2].name : 'main'
  const prefix = parts.length > 1 ? `${schemaRef}.` : ''

  if (plan.ddl === 'create') {
    const type = plan.objectType
    const nameParts = plan.name.parts
    const last = nameParts[nameParts.length - 1].name
    const masterSchema = nameParts.length > 1 ? dq(nameParts[nameParts.length - 2].name) : 'main'
    return {
      before: `SELECT count(*) FROM ${masterSchema}.sqlite_master WHERE type = ${lit(type.toLowerCase())} AND name = ${lit(last)} COLLATE NOCASE`,
      existed: (b) => Number(cell(/** @type {SqlResult} */ (b)) ?? 0) > 0,
      after: null,
      made: () => createWords(type, plan.name.text, `DROP ${type} ${plan.name.text};`, type === 'TABLE' ? `SELECT count(*) FROM ${plan.name.text}` : ''),
    }
  }
  const tableLast = parts[parts.length - 1].name
  if (plan.ddl === 'add-column') {
    const col = /** @type {NameRef} */ (plan.column).name
    return {
      before: `SELECT count(*) FROM pragma_table_xinfo(${lit(tableLast)}, ${lit(schemaName)}) WHERE name = ${lit(col)} COLLATE NOCASE`,
      existed: (b) => Number(cell(/** @type {SqlResult} */ (b)) ?? 0) > 0,
      after: null,
      made: () => ({
        revert: `ALTER TABLE ${plan.name.text} DROP COLUMN ${dq(col)};`,
        table: plan.name.text,
        words: { title: 'Revert ADD COLUMN', body: `Drops the column ${col} it added to ${plan.name.text}.`, note: 'Values written to it since are lost with it.', done: `Dropped column ${col} from ${plan.name.text}`, action: 'Drop column', destructive: true },
      }),
    }
  }
  if (plan.ddl === 'rename-table') {
    const to = /** @type {NameRef} */ (plan.to).name
    return {
      before: null,
      existed: () => false,
      after: null,
      made: () => ({
        revert: `ALTER TABLE ${prefix}${dq(to)} RENAME TO ${dq(tableLast)};`,
        table: `${prefix}${to}`,
        words: { title: 'Revert RENAME', body: `Renames ${prefix}${to} back to ${tableLast}.`, note: '', done: `Renamed back to ${tableLast}`, action: 'Rename back', destructive: false },
      }),
    }
  }
  const from = /** @type {NameRef} */ (plan.column).name
  const to = /** @type {NameRef} */ (plan.to).name
  return {
    before: null,
    existed: () => false,
    after: null,
    made: () => ({
      revert: `ALTER TABLE ${plan.name.text} RENAME COLUMN ${dq(to)} TO ${dq(from)};`,
      table: plan.name.text,
      words: { title: 'Revert RENAME COLUMN', body: `Renames the column ${to} of ${plan.name.text} back to ${from}.`, note: '', done: `Renamed column ${to} back to ${from}`, action: 'Rename back', destructive: false },
    }),
  }
}

/**
 * MySQL's schema changes. Its DDL commits on its own, so the transaction only
 * orders the reads around it: whether the object was there, then its full name.
 * @param {DdlPlan} plan @returns {DdlSteps}
 */
function mysqlDdlSteps(plan) {
  const lit = mysqlText
  /** The `schema`.`name` a catalog row names, quoted. @param {string} schemaCol @param {string} nameCol */
  const quoted = (schemaCol, nameCol) => `CONCAT('\`', REPLACE(${schemaCol}, '\`', '\`\`'), '\`.\`', REPLACE(${nameCol}, '\`', '\`\`'), '\`')`
  const target = plan.table ?? plan.name
  const tableName = lit(target.parts[target.parts.length - 1].name)
  const tableSchema = mysqlSchema(target)
  const tableRow = `FROM information_schema.TABLES WHERE TABLE_SCHEMA = ${tableSchema} AND TABLE_NAME = ${tableName}`

  if (plan.ddl === 'create') {
    const type = plan.objectType
    const last = plan.name.parts[plan.name.parts.length - 1].name
    if (type === 'INDEX') {
      return {
        before: `SELECT COUNT(*) FROM information_schema.STATISTICS WHERE TABLE_SCHEMA = ${tableSchema} AND TABLE_NAME = ${tableName} AND INDEX_NAME = ${lit(last)}`,
        existed: (b) => Number(cell(/** @type {SqlResult} */ (b)) ?? 0) > 0,
        after: () => `SELECT ${quoted('TABLE_SCHEMA', 'TABLE_NAME')} ${tableRow}`,
        made: (_b, a) => {
          const t = a ? cell(a) : null
          if (!t) return null
          return createWords('INDEX', `${bq(last)} on ${t}`, `DROP INDEX ${bq(last)} ON ${t};`, '')
        },
      }
    }
    const nameRow = `FROM information_schema.TABLES WHERE TABLE_SCHEMA = ${mysqlSchema(plan.name)} AND TABLE_NAME = ${lit(last)}`
    return {
      before: `SELECT COUNT(*) ${nameRow}`,
      existed: (b) => Number(cell(/** @type {SqlResult} */ (b)) ?? 0) > 0,
      after: () => `SELECT ${quoted('TABLE_SCHEMA', 'TABLE_NAME')} ${nameRow}`,
      made: (_b, a) => {
        const q = a ? cell(a) : null
        if (!q) return null
        return createWords(type, q, `DROP ${type} ${q};`, type === 'TABLE' ? `SELECT COUNT(*) FROM ${q}` : '')
      },
    }
  }

  if (plan.ddl === 'add-column') {
    const col = /** @type {NameRef} */ (plan.column).name
    return {
      before: `SELECT COUNT(*) FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = ${tableSchema} AND TABLE_NAME = ${tableName} AND COLUMN_NAME = ${lit(col)}`,
      existed: (b) => Number(cell(/** @type {SqlResult} */ (b)) ?? 0) > 0,
      after: () => `SELECT ${quoted('TABLE_SCHEMA', 'TABLE_NAME')} ${tableRow}`,
      made: (_b, a) => {
        const q = a ? cell(a) : null
        if (!q) return null
        return {
          revert: `ALTER TABLE ${q} DROP COLUMN ${bq(col)};`,
          table: q,
          words: { title: 'Revert ADD COLUMN', body: `Drops the column ${col} it added to ${q}.`, note: 'Values written to it since are lost with it.', done: `Dropped column ${col} from ${q}`, action: 'Drop column', destructive: true },
        }
      },
    }
  }

  // Renames: the schema is read before, so the revert names the table where it is.
  const schemaBefore = `SELECT REPLACE(TABLE_SCHEMA, '\`', '\`\`') ${tableRow}`
  if (plan.ddl === 'rename-table') {
    const from = target.parts[target.parts.length - 1].name
    const to = /** @type {NameRef} */ (plan.to).name
    return {
      before: schemaBefore,
      existed: () => false,
      after: null,
      made: (b) => {
        const schema = b ? cell(b) : null
        if (!schema) return null
        const now = `\`${schema}\`.${bq(to)}`
        return {
          revert: `ALTER TABLE ${now} RENAME TO \`${schema}\`.${bq(from)};`,
          table: now,
          words: { title: 'Revert RENAME', body: `Renames ${now} back to ${from}.`, note: '', done: `Renamed back to ${from}`, action: 'Rename back', destructive: false },
        }
      },
    }
  }
  const from = /** @type {NameRef} */ (plan.column).name
  const to = /** @type {NameRef} */ (plan.to).name
  return {
    before: schemaBefore,
    existed: () => false,
    after: null,
    made: (b) => {
      const schema = b ? cell(b) : null
      if (!schema) return null
      const q = `\`${schema}\`.${bq(target.parts[target.parts.length - 1].name)}`
      return {
        revert: `ALTER TABLE ${q} RENAME COLUMN ${bq(to)} TO ${bq(from)};`,
        table: q,
        words: { title: 'Revert RENAME COLUMN', body: `Renames the column ${to} of ${q} back to ${from}.`, note: '', done: `Renamed column ${to} back to ${from}`, action: 'Rename back', destructive: false },
      }
    },
  }
}

/**
 * The revert of a CREATE, in words.
 * @param {DdlPlan['objectType']} type @param {string} q the object, quoted and qualified
 * @param {string} revert @param {string} countSql rows it holds now, '' when it holds none
 */
function createWords(type, q, revert, countSql) {
  const noun = DROP_NOUN[type]
  return {
    revert,
    table: q,
    words: {
      title: `Revert CREATE ${type}`,
      body: `Drops the ${noun} ${q} it created.`,
      note: type === 'SCHEMA'
        ? 'If anything was created in it since, the drop fails and changes nothing.'
        : 'If something depends on it now, the drop fails and changes nothing.',
      done: `Dropped ${noun} ${q}`,
      action: `Drop ${noun}`,
      destructive: true,
    },
    precheck: countSql ? { sql: countSql, says: 'Dropping it deletes the {rows} it holds now.' } : undefined,
  }
}

/**
 * Read the table's columns and key, check the write is one this can undo, and
 * that it touches few enough rows to keep a copy of.
 * @param {WritePlan} plan @param {'postgres' | 'sqlite' | 'mysql'} dialect @param {UndoIo} io
 * @returns {Promise<{ fallback: true, note: string } | { m: TableMeta, setCols: ColumnMeta[], restore: ColumnMeta[], captured: ColumnMeta[] }>}
 */
async function prepare(plan, dialect, io) {
  /** @type {TableMeta | string} */
  let meta
  if (dialect === 'postgres') {
    meta = pgReadMeta((await io.inspect(pgMetaSql(plan))).rows ?? [])
  } else if (dialect === 'mysql') {
    meta = mysqlReadMeta((await io.inspect(mysqlMetaSql(plan))).rows ?? [])
  } else {
    const [colSql, tableSql] = sqliteMetaSqls(plan)
    meta = sqliteReadMeta((await io.inspect(colSql)).rows ?? [], (await io.inspect(tableSql)).rows ?? [], plan)
  }
  if (typeof meta === 'string') return { fallback: true, note: meta }
  const m = meta
  if (!m.pk.length && plan.kind !== 'delete') return { fallback: true, note: `${m.qualified} has no primary key` }

  /** @type {ColumnMeta[]} */
  const setCols = []
  if (plan.kind === 'update') {
    for (const ref of plan.setColumns) {
      const col = findColumn(m.columns, ref, dialect)
      // An unknown column: the write fails, and the ordinary run says where.
      if (!col) return { fallback: true, note: '' }
      if (col.pk) return { fallback: true, note: 'it changes the primary key' }
      if (!setCols.includes(col)) setCols.push(col)
    }
  }
  // A SQLite table without a declared key keeps its rowid on the way back.
  const rowid = dialect === 'sqlite' && m.pk.length === 1 && m.pk[0].name === 'rowid' && !m.columns.some((c) => c.name === 'rowid')
  const restore = [...(rowid ? m.pk : []), ...m.columns.filter((c) => !c.generated)]
  const captured = plan.kind === 'update' ? [...m.pk, ...setCols] : restore
  if (captured.some((c) => MYSQL_SPATIAL.has(c.dataType ?? ''))) return { fallback: true, note: 'spatial values have no exact literal' }

  if (plan.kind === 'insert') {
    if (plan.insertRows > UNDO_MAX_ROWS) return { fallback: true, note: `it inserts over ${UNDO_MAX_ROWS.toLocaleString()} rows` }
  } else {
    const size = (await io.inspect(sizeSql(plan, m, captured, dialect))).rows?.[0] ?? []
    const rows = Number(size[0] ?? 0)
    const bytes = Number(size[1] ?? 0)
    if (rows > UNDO_MAX_ROWS) return { fallback: true, note: `it changes over ${UNDO_MAX_ROWS.toLocaleString()} rows` }
    if (bytes > UNDO_MAX_BYTES) return { fallback: true, note: 'the rows it changes are too large to keep a copy of' }
  }
  return { m, setCols, restore, captured }
}

/**
 * Run `plan` with an undo copy. Anything that goes wrong before the write
 * commits (including the write itself failing) rolls back and asks for the
 * ordinary run, which reports a failure the usual way, with its position.
 * @param {WritePlan | DdlPlan} plan @param {'postgres' | 'sqlite' | 'mysql'} dialect @param {UndoIo} io
 * @returns {Promise<UndoOutcome>}
 */
export async function runWithUndo(plan, dialect, io) {
  if (plan.kind === 'ddl') return runDdlWithUndo(plan, dialect, io)
  // Inserts are Postgres only: SQLite has no INSERT inside WITH, and MySQL
  // returns no keys a count could be trusted to rebuild.
  if (plan.kind === 'insert' && dialect !== 'postgres') return { fallback: true, note: '' }
  /** @type {{ m: TableMeta, setCols: ColumnMeta[], restore: ColumnMeta[], captured: ColumnMeta[] }} */
  let ready
  try {
    const prep = await prepare(plan, dialect, io)
    if ('fallback' in prep) return prep
    ready = prep
  } catch (e) {
    // Stop stops the run; anything else (an odd name, an old server) just
    // means no undo copy, and the ordinary run reports any real problem.
    if (/cancel/i.test(String(e))) throw e
    return { fallback: true, note: '' }
  }
  const { m, setCols, restore, captured } = ready

  try {
    await io.begin()
  } catch {
    return { fallback: true, note: '' }
  }
  /** @type {SqlResult} */
  let result
  /** @type {string[]} */
  let statements = []
  let rows = 0
  try {
    // A write waiting on someone else's lock can't be stopped from here: give
    // up quickly and let the ordinary (stoppable) run wait instead.
    if (dialect === 'postgres') await io.exec("SET LOCAL lock_timeout = '5s'")
    if (plan.kind === 'insert') {
      const res = await io.run(insertReturningSql(plan, m))
      const keys = /** @type {string[][]} */ ((res.rows ?? []).map((r) => r.map(String)))
      rows = keys.length
      statements = revertInsertSql(m, keys, dialect)
      // The keys are this module's, not a result to show.
      result = { ...res, columns: [], rows: [], rowCount: rows, message: null }
    } else {
      const before = /** @type {string[][]} */ (((await io.exec(captureSql(plan, captured, dialect))).rows ?? []).map((r) => r.map(String)))
      const res = await io.run(plan.body)
      result = { ...res, message: null }
      rows = Number(res.rowCount ?? 0)
      // MySQL counts the rows it changed, not the ones it matched: a row that
      // already held the new value is in the copy and reverts to itself.
      if (dialect === 'mysql' ? rows > before.length : rows !== before.length) {
        // Not the rows that were read: a volatile WHERE, or something else
        // writing. The write stands; it just has no undo.
        await io.commit()
        return { result, undo: null, note: 'the rows it changed could not be read exactly' }
      }
      if (plan.kind === 'delete') {
        statements = revertDeleteSql(m, restore, before, dialect)
      } else {
        const pkN = m.pk.length
        const keys = before.map((r) => r.slice(0, pkN))
        const after = /** @type {string[][]} */ (((await io.exec(afterSql(m, captured, keys, dialect))).rows ?? []).map((r) => r.map(String)))
        const byKey = new Map(after.map((r) => [r.slice(0, pkN).join('\u0000'), r.slice(pkN)]))
        const changed = before.map((r) => ({
          key: r.slice(0, pkN),
          before: r.slice(pkN),
          after: byKey.get(r.slice(0, pkN).join('\u0000')) ?? [],
        }))
        if (changed.some((r) => r.after.length !== setCols.length)) {
          await io.commit()
          return { result, undo: null, note: 'the rows it changed could not be read back' }
        }
        statements = revertUpdateSql(m, setCols, changed, dialect)
      }
    }
    await io.commit()
  } catch {
    try { await io.rollback() } catch { /* already closed */ }
    return { fallback: true, note: '' }
  }

  /** @type {string[]} */
  const warnings = []
  if (m.triggers) warnings.push(`${m.qualified} has triggers. What they did when this ran is not undone, and they run again for the revert.`)
  if (plan.kind === 'delete' && m.cascades) warnings.push('Rows in other tables that ON DELETE rules removed or changed are not restored.')
  return {
    result,
    undo: rows
      ? {
          id: `undo-${Date.now().toString(36)}-${++seq}`,
          dialect,
          kind: plan.kind,
          table: m.qualified,
          rows,
          columns: setCols.map((c) => c.name),
          statements,
          warnings,
          sql: plan.body,
          at: Date.now(),
        }
      : null,
    note: '',
  }
}

// ── Describing a revert ──────────────────────────────────────────────────────

/** @param {number} n @param {string} one @param {string} [many] */
const count = (n, one, many = `${one}s`) => `${n.toLocaleString()} ${n === 1 ? one : many}`

/** @param {string[]} names */
function listNames(names) {
  if (names.length <= 1) return names.join('')
  if (names.length === 2) return `${names[0]} and ${names[1]}`
  return `${names.slice(0, -1).join(', ')} and ${names.at(-1)}`
}

/**
 * The confirm dialog's words for a revert: its title, what it does, what it
 * leaves alone, and the button that does it.
 * @param {UndoRecord} u
 * @returns {{ title: string, body: string, note: string, action: string, destructive: boolean }}
 */
export function describeUndo(u) {
  if (u.words) return { title: u.words.title, body: u.words.body, note: u.words.note, action: u.words.action, destructive: u.words.destructive }
  const rows = count(u.rows, 'row')
  if (u.kind === 'update') {
    return {
      title: 'Revert UPDATE',
      body: `Puts back the previous ${listNames(u.columns)} in ${rows} of ${u.table}.`,
      note: 'Rows that changed again since it ran are left as they are.',
      action: `Revert ${rows}`,
      destructive: false,
    }
  }
  if (u.kind === 'delete') {
    return {
      title: 'Revert DELETE',
      body: `Inserts the ${count(u.rows, 'deleted row')} back into ${u.table}.`,
      note: 'A row whose key is taken again is skipped.',
      action: `Restore ${rows}`,
      destructive: false,
    }
  }
  return {
    title: 'Revert INSERT',
    body: `Deletes the ${rows} it inserted from ${u.table}.`,
    note: 'Rows changed since are deleted too.',
    action: `Delete ${rows}`,
    destructive: true,
  }
}

/**
 * The toast after a revert ran.
 * @param {UndoRecord} u @param {number} affected rows the revert changed
 * @returns {{ ok: boolean, title: string, description: string }}
 */
export function revertSummary(u, affected) {
  if (u.words) return { ok: true, title: u.words.done, description: '' }
  const verb = u.kind === 'insert' ? 'Deleted' : u.kind === 'delete' ? 'Restored' : 'Reverted'
  if (affected >= u.rows) return { ok: true, title: `${verb} ${count(u.rows, 'row')} in ${u.table}`, description: '' }
  const skipped = u.rows - affected
  return {
    ok: false,
    title: `${verb} ${affected.toLocaleString()} of ${count(u.rows, 'row')} in ${u.table}`,
    description: u.kind === 'delete'
      ? `${count(skipped, 'row')} ${skipped === 1 ? 'was' : 'were'} skipped: the key is taken again.`
      : `${count(skipped, 'row')} had changed since and ${skipped === 1 ? 'was' : 'were'} left alone.`,
  }
}

// ── Kept undo copies ─────────────────────────────────────────────────────────

/** Copies kept in memory, newest last. Never written to disk: they hold row data. */
const kept = new Map()
const KEEP = 30

/** @param {UndoRecord} u */
export function keepUndo(u) {
  kept.set(u.id, u)
  while (kept.size > KEEP) kept.delete(kept.keys().next().value)
}

/** @param {string} id @returns {UndoRecord | null} */
export function getUndo(id) {
  return kept.get(id) ?? null
}

/** A revert runs once. @param {string} id */
export function dropUndo(id) {
  kept.delete(id)
}
