/**
 * Settings → SQL editor → Add LIMIT to SELECT: before a run, append `LIMIT n`
 * to each SELECT that has no row limit of its own.
 *
 * Off by default. Millions of rows are a thing this app is built to show, so
 * a cap is something to opt into, not a guard put in the way.
 *
 * Only a statement that is a query from start to finish qualifies: SELECT, or
 * WITH ... SELECT. It is left alone when it already limits (LIMIT, FETCH,
 * OFFSET, TOP), writes (SELECT ... INTO, WITH ... INSERT), locks (FOR UPDATE, LOCK
 * IN SHARE MODE), or ends in a clause LIMIT has to come before (ClickHouse
 * FORMAT and SETTINGS, MySQL PROCEDURE). SQL Server has no LIMIT and is never
 * touched. Words are read at the statement's top level only, outside strings,
 * comments and parentheses, so a LIMIT inside a subquery does not count.
 */
import { splitSqlStatements } from './sql-statements.js'

/** Engines without LIMIT. */
const NO_LIMIT_ENGINES = new Set(['mssql', 'redis', 'oracle'])
/** Any of these at the top level leaves the statement as it is. */
const STOP_WORDS = new Set(['LIMIT', 'FETCH', 'OFFSET', 'TOP', 'INTO', 'FOR', 'FORMAT', 'SETTINGS', 'LOCK', 'PROCEDURE'])
/** After WITH, the first of these says what the statement does. */
const VERBS = new Set(['SELECT', 'INSERT', 'UPDATE', 'DELETE', 'MERGE', 'VALUES', 'TABLE', 'UPSERT', 'REPLACE'])

/**
 * The words of a statement at parenthesis depth 0, upper-cased, skipping
 * strings, quoted names, comments and dollar-quoted bodies.
 * @param {string} sql
 * @returns {string[]}
 */
export function topLevelWords(sql) {
  /** @type {string[]} */
  const words = []
  const s = sql
  const n = s.length
  let depth = 0
  let i = 0
  while (i < n) {
    const c = s[i]
    if (c === '-' && s[i + 1] === '-') {
      const nl = s.indexOf('\n', i)
      i = nl === -1 ? n : nl + 1
    } else if (c === '/' && s[i + 1] === '*') {
      const end = s.indexOf('*/', i + 2)
      i = end === -1 ? n : end + 2
    } else if (c === "'" || c === '"' || c === '`' || c === '[') {
      const close = c === '[' ? ']' : c
      i++
      while (i < n) {
        if (c === "'" && s[i] === '\\') { i += 2; continue }
        if (s[i] === close) {
          if (s[i + 1] === close && c !== '[') { i += 2; continue }
          i++
          break
        }
        i++
      }
    } else if (c === '$') {
      const m = /^\$(?:[A-Za-z_][A-Za-z0-9_]*)?\$/.exec(s.slice(i, i + 64))
      if (m) {
        const close = s.indexOf(m[0], i + m[0].length)
        i = close === -1 ? n : close + m[0].length
      } else i++
    } else if (c === '(') {
      depth++
      i++
    } else if (c === ')') {
      depth = Math.max(0, depth - 1)
      i++
    } else if (/[A-Za-z_]/.test(c)) {
      let j = i + 1
      while (j < n && /[A-Za-z0-9_$]/.test(s[j])) j++
      if (depth === 0) words.push(s.slice(i, j).toUpperCase())
      i = j
    } else {
      i++
    }
  }
  return words
}

/**
 * Whether a statement is a plain query with no row limit of its own.
 * @param {string} sql one statement
 */
export function wantsLimit(sql) {
  const words = topLevelWords(sql)
  if (!words.length) return false
  if (words[0] === 'WITH') {
    const verb = words.find((w, i) => i > 0 && VERBS.has(w))
    if (verb !== 'SELECT') return false
  } else if (words[0] !== 'SELECT') {
    return false
  }
  return !words.some((w) => STOP_WORDS.has(w))
}

/**
 * The SQL with `LIMIT limit` added to each statement that wants one. Text
 * between statements (comments, blank lines) is kept as it was.
 * @param {string} sql
 * @param {number} limit 0 or less leaves the SQL alone
 * @param {string | null | undefined} engine
 * @returns {{ sql: string, changed: boolean }}
 */
export function applyAutoLimit(sql, limit, engine) {
  const text = String(sql ?? '')
  const n = Math.floor(Number(limit))
  if (!(n > 0) || NO_LIMIT_ENGINES.has(String(engine ?? ''))) return { sql: text, changed: false }
  let out = text
  let changed = false
  // Right to left, so the earlier statements' offsets stay valid.
  for (const st of splitSqlStatements(text).reverse()) {
    if (!wantsLimit(st.text)) continue
    const semi = st.text.endsWith(';')
    const body = semi ? st.text.slice(0, -1).trimEnd() : st.text
    // On its own line: a trailing `-- comment` would swallow it otherwise.
    const next = `${body}\nLIMIT ${n}${semi ? ';' : ''}`
    out = out.slice(0, st.start) + next + out.slice(st.end)
    changed = true
  }
  return { sql: out, changed }
}
