/**
 * Plain words for a failed statement, and what was probably meant when it names
 * something that is not there. "relation "x" does not exist" is Postgres talking
 * to itself; the console says "No table named x" and offers the closest table.
 */
import { quoteIdent } from './dml-preview.js'

/**
 * @typedef {'table' | 'column' | 'schema' | 'function' | 'syntax' | 'other'} ErrorKind
 * @typedef {{
 *   kind: ErrorKind,
 *   title: string,
 *   name: string,
 *   detail: string,
 * }} ErrorExplanation
 * `title` reads before `name` ("No table named" + `orders`); `name` is '' when
 * the error does not name anything. `detail` is the database's own wording,
 * shown small under the title so it can still be searched for; for a message
 * the rules do not know, the title is its first line and `detail` the rest.
 */

/** Unquoted identifier, `"quoted"`, `'quoted'` or `` `quoted` ``. */
const IDENT = String.raw`"((?:[^"]|"")+)"|'([^']+)'|` + '`([^`]+)`' + String.raw`|([\w$.]+)`

/**
 * One rule per message shape, across the engines the console runs against.
 * Unanchored on purpose: drivers prefix codes (`1146 (42S02): `, `(code: 1) `).
 * `bare` rules name nothing, so the title stands alone.
 * @type {{ re: RegExp, kind: ErrorKind, title: string, bare?: boolean }[]}
 */
const RULES = [
  // Postgres
  { re: new RegExp(String.raw`column (?:${IDENT}) of relation (?:${IDENT}) does not exist`, 'i'), kind: 'column', title: 'No column named' },
  { re: new RegExp(String.raw`relation (?:${IDENT}) does not exist`, 'i'), kind: 'table', title: 'No table named' },
  { re: new RegExp(String.raw`column (?:${IDENT}) does not exist`, 'i'), kind: 'column', title: 'No column named' },
  { re: new RegExp(String.raw`schema (?:${IDENT}) does not exist`, 'i'), kind: 'schema', title: 'No schema named' },
  { re: /function ([\w$."]+)\(.*?\) does not exist/i, kind: 'function', title: 'No function matches' },
  { re: new RegExp(String.raw`syntax error at or near (?:${IDENT})`, 'i'), kind: 'syntax', title: 'Syntax error near' },
  { re: /syntax error at end of input/i, kind: 'syntax', title: 'The statement ends too early', bare: true },
  // MySQL / MariaDB
  { re: /Table '(?:[^'.]+\.)?([^']+)' doesn't exist/i, kind: 'table', title: 'No table named' },
  { re: /Unknown column '([^']+)'/i, kind: 'column', title: 'No column named' },
  { re: /error in your SQL syntax;.*?near '((?:[^'\n]|'')*?)'/is, kind: 'syntax', title: 'Syntax error near' },
  // SQLite / D1 / libSQL
  { re: /no such table: ([\w$."]+)/i, kind: 'table', title: 'No table named' },
  { re: /no such column: ([\w$."]+)/i, kind: 'column', title: 'No column named' },
  { re: /near "([^"]*)": syntax error/i, kind: 'syntax', title: 'Syntax error near' },
  // SQL Server
  { re: /Invalid object name '([^']+)'/i, kind: 'table', title: 'No table named' },
  { re: /Invalid column name '([^']+)'/i, kind: 'column', title: 'No column named' },
  { re: /Incorrect syntax near '((?:[^']|'')*)'/i, kind: 'syntax', title: 'Syntax error near' },
  // DuckDB
  { re: /Table with name ([\w$."]+) does not exist/i, kind: 'table', title: 'No table named' },
  { re: /Referenced column "([^"]+)" not found/i, kind: 'column', title: 'No column named' },
  // ClickHouse
  { re: /Table ([\w$.]+) does not exist/i, kind: 'table', title: 'No table named' },
  { re: /Missing columns: '([^']+)'/i, kind: 'column', title: 'No column named' },
]

/** The first captured group that matched (each IDENT alternative is its own group). */
function firstGroup(/** @type {RegExpMatchArray} */ m) {
  for (let i = 1; i < m.length; i++) if (m[i] != null) return m[i].replace(/""/g, '"')
  return ''
}

/**
 * @param {string} message the database's message, already through cleanErrorMessage
 * @returns {ErrorExplanation}
 */
export function explainSqlError(message) {
  const text = String(message ?? '').trim()
  for (const rule of RULES) {
    const m = text.match(rule.re)
    if (!m) continue
    let name = rule.bare ? '' : firstGroup(m)
    // MySQL quotes the rest of the statement after the error: keep the first token.
    if (rule.kind === 'syntax') name = name.split(/\s+/)[0] ?? ''
    // MySQL's "near ''" is its way of saying the statement stopped early.
    const title = rule.kind === 'syntax' && !name ? 'The statement ends too early' : rule.title
    return { kind: rule.kind, title, name, detail: text }
  }
  // Unknown shape: the message is the title, and there is nothing to repeat.
  const firstLine = text.split('\n')[0]
  return { kind: 'other', title: firstLine, name: '', detail: text.slice(firstLine.length).trim() }
}

/** Edit distance, a swapped pair counting as one edit (`FORM` -> `FROM`). */
function distance(/** @type {string} */ a, /** @type {string} */ b) {
  if (a === b) return 0
  /** @type {number[][]} */
  const d = Array.from({ length: a.length + 1 }, (_, i) => [i])
  for (let j = 1; j <= b.length; j++) d[0][j] = j
  for (let i = 1; i <= a.length; i++) {
    for (let j = 1; j <= b.length; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1
      d[i][j] = Math.min(d[i - 1][j] + 1, d[i][j - 1] + 1, d[i - 1][j - 1] + cost)
      if (i > 1 && j > 1 && a[i - 1] === b[j - 2] && a[i - 2] === b[j - 1]) d[i][j] = Math.min(d[i][j], d[i - 2][j - 2] + 1)
    }
  }
  return d[a.length][b.length]
}

/** The part after the last dot: `public.orders` -> `orders`, `u.email` -> `email`. */
export function bareName(/** @type {string} */ name) {
  const m = String(name ?? '').trim().match(/(?:"((?:[^"]|"")+)"|`([^`]+)`|'([^']+)'|([^."'`\s]+))$/)
  if (!m) return ''
  return m[1]?.replace(/""/g, '"') ?? m[2] ?? m[3] ?? m[4] ?? ''
}

/**
 * How to write `name` so the database finds it: as is when it is a plain
 * lowercase identifier (any case on MySQL/SQLite), quoted otherwise.
 * @param {string} name @param {string} dialect
 */
export function writeIdent(name, dialect) {
  const plain = /^[A-Za-z_][\w$]*$/.test(name)
  const caseFolds = dialect === 'postgres' || dialect === 'postgresql' || dialect === 'cockroachdb'
  if (plain && (!caseFolds || name === name.toLowerCase())) return name
  return quoteIdent(name, dialect === 'mysql' || dialect === 'mariadb' ? 'mysql' : 'postgres')
}

/**
 * Names close to `missing`, written the way the statement needs them. A name
 * that differs only in case comes first: on Postgres that is the quoting trap
 * (`Orders` only matches as `"Orders"`), and it is almost always the intent.
 * @param {string} missing as the error reported it (may be qualified)
 * @param {string[]} candidates names that exist
 * @param {string} dialect
 * @param {number} [limit]
 * @returns {string[]}
 */
export function suggestNames(missing, candidates, dialect, limit = 3) {
  const want = bareName(missing)
  if (!want) return []
  const lower = want.toLowerCase()
  /** @type {{ name: string, score: number }[]} */
  const scored = []
  const seen = new Set()
  for (const c of candidates) {
    const name = bareName(c)
    if (!name || seen.has(name) || name === want) continue
    seen.add(name)
    const cl = name.toLowerCase()
    if (cl === lower) { scored.push({ name, score: -1 }); continue }
    const d = distance(lower, cl)
    // Close enough to be a typo: a third of the name, at least one edit.
    if (d <= Math.max(1, Math.floor(lower.length / 3)) && d < lower.length) scored.push({ name, score: d })
    // A prefix of something longer (`order` -> `orders_archive`) is worth a mention too.
    else if (lower.length >= 4 && cl.startsWith(lower)) scored.push({ name, score: d + 1 })
  }
  // On a tie, the name closest in length: a typo usually keeps it (`FORM` -> `FROM`, not `FOR`).
  const gap = (/** @type {string} */ n) => Math.abs(n.length - want.length)
  scored.sort((a, b) => a.score - b.score || gap(a.name) - gap(b.name) || a.name.localeCompare(b.name))
  return scored.slice(0, limit).map((s) => writeIdent(s.name, dialect))
}

/**
 * Tables a statement reads or writes, by the names it was written with.
 * Rough on purpose: it only narrows "did you mean" for columns.
 * @param {string} sql
 * @returns {string[]}
 */
export function tablesIn(sql) {
  const out = new Set()
  const re = /\b(?:from|join|update|into|table)\s+((?:"[^"]+"|`[^`]+`|[\w$]+)(?:\.(?:"[^"]+"|`[^`]+`|[\w$]+))?)/gi
  for (const m of String(sql ?? '').matchAll(re)) out.add(bareName(m[1]))
  return [...out]
}
