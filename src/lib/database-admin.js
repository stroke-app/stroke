// Server-level database operations: create, rename, duplicate, drop, and the
// read-only "what is this database" lookup behind Database info.
//
// The SQL lives here rather than in the shell because every statement has an
// engine-specific spelling and an engine-specific set of things it cannot do
// (MySQL has no RENAME DATABASE at all), and those rules have to be the same in
// the sidebar menu that enables the item and in the handler that runs it. One
// place, one answer.

import { engineFamily } from '$lib/stores/connections.js'

/** @typedef {import('$lib/stores/connections.js').SavedConnection} Conn */
/** @typedef {'postgres' | 'mysql' | 'mssql' | 'clickhouse'} AdminKind */
/** @typedef {'create' | 'rename' | 'duplicate' | 'drop' | 'terminate' | 'info'} AdminAction */

/**
 * Which dialect to write the DDL in, or null when this connection has no
 * server-level databases to administer at all.
 *
 * Provider connections (Neon, Supabase, PlanetScale) are ruled out on purpose:
 * their sibling "databases" are projects and branches addressed by an API ref,
 * not names in a catalog, so a `DROP DATABASE "<ref>"` would either miss or hit
 * the wrong thing. SQLite and DuckDB are single files, D1 databases are
 * Cloudflare resources, PostHog is read-only and Redis addresses numbered
 * logical DBs, so none of them get the menu.
 * @param {Conn | null | undefined} conn
 * @returns {AdminKind | null}
 */
export function dbAdminKind(conn) {
  if (!conn || conn.provider) return null
  const family = engineFamily(conn.type)
  if (family === 'postgres') return 'postgres'
  if (family === 'mysql') return 'mysql'
  if (family === 'mssql') return 'mssql'
  if (family === 'clickhouse') return 'clickhouse'
  return null
}

/** PlanetScale serves one database per branch and refuses CREATE and DROP
 *  DATABASE over SQL, even when it was added by hand as plain MySQL.
 *  @param {Conn | null | undefined} conn */
function isPlanetScaleHost(conn) {
  return /(^|\.)psdb\.cloud$/i.test(String(conn?.host ?? ''))
}

/**
 * Whether the drop can close other sessions first: Postgres 13+ has
 * `WITH (FORCE)`, SQL Server rolls them back by going single-user. CockroachDB
 * speaks Postgres but has neither.
 * @param {Conn | null | undefined} conn
 */
export function canForceDrop(conn) {
  const kind = dbAdminKind(conn)
  return (kind === 'postgres' && conn?.type !== 'cockroachdb') || kind === 'mssql'
}

/**
 * Why an action is unavailable, or '' when it can run.
 *
 * Two separate reasons to say no: the engine cannot express the statement, or
 * it can but not against the database this session is attached to. Postgres
 * refuses to rename, drop or use as a template the database you are connected
 * to, so those are offered on the other rows only.
 * @param {AdminAction} action
 * @param {Conn | null | undefined} conn
 * @param {{ isCurrent?: boolean }} [opts]
 */
export function dbActionBlocker(action, conn, opts = {}) {
  const kind = dbAdminKind(conn)
  if (!kind) return 'This connection has no server-level databases to manage.'
  if (action !== 'info' && isPlanetScaleHost(conn)) return 'PlanetScale manages databases per branch. Create, copy or delete them in PlanetScale.'
  if (kind === 'mysql') {
    if (action === 'rename') return 'MySQL has no RENAME DATABASE. Duplicate it under the new name, then drop the old one.'
    if (action === 'terminate') return 'Not needed on MySQL: DROP DATABASE does not wait for other sessions.'
  }
  if (kind === 'mssql' && action === 'terminate') return 'Turn on "Close other sessions" when dropping: SQL Server ends them as part of the drop.'
  if (kind === 'clickhouse' && action === 'terminate') return 'Not needed on ClickHouse: a query holds no session on a database.'
  if (conn?.type === 'cockroachdb') {
    if (action === 'duplicate') return 'CockroachDB has no database templates. Use BACKUP and RESTORE from the SQL editor.'
    if (action === 'terminate') return 'CockroachDB cannot close sessions by database.'
  }
  if (!opts.isCurrent) return ''
  if (action === 'rename') return 'Cannot rename the database you are connected to. Switch to another one first.'
  if (action === 'drop') return 'Cannot drop the database you are connected to. Switch to another one first.'
  if (action === 'duplicate' && kind === 'postgres') return 'Postgres cannot copy a database while a session is connected to it. Switch away first.'
  return ''
}

/** @param {AdminAction} action @param {Conn | null | undefined} conn @param {{ isCurrent?: boolean }} [opts] */
export function canDbAction(action, conn, opts = {}) {
  return dbActionBlocker(action, conn, opts) === ''
}

/** Quote a database name for the dialect. @param {AdminKind} kind @param {string} name */
export function quoteDb(kind, name) {
  const n = String(name)
  if (kind === 'mysql' || kind === 'clickhouse') return `\`${n.replace(/`/g, '``')}\``
  if (kind === 'mssql') return `[${n.replace(/]/g, ']]')}]`
  return `"${n.replace(/"/g, '""')}"`
}

/** Single-quote a string literal - these statements have no parameter binding. @param {string} v */
function lit(v) {
  return `'${String(v).replace(/'/g, "''")}'`
}

/**
 * The name rules both engines agree on, kept deliberately tighter than what the
 * server would accept: no quote characters, nothing that needs escaping to
 * survive a round trip through a DDL string.
 * @param {string} name
 * @param {string[]} [existing] names already on the server, to catch collisions early
 * @returns {string} the problem, or '' when the name is usable
 */
export function validateDbName(name, existing = []) {
  const n = name.trim()
  if (!n) return 'Name is required'
  if (n.length > 63) return 'Name must be 63 characters or fewer'
  if (/["'`\\\0\n\r]/.test(n)) return 'Name cannot contain quotes, backslashes or line breaks'
  if (existing.some((e) => e.toLowerCase() === n.toLowerCase())) return 'A database with that name already exists'
  return ''
}

/** @typedef {{ name: string, owner?: string, encoding?: string, lcCollate?: string, lcCtype?: string, template?: string, connectionLimit?: number }} CreateDbOptions */

/** @param {AdminKind} kind @param {CreateDbOptions} opts */
export function createDatabaseSql(kind, opts) {
  const { name, owner = '', encoding = '', lcCollate = '', lcCtype = '', template = '', connectionLimit = -1 } = opts
  // The create dialog offers no options for these two; the server defaults apply.
  if (kind === 'mssql' || kind === 'clickhouse') return `CREATE DATABASE ${quoteDb(kind, name)}`
  if (kind === 'mysql') {
    let sql = `CREATE DATABASE ${quoteDb(kind, name)}`
    if (encoding) sql += ` CHARACTER SET ${encoding}`
    if (lcCollate) sql += ` COLLATE ${lcCollate}`
    return sql
  }
  let sql = `CREATE DATABASE ${quoteDb(kind, name)}`
  if (encoding) sql += `\n  ENCODING ${lit(encoding)}`
  if (template) sql += `\n  TEMPLATE ${template}`
  if (lcCollate) sql += `\n  LC_COLLATE ${lit(lcCollate)}`
  if (lcCtype) sql += `\n  LC_CTYPE ${lit(lcCtype)}`
  if (owner) sql += `\n  OWNER ${quoteDb(kind, owner)}`
  if (connectionLimit != null && connectionLimit !== -1) sql += `\n  CONNECTION LIMIT ${connectionLimit}`
  return sql
}

/** @param {AdminKind} kind @param {string} from @param {string} to */
export function renameDatabaseSql(kind, from, to) {
  if (kind === 'mysql') throw new Error('MySQL cannot rename a database')
  if (kind === 'mssql') return `ALTER DATABASE ${quoteDb(kind, from)} MODIFY NAME = ${quoteDb(kind, to)}`
  if (kind === 'clickhouse') return `RENAME DATABASE ${quoteDb(kind, from)} TO ${quoteDb(kind, to)}`
  return `ALTER DATABASE ${quoteDb(kind, from)} RENAME TO ${quoteDb(kind, to)}`
}

/**
 * What copying a database runs, for the dialog's preview. The copy itself is
 * `cloneDatabase` in api.js. Postgres does it in one statement by using the
 * source as a template, which is why the source has to be session-free. MySQL
 * and ClickHouse have no such statement, so the server rebuilds the copy table
 * by table; SQL Server goes through a backup. Those show the outline.
 * @param {AdminKind} kind @param {string} from @param {string} to
 */
export function duplicateDatabaseSql(kind, from, to) {
  const f = quoteDb(kind, from)
  const t = quoteDb(kind, to)
  if (kind === 'mysql') {
    return `CREATE DATABASE ${t}\n-- then, for each table:\n--   CREATE TABLE as SHOW CREATE TABLE ${f}.<table> reports it\n--   INSERT INTO ${t}.<table> SELECT ... FROM ${f}.<table>\n-- then its routines, views and triggers`
  }
  if (kind === 'clickhouse') {
    return `CREATE DATABASE ${t}\n-- then, for each table:\n--   CREATE TABLE ${t}.<table> AS ${f}.<table>\n--   INSERT INTO ${t}.<table> SELECT * FROM ${f}.<table>\n-- then its views and materialized views`
  }
  if (kind === 'mssql') {
    return `BACKUP DATABASE ${f} TO DISK = N'<default backup folder>' WITH COPY_ONLY\nRESTORE DATABASE ${t} FROM DISK = N'<default backup folder>' WITH MOVE ...`
  }
  return `CREATE DATABASE ${t} WITH TEMPLATE ${f}`
}

/**
 * @param {AdminKind} kind @param {string} name
 * @param {{ force?: boolean }} [opts] close other sessions instead of refusing:
 *   Postgres 13+ has FORCE, SQL Server rolls them back by going single-user.
 *   Without it a single idle connection blocks the drop on both.
 */
export function dropDatabaseSql(kind, name, opts = {}) {
  const q = quoteDb(kind, name)
  if (kind === 'postgres') return `DROP DATABASE ${q}${opts.force ? ' WITH (FORCE)' : ''}`
  if (kind === 'mssql' && opts.force) return `ALTER DATABASE ${q} SET SINGLE_USER WITH ROLLBACK IMMEDIATE;\nDROP DATABASE ${q}`
  return `DROP DATABASE ${q}`
}

/** Close every other session on a database, so a rename or copy can proceed.
 *  @param {AdminKind} kind @param {string} name */
export function terminateSessionsSql(kind, name) {
  if (kind !== 'postgres') throw new Error('Only supported on Postgres')
  return `SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = ${lit(name)} AND pid <> pg_backend_pid()`
}

/**
 * How many sessions are on a database, for the drop dialog's warning, or ''
 * where the engine has no per-database sessions to count.
 * @param {AdminKind} kind @param {string} name
 */
export function sessionCountSql(kind, name) {
  if (kind === 'postgres') return `SELECT count(*) FROM pg_stat_activity WHERE datname = ${lit(name)}`
  if (kind === 'mssql') return `SELECT COUNT(*) FROM sys.dm_exec_sessions WHERE database_id = DB_ID(${lit(name)})`
  return ''
}

/** @typedef {{ label: string, value: string }} DbInfoRow */

/** Catalog read behind Database info. @param {AdminKind} kind @param {string} name */
export function databaseInfoSql(kind, name) {
  if (kind === 'mssql') {
    return `SELECT d.name,
       SUSER_SNAME(d.owner_sid) AS owner,
       d.collation_name,
       d.state_desc,
       d.recovery_model_desc,
       d.compatibility_level,
       CONVERT(varchar(19), d.create_date, 120) AS created,
       (SELECT SUM(CAST(f.size AS bigint)) * 8192 FROM sys.master_files f WHERE f.database_id = d.database_id) AS size_bytes,
       (SELECT COUNT(*) FROM sys.dm_exec_sessions s WHERE s.database_id = d.database_id) AS sessions
FROM sys.databases d
WHERE d.name = ${lit(name)}`
  }
  if (kind === 'clickhouse') {
    // Scalar subqueries on the literal: older ClickHouse has no correlated ones.
    return `SELECT name,
       engine,
       comment,
       (SELECT count() FROM system.tables WHERE database = ${lit(name)}) AS table_count,
       (SELECT sum(total_rows) FROM system.tables WHERE database = ${lit(name)}) AS total_rows,
       (SELECT sum(total_bytes) FROM system.tables WHERE database = ${lit(name)}) AS size_bytes
FROM system.databases
WHERE name = ${lit(name)}`
  }
  if (kind === 'mysql') {
    return `SELECT s.SCHEMA_NAME,
       s.DEFAULT_CHARACTER_SET_NAME,
       s.DEFAULT_COLLATION_NAME,
       (SELECT COUNT(*) FROM information_schema.TABLES t WHERE t.TABLE_SCHEMA = s.SCHEMA_NAME) AS table_count,
       (SELECT IFNULL(SUM(t.DATA_LENGTH + t.INDEX_LENGTH), 0) FROM information_schema.TABLES t WHERE t.TABLE_SCHEMA = s.SCHEMA_NAME) AS size_bytes
FROM information_schema.SCHEMATA s
WHERE s.SCHEMA_NAME = ${lit(name)}`
  }
  return `SELECT d.datname,
       pg_get_userbyid(d.datdba) AS owner,
       pg_encoding_to_char(d.encoding) AS encoding,
       d.datcollate,
       d.datctype,
       d.datconnlimit,
       d.datallowconn,
       pg_size_pretty(pg_database_size(d.datname)) AS size,
       (SELECT count(*) FROM pg_stat_activity a WHERE a.datname = d.datname) AS sessions,
       shobj_description(d.oid, 'pg_database') AS comment
FROM pg_database d
WHERE d.datname = ${lit(name)}`
}

/** @param {number} bytes */
function prettyBytes(bytes) {
  const n = Number(bytes)
  if (!Number.isFinite(n)) return '-'
  const units = ['B', 'kB', 'MB', 'GB', 'TB']
  let v = n
  let i = 0
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i++ }
  // One decimal below 10, and never a trailing ".0" - "5 MB", not "5.0 MB".
  const shown = i === 0 ? String(v) : v.toFixed(v < 10 ? 1 : 0).replace(/\.0$/, '')
  return `${shown} ${units[i]}`
}

/**
 * Shape the info query's single row into label/value pairs for display. Kept
 * next to the query so a changed column list can only break in one file.
 * @param {AdminKind} kind
 * @param {{ rows?: unknown[][] } | null | undefined} result
 * @returns {DbInfoRow[]}
 */
export function databaseInfoRows(kind, result) {
  const row = result?.rows?.[0]
  if (!row) return []
  const str = (/** @type {unknown} */ v) => (v == null || v === '' ? '-' : String(v))
  if (kind === 'mssql') {
    return [
      { label: 'Name', value: str(row[0]) },
      { label: 'Owner', value: str(row[1]) },
      { label: 'Collation', value: str(row[2]) },
      { label: 'State', value: str(row[3]) },
      { label: 'Recovery model', value: str(row[4]) },
      { label: 'Compatibility level', value: str(row[5]) },
      { label: 'Created', value: str(row[6]) },
      { label: 'Size on disk', value: prettyBytes(Number(row[7])) },
      { label: 'Active sessions', value: str(row[8]) },
    ]
  }
  if (kind === 'clickhouse') {
    return [
      { label: 'Name', value: str(row[0]) },
      { label: 'Engine', value: str(row[1]) },
      { label: 'Tables', value: str(row[3]) },
      { label: 'Rows', value: str(row[4]) },
      { label: 'Size', value: prettyBytes(Number(row[5])) },
      { label: 'Comment', value: str(row[2]) },
    ]
  }
  if (kind === 'mysql') {
    return [
      { label: 'Name', value: str(row[0]) },
      { label: 'Character set', value: str(row[1]) },
      { label: 'Collation', value: str(row[2]) },
      { label: 'Tables', value: str(row[3]) },
      { label: 'Size', value: prettyBytes(Number(row[4])) },
    ]
  }
  return [
    { label: 'Name', value: str(row[0]) },
    { label: 'Owner', value: str(row[1]) },
    { label: 'Encoding', value: str(row[2]) },
    { label: 'Collation', value: str(row[3]) },
    { label: 'Character type', value: str(row[4]) },
    { label: 'Connection limit', value: Number(row[5]) === -1 ? 'Unlimited' : str(row[5]) },
    { label: 'Accepts connections', value: row[6] === false || row[6] === 'false' || row[6] === 0 ? 'No' : 'Yes' },
    { label: 'Size on disk', value: str(row[7]) },
    { label: 'Active sessions', value: str(row[8]) },
    { label: 'Comment', value: str(row[9]) },
  ]
}
