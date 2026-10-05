/**
 * Query variables for the SQL console: `:name`, `$name` and `${name}`.
 *
 * Variables are located with a small scanner rather than a bare regex so that
 * quoted strings ('…', "…", `…`), dollar-quoted strings ($tag$…$tag$), line and
 * block comments, Postgres `::type` casts and `$1` positional parameters never
 * produce false positives. MySQL's and SQL Server's `@var` is real SQL and is
 * left alone. Before execution the values are inlined as escaped SQL literals -
 * the substituted text is what runs and what lands in query history, so a run
 * is always reproducible.
 *
 * One name is one variable whichever way it is written: `:id` and `$id` in the
 * same query take the same value.
 */

/** @typedef {'auto' | 'text' | 'raw' | 'null'} SqlParamMode */
/**
 * How the target engine escapes a quote inside a string literal.
 * - `standard` — only `''` escapes; a backslash is an ordinary character
 *   (Postgres with `standard_conforming_strings = on`, SQLite, most others).
 * - `backslash` — MySQL's default, where `\'` also escapes and `\\` is one
 *   backslash.
 * @typedef {'standard' | 'backslash'} SqlDialect
 */
/** @typedef {{ value: string, mode: SqlParamMode }} SqlParamValue */
/**
 * @typedef {{ name: string, sigil: ':' | '$', positions: Array<{ start: number, end: number }> }} SqlParam
 * `sigil` is how the first occurrence was written, for showing it back.
 * @typedef {{ engine?: string | null }} SqlParamOptions
 */

const NAME_AT = /[A-Za-z_][A-Za-z0-9_]*/y
const BRACED_AT = /\{([A-Za-z_][A-Za-z0-9_]*)\}/y
const DOLLAR_TAG_AT = /\$(?:[A-Za-z_][A-Za-z0-9_]*)?\$/y
/** A `$` after one of these is part of a name (`price$usd`), not a variable. */
const IDENT_CHAR = /[A-Za-z0-9_$]/
/**
 * SQL Server's own `$` words: `$action` in MERGE ... OUTPUT, `$identity`,
 * `$rowguid`, `$partition`, and the graph table pseudo-columns.
 */
const MSSQL_DOLLAR_WORDS = new Set(['action', 'identity', 'rowguid', 'partition', 'node_id', 'edge_id', 'from_id', 'to_id'])

/**
 * @param {RegExp} re a sticky regex @param {string} s @param {number} at
 * @returns {RegExpExecArray | null}
 */
function matchAt(re, s, at) {
  re.lastIndex = at
  return re.exec(s)
}

/**
 * All variables in the SQL, in first-appearance order.
 * @param {string} sql
 * @param {SqlParamOptions} [opts] `engine` keeps SQL Server's `$action` and
 *   friends from reading as variables there.
 * @returns {SqlParam[]}
 */
export function extractSqlParams(sql, opts = {}) {
  /** @type {Map<string, SqlParam>} */
  const found = new Map()
  const s = String(sql ?? '')
  const n = s.length
  /** @param {string} name @param {number} start @param {number} end @param {':' | '$'} sigil */
  const add = (name, start, end, sigil) => {
    const entry = found.get(name) ?? { name, sigil, positions: [] }
    entry.positions.push({ start, end })
    found.set(name, entry)
  }
  let i = 0
  while (i < n) {
    const c = s[i]
    if (c === '-' && s[i + 1] === '-') {
      const nl = s.indexOf('\n', i)
      i = nl === -1 ? n : nl + 1
      continue
    }
    if (c === '/' && s[i + 1] === '*') {
      const end = s.indexOf('*/', i + 2)
      i = end === -1 ? n : end + 2
      continue
    }
    if (c === "'") {
      i++
      while (i < n) {
        if (s[i] === "'") {
          if (s[i + 1] === "'") { i += 2; continue }
          i++
          break
        }
        i++
      }
      continue
    }
    if (c === '"' || c === '`') {
      const q = c
      i++
      while (i < n && s[i] !== q) i++
      i++
      continue
    }
    if (c === '$') {
      // Postgres dollar-quoted string: $tag$ … $tag$
      const tag = matchAt(DOLLAR_TAG_AT, s, i)
      if (tag) {
        const close = s.indexOf(tag[0], i + tag[0].length)
        i = close === -1 ? n : close + tag[0].length
        continue
      }
      // $name and ${name}. A digit after the $ is a positional parameter ($1)
      // and NAME_AT does not match it.
      if (i === 0 || !IDENT_CHAR.test(s[i - 1])) {
        const braced = s[i + 1] === '{' ? matchAt(BRACED_AT, s, i + 1) : null
        const bare = braced ? null : matchAt(NAME_AT, s, i + 1)
        const name = braced ? braced[1] : bare ? bare[0] : null
        if (name && !(opts.engine === 'mssql' && MSSQL_DOLLAR_WORDS.has(name.toLowerCase()))) {
          const end = i + 1 + (braced ? braced[0].length : name.length)
          add(name, i, end, '$')
          i = end
          continue
        }
      }
    }
    if (c === ':') {
      // `::type` casts and `a:b` slice colons never start a parameter
      if (s[i + 1] === ':' || s[i - 1] === ':') {
        i += s[i + 1] === ':' ? 2 : 1
        continue
      }
      const m = matchAt(NAME_AT, s, i + 1)
      if (m) {
        add(m[0], i, i + 1 + m[0].length, ':')
        i += 1 + m[0].length
        continue
      }
    }
    i++
  }
  return [...found.values()]
}

/**
 * Render one parameter value as a SQL literal.
 * - null  → NULL
 * - raw   → inserted verbatim (expressions, column refs - user's responsibility)
 * - auto  → numbers / TRUE / FALSE / NULL pass through, everything else quoted
 * - text  → always a quoted string
 *
 * `dialect` decides how the quoting is done, and it matters: under MySQL's
 * rules a value ending in a backslash turns `'…\'` into an unterminated
 * literal, so a value like `\' OR 1=1 -- ` would escape its own quoting and
 * run as SQL. Doubling the backslash is required there - and wrong everywhere
 * else, where it would store two backslashes instead of one.
 *
 * @param {string} raw @param {SqlParamMode} mode @param {SqlDialect} [dialect]
 */
export function formatParamLiteral(raw, mode, dialect = 'standard') {
  if (mode === 'null') return 'NULL'
  const v = String(raw ?? '')
  if (mode === 'raw') return v
  if (mode === 'auto') {
    const t = v.trim()
    if (/^(true|false)$/i.test(t)) return t.toUpperCase()
    if (/^null$/i.test(t)) return 'NULL'
    if (/^-?\d+(\.\d+)?([eE][+-]?\d+)?$/.test(t)) return t
  }
  const escaped =
    dialect === 'backslash' ? v.replace(/\\/g, '\\\\').replace(/'/g, "''") : v.replace(/'/g, "''")
  return "'" + escaped + "'"
}

/**
 * The escaping rules for a connection type, as `engineFamily` reports it.
 * @param {string | null | undefined} engine
 * @returns {SqlDialect}
 */
export function dialectForEngine(engine) {
  return engine === 'mysql' ? 'backslash' : 'standard'
}

/**
 * Parameters that still need a value before the SQL can run.
 * `text` (explicit empty string) and `null` are always satisfied.
 * @param {string} sql @param {Record<string, SqlParamValue>} values
 * @param {SqlParamOptions} [opts]
 */
export function missingSqlParams(sql, values, opts = {}) {
  return extractSqlParams(sql, opts).filter((p) => {
    const v = values[p.name]
    if (!v) return true
    if (v.mode === 'null' || v.mode === 'text') return false
    return v.value.trim() === ''
  })
}

/**
 * Inline every variable occurrence with its formatted literal. Variables
 * without a value entry are left untouched.
 * @param {string} sql @param {Record<string, SqlParamValue>} values
 * @param {SqlDialect} [dialect] @param {SqlParamOptions} [opts]
 */
export function substituteSqlParams(sql, values, dialect = 'standard', opts = {}) {
  const repls = extractSqlParams(sql, opts)
    .filter((p) => values[p.name])
    .flatMap((p) => p.positions.map((pos) => ({ ...pos, name: p.name })))
    .sort((a, b) => b.start - a.start)
  let out = String(sql ?? '')
  for (const r of repls) {
    const v = values[r.name]
    out = out.slice(0, r.start) + formatParamLiteral(v.value, v.mode, dialect) + out.slice(r.end)
  }
  return out
}

// ── Last-used values (remembered across sessions, keyed by param name) ───────

const STORAGE_KEY = 'stroke:sql-param-values'

/** @returns {Record<string, SqlParamValue>} */
export function loadStoredParamValues() {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    const obj = raw ? JSON.parse(raw) : {}
    return obj && typeof obj === 'object' ? obj : {}
  } catch {
    return {}
  }
}

/** @param {Record<string, SqlParamValue>} values */
export function saveStoredParamValues(values) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(values))
  } catch {
    // localStorage unavailable - values just won't persist
  }
}

// ── Values per editor tab ────────────────────────────────────────────────────
// Keyed by a scope the console is given (the connection, plus the tab's saved
// query or its title): tab ids restart at tab-1 every session, so keying by id
// would hand one session's values to a different tab the next. A scope has
// its own values over the shared last-used set above, which fills the gaps.

const SCOPES_KEY = 'stroke:sql-param-values:scopes'
/** Scopes kept; the least recently written go first. */
export const PARAM_SCOPES_MAX = 100

/** @returns {Record<string, { at: number, values: Record<string, SqlParamValue> }>} */
function loadScopes() {
  try {
    const raw = localStorage.getItem(SCOPES_KEY)
    const obj = raw ? JSON.parse(raw) : {}
    return obj && typeof obj === 'object' && !Array.isArray(obj) ? obj : {}
  } catch {
    return {}
  }
}

/**
 * The values a scope starts with: its own over the shared last-used set.
 * @param {string} scope
 * @returns {Record<string, SqlParamValue>}
 */
export function loadScopedParamValues(scope) {
  const own = scope ? loadScopes()[scope]?.values : null
  return { ...loadStoredParamValues(), ...(own && typeof own === 'object' ? own : {}) }
}

/**
 * Keep a scope's values, and each one as the shared last-used value too.
 * @param {string} scope @param {Record<string, SqlParamValue>} values
 */
export function saveScopedParamValues(scope, values) {
  saveStoredParamValues({ ...loadStoredParamValues(), ...values })
  if (!scope) return
  try {
    const all = loadScopes()
    all[scope] = { at: Date.now(), values }
    const keys = Object.keys(all)
    if (keys.length > PARAM_SCOPES_MAX) {
      keys.sort((a, b) => (all[a]?.at ?? 0) - (all[b]?.at ?? 0))
      for (const k of keys.slice(0, keys.length - PARAM_SCOPES_MAX)) delete all[k]
    }
    localStorage.setItem(SCOPES_KEY, JSON.stringify(all))
  } catch {
    // localStorage unavailable - values just won't persist
  }
}

/** Forget every remembered value (the Remember setting was switched off). */
export function clearStoredParamValues() {
  try {
    localStorage.removeItem(STORAGE_KEY)
    localStorage.removeItem(SCOPES_KEY)
  } catch {
    // nothing to clear
  }
}
