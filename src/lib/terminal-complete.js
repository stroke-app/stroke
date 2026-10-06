/**
 * Suggestions for the terminal tab: what to offer for the line being typed at
 * the client's prompt, drawn on top of the real client.
 *
 * The client keeps its own line editing (and psql its own Tab completion); this
 * only reads what has been typed and proposes the rest, and the page types the
 * accepted text into the client like a user would. Three sources:
 *
 * - the client's own commands, with what each does (`\dt` List tables,
 *   `.schema`, redis commands);
 * - SQL: keywords, functions, and the connection's tables and columns, by
 *   context (tables after FROM, a table's columns after `t.`), the same
 *   analysis the SQL editor uses;
 * - translations, for a command typed in another client's dialect: `show
 *   databases` in psql offers `\l`, `\dt` in mysql offers `SHOW TABLES;`.
 *   psql has no SHOW DATABASES, and mysql reads `\d` as "change delimiter", so
 *   these save a confusing error (or worse).
 *
 * Pure: no DOM, no client. The page passes the line and the schema hints.
 */
import { PG_KEYWORDS, PG_FUNCTIONS, DIALECT_KEYWORDS, analyzeQuery } from '$lib/sql-complete-data.js'

/**
 * @typedef {object} Suggestion
 * @property {string} label what the list shows
 * @property {string} insert what accepting types
 * @property {string} [detail] the muted text on the right
 * @property {'meta' | 'keyword' | 'table' | 'column' | 'function' | 'command' | 'translate'} kind
 * @property {boolean} [replaceLine] accepting replaces the whole line
 * @property {boolean} [run] accepting also presses Enter
 * @property {string} [keys] exact keystrokes to send instead (`clear` sends Ctrl+L)
 * @property {boolean} [exact] it is what was typed already: Enter on it runs the line
 * @property {number} [rank] position in a curated list (most used first)
 */

/**
 * @typedef {object} SchemaHints
 * @property {string[]} [tables]
 * @property {Record<string, Array<string | { name: string, type?: string }>>} [columnsByTable]
 */

/** @type {Array<[string, string]>} */
export const PSQL_META = [
  ['\\l', 'List databases'], ['\\l+', 'List databases with sizes'],
  ['\\c', 'Connect to another database'], ['\\conninfo', 'Show the current connection'],
  ['\\dt', 'List tables'], ['\\dt+', 'List tables with sizes'],
  ['\\d', 'Describe a table, view or index'], ['\\d+', 'Describe, with storage and comments'],
  ['\\dn', 'List schemas'], ['\\dv', 'List views'], ['\\dm', 'List materialized views'],
  ['\\di', 'List indexes'], ['\\ds', 'List sequences'], ['\\df', 'List functions'],
  ['\\dT', 'List data types'], ['\\du', 'List roles'], ['\\dp', 'List privileges'],
  ['\\dx', 'List extensions'], ['\\dE', 'List foreign tables'],
  ['\\sf', 'Show a function definition'], ['\\sv', 'Show a view definition'],
  ['\\x', 'Toggle expanded output'], ['\\timing', 'Toggle query timing'],
  ['\\e', 'Edit the query in $EDITOR'], ['\\i', 'Run commands from a file'],
  ['\\o', 'Send results to a file'], ['\\copy', 'Copy between a table and a local file'],
  ['\\g', 'Run the query buffer'], ['\\gx', 'Run the query buffer, expanded'],
  ['\\watch', 'Run the query again every few seconds'], ['\\s', 'Show command history'],
  ['\\h', 'Help on an SQL command'], ['\\?', 'Help on psql commands'],
  ['\\set', 'Set a psql variable'], ['\\pset', 'Set an output option'],
  ['\\a', 'Toggle aligned output'], ['\\t', 'Toggle rows only'],
  ['\\echo', 'Print text'], ['\\!', 'Run a shell command'], ['\\q', 'Quit psql'],
]

/** psql commands whose argument is a table (or view) name. */
const PSQL_TABLE_ARG = new Set(['\\d', '\\d+', '\\dt', '\\dt+', '\\di', '\\dv', '\\dm', '\\sv', '\\dp', '\\z'])

/** @type {Array<[string, string]>} */
export const SQLITE_DOT = [
  ['.tables', 'List tables'], ['.schema', 'Show CREATE statements'],
  ['.indexes', 'List indexes'], ['.databases', 'List attached databases'],
  ['.mode', 'Output mode: box, table, csv, json, line'], ['.headers', 'Column headers on or off'],
  ['.timer', 'Time each statement, on or off'], ['.dump', 'Dump the database as SQL'],
  ['.import', 'Import a file into a table'], ['.output', 'Send output to a file'],
  ['.read', 'Run SQL from a file'], ['.show', 'Show the current settings'],
  ['.help', 'Help'], ['.quit', 'Quit sqlite3'],
]

const SQLITE_TABLE_ARG = new Set(['.schema', '.indexes', '.dump'])

/** @type {Array<[string, string]>} */
const MYSQL_STATEMENTS = [
  ['SHOW DATABASES;', 'List databases'], ['SHOW TABLES;', 'List tables'],
  ['SHOW COLUMNS FROM', 'List a table\'s columns'], ['SHOW CREATE TABLE', 'Show a table\'s DDL'],
  ['SHOW PROCESSLIST;', 'List running sessions'], ['SHOW VARIABLES LIKE', 'Find a server setting'],
  ['SHOW INDEX FROM', 'List a table\'s indexes'], ['SHOW GRANTS;', 'Your privileges'],
  ['DESCRIBE', 'Describe a table'], ['USE', 'Switch database'], ['STATUS', 'Connection status'],
]

/** @type {Array<[string, string]>} */
const REDIS_COMMANDS = [
  ['GET', 'Value of a key'], ['SET', 'Set a key'], ['DEL', 'Delete keys'],
  ['EXISTS', 'Whether keys exist'], ['TYPE', 'Type of a key'], ['TTL', 'Seconds until a key expires'],
  ['EXPIRE', 'Expire a key after seconds'], ['KEYS', 'Keys matching a pattern (blocks; prefer SCAN)'],
  ['SCAN', 'Iterate keys'], ['HGETALL', 'Every field of a hash'], ['HGET', 'One field of a hash'],
  ['HSET', 'Set hash fields'], ['LRANGE', 'A range of a list'], ['LPUSH', 'Push onto a list'],
  ['SMEMBERS', 'Every member of a set'], ['SADD', 'Add to a set'],
  ['ZRANGE', 'A range of a sorted set'], ['ZADD', 'Add to a sorted set'],
  ['INCR', 'Increment a counter'], ['INFO', 'Server information'], ['DBSIZE', 'Number of keys'],
  ['SELECT', 'Switch logical database'], ['PING', 'Check the connection'],
  ['CLIENT LIST', 'Connected clients'], ['CONFIG GET', 'Read a setting'],
  ['MEMORY USAGE', 'Bytes a key uses'], ['MONITOR', 'Stream every command (debug)'],
]

/** Ctrl+E, Ctrl+U, Ctrl+L: clear the typed line, then the screen (readline's clear-screen). */
const CLEAR_KEYS = '\x05\x15\x0c'

/**
 * Commands typed in another client's dialect (or a shell's), and what this
 * client calls them. `$1` is the captured name; a fourth entry sends those
 * keystrokes instead of a command.
 * @type {Record<string, Array<[RegExp, string, string, string?]>>}
 */
const TRANSLATIONS = {
  psql: [
    [/^ls(?:\s+-\w+)?$/, '\\dt', 'Tables, the psql way'],
    [/^cd\s+([\w"-]+)$/, '\\c $1', 'psql switches database with \\c'],
    [/^(?:clear|cls)$/, 'clear', 'Clear the screen', CLEAR_KEYS],
    [/^show\s+databases?\s*;?$/i, '\\l', 'psql lists databases with \\l'],
    [/^show\s+tables\s*;?$/i, '\\dt', 'psql lists tables with \\dt'],
    [/^show\s+(?:schemas|schemata)\s*;?$/i, '\\dn', 'psql lists schemas with \\dn'],
    [/^(?:describe|desc)\s+([\w."]+)\s*;?$/i, '\\d $1', 'psql describes a table with \\d'],
    [/^show\s+(?:full\s+)?(?:columns|fields)\s+from\s+([\w."]+)\s*;?$/i, '\\d $1', 'psql describes a table with \\d'],
    [/^show\s+create\s+table\s+([\w."]+)\s*;?$/i, '\\d+ $1', 'psql shows a table\'s definition with \\d+'],
    [/^show\s+index(?:es)?\s+from\s+([\w."]+)\s*;?$/i, '\\d $1', 'psql lists indexes in \\d'],
    [/^use\s+([\w"-]+)\s*;?$/i, '\\c $1', 'psql switches database with \\c'],
    [/^show\s+(?:users|grants)\s*;?$/i, '\\du', 'psql lists roles with \\du'],
    [/^show\s+(?:full\s+)?processlist\s*;?$/i, 'SELECT pid, usename, state, query FROM pg_stat_activity;', 'Sessions on this server'],
    [/^\.tables\s*$/, '\\dt', 'psql lists tables with \\dt'],
    [/^\.schema\s+(\S+)$/, '\\d $1', 'psql describes a table with \\d'],
  ],
  mysql: [
    [/^ls$/, 'SHOW TABLES;', 'Tables, the MySQL way'],
    [/^(?:clear|cls)$/, 'clear', 'Clear the screen', CLEAR_KEYS],
    [/^\\l\+?$/, 'SHOW DATABASES;', 'MySQL lists databases with SHOW DATABASES'],
    [/^\\dt\+?$/, 'SHOW TABLES;', 'MySQL lists tables with SHOW TABLES'],
    [/^\\dn$/, 'SHOW DATABASES;', 'Schemas are databases in MySQL'],
    [/^\\d\+?\s+(\S+)$/, 'DESCRIBE $1;', 'MySQL describes a table with DESCRIBE'],
    [/^\\c\s+(\S+)$/, 'USE $1;', 'MySQL switches database with USE'],
    [/^\\du$/, 'SELECT user, host FROM mysql.user;', 'Users on this server'],
    [/^\.tables$/, 'SHOW TABLES;', 'MySQL lists tables with SHOW TABLES'],
  ],
  sqlite3: [
    [/^ls$/, '.tables', 'Tables, the sqlite3 way'],
    [/^(?:clear|cls)$/, 'clear', 'Clear the screen', CLEAR_KEYS],
    [/^(?:\\dt\+?|show\s+tables\s*;?)$/i, '.tables', 'sqlite3 lists tables with .tables'],
    [/^(?:\\l|show\s+databases?\s*;?)$/i, '.databases', 'sqlite3 lists databases with .databases'],
    [/^(?:\\d\+?|describe|desc)\s+(\S+?)\s*;?$/i, '.schema $1', 'sqlite3 shows a table with .schema'],
    [/^\\di$/, '.indexes', 'sqlite3 lists indexes with .indexes'],
    [/^(?:\\q|exit|quit)$/i, '.quit', 'sqlite3 quits with .quit'],
  ],
}

/** @param {string} client */
function familyOf(client) {
  if (client === 'mariadb') return 'mysql'
  if (client === 'valkey-cli') return 'redis-cli'
  return client
}

const KIND_ORDER = { translate: 0, meta: 1, command: 1, column: 2, table: 3, function: 4, keyword: 5 }
const MAX_ITEMS = 8

/** First words of a statement, by client family: all that is offered for the first word. */
const START_KEYWORDS = [
  'SELECT', 'WITH', 'INSERT INTO', 'UPDATE', 'DELETE FROM', 'CREATE TABLE', 'CREATE INDEX', 'CREATE VIEW',
  'ALTER TABLE', 'DROP TABLE', 'TRUNCATE', 'EXPLAIN', 'EXPLAIN ANALYZE', 'BEGIN', 'COMMIT', 'ROLLBACK',
  'GRANT', 'REVOKE', 'VACUUM', 'ANALYZE', 'COPY', 'SHOW', 'SET', 'TABLE', 'VALUES',
]
const START_EXTRA = /** @type {Record<string, string[]>} */ ({
  mysql: ['USE', 'DESCRIBE', 'REPLACE INTO'],
  sqlite3: ['PRAGMA', 'ATTACH DATABASE'],
})

/** First words that make the line SQL. Anything else (`ls`, a typo) gets no SQL suggestions. */
const SQL_STARTERS = new Set([
  'select', 'with', 'values', 'table', 'insert', 'update', 'delete', 'merge', 'replace', 'create', 'alter',
  'drop', 'truncate', 'comment', 'grant', 'revoke', 'explain', 'analyze', 'analyse', 'vacuum', 'reindex',
  'cluster', 'refresh', 'show', 'describe', 'desc', 'pragma', 'call', 'exec', 'execute', 'set', 'reset',
  'use', 'begin', 'start', 'commit', 'rollback', 'savepoint', 'release', 'lock', 'copy', 'attach',
  'detach', 'do', 'prepare', 'deallocate', 'discard', 'listen', 'notify', 'declare', 'fetch', 'move', 'close',
])

/** Keywords and functions wait for this many letters; one letter matches too much to help. */
const MIN_WORD = 2

/**
 * @param {Array<[string, string]>} list
 * @param {string} typed
 * @param {Suggestion['kind']} kind
 * @returns {Suggestion[]}
 */
function fromList(list, typed, kind) {
  const lower = typed.toLowerCase()
  return list
    .map(([cmd, detail], rank) => ({ label: cmd, insert: cmd, detail, kind, rank, exact: cmd.toLowerCase() === lower }))
    .filter((item) => item.label.toLowerCase().startsWith(lower))
}

/**
 * The list to show: exact matches first (Enter on one runs the line), then by
 * kind and length; nothing at all when only exact matches are left, since a
 * list that offers what was typed is noise.
 * @param {Suggestion[]} items
 * @param {string} token
 */
function finish(items, token) {
  if (!items.some((i) => !i.exact)) return { items: [], token: '' }
  const seen = new Set()
  const unique = items.filter((i) => {
    const key = i.insert.toLowerCase()
    if (seen.has(key)) return false
    seen.add(key)
    return true
  })
  // Curated lists keep their order (most used first); names go shorter first,
  // `user` before `user_sessions`.
  unique.sort((a, b) => (Number(!!b.exact) - Number(!!a.exact))
    || (KIND_ORDER[a.kind] - KIND_ORDER[b.kind])
    || ((a.rank ?? 0) - (b.rank ?? 0))
    || (a.label.length - b.label.length)
    || a.label.localeCompare(b.label))
  return { items: unique.slice(0, MAX_ITEMS), token }
}

/** A name as SQL needs it: bare when plain, quoted when not. @param {string} name @param {string} family */
function sqlName(name, family) {
  if (/^[a-z_][a-z0-9_$]*$/.test(name)) return name
  const q = family === 'mysql' ? '`' : '"'
  return `${q}${name.replaceAll(q, q + q)}${q}`
}

/** @param {SchemaHints} hints @param {string} table */
function columnsOf(hints, table) {
  const all = hints.columnsByTable ?? {}
  const lower = table.toLowerCase()
  const key = Object.keys(all).find((k) => k.toLowerCase() === lower || k.toLowerCase().endsWith(`.${lower}`))
  return (key ? all[key] : []).map((c) => (typeof c === 'string' ? { name: c, type: '' } : c))
}

/**
 * What to suggest for `line`, the text typed so far on the prompt's line.
 * @param {{ client: string, line: string, statement?: string, hints?: SchemaHints }} input
 *   `statement` is the earlier lines of a statement that is still open.
 * @returns {{ items: Suggestion[], token: string }} `token` is the typed text
 *   the accepted item replaces (the end of `line`).
 */
export function suggest({ client, line, statement = '', hints = {} }) {
  const family = familyOf(client)
  const trimmed = line.trimStart()
  const none = { items: [], token: '' }
  if (!trimmed) return none

  // A whole command in another dialect: offer this client's spelling.
  if (!statement) {
    for (const [re, to, why, keys] of TRANSLATIONS[family] ?? []) {
      const m = trimmed.match(re)
      if (!m) continue
      const insert = to.replace('$1', m[1] ?? '')
      return { items: [{ label: insert, insert, detail: why, kind: 'translate', replaceLine: true, run: true, keys }], token: '' }
    }
  }

  const tables = hints.tables ?? []
  const tableItems = (/** @type {string} */ typed) =>
    tables
      .filter((t) => t.toLowerCase().startsWith(typed.toLowerCase()))
      .map((t) => /** @type {Suggestion} */ ({
        label: t, insert: sqlName(t, family), detail: 'table', kind: 'table', exact: t.toLowerCase() === typed.toLowerCase(),
      }))

  // The client's own commands: psql's \x, sqlite3's .x, the first word in redis-cli.
  const metaList = family === 'psql' ? PSQL_META : family === 'sqlite3' ? SQLITE_DOT : null
  const metaLead = family === 'psql' ? '\\' : '.'
  if (metaList && !statement && trimmed.startsWith(metaLead)) {
    const space = trimmed.search(/\s/)
    if (space === -1) return finish(fromList(metaList, trimmed, 'meta'), trimmed)
    const cmd = trimmed.slice(0, space)
    const argCmds = family === 'psql' ? PSQL_TABLE_ARG : SQLITE_TABLE_ARG
    if (!argCmds.has(cmd)) return none
    const arg = trimmed.slice(space).trimStart()
    if (/\s/.test(arg) || !arg) return none
    return finish(tableItems(arg), arg)
  }
  if (family === 'redis-cli') {
    if (/\s/.test(trimmed)) return none
    return finish(fromList(REDIS_COMMANDS, trimmed, 'command'), trimmed)
  }
  if (family === 'sqlcmd' && !statement && /^go$/i.test(trimmed)) return none

  // SQL. Nothing inside a string literal.
  const before = `${statement}\n${line}`
  if ((before.match(/'/g)?.length ?? 0) % 2 === 1) return none
  const token = line.match(/[\w$.]*$/)?.[0] ?? ''
  if (!token || /^\d/.test(token)) return none
  const lower = token.toLowerCase()
  const upperCase = token !== lower

  // The statement's first word: only what can start a statement.
  if (!statement && trimmed === token) {
    if (token.length < MIN_WORD) return none
    const words = [...START_KEYWORDS, ...(START_EXTRA[family] ?? [])]
    /** @type {Suggestion[]} */
    const items = words
      .filter((w) => w.toLowerCase().startsWith(lower))
      .map((w) => {
        const text = upperCase ? w : w.toLowerCase()
        return { label: text, insert: text, kind: /** @type {const} */ ('keyword'), exact: w.toLowerCase() === lower }
      })
    if (family === 'mysql') {
      items.push(...fromList(MYSQL_STATEMENTS, token, 'command').map((i) => (upperCase ? i : { ...i, label: i.label.toLowerCase(), insert: i.insert.toLowerCase() })))
    }
    return finish(items, token)
  }
  // Past the first word, only a line that is SQL gets SQL suggestions.
  const firstWord = before.trimStart().match(/^[A-Za-z_]+/)?.[0]?.toLowerCase() ?? ''
  if (!SQL_STARTERS.has(firstWord)) return none

  // `alias.` or `table.`: that table's columns.
  const dot = token.lastIndexOf('.')
  if (dot !== -1) {
    const qualifier = token.slice(0, dot).split('.').pop()?.toLowerCase() ?? ''
    const part = token.slice(dot + 1)
    const { aliasMap } = analyzeQuery(before, tables)
    const table = aliasMap[qualifier] ?? qualifier
    const items = columnsOf(hints, table)
      .filter((c) => c.name.toLowerCase().startsWith(part.toLowerCase()))
      .map((c) => /** @type {Suggestion} */ ({
        label: c.name, insert: sqlName(c.name, family), detail: c.type || 'column', kind: 'column',
        exact: c.name.toLowerCase() === part.toLowerCase(),
      }))
    return finish(items, part)
  }

  // Context from the text before the word, or a half-typed `Or` reads as OR.
  const { kind, referencedTables } = analyzeQuery(before.slice(0, before.length - token.length), tables)
  /** @type {Suggestion[]} */
  const items = []
  if (kind === 'table') {
    items.push(...tableItems(token))
  } else {
    if (kind === 'column') {
      for (const t of referencedTables) {
        for (const c of columnsOf(hints, t)) {
          if (c.name.toLowerCase().startsWith(lower)) {
            items.push({ label: c.name, insert: sqlName(c.name, family), detail: c.type || t, kind: 'column', exact: c.name.toLowerCase() === lower })
          }
        }
      }
    } else {
      items.push(...tableItems(token))
    }
    if (token.length >= MIN_WORD) {
      const dialect = family === 'psql' ? 'postgres' : family === 'sqlite3' ? 'sqlite' : family === 'sqlcmd' ? 'mssql' : family
      for (const kw of new Set([...PG_KEYWORDS, ...(DIALECT_KEYWORDS[dialect] ?? [])])) {
        if (kw.toLowerCase().startsWith(lower)) {
          const text = upperCase ? kw : kw.toLowerCase()
          items.push({ label: text, insert: text, kind: 'keyword', exact: kw.toLowerCase() === lower })
        }
      }
      // Functions belong in expressions: the select list, WHERE, and so on.
      if (family === 'psql' && kind === 'column') {
        for (const fn of PG_FUNCTIONS) {
          if (fn.label.startsWith(lower)) {
            items.push({ label: `${fn.label}()`, insert: `${fn.label}(`, detail: 'function', kind: 'function', exact: fn.label === lower })
          }
        }
      }
    }
  }
  return finish(items, token)
}

/**
 * The keystrokes that turn the typed `token` into `item`: the rest of the word
 * when it only extends what was typed, otherwise erase the word and type it
 * whole. A whole-line replacement goes to the end of the line, clears it
 * (Ctrl+E, Ctrl+U: readline, editline and linenoise all know both) and types
 * the new line.
 * @param {Suggestion} item
 * @param {string} token
 */
export function keystrokesFor(item, token) {
  if (item.keys) return item.keys
  if (item.replaceLine) return `\x05\x15${item.insert}${item.run ? '\r' : ''}`
  if (item.insert.startsWith(token)) return item.insert.slice(token.length)
  return '\x7f'.repeat(token.length) + item.insert
}
