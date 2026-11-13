/**
 * Suggestions for the terminal tab: what to offer for the line being typed at
 * the client's prompt, drawn on top of the real client.
 *
 * The client keeps its own line editing (and psql its own Tab completion); this
 * only reads what has been typed and proposes the rest, and the page types the
 * accepted text into the client like a user would, in the keys its line editor
 * takes (terminal-keys.js). Three sources:
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
import { clearLineKeys, consoleClearCommand } from '$lib/terminal-keys.js'

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

/**
 * Every psql backslash command (psql 16's `\\?`), the most used first so the
 * short list opens on them. The third entry is what the argument is, for
 * completing it: `rel` a table/view/index name, `onoff`, `xmode`, `pset`,
 * `help` (`\\?` topics), `sqlhelp` (`\\h` SQL commands).
 * @type {Array<[string, string, string?]>}
 */
export const PSQL_META_FULL = [
  ['\\dt', 'List tables', 'rel'], ['\\d', 'Describe a table, view, sequence or index', 'rel'],
  ['\\d+', 'Describe, with storage and comments', 'rel'], ['\\dt+', 'List tables with sizes', 'rel'],
  ['\\l', 'List databases'], ['\\l+', 'List databases with sizes'], ['\\c', 'Connect to another database'],
  ['\\dn', 'List schemas'], ['\\dv', 'List views', 'rel'], ['\\di', 'List indexes', 'rel'],
  ['\\df', 'List functions'], ['\\du', 'List roles'], ['\\dx', 'List extensions'],
  ['\\x', 'Toggle expanded output', 'xmode'], ['\\timing', 'Toggle query timing', 'onoff'],
  ['\\conninfo', 'Show the current connection'], ['\\q', 'Quit psql'],
  ['\\?', 'Help on psql commands', 'help'], ['\\h', 'Help on an SQL command', 'sqlhelp'],
  ['\\dm', 'List materialized views', 'rel'], ['\\ds', 'List sequences', 'rel'], ['\\dT', 'List data types'],
  ['\\dp', 'List access privileges', 'rel'], ['\\z', 'Access privileges, same as \\dp', 'rel'],
  ['\\sf', 'Show a function\'s definition'], ['\\sv', 'Show a view\'s definition', 'rel'],
  ['\\e', 'Edit the query buffer in $EDITOR'], ['\\ef', 'Edit a function in $EDITOR'], ['\\ev', 'Edit a view in $EDITOR', 'rel'],
  ['\\g', 'Run the query buffer (same as ;)'], ['\\gx', 'Run the query buffer, expanded'],
  ['\\gdesc', 'Describe the query\'s result without running it'], ['\\gexec', 'Run the query, then run each value it returns'],
  ['\\gset', 'Run the query, store the result in variables'], ['\\watch', 'Run the query again every N seconds'],
  ['\\crosstabview', 'Show the result as a crosstab'], ['\\errverbose', 'Last error, at full verbosity'],
  ['\\p', 'Show the query buffer'], ['\\r', 'Reset the query buffer'], ['\\s', 'Show or save command history'],
  ['\\w', 'Write the query buffer to a file'],
  ['\\copy', 'Copy between a table and a local file', 'rel'], ['\\echo', 'Print text'], ['\\i', 'Run commands from a file'],
  ['\\ir', 'Run a file relative to this script'], ['\\o', 'Send results to a file or |pipe'],
  ['\\qecho', 'Print text to the \\o stream'], ['\\warn', 'Print text to stderr'],
  ['\\if', 'Begin a conditional block'], ['\\elif', 'Alternative in a conditional block'],
  ['\\else', 'Final alternative in a conditional block'], ['\\endif', 'End a conditional block'],
  ['\\da', 'List aggregates'], ['\\dA', 'List access methods'], ['\\dAc', 'List operator classes'],
  ['\\dAf', 'List operator families'], ['\\dAo', 'List operators of operator families'],
  ['\\dAp', 'List support functions of operator families'], ['\\db', 'List tablespaces'],
  ['\\dc', 'List conversions'], ['\\dC', 'List casts'], ['\\dd', 'Object descriptions not shown elsewhere'],
  ['\\dD', 'List domains'], ['\\ddp', 'List default privileges'], ['\\dE', 'List foreign tables', 'rel'],
  ['\\des', 'List foreign servers'], ['\\det', 'List foreign tables'], ['\\deu', 'List user mappings'],
  ['\\dew', 'List foreign-data wrappers'], ['\\dF', 'List text search configurations'],
  ['\\dFd', 'List text search dictionaries'], ['\\dFp', 'List text search parsers'],
  ['\\dFt', 'List text search templates'], ['\\dg', 'List roles'], ['\\dl', 'List large objects'],
  ['\\dL', 'List procedural languages'], ['\\do', 'List operators'], ['\\dO', 'List collations'],
  ['\\dP', 'List partitioned relations', 'rel'], ['\\drds', 'List per-database role settings'],
  ['\\dRp', 'List replication publications'], ['\\dRs', 'List replication subscriptions'],
  ['\\dX', 'List extended statistics'], ['\\dy', 'List event triggers'],
  ['\\a', 'Toggle aligned / unaligned output'], ['\\C', 'Set the table title'],
  ['\\f', 'Field separator for unaligned output'], ['\\H', 'Toggle HTML output'],
  ['\\pset', 'Set a table output option', 'pset'], ['\\t', 'Show only rows', 'onoff'],
  ['\\T', 'HTML <table> attributes'],
  ['\\encoding', 'Show or set the client encoding'], ['\\password', 'Change a user\'s password'],
  ['\\cd', 'Change the working directory'], ['\\setenv', 'Set an environment variable'],
  ['\\!', 'Run a shell command'], ['\\prompt', 'Ask for a variable\'s value'],
  ['\\set', 'Set a variable, or list them all'], ['\\unset', 'Unset a variable'],
  ['\\lo_export', 'Export a large object'], ['\\lo_import', 'Import a large object'],
  ['\\lo_list', 'List large objects'], ['\\lo_unlink', 'Delete a large object'],
  ['\\copyright', 'PostgreSQL usage and distribution terms'],
]
/** @type {Array<[string, string]>} */
export const PSQL_META = PSQL_META_FULL.map(([c, d]) => [c, d])
const PSQL_ARG = new Map(PSQL_META_FULL.filter((m) => m[2]).map((m) => [m[0], /** @type {string} */ (m[2])]))

/** `\\dtS+` → `\\dt`: psql's S (system objects) and + (more detail) modifiers. @param {string} cmd */
function psqlBase(cmd) {
  if (PSQL_ARG.has(cmd)) return cmd
  let c = cmd.replace(/\+$/, '')
  if (PSQL_ARG.has(c)) return c
  if (c.length > 2 && c.endsWith('S')) c = c.slice(0, -1)
  return c
}

/** Fixed argument values for psql commands. @type {Record<string, Array<[string, string]>>} */
const PSQL_ARG_VALUES = {
  onoff: [['on', 'Turn on'], ['off', 'Turn off']],
  xmode: [['on', 'Always expanded'], ['off', 'Never expanded'], ['auto', 'Expanded when a row is too wide']],
  help: [['commands', 'Backslash commands'], ['options', 'Command-line options'], ['variables', 'Special variables']],
  pset: [
    ['format', 'aligned, unaligned, wrapped, csv, html, ...'], ['border', '0, 1 or 2'], ['expanded', 'on, off or auto'],
    ['null', 'How NULL prints'], ['linestyle', 'ascii, old-ascii or unicode'], ['pager', 'on, off or always'],
    ['title', 'Table title'], ['tuples_only', 'Rows only'], ['footer', 'Row-count footer'], ['columns', 'Width for wrapped'],
    ['fieldsep', 'Unaligned field separator'], ['recordsep', 'Unaligned record separator'], ['csv_fieldsep', 'CSV separator'],
    ['numericlocale', 'Locale digit grouping'], ['pager_min_lines', 'Lines before paging'], ['tableattr', 'HTML attributes'],
    ['unicode_border_linestyle', 'single or double'], ['unicode_column_linestyle', 'single or double'],
    ['unicode_header_linestyle', 'single or double'], ['fieldsep_zero', 'Zero-byte field separator'], ['recordsep_zero', 'Zero-byte record separator'],
  ],
  sqlhelp: ['SELECT', 'INSERT', 'UPDATE', 'DELETE', 'CREATE TABLE', 'CREATE INDEX', 'CREATE VIEW', 'ALTER TABLE', 'DROP TABLE',
    'TRUNCATE', 'EXPLAIN', 'COPY', 'GRANT', 'REVOKE', 'VACUUM', 'ANALYZE', 'BEGIN', 'COMMIT', 'ROLLBACK', 'WITH', 'MERGE',
    'CREATE FUNCTION', 'CREATE EXTENSION', 'CREATE SCHEMA', 'CREATE ROLE', 'ALTER ROLE', 'SET', 'SHOW', 'REINDEX', 'CLUSTER',
  ].map((c) => /** @type {[string, string]} */ ([c, 'SQL syntax'])),
}

/**
 * The mysql client's own commands (`help` in mysql 8), long and short forms.
 * @type {Array<[string, string]>}
 */
const MYSQL_CLIENT = [
  ['\\G', 'Run the statement, one column per line'], ['\\g', 'Run the statement'], ['\\s', 'Connection status'],
  ['\\u', 'Use another database'], ['\\.', 'Run a SQL file'], ['\\c', 'Clear the current statement'],
  ['\\q', 'Quit'], ['\\h', 'Help'], ['\\e', 'Edit the statement in $EDITOR'], ['\\p', 'Print the statement'],
  ['\\P', 'Set a pager'], ['\\n', 'Turn the pager off'], ['\\T', 'Tee output to a file'], ['\\t', 'Stop the tee'],
  ['\\W', 'Show warnings after each statement'], ['\\w', 'Stop showing warnings'], ['\\R', 'Change the prompt'],
  ['\\#', 'Rebuild completion hash'], ['\\!', 'Run a shell command'], ['\\C', 'Switch character set'],
  ['\\d', 'Set the statement delimiter'], ['\\r', 'Reconnect'], ['\\x', 'Reset the session'],
]
/** Client words that start a line instead of SQL. @type {Array<[string, string]>} */
const MYSQL_CLIENT_WORDS = [
  ['status', 'Connection status (\\s)'], ['source', 'Run a SQL file (\\.)'], ['delimiter', 'Set the statement delimiter'],
  ['help', 'Client help (\\h)'], ['quit', 'Quit (\\q)'], ['exit', 'Quit'], ['system', 'Run a shell command (\\!)'],
  ['tee', 'Tee output to a file'], ['notee', 'Stop the tee'], ['pager', 'Set a pager'], ['nopager', 'Turn the pager off'],
  ['warnings', 'Show warnings after each statement'], ['nowarning', 'Stop showing warnings'], ['rehash', 'Rebuild completion'],
  ['connect', 'Reconnect'], ['charset', 'Switch character set'], ['prompt', 'Change the prompt'], ['edit', 'Edit in $EDITOR'],
  ['ego', 'Run, one column per line (\\G)'], ['go', 'Run the statement (\\g)'], ['print', 'Print the statement'],
  ['resetconnection', 'Reset the session'],
]
/** MySQL statements whose next word is a table. */
const MYSQL_TABLE_AFTER = /\b(?:describe|desc|explain|show\s+(?:full\s+)?(?:columns|fields|index|indexes|keys)\s+(?:from|in)|show\s+create\s+table|show\s+table\s+status\s+like|truncate(?:\s+table)?|optimize\s+table|analyze\s+table|check\s+table|repair\s+table|lock\s+tables?)\s+$/i

/**
 * sqlite3's dot-commands (3.4x `.help`), the most used first. The third entry
 * is the argument: `rel` a table, `onoff`, `mode`, `nullv` (`.nullvalue`).
 * @type {Array<[string, string, string?]>}
 */
const SQLITE_DOT_FULL = [
  ['.tables', 'List tables', 'rel'], ['.schema', 'Show CREATE statements', 'rel'], ['.indexes', 'List indexes', 'rel'],
  ['.mode', 'Output mode: box, table, csv, json, line, ...', 'mode'], ['.headers', 'Column headers on or off', 'onoff'],
  ['.timer', 'Time each statement', 'onoff'], ['.databases', 'List attached databases'],
  ['.dump', 'Dump the database (or a table) as SQL', 'rel'], ['.import', 'Import a file into a table'],
  ['.output', 'Send output to a file'], ['.once', 'Send the next output to a file'], ['.read', 'Run SQL from a file'],
  ['.show', 'Show the current settings'], ['.help', 'Help'], ['.quit', 'Quit sqlite3'], ['.exit', 'Quit sqlite3'],
  ['.width', 'Column widths for column modes'], ['.nullvalue', 'How NULL prints'], ['.separator', 'Column and row separators'],
  ['.changes', 'Show rows changed by each statement', 'onoff'], ['.echo', 'Echo each command', 'onoff'],
  ['.bail', 'Stop at the first error', 'onoff'], ['.eqp', 'Show the query plan of each statement', 'eqp'],
  ['.expert', 'Suggest indexes for queries'], ['.fullschema', 'Schema plus sqlite_stat tables'],
  ['.lint', 'Report schema issues'], ['.dbinfo', 'Database file information'], ['.dbconfig', 'List or change sqlite3_db_config()'],
  ['.open', 'Close this database and open a file'], ['.save', 'Write the database to a file'], ['.backup', 'Back up to a file'],
  ['.restore', 'Restore from a file'], ['.clone', 'Clone into a new database'], ['.archive', 'Manage SQL archives'],
  ['.recover', 'Recover data from a corrupt database'], ['.excel', 'Open the next output in a spreadsheet'],
  ['.print', 'Print text'], ['.prompt', 'Change the prompts'], ['.shell', 'Run a shell command'], ['.system', 'Run a shell command'],
  ['.cd', 'Change the working directory'], ['.log', 'Turn logging on or off'], ['.stats', 'Show statistics', 'onoff'],
  ['.scanstats', 'Show scan statistics', 'onoff'], ['.trace', 'Trace each statement'], ['.timeout', 'Retry locked tables for N ms'],
  ['.load', 'Load an extension library'], ['.limit', 'Show or change SQLite limits'], ['.parameter', 'Manage bound parameters'],
  ['.progress', 'Progress handler'], ['.selftest', 'Run self-tests'], ['.sha3sum', 'Hash the database content'],
  ['.testcase', 'Begin a test case'], ['.vfsinfo', 'VFS information'], ['.vfslist', 'List VFSes'], ['.vfsname', 'VFS name'],
  ['.auth', 'Show authorizer callbacks', 'onoff'], ['.binary', 'Binary output', 'onoff'], ['.check', 'Check output against a pattern'],
  ['.connection', 'Open or close an auxiliary connection'], ['.filectrl', 'Run sqlite3_file_control()'],
  ['.iotrace', 'Trace I/O'], ['.imposter', 'Create an imposter table'], ['.nonce', 'Suspend safe mode for one command'],
  ['.unmodule', 'Unregister virtual table modules'], ['.crnl', 'CR/NL line endings', 'onoff'],
]
/** @type {Array<[string, string]>} */
export const SQLITE_DOT = SQLITE_DOT_FULL.map(([c, d]) => [c, d])
const SQLITE_ARG = new Map(SQLITE_DOT_FULL.filter((m) => m[2]).map((m) => [m[0], /** @type {string} */ (m[2])]))
/** @type {Record<string, Array<[string, string]>>} */
const SQLITE_ARG_VALUES = {
  onoff: [['on', 'Turn on'], ['off', 'Turn off']],
  eqp: [['on', 'Show the plan'], ['off', 'Hide it'], ['full', 'Plan and bytecode'], ['trigger', 'Plans inside triggers too']],
  mode: [
    ['box', 'Unicode box table'], ['table', 'ASCII table'], ['column', 'Aligned columns'], ['markdown', 'Markdown table'],
    ['csv', 'Comma-separated'], ['json', 'JSON array'], ['line', 'One value per line'], ['list', 'Separator-delimited'],
    ['tabs', 'Tab-separated'], ['html', 'HTML <table>'], ['insert', 'INSERT statements'], ['quote', 'SQL literals'],
    ['ascii', 'ASCII unit/record separators'], ['tcl', 'TCL list'], ['qbox', 'Box with quoted values'],
  ],
}

/**
 * sqlcmd's own commands (a line starting with `:`, plus GO and !!).
 * `:r` and `:out` take files; `:on error` takes exit|ignore.
 * @type {Array<[string, string]>}
 */
const SQLCMD_COMMANDS = [
  ['GO', 'Run the batch'], [':r', 'Run a .sql file'], [':setvar', 'Set a sqlcmd variable'], [':listvar', 'List variables'],
  [':connect', 'Connect to another server'], [':on error', 'exit or ignore on error'], [':out', 'Send results to a file'],
  [':error', 'Send errors to a file'], [':reset', 'Clear the batch'], [':ed', 'Edit the batch in an editor'],
  [':serverlist', 'List local servers'], [':xml', 'XML output on or off'], [':perftrace', 'Send timing to a file'],
  [':help', 'List sqlcmd commands'], [':quit', 'Quit sqlcmd'], [':exit', 'Quit, optionally with a query\'s value'],
  ['!!', 'Run a shell command'],
]
/** The usual catalog procedures, typed as a statement's first word. @type {Array<[string, string]>} */
const MSSQL_STATEMENTS = [
  ['sp_help', 'Describe an object'], ['sp_helpindex', 'Indexes of a table'], ['sp_columns', 'Columns of a table'],
  ['sp_helptext', 'Definition of a view, procedure or function'], ['sp_tables', 'List tables'], ['sp_who', 'Sessions'],
  ['sp_who2', 'Sessions, more detail'], ['sp_spaceused', 'Size of a table or the database'], ['sp_databases', 'List databases'],
  ['sp_helpdb', 'Database information'], ['sp_lock', 'Locks held'], ['sp_configure', 'Server settings'],
  ['SELECT name FROM sys.tables;', 'List tables'], ['SELECT name FROM sys.databases;', 'List databases'],
  ['SELECT @@VERSION;', 'Server version'], ['SELECT DB_NAME();', 'Current database'],
]
/** sqlcmd: words after which a table name goes. */
const MSSQL_TABLE_AFTER = /\b(?:(?:exec(?:ute)?\s+)?sp_(?:help|helpindex|columns|helptext|spaceused|helpconstraint|helptrigger|depends|fkeys|pkeys|statistics)|truncate\s+table|update\s+statistics|dbcc\s+checktable\s*\(?)\s*'?$/i

/** @type {Array<[string, string]>} */
const MYSQL_STATEMENTS = [
  ['SHOW DATABASES;', 'List databases'], ['SHOW TABLES;', 'List tables'],
  ['SHOW COLUMNS FROM', 'List a table\'s columns'], ['SHOW CREATE TABLE', 'Show a table\'s DDL'],
  ['SHOW PROCESSLIST;', 'List running sessions'], ['SHOW VARIABLES LIKE', 'Find a server setting'],
  ['SHOW INDEX FROM', 'List a table\'s indexes'], ['SHOW GRANTS;', 'Your privileges'],
  ['DESCRIBE', 'Describe a table'], ['USE', 'Switch database'],
  ['SHOW TABLE STATUS;', 'Tables with rows and sizes'], ['SHOW FULL PROCESSLIST;', 'Running sessions with full queries'],
  ['SHOW WARNINGS;', 'Warnings from the last statement'], ['SHOW ERRORS;', 'Errors from the last statement'],
  ['SHOW STATUS LIKE', 'Server status counters'], ['SHOW ENGINE INNODB STATUS\\G', 'InnoDB internals'],
  ['SHOW TRIGGERS;', 'List triggers'], ['SHOW EVENTS;', 'List scheduled events'],
  ['SHOW FUNCTION STATUS;', 'List functions'], ['SHOW PROCEDURE STATUS;', 'List procedures'],
  ['SELECT DATABASE();', 'Current database'], ['SELECT VERSION();', 'Server version'],
]

/** redis-cli: commands, most used first. @type {Array<[string, string]>} */
const REDIS_COMMANDS = [
  ['GET', 'Value of a key'], ['SET', 'Set a key'], ['DEL', 'Delete keys'], ['EXISTS', 'Whether keys exist'],
  ['TYPE', 'Type of a key'], ['TTL', 'Seconds until a key expires'], ['EXPIRE', 'Expire a key after seconds'],
  ['SCAN', 'Iterate keys'], ['KEYS', 'Keys matching a pattern (blocks; prefer SCAN)'], ['HGETALL', 'Every field of a hash'],
  ['HGET', 'One field of a hash'], ['HSET', 'Set hash fields'], ['LRANGE', 'A range of a list'], ['SMEMBERS', 'Every member of a set'],
  ['ZRANGE', 'A range of a sorted set'], ['INFO', 'Server information'], ['DBSIZE', 'Number of keys'], ['SELECT', 'Switch logical database'],
  ['PING', 'Check the connection'], ['CLIENT', 'Client connections: LIST, KILL, SETNAME, ...'], ['CONFIG', 'Server config: GET, SET, ...'],
  ['MEMORY', 'Memory: USAGE, STATS, DOCTOR'], ['OBJECT', 'Key internals: ENCODING, IDLETIME, FREQ'],
  ['MGET', 'Values of several keys'], ['MSET', 'Set several keys'], ['SETNX', 'Set if it does not exist'], ['SETEX', 'Set with a TTL'],
  ['GETSET', 'Set and return the old value'], ['GETDEL', 'Get and delete'], ['GETEX', 'Get and set the TTL'], ['APPEND', 'Append to a string'],
  ['STRLEN', 'Length of a string'], ['INCR', 'Increment a counter'], ['INCRBY', 'Increment by N'], ['INCRBYFLOAT', 'Increment by a float'],
  ['DECR', 'Decrement a counter'], ['DECRBY', 'Decrement by N'], ['GETRANGE', 'Part of a string'], ['SETRANGE', 'Overwrite part of a string'],
  ['PEXPIRE', 'Expire after milliseconds'], ['EXPIREAT', 'Expire at a Unix time'], ['PTTL', 'Milliseconds until expiry'],
  ['PERSIST', 'Remove the expiry'], ['EXPIRETIME', 'Unix time it expires'], ['RENAME', 'Rename a key'], ['RENAMENX', 'Rename if the new name is free'],
  ['COPY', 'Copy a key'], ['MOVE', 'Move a key to another db'], ['UNLINK', 'Delete keys in the background'], ['DUMP', 'Serialized value'],
  ['RESTORE', 'Create a key from DUMP'], ['TOUCH', 'Update last access'], ['RANDOMKEY', 'A random key'], ['SORT', 'Sort a list, set or zset'],
  ['HMGET', 'Several hash fields'], ['HDEL', 'Delete hash fields'], ['HEXISTS', 'Whether a hash field exists'], ['HKEYS', 'Hash field names'],
  ['HVALS', 'Hash values'], ['HLEN', 'Number of hash fields'], ['HINCRBY', 'Increment a hash field'], ['HSETNX', 'Set a hash field if absent'],
  ['HSCAN', 'Iterate hash fields'], ['HRANDFIELD', 'Random hash fields'], ['HSTRLEN', 'Length of a hash value'],
  ['LPUSH', 'Push onto the head'], ['RPUSH', 'Push onto the tail'], ['LPOP', 'Pop from the head'], ['RPOP', 'Pop from the tail'],
  ['LLEN', 'Length of a list'], ['LINDEX', 'Element by index'], ['LSET', 'Set an element'], ['LREM', 'Remove elements'],
  ['LTRIM', 'Trim to a range'], ['LINSERT', 'Insert before or after'], ['LMOVE', 'Move between lists'], ['LPOS', 'Index of an element'],
  ['BLPOP', 'Blocking pop from the head'], ['BRPOP', 'Blocking pop from the tail'],
  ['SADD', 'Add to a set'], ['SREM', 'Remove from a set'], ['SCARD', 'Size of a set'], ['SISMEMBER', 'Whether a member is in a set'],
  ['SMISMEMBER', 'Membership of several'], ['SPOP', 'Pop random members'], ['SRANDMEMBER', 'Random members'], ['SINTER', 'Intersection'],
  ['SUNION', 'Union'], ['SDIFF', 'Difference'], ['SINTERSTORE', 'Store an intersection'], ['SUNIONSTORE', 'Store a union'],
  ['SDIFFSTORE', 'Store a difference'], ['SMOVE', 'Move a member'], ['SSCAN', 'Iterate a set'],
  ['ZADD', 'Add to a sorted set'], ['ZREM', 'Remove from a sorted set'], ['ZSCORE', 'Score of a member'], ['ZINCRBY', 'Increment a score'],
  ['ZCARD', 'Size of a sorted set'], ['ZCOUNT', 'Members in a score range'], ['ZRANK', 'Rank of a member'], ['ZREVRANK', 'Rank, high to low'],
  ['ZRANGEBYSCORE', 'Members by score'], ['ZREVRANGE', 'Range, high to low'], ['ZPOPMIN', 'Pop the lowest'], ['ZPOPMAX', 'Pop the highest'],
  ['ZRANGESTORE', 'Store a range'], ['ZREMRANGEBYSCORE', 'Remove by score'], ['ZREMRANGEBYRANK', 'Remove by rank'], ['ZSCAN', 'Iterate a sorted set'],
  ['ZMSCORE', 'Scores of several'], ['ZUNIONSTORE', 'Store a union'], ['ZINTERSTORE', 'Store an intersection'],
  ['XADD', 'Append to a stream'], ['XRANGE', 'A range of a stream'], ['XREVRANGE', 'Range, newest first'], ['XLEN', 'Length of a stream'],
  ['XREAD', 'Read from streams'], ['XREADGROUP', 'Read as a consumer group'], ['XGROUP', 'Manage consumer groups'], ['XACK', 'Acknowledge'],
  ['XPENDING', 'Pending entries'], ['XCLAIM', 'Claim entries'], ['XAUTOCLAIM', 'Claim idle entries'], ['XDEL', 'Delete entries'],
  ['XTRIM', 'Trim a stream'], ['XINFO', 'Stream information'],
  ['PFADD', 'Add to a HyperLogLog'], ['PFCOUNT', 'Approximate cardinality'], ['PFMERGE', 'Merge HyperLogLogs'],
  ['GEOADD', 'Add locations'], ['GEOSEARCH', 'Search by area'], ['GEODIST', 'Distance between members'], ['GEOPOS', 'Positions'],
  ['SETBIT', 'Set a bit'], ['GETBIT', 'Get a bit'], ['BITCOUNT', 'Count set bits'], ['BITOP', 'Bitwise operation'], ['BITPOS', 'First set bit'],
  ['PUBLISH', 'Publish to a channel'], ['SUBSCRIBE', 'Listen to channels'], ['PSUBSCRIBE', 'Listen to patterns'], ['PUBSUB', 'Pub/sub introspection'],
  ['MULTI', 'Start a transaction'], ['EXEC', 'Run the transaction'], ['DISCARD', 'Drop the transaction'], ['WATCH', 'Watch keys'], ['UNWATCH', 'Stop watching'],
  ['EVAL', 'Run a Lua script'], ['EVALSHA', 'Run a cached script'], ['SCRIPT', 'Script cache: LOAD, EXISTS, FLUSH'], ['FUNCTION', 'Functions: LIST, LOAD, ...'], ['FCALL', 'Call a function'],
  ['SLOWLOG', 'Slow queries: GET, LEN, RESET'], ['LATENCY', 'Latency monitoring'], ['MONITOR', 'Stream every command (debug)'],
  ['COMMAND', 'Command introspection'], ['ACL', 'Access control: WHOAMI, LIST, USERS, ...'], ['AUTH', 'Authenticate'], ['HELLO', 'Handshake'],
  ['TIME', 'Server time'], ['LASTSAVE', 'Last save time'], ['SAVE', 'Save to disk (blocks)'], ['BGSAVE', 'Save in the background'],
  ['BGREWRITEAOF', 'Rewrite the AOF'], ['ROLE', 'Replication role'], ['REPLICAOF', 'Replicate another server'], ['CLUSTER', 'Cluster commands'],
  ['WAIT', 'Wait for replicas'], ['SWAPDB', 'Swap two databases'], ['FLUSHDB', 'Delete every key in this db'], ['FLUSHALL', 'Delete every key everywhere'],
  ['DEBUG', 'Debugging commands'], ['QUIT', 'Close the connection'], ['ECHO', 'Echo a message'], ['RESET', 'Reset the connection'],
]
/** Subcommands, offered after the command. @type {Record<string, Array<[string, string]>>} */
const REDIS_SUB = {
  CLIENT: [['LIST', 'Connected clients'], ['INFO', 'This connection'], ['KILL', 'Close a connection'], ['SETNAME', 'Name this connection'],
    ['GETNAME', 'This connection\'s name'], ['ID', 'This connection\'s id'], ['PAUSE', 'Pause clients'], ['UNPAUSE', 'Resume clients'],
    ['NO-EVICT', 'Exempt from eviction'], ['TRACKING', 'Client-side caching'], ['REPLY', 'ON, OFF or SKIP']],
  CONFIG: [['GET', 'Read a setting'], ['SET', 'Change a setting'], ['RESETSTAT', 'Reset INFO counters'], ['REWRITE', 'Write redis.conf']],
  MEMORY: [['USAGE', 'Bytes a key uses'], ['STATS', 'Memory breakdown'], ['DOCTOR', 'Memory advice'], ['MALLOC-STATS', 'Allocator stats'], ['PURGE', 'Release memory']],
  OBJECT: [['ENCODING', 'Internal encoding'], ['IDLETIME', 'Seconds since last access'], ['FREQ', 'LFU counter'], ['REFCOUNT', 'References']],
  SLOWLOG: [['GET', 'Slow queries'], ['LEN', 'How many'], ['RESET', 'Clear the log']],
  SCRIPT: [['LOAD', 'Cache a script'], ['EXISTS', 'Whether scripts are cached'], ['FLUSH', 'Clear the cache'], ['KILL', 'Stop the running script']],
  FUNCTION: [['LIST', 'List libraries'], ['LOAD', 'Load a library'], ['DELETE', 'Delete a library'], ['FLUSH', 'Delete all'], ['STATS', 'Running function'], ['DUMP', 'Serialize'], ['RESTORE', 'Restore']],
  ACL: [['WHOAMI', 'Current user'], ['LIST', 'Rules'], ['USERS', 'User names'], ['GETUSER', 'One user'], ['SETUSER', 'Create or change a user'],
    ['DELUSER', 'Delete users'], ['CAT', 'Command categories'], ['LOG', 'Denied commands'], ['GENPASS', 'Generate a password']],
  LATENCY: [['LATEST', 'Latest spikes'], ['HISTORY', 'History of an event'], ['DOCTOR', 'Advice'], ['RESET', 'Clear']],
  COMMAND: [['COUNT', 'Number of commands'], ['INFO', 'About commands'], ['DOCS', 'Documentation'], ['LIST', 'Command names'], ['GETKEYS', 'Keys a command touches']],
  PUBSUB: [['CHANNELS', 'Active channels'], ['NUMSUB', 'Subscribers per channel'], ['NUMPAT', 'Pattern subscriptions']],
  XINFO: [['STREAM', 'A stream'], ['GROUPS', 'Its consumer groups'], ['CONSUMERS', 'A group\'s consumers']],
  XGROUP: [['CREATE', 'Create a group'], ['DESTROY', 'Delete a group'], ['SETID', 'Move the last-delivered id'], ['CREATECONSUMER', 'Add a consumer'], ['DELCONSUMER', 'Remove a consumer']],
  CLUSTER: [['INFO', 'Cluster state'], ['NODES', 'Nodes'], ['SHARDS', 'Shards'], ['SLOTS', 'Slot ranges'], ['KEYSLOT', 'Slot of a key'], ['MYID', 'This node']],
  INFO: [['server', ''], ['clients', ''], ['memory', ''], ['persistence', ''], ['stats', ''], ['replication', ''], ['cpu', ''],
    ['commandstats', ''], ['latencystats', ''], ['keyspace', ''], ['errorstats', ''], ['cluster', ''], ['modules', ''], ['all', ''], ['everything', '']],
}
/** Options that may follow a command's fixed arguments. @type {Record<string, Array<[string, string]>>} */
const REDIS_OPTS = {
  SET: [['EX', 'Expire after seconds'], ['PX', 'Expire after milliseconds'], ['EXAT', 'Expire at a Unix time'], ['NX', 'Only if absent'], ['XX', 'Only if present'], ['KEEPTTL', 'Keep the TTL'], ['GET', 'Return the old value']],
  SCAN: [['MATCH', 'Pattern'], ['COUNT', 'Hint per call'], ['TYPE', 'Only this type']],
  HSCAN: [['MATCH', 'Pattern'], ['COUNT', 'Hint per call'], ['NOVALUES', 'Field names only']],
  SSCAN: [['MATCH', 'Pattern'], ['COUNT', 'Hint per call']],
  ZSCAN: [['MATCH', 'Pattern'], ['COUNT', 'Hint per call']],
  ZRANGE: [['BYSCORE', 'By score'], ['BYLEX', 'By member'], ['REV', 'High to low'], ['LIMIT', 'Offset and count'], ['WITHSCORES', 'Include scores']],
  ZADD: [['NX', 'Only new members'], ['XX', 'Only existing'], ['GT', 'Only raise scores'], ['LT', 'Only lower scores'], ['CH', 'Count changes'], ['INCR', 'Like ZINCRBY']],
  EXPIRE: [['NX', 'Only without a TTL'], ['XX', 'Only with a TTL'], ['GT', 'Only longer'], ['LT', 'Only shorter']],
  GETEX: [['EX', 'Seconds'], ['PX', 'Milliseconds'], ['EXAT', 'Unix time'], ['PERSIST', 'Remove the TTL']],
  COPY: [['DB', 'Into another db'], ['REPLACE', 'Overwrite']],
  XADD: [['NOMKSTREAM', 'Do not create'], ['MAXLEN', 'Cap the length'], ['MINID', 'Trim older ids']],
  'MEMORY USAGE': [['SAMPLES', 'Nested values to sample']],
  'CLIENT KILL': [['ID', 'By id'], ['ADDR', 'By address'], ['USER', 'By user'], ['SKIPME', 'yes or no']],
  'CLIENT LIST': [['TYPE', 'normal, master, replica, pubsub'], ['ID', 'By ids']],
}
/** Common CONFIG GET/SET parameters. */
const REDIS_CONFIG_PARAMS = ['maxmemory', 'maxmemory-policy', 'maxclients', 'timeout', 'save', 'appendonly', 'appendfsync',
  'databases', 'dir', 'dbfilename', 'bind', 'port', 'requirepass', 'notify-keyspace-events', 'slowlog-log-slower-than',
  'slowlog-max-len', 'latency-monitor-threshold', 'hz', 'lazyfree-lazy-eviction', 'loglevel', 'tcp-keepalive', '*']
  .map((p) => /** @type {[string, string]} */ ([p, 'setting']))
const REDIS_POLICIES = ['noeviction', 'allkeys-lru', 'allkeys-lfu', 'allkeys-random', 'volatile-lru', 'volatile-lfu', 'volatile-random', 'volatile-ttl']
  .map((p) => /** @type {[string, string]} */ ([p, 'maxmemory-policy']))

/** mongosh: what a line can start with. @type {Array<[string, string]>} */
const MONGO_START = [
  ['db.', 'This database'], ['show dbs', 'List databases'], ['show collections', 'List collections'], ['use', 'Switch database'],
  ['show users', 'List users'], ['show roles', 'List roles'], ['show profile', 'Slow operations'], ['show log', 'Server log'],
  ['it', 'Next page of results'], ['help', 'Shell help'], ['exit', 'Quit mongosh'], ['cls', 'Clear the screen'],
  ['rs.status()', 'Replica set status'], ['rs.conf()', 'Replica set config'], ['sh.status()', 'Sharding status'],
  ['printjson(', 'Print as JSON'], ['print(', 'Print'], ['load(', 'Run a script file'], ['sleep(', 'Wait N ms'], ['version()', 'Shell version'],
  ['ObjectId(', 'An ObjectId'], ['ISODate(', 'A date'], ['NumberLong(', 'A 64-bit integer'], ['NumberDecimal(', 'A decimal'], ['UUID(', 'A UUID'],
]
const MONGO_SHOW = [['dbs', 'Databases'], ['databases', 'Databases'], ['collections', 'Collections'], ['tables', 'Collections'],
  ['users', 'Users'], ['roles', 'Roles'], ['profile', 'Slow operations'], ['log', 'Server log'], ['logs', 'Log names']]
/** `db.` methods. @type {Array<[string, string]>} */
const MONGO_DB = [
  ['getCollectionNames()', 'Collection names'], ['getCollection(', 'A collection by name'], ['stats()', 'Database stats'],
  ['runCommand(', 'Run a command'], ['adminCommand(', 'Run an admin command'], ['createCollection(', 'Create a collection'],
  ['getName()', 'Database name'], ['getSiblingDB(', 'Another database'], ['aggregate(', 'Database-level aggregation'],
  ['currentOp()', 'Operations in progress'], ['killOp(', 'Stop an operation'], ['serverStatus()', 'Server status'], ['version()', 'Server version'],
  ['hello()', 'Topology'], ['getUsers()', 'Users'], ['createUser(', 'Create a user'], ['dropUser(', 'Delete a user'],
  ['getRoles()', 'Roles'], ['createView(', 'Create a view'], ['dropDatabase()', 'Delete this database'], ['getProfilingStatus()', 'Profiler setting'],
  ['setProfilingLevel(', 'Turn the profiler on or off'], ['fsyncLock()', 'Block writes'], ['fsyncUnlock()', 'Resume writes'],
  ['listCommands()', 'Available commands'], ['getCollectionInfos()', 'Collection details'], ['watch(', 'Change stream'],
]
/** `db.coll.` methods. @type {Array<[string, string]>} */
const MONGO_COLL = [
  ['find(', 'Documents matching a filter'], ['findOne(', 'The first match'], ['insertOne(', 'Insert a document'], ['insertMany(', 'Insert documents'],
  ['updateOne(', 'Update the first match'], ['updateMany(', 'Update every match'], ['replaceOne(', 'Replace a document'],
  ['deleteOne(', 'Delete the first match'], ['deleteMany(', 'Delete every match'], ['aggregate(', 'Run a pipeline'],
  ['countDocuments(', 'Count matches'], ['estimatedDocumentCount()', 'Fast approximate count'], ['distinct(', 'Distinct values of a field'],
  ['createIndex(', 'Create an index'], ['getIndexes()', 'List indexes'], ['dropIndex(', 'Drop an index'], ['dropIndexes()', 'Drop all indexes'],
  ['findOneAndUpdate(', 'Update and return'], ['findOneAndReplace(', 'Replace and return'], ['findOneAndDelete(', 'Delete and return'],
  ['bulkWrite(', 'Several writes at once'], ['stats()', 'Collection stats'], ['drop()', 'Delete the collection'],
  ['renameCollection(', 'Rename'], ['watch(', 'Change stream'], ['explain()', 'Explain the next operation'], ['validate()', 'Check the collection'],
  ['totalSize()', 'Data plus indexes'], ['storageSize()', 'Bytes on disk'], ['getShardDistribution()', 'Per-shard data'],
]
/** Cursor methods, after `find(...)` / `aggregate(...)`. @type {Array<[string, string]>} */
const MONGO_CURSOR = [
  ['sort(', 'Order'], ['limit(', 'At most N'], ['skip(', 'Skip N'], ['projection(', 'Fields to return'], ['toArray()', 'All results as an array'],
  ['count()', 'How many'], ['pretty()', 'Indented output'], ['explain(', 'Query plan'], ['hint(', 'Force an index'], ['batchSize(', 'Per batch'],
  ['forEach(', 'Run for each'], ['map(', 'Transform each'], ['hasNext()', 'More results?'], ['next()', 'The next document'], ['itcount()', 'Iterate and count'],
  ['collation(', 'String comparison rules'], ['maxTimeMS(', 'Time limit'], ['allowDiskUse()', 'Spill large sorts to disk'],
]
/** `$` operators: query, update and aggregation stages. @type {Array<[string, string]>} */
const MONGO_OPS = [
  ['$eq', 'Equal'], ['$ne', 'Not equal'], ['$gt', 'Greater than'], ['$gte', 'At least'], ['$lt', 'Less than'], ['$lte', 'At most'],
  ['$in', 'One of'], ['$nin', 'None of'], ['$and', 'All of'], ['$or', 'Any of'], ['$not', 'Not'], ['$nor', 'None of these'],
  ['$exists', 'Field present'], ['$type', 'BSON type'], ['$regex', 'Pattern'], ['$elemMatch', 'An array element matching'],
  ['$size', 'Array length'], ['$all', 'Array contains all'], ['$expr', 'Aggregation expression'], ['$text', 'Text search'],
  ['$set', 'Set fields'], ['$unset', 'Remove fields'], ['$inc', 'Increment'], ['$push', 'Append to an array'], ['$pull', 'Remove from an array'],
  ['$addToSet', 'Add if absent'], ['$pop', 'Remove first or last'], ['$rename', 'Rename a field'], ['$setOnInsert', 'Set on upsert insert'],
  ['$min', 'Lower to'], ['$max', 'Raise to'], ['$mul', 'Multiply'], ['$currentDate', 'Set to now'],
  ['$match', 'Stage: filter'], ['$group', 'Stage: group'], ['$project', 'Stage: shape'], ['$sort', 'Stage: order'], ['$limit', 'Stage: at most N'],
  ['$skip', 'Stage: skip N'], ['$lookup', 'Stage: join'], ['$unwind', 'Stage: one document per element'], ['$count', 'Stage: count'],
  ['$addFields', 'Stage: add fields'], ['$facet', 'Stage: several pipelines'], ['$sum', 'Accumulator: sum'], ['$avg', 'Accumulator: average'],
  ['$first', 'Accumulator: first'], ['$last', 'Accumulator: last'], ['$out', 'Stage: write a collection'], ['$merge', 'Stage: merge into a collection'],
]

/** redis-cli suggestions for one line. @param {string} line */
function redisSuggest(line) {
  const parts = line.trimStart().split(/\s+/)
  const word = parts[parts.length - 1]
  // Commands, subcommands and options print in capitals, as redis documents them.
  const cased = (/** @type {Suggestion[]} */ items, /** @type {string} */ _ref) => items
  if (parts.length === 1) return finish(cased(fromList(REDIS_COMMANDS, word, 'command'), word), word)
  const cmd = parts[0].toUpperCase()
  const sub = parts.length > 2 ? `${cmd} ${parts[1].toUpperCase()}` : ''
  // Subcommand: `client li` → LIST.
  if (parts.length === 2 && REDIS_SUB[cmd]) {
    const items = fromList(REDIS_SUB[cmd], word, 'command')
    return finish(cmd === 'INFO' ? items : cased(items, word || parts[0]), word)
  }
  // `config get maxm` / `config set maxmemory-policy all`.
  if (cmd === 'CONFIG' && /^(get|set)$/i.test(parts[1] ?? '')) {
    if (parts.length === 3) return finish(fromList(REDIS_CONFIG_PARAMS, word, 'command'), word)
    if (parts.length === 4 && /^maxmemory-policy$/i.test(parts[2])) return finish(fromList(REDIS_POLICIES, word, 'command'), word)
    return { items: [], token: '' }
  }
  const opts = REDIS_OPTS[sub] ?? REDIS_OPTS[cmd]
  // Options come after the fixed arguments; only offered on a word that has begun.
  if (opts && word && /^[a-z-]+$/i.test(word)) return finish(cased(fromList(opts, word, 'command'), word), word)
  return { items: [], token: '' }
}

/**
 * mongosh suggestions for one line.
 * @param {string} line @param {string[]} collections @param {SchemaHints} hints
 */
function mongoSuggest(line, collections, hints) {
  const t = line.trimStart()
  const none = { items: [], token: '' }
  /** @param {Array<[string, string]>} list @param {string} typed @param {Suggestion['kind']} kind */
  const from = (list, typed, kind = 'command') => finish(fromList(list, typed, kind), typed)
  if (/^show\s+\w*$/i.test(t)) { const w = t.split(/\s+/)[1] ?? ''; return from(MONGO_SHOW, w) }
  if (/^use\s+[\w-]*$/i.test(t)) {
    const w = t.split(/\s+/)[1] ?? ''
    return from((hints.schemas ?? []).map((d) => /** @type {[string, string]} */ ([d, 'database'])), w)
  }
  // db.getCollection('us / db.getSiblingDB('x
  const gc = /\bdb\.getCollection\(\s*['"]([\w.-]*)$/.exec(t)
  if (gc) return from(collections.map((c) => /** @type {[string, string]} */ ([c, 'collection'])), gc[1], 'table')
  // A `$` operator anywhere inside an argument.
  const op = /(\$\w*)$/.exec(t)
  if (op && /[({[,:]\s*['"]?\$\w*$/.test(t)) return from(MONGO_OPS, op[1])
  // Field names inside `{` of a call on db.coll: `db.users.find({ na`.
  const field = /\bdb\.(?:getCollection\(['"]([\w.-]+)['"]\)|([\w-]+))\.\w+\([^)]*[{,]\s*['"]?([\w.]*)$/.exec(t)
  if (field) {
    const coll = field[1] ?? field[2]
    const cols = columnsOf(hints, coll)
    if (cols.length) {
      const items = cols.filter((c) => c.name.toLowerCase().startsWith(field[3].toLowerCase()))
        .map((c) => /** @type {Suggestion} */ ({ label: c.name, insert: c.name, detail: c.type || 'field', kind: 'column', exact: c.name === field[3] }))
      return finish(items, field[3])
    }
    return none
  }
  // `db.users.find({...}).so` → cursor methods.
  if (/\)\s*\.\s*(\w*)$/.test(t) && /\bdb\./.test(t)) {
    const w = /(\w*)$/.exec(t)?.[1] ?? ''
    return from(MONGO_CURSOR, w, 'function')
  }
  // `db.users.fi` → collection methods; `db.us` → collections and db methods.
  const m = /\bdb\.(?:getCollection\(['"][\w.-]+['"]\)|([\w-]+))\.(\w*)$/.exec(t)
  if (m) return from(MONGO_COLL, m[2], 'function')
  const d = /\bdb\.(\w*)$/.exec(t)
  if (d) {
    const items = [
      ...collections.filter((c) => c.toLowerCase().startsWith(d[1].toLowerCase()))
        .map((c) => /** @type {Suggestion} */ ({ label: c, insert: /^[A-Za-z_$][\w$]*$/.test(c) ? c : c, detail: 'collection', kind: 'table', exact: c === d[1] })),
      ...fromList(MONGO_DB, d[1], 'function'),
    ]
    return finish(items, d[1])
  }
  // First word of the line.
  if (!/\s/.test(t)) {
    const items = fromList(MONGO_START, t, 'meta')
    return finish(items, t)
  }
  return none
}

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
  if (client === 'mongo') return 'mongosh'
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
 * @param {{ client: string, line: string, statement?: string, hints?: SchemaHints, editor?: import('$lib/terminal-keys.js').LineEditor }} input
 *   `statement` is the earlier lines of a statement that is still open.
 * @returns {{ items: Suggestion[], token: string }} `token` is the typed text
 *   the accepted item replaces (the end of `line`).
 */
export function suggest({ client, line, statement = '', hints = {}, editor = 'readline' }) {
  const family = familyOf(client)
  const trimmed = line.trimStart()
  const none = { items: [], token: '' }
  if (!trimmed) return none

  // A whole command in another dialect: offer this client's spelling.
  if (!statement) {
    for (const [re, to, why, keys] of TRANSLATIONS[family] ?? []) {
      const m = trimmed.match(re)
      if (!m) continue
      // The Windows console has no Ctrl+L: the client runs `cls` instead, if it can.
      if (keys === CLEAR_KEYS && editor === 'console') {
        const cls = consoleClearCommand(client)
        if (!cls) continue
        return { items: [{ label: cls, insert: cls, detail: why, kind: 'translate', replaceLine: true, run: true }], token: '' }
      }
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

  // The client's own commands: psql's \\x, sqlite3's .x, mysql's \\G and words.
  if (family === 'psql' && !statement && trimmed.startsWith('\\')) {
    const space = trimmed.search(/\s/)
    if (space === -1) return finish(fromList(PSQL_META, trimmed, 'meta'), trimmed)
    const cmd = trimmed.slice(0, space)
    const kind = PSQL_ARG.get(psqlBase(cmd))
    if (!kind) return none
    const rest = trimmed.slice(space).trimStart()
    // One argument (\\pset takes a second, its value: left to the user).
    if (/\s/.test(rest)) return none
    if (kind === 'rel') return rest ? finish(tableItems(rest.replace(/^"/, '')), rest) : none
    const values = PSQL_ARG_VALUES[kind] ?? []
    return finish(fromList(values, rest, 'command').map((i) => (kind === 'sqlhelp' && rest && rest === rest.toLowerCase() ? { ...i, label: i.label.toLowerCase(), insert: i.insert.toLowerCase() } : i)), rest)
  }
  if (family === 'sqlite3' && !statement && trimmed.startsWith('.')) {
    const space = trimmed.search(/\s/)
    if (space === -1) return finish(fromList(SQLITE_DOT, trimmed, 'meta'), trimmed)
    const kind = SQLITE_ARG.get(trimmed.slice(0, space))
    if (!kind) return none
    const arg = trimmed.slice(space).trimStart()
    if (/\s/.test(arg)) return none
    if (kind === 'rel') return arg ? finish(tableItems(arg.replace(/^['"]/, '')), arg) : none
    return finish(fromList(SQLITE_ARG_VALUES[kind] ?? [], arg, 'command'), arg)
  }
  if (family === 'mysql' && !statement) {
    if (trimmed.startsWith('\\') && !/\s/.test(trimmed)) return finish(fromList(MYSQL_CLIENT, trimmed, 'meta'), trimmed)
    // `use <db>` / `source <file>` take no table; `describe us` does.
    const m = /^(.*?)([\w$]*)$/.exec(line)
    if (m && MYSQL_TABLE_AFTER.test(m[1]) && m[2]) return finish(tableItems(m[2]), m[2])
    // `show ta` → SHOW TABLES; / SHOW TABLE STATUS;: the whole statement, as typed so far.
    if (/\s/.test(trimmed) && /^[a-z ]+$/i.test(trimmed)) {
      const t = trimmed.toLowerCase().replace(/\s+/g, ' ')
      const upper = trimmed !== trimmed.toLowerCase()
      const items = MYSQL_STATEMENTS
        .filter(([cmd]) => cmd.toLowerCase().startsWith(t) && cmd.toLowerCase() !== t)
        .map(([cmd, detail], rank) => {
          const text = upper ? cmd : cmd.toLowerCase()
          return /** @type {Suggestion} */ ({ label: text, insert: text, detail, kind: 'command', rank, replaceLine: true })
        })
      if (items.length) return finish(items, trimmed)
    }
  }
  if (family === 'redis-cli') return redisSuggest(line)
  if (family === 'mongosh') return mongoSuggest(line, hints.tables ?? [], hints)
  // `pragma table_info(us`: the table it reads.
  if (family === 'sqlite3') {
    const pm = /\bpragma\s+(?:table_info|table_xinfo|index_list|foreign_key_list|index_info|index_xinfo)\s*\(\s*['"]?([\w$]*)$/i.exec(line)
    if (pm && pm[1]) return finish(tableItems(pm[1]), pm[1])
  }
  if (family === 'sqlcmd' && !statement) {
    if (/^go$/i.test(trimmed)) return none
    // `:r`, `:setvar`, `!!`: sqlcmd's own commands.
    if (/^[:!]/.test(trimmed)) {
      if (/^:on\s+e?r?r?o?r?\s+\w*$/i.test(trimmed)) {
        const arg = trimmed.split(/\s+/)[2] ?? ''
        return finish(fromList([['exit', 'Stop at the first error'], ['ignore', 'Carry on']], arg, 'command'), arg)
      }
      if (/^:xml\s+\w*$/i.test(trimmed)) {
        const arg = trimmed.split(/\s+/)[1] ?? ''
        return finish(fromList([['on', 'XML output'], ['off', 'Normal output']], arg, 'command'), arg)
      }
      if (/\s/.test(trimmed.replace(/^:on\s/i, ':on_'))) return none
      return finish(fromList(SQLCMD_COMMANDS.filter(([c]) => /^[:!]/.test(c)), trimmed, 'meta'), trimmed)
    }
    // `sp_help us`, `exec sp_columns 'or`: the table it takes.
    const m = /^(.*?)([\w$.\[\]]*)$/.exec(line)
    if (m && m[2] && MSSQL_TABLE_AFTER.test(m[1])) return finish(tableItems(m[2].replace(/^\[/, '')), m[2])
  }

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
    if (family === 'sqlcmd') {
      items.push(...fromList(MSSQL_STATEMENTS, token, 'command'))
      items.push(...fromList([['GO', 'Run the batch']], token, 'meta').map((i) => (upperCase ? i : { ...i, label: 'go', insert: 'go' })))
    }
    if (family === 'sqlite3') items.push(...fromList([['PRAGMA table_info(', 'Columns of a table'], ['PRAGMA index_list(', 'Indexes of a table'], ['PRAGMA foreign_key_list(', 'Foreign keys'], ['PRAGMA integrity_check;', 'Check the database'], ['VACUUM;', 'Rebuild the file']], token, 'command').map((i) => (upperCase ? i : { ...i, label: i.label.toLowerCase(), insert: i.insert.toLowerCase() })))
    if (family === 'mysql') {
      items.push(...fromList(MYSQL_CLIENT_WORDS, token, 'meta').map((i) => ({ ...i, rank: (i.rank ?? 0) + 100 })))
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
 * whole. A whole-line replacement empties the line in the keys the client's
 * line editor takes and types the new line.
 * @param {Suggestion} item
 * @param {string} token
 * @param {import('$lib/terminal-keys.js').LineEditor} [editor]
 */
export function keystrokesFor(item, token, editor = 'readline') {
  if (item.keys) return item.keys
  if (item.replaceLine) return `${clearLineKeys(editor)}${item.insert}${item.run ? '\r' : ''}`
  if (item.insert.startsWith(token)) return item.insert.slice(token.length)
  return '\x7f'.repeat(token.length) + item.insert
}
