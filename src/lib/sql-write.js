/**
 * Does this SQL modify anything?
 *
 * Read-only mode has to let people browse - running SELECTs in the console is
 * most of what read-only mode is *for* - so the gate can't simply refuse all
 * SQL. This is the one place that decides, so the console, the structure editor,
 * the AI tool calls and the API layer all draw the line identically.
 *
 * Bias: when a statement is ambiguous, call it a write. A false positive shows a
 * "read-only" toast the user can act on; a false negative writes to a database
 * they asked us not to touch.
 */
import { splitSqlStatements } from './sql-statements.js'

/**
 * Statements whose leading keyword is enough to condemn them. `vacuum`,
 * `reindex` and `cluster` write nothing a user typed but do rewrite storage, so
 * they belong on a read-only connection's blocklist too.
 */
const WRITE_HEADS = new Set([
  'insert', 'update', 'delete', 'merge', 'upsert', 'replace', 'truncate',
  'drop', 'alter', 'create', 'rename', 'comment',
  'grant', 'revoke',
  'reindex', 'vacuum', 'cluster', 'refresh', 'analyze',
  'call', 'do', 'execute',
  'attach', 'detach', 'load', 'import', 'restore',
  // Redis speaks its own verbs through the same execute path as SQL, so its
  // mutating commands belong in the same list.
  'del', 'unlink', 'hdel', 'hset', 'hmset', 'setex', 'setnx', 'psetex', 'mset',
  'msetnx', 'getset', 'getdel', 'append', 'incr', 'incrby', 'incrbyfloat',
  'decr', 'decrby', 'hincrby', 'hincrbyfloat', 'lpush', 'lpushx', 'rpush',
  'rpushx', 'lpop', 'rpop', 'lset', 'lrem', 'ltrim', 'rpoplpush', 'sadd',
  'srem', 'spop', 'smove', 'zadd', 'zrem', 'zincrby', 'zremrangebyrank',
  'zremrangebyscore', 'expire', 'expireat', 'pexpire', 'pexpireat', 'persist',
  'move', 'flushdb', 'flushall', 'xadd', 'xdel', 'xtrim',
])

/**
 * Remove `--` line comments and `/* *​/` block comments so the leading keyword is
 * the real one. A comment is the easiest way to hide `DROP` behind a `SELECT`.
 * @param {string} sql
 */
export function stripSqlComments(sql) {
  let out = ''
  let i = 0
  const n = sql.length
  while (i < n) {
    const c = sql[i]
    const next = sql[i + 1]
    // Quoted literals pass through untouched - a `--` inside a string is data.
    if (c === "'" || c === '"' || c === '`') {
      const quote = c
      out += c
      i++
      while (i < n) {
        out += sql[i]
        if (sql[i] === quote) {
          // Doubled quote is an escaped quote, not the end.
          if (sql[i + 1] === quote) { out += sql[i + 1]; i += 2; continue }
          i++
          break
        }
        i++
      }
      continue
    }
    if (c === '-' && next === '-') {
      while (i < n && sql[i] !== '\n') i++
      continue
    }
    if (c === '/' && next === '*') {
      i += 2
      while (i < n && !(sql[i] === '*' && sql[i + 1] === '/')) i++
      i += 2
      out += ' '
      continue
    }
    out += c
    i++
  }
  return out
}

/**
 * Split a script into statements: the editor's splitter, so a dollar-quoted
 * body or a routine's BEGIN ... END counts as one statement here too. A local
 * splitter that knew neither read the DELETE inside a procedure body as a
 * write of its own and refetched every open table.
 * @param {string} sql
 */
function splitStatements(sql) {
  return splitSqlStatements(sql).map((s) => s.text)
}

/**
 * PRAGMAs that only report. The app introspects SQLite schemas with these, so
 * treating every PRAGMA as a write would break schema browsing on a read-only
 * SQLite file - the exact case read-only mode exists for.
 */
const MUTATING_PRAGMAS = new Set([
  'optimize', 'shrink_memory', 'incremental_vacuum', 'wal_checkpoint', 'vacuum',
])

/**
 * True when any statement in `sql` would modify data, schema or storage.
 * @param {string} sql
 */
export function isWriteSql(sql) {
  const clean = stripSqlComments(String(sql ?? ''))
  if (!clean.trim()) return false

  for (const raw of splitStatements(clean)) {
    // Leading parens wrap `(SELECT …) UNION …`, and a CTE can be parenthesised.
    const stmt = raw.trim().replace(/^[(\s]+/, '')
    if (!stmt) continue
    const head = (stmt.match(/^[a-z_]+/i)?.[0] ?? '').toLowerCase()

    if (WRITE_HEADS.has(head)) return true

    // A CTE hides the real verb behind the WITH list: `WITH x AS (…) DELETE …`.
    if (head === 'with' && /\b(insert\s+into|update\s+[\w"`.]|delete\s+from|merge\s+into)\b/i.test(stmt))
      return true

    // `SELECT … INTO t FROM …` creates a table, and MySQL's INTO OUTFILE writes a
    // file. Only the clause before FROM counts, so a `LIKE '%into%'` in a WHERE
    // can't be mistaken for one.
    if (head === 'select') {
      const beforeFrom = stmt.split(/\bfrom\b/i)[0]
      if (/\binto\s+(?:outfile\s+|dumpfile\s+)?["`\w]/i.test(beforeFrom)) return true
    }

    // `PRAGMA x = y` sets; `PRAGMA table_info(t)` reads.
    if (head === 'pragma') {
      const name = (stmt.match(/^pragma\s+(?:[\w"'`.]+\s*\.\s*)?([\w]+)/i)?.[1] ?? '').toLowerCase()
      if (/=/.test(stmt) || MUTATING_PRAGMAS.has(name)) return true
      continue
    }

    // COPY reads (`COPY … TO`) or writes (`COPY … FROM`).
    if (head === 'copy' && /\bfrom\b/i.test(stmt)) return true

    // `SET` is session state, except when it isn't: SET ROLE / SET SESSION
    // AUTHORIZATION change who you are for everything after it - and Redis's
    // `SET key value` is a plain write. SQL's form always has `=` or `TO`, so the
    // absence of both marks the Redis one.
    if (head === 'set') {
      if (/^set\s+(role|session\s+authorization)\b/i.test(stmt)) return true
      if (!/[=]|\bto\b/i.test(stmt)) return true
    }
  }
  return false
}

// ── What a run changed ───────────────────────────────────────────────────────

/** One identifier part: "quoted", `quoted`, [quoted] (T-SQL) or bare. */
const IDENT = String.raw`(?:"(?:[^"]|"")+"|` + '`(?:[^`]|``)+`' + String.raw`|\[[^\]]+\]|[\w$]+)`
/** table, schema.table, or db.schema.table (T-SQL). */
const QNAME = String.raw`${IDENT}(?:\s*\.\s*${IDENT}){0,2}`
const QLIST = String.raw`${QNAME}(?:\s*,\s*${QNAME})*`
/** @param {string} src */
const ci = (src) => new RegExp(src, 'i')

const INSERT_RE = ci(String.raw`^insert\s+(?:(?:low_priority|delayed|high_priority|ignore|or\s+\w+)\s+)*(?:into\s+)?(?:table\s+)?(${QNAME})`)
const REPLACE_RE = ci(String.raw`^replace\s+(?:(?:low_priority|delayed)\s+)*(?:into\s+)?(${QNAME})`)
const UPSERT_RE = ci(String.raw`^(?:upsert|merge)\s+(?:into\s+)?(${QNAME})`)
const UPDATE_RE = ci(String.raw`^update\s+(?:(?:low_priority|ignore|only|or\s+\w+)\s+)*(${QNAME})`)
const DELETE_RE = ci(String.raw`^delete\s+(?:(?:low_priority|quick|ignore)\s+)*from\s+(?:only\s+)?(${QNAME})`)
const TRUNCATE_RE = ci(String.raw`^truncate\s+(?:table\s+)?(?:only\s+)?(${QLIST})`)
const COPY_RE = ci(String.raw`^copy\s+(${QNAME})`)
const LOAD_RE = ci(String.raw`\binto\s+table\s+(${QNAME})`)
const REFRESH_RE = ci(String.raw`^refresh\s+materialized\s+view\s+(?:concurrently\s+)?(${QNAME})`)
/** What a CREATE / DROP / ALTER is about, past OR REPLACE (OR ALTER in T-SQL),
 *  MySQL's DEFINER=… / ALGORITHM=… / SQL SECURITY …, TEMP and the like. */
const OBJECT_RE = /^(?:create|drop|alter)\s+(?:or\s+(?:replace|alter)\s+)?(?:(?:definer\s*=\s*\S+|algorithm\s*=\s*\w+|sql\s+security\s+\w+|global|local|temp|temporary|unlogged|external|recursive|constraint)\s+)*(materialized\s+view|foreign\s+table|\w+)/i
/**
 * The sidebar's Objects groups a statement's object belongs to. A macro is
 * DuckDB's function, an aggregate is a function, a domain is a type.
 * @type {Record<string, string>}
 */
const OBJECT_KINDS = {
  view: 'view', 'materialized view': 'matview',
  function: 'function', macro: 'function', aggregate: 'function',
  procedure: 'procedure', proc: 'procedure',
  trigger: 'trigger', sequence: 'sequence', type: 'type', domain: 'type', event: 'event',
}
const COMMENT_ON_RE = /^comment\s+on\s+(materialized\s+view|\w+)/i
const OBJECT_NAMES_RE = ci(String.raw`^(?:create|drop|alter)\s+.*?\b(?:table|view)\s+(?:if\s+(?:not\s+)?exists\s+)?(?:only\s+)?(${QLIST})`)
const SELECT_INTO_RE = ci(String.raw`\binto\s+(?!outfile\b|dumpfile\b)(${QNAME})`)
const CTE_TARGET_RE = new RegExp(String.raw`\b(?:insert\s+into|update(?:\s+only)?|delete\s+from(?:\s+only)?|merge\s+into)\s+(${QNAME})`, 'gi')

/** @param {string} part */
function unquoteIdent(part) {
  const q = part[0]
  if ((q === '"' || q === '`') && part.endsWith(q)) return part.slice(1, -1).replaceAll(q + q, q)
  if (q === '[' && part.endsWith(']')) return part.slice(1, -1)
  return part
}

/**
 * Lowercased on both parts: the caller compares against open tabs, and a tab
 * refetched for nothing costs one query where a missed one shows stale rows.
 * @param {string} qname
 */
function tableRef(qname) {
  const parts = [...qname.matchAll(new RegExp(IDENT, 'g'))].map((m) => unquoteIdent(m[0]).toLowerCase())
  return { schema: parts.length > 1 ? parts[parts.length - 2] : null, name: parts[parts.length - 1] ?? '' }
}

/**
 * @typedef {{ schema: string | null, name: string }} TableRef
 * @typedef {{ catalog: boolean, schemas: boolean, data: boolean, tables: TableRef[] | null, objects: string[] }} RunEffects
 *   catalog: objects were created, dropped or altered, so the table list and the
 *   schema catalog are stale. schemas: a schema or database came or went.
 *   data: rows changed, so row counts are stale. tables: the tables a statement
 *   named as its target; null when one could have touched any table (CALL, DO,
 *   a multi-table DELETE). objects: the sidebar's Objects groups that are stale
 *   (view, matview, function, procedure, trigger, sequence, type, event),
 *   'comment' after a COMMENT ON, 'any' when a CALL or DO could have changed
 *   anything.
 */

/**
 * What a run in the SQL editor may have changed, so the sidebar and the open
 * table tabs can catch up without a manual refresh. Leans the same way
 * `isWriteSql` does: an unrecognised write reports "any table" rather than none.
 * @param {string} sql
 * @returns {RunEffects}
 */
export function sqlRunEffects(sql) {
  /** @type {RunEffects} */
  const fx = { catalog: false, schemas: false, data: false, tables: [], objects: [] }
  /** @param {string} kind */
  const stale = (kind) => { if (!fx.objects.includes(kind)) fx.objects.push(kind) }
  /** @param {string} list */
  const touch = (list) => {
    if (!fx.tables) return
    for (const m of list.matchAll(new RegExp(QNAME, 'g'))) fx.tables.push(tableRef(m[0]))
  }
  const anyTable = () => { fx.tables = null }
  /** @param {RegExpExecArray | null} m */
  const touchOrAny = (m) => (m ? touch(m[1]) : anyTable())

  for (const raw of splitStatements(stripSqlComments(String(sql ?? '')))) {
    const stmt = raw.trim().replace(/^[(\s]+/, '')
    if (!stmt) continue
    const head = (stmt.match(/^[a-z_]+/i)?.[0] ?? '').toLowerCase()
    /** @type {RegExpExecArray | null} */
    let m
    switch (head) {
      case 'insert':
        fx.data = true
        touchOrAny(INSERT_RE.exec(stmt))
        break
      case 'replace':
        fx.data = true
        touchOrAny(REPLACE_RE.exec(stmt))
        break
      case 'upsert':
      case 'merge':
        fx.data = true
        touchOrAny(UPSERT_RE.exec(stmt))
        break
      case 'update':
        fx.data = true
        m = UPDATE_RE.exec(stmt)
        // `UPDATE a JOIN b … SET` and `UPDATE a, b SET` (MySQL) can write either.
        if (m && /,|\bjoin\b/i.test(stmt.slice(m.index + m[0].length).split(/\bset\b/i)[0])) anyTable()
        else touchOrAny(m)
        break
      case 'delete':
        fx.data = true
        // `DELETE a FROM a JOIN b` (MySQL) and `DELETE t WHERE` (T-SQL) put the
        // target where this does not look.
        touchOrAny(DELETE_RE.exec(stmt))
        break
      case 'truncate':
        fx.data = true
        touchOrAny(TRUNCATE_RE.exec(stmt))
        break
      case 'copy':
        // `COPY t FROM` loads rows; `COPY t TO` and `COPY (SELECT …) TO` read.
        m = COPY_RE.exec(stmt)
        if (m && /\bfrom\b/i.test(stmt.slice(m.index + m[0].length))) { fx.data = true; touch(m[1]) }
        break
      case 'load':
        // MySQL's LOAD DATA … INTO TABLE t. DuckDB's `LOAD ext` loads an extension.
        m = LOAD_RE.exec(stmt)
        if (m) { fx.data = true; touch(m[1]) }
        break
      case 'refresh':
        m = REFRESH_RE.exec(stmt)
        if (m) { fx.data = true; touch(m[1]) }
        break
      case 'create':
      case 'drop':
      case 'alter': {
        fx.catalog = true
        const kind = (OBJECT_RE.exec(stmt)?.[1] ?? '').toLowerCase().replace(/\s+/g, ' ')
        if (OBJECT_KINDS[kind]) stale(OBJECT_KINDS[kind])
        if (kind === 'schema' || kind === 'database') fx.schemas = true
        else if (kind === 'table' || kind === 'view' || kind === 'materialized view' || kind === 'foreign table') {
          touchOrAny(OBJECT_NAMES_RE.exec(stmt))
        }
        break
      }
      case 'rename': {
        fx.catalog = true
        // MySQL: RENAME TABLE a TO b, c TO d. Both names of a pair count.
        const pairs = stmt.replace(/^rename\s+tables?\s+/i, '')
        if (pairs === stmt || !fx.tables) break
        for (const n of pairs.matchAll(new RegExp(QNAME, 'g'))) {
          if (n[0].toLowerCase() !== 'to') fx.tables.push(tableRef(n[0]))
        }
        break
      }
      case 'comment': {
        fx.catalog = true
        stale('comment')
        const on = COMMENT_ON_RE.exec(stmt)?.[1].toLowerCase().replace(/\s+/g, ' ') ?? ''
        if (OBJECT_KINDS[on]) stale(OBJECT_KINDS[on])
        break
      }
      case 'attach':
      case 'detach':
        // SQLite and DuckDB list attached databases as schemas.
        fx.catalog = true
        fx.schemas = true
        break
      case 'analyze':
      case 'vacuum':
      case 'optimize':
        // No row changes, but the statistics row counts are read from do.
        fx.data = true
        break
      case 'call':
      case 'do':
      case 'exec':
      case 'execute':
      case 'import':
      case 'restore':
        fx.catalog = true
        fx.data = true
        anyTable()
        stale('any')
        break
      case 'with':
        if (/\b(insert\s+into|update\s+[\w"`.]|delete\s+from|merge\s+into)\b/i.test(stmt)) {
          fx.data = true
          for (const w of stmt.matchAll(CTE_TARGET_RE)) touch(w[1])
        }
        break
      case 'select':
        // `SELECT … INTO t FROM …` creates t. MySQL's INTO @var and INTO OUTFILE
        // create nothing in the catalog.
        m = SELECT_INTO_RE.exec(stmt.split(/\bfrom\b/i)[0])
        if (m) { fx.catalog = true; touch(m[1]) }
        break
    }
  }
  return fx
}
