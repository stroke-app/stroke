// The three engines the bridge speaks, behind one small interface the web
// studio calls: tables, columns, a page of rows, and a SQL console.

/**
 * @typedef {{ schema: string, name: string, kind: 'table' | 'view' }} TableRef
 * @typedef {{ name: string, type: string, nullable: boolean, primaryKey: boolean }} Column
 * @typedef {{ columns: string[], rows: unknown[][], rowCount?: number | null, command?: string }} Result
 * @typedef {{ column: string, dir: 'asc' | 'desc' }} Sort
 * @typedef {{
 *   engine: 'postgres' | 'mysql' | 'sqlite',
 *   info(): Promise<{ engine: string, database: string, host: string, version: string }>,
 *   tables(): Promise<TableRef[]>,
 *   columns(schema: string, table: string): Promise<Column[]>,
 *   rows(o: { schema: string, table: string, limit: number, offset: number, sort?: Sort | null }): Promise<Result & { total: number | null, estimated: boolean }>,
 *   query(sql: string): Promise<Result>,
 *   close(): Promise<void>,
 * }} Engine
 */

/** Exact counts above this many (estimated) rows cost too much to run per page. */
const EXACT_COUNT_MAX = 200_000

/**
 * JSON-safe cell: dates as ISO strings, bytes as \x-hex, bigints as strings.
 * @param {unknown} v
 * @returns {unknown}
 */
export function cell(v) {
  if (v === null || v === undefined) return null
  if (typeof v === 'bigint') return v.toString()
  if (v instanceof Date) return Number.isNaN(v.getTime()) ? null : v.toISOString()
  if (v instanceof Uint8Array) return `\\x${Buffer.from(v).toString('hex')}`
  if (Array.isArray(v)) return v.map(cell)
  if (typeof v === 'object') return JSON.parse(JSON.stringify(v, (_k, x) => (typeof x === 'bigint' ? x.toString() : x)))
  return v
}

/** @param {string} id */
const pgId = (id) => `"${String(id).replace(/"/g, '""')}"`
/** @param {string} id */
const myId = (id) => `\`${String(id).replace(/`/g, '``')}\``

/** @param {Sort | null | undefined} sort @param {(id: string) => string} q */
function orderBy(sort, q) {
  if (!sort?.column) return ''
  return ` ORDER BY ${q(sort.column)} ${sort.dir === 'desc' ? 'DESC' : 'ASC'}`
}

/** @param {number} n @param {number} max */
const clampInt = (n, max) => Math.max(0, Math.min(max, Math.floor(Number(n) || 0)))

/**
 * @param {string} uri
 * @returns {Promise<Engine>}
 */
export async function openPostgres(uri) {
  const { default: pg } = await import('pg')
  // Dates and timestamps as text, exactly as stored: no timezone shifts.
  for (const oid of [1082, 1114, 1184, 1083, 1266]) pg.types.setTypeParser(oid, (v) => v)
  const pool = new pg.Pool({ connectionString: uri, max: 4, connectionTimeoutMillis: 15_000 })
  await (await pool.connect()).release()

  /** @param {string} text @param {unknown[]} [values] */
  const arr = async (text, values) => {
    const r = await pool.query({ text, values, rowMode: 'array' })
    const last = Array.isArray(r) ? r[r.length - 1] : r
    return {
      columns: (last.fields ?? []).map((/** @type {{ name: string }} */ f) => f.name),
      rows: (last.rows ?? []).map((/** @type {unknown[]} */ row) => row.map(cell)),
      rowCount: last.rowCount ?? null,
      command: last.command ?? '',
    }
  }

  return {
    engine: 'postgres',
    async info() {
      const r = await pool.query('select current_database() as db, version() as v')
      const u = new URL(uri.replace(/^[a-z0-9+.-]+:/i, 'http:'))
      return { engine: 'postgres', database: r.rows[0].db, host: u.hostname, version: String(r.rows[0].v).split(' on ')[0] }
    },
    async tables() {
      const r = await pool.query(`
        select table_schema as schema, table_name as name,
               case when table_type = 'VIEW' then 'view' else 'table' end as kind
        from information_schema.tables
        where table_schema not in ('pg_catalog', 'information_schema') and table_schema not like 'pg_toast%' and table_schema not like 'pg_temp%'
        union all
        select schemaname, matviewname, 'view' from pg_matviews
        order by 1, 2`)
      return r.rows
    },
    async columns(schema, table) {
      const r = await pool.query(
        `select a.attname as name, format_type(a.atttypid, a.atttypmod) as type, not a.attnotnull as nullable,
                coalesce(a.attnum = any(i.indkey), false) as "primaryKey"
         from pg_attribute a
         join pg_class c on c.oid = a.attrelid
         join pg_namespace n on n.oid = c.relnamespace
         left join pg_index i on i.indrelid = c.oid and i.indisprimary
         where n.nspname = $1 and c.relname = $2 and a.attnum > 0 and not a.attisdropped
         order by a.attnum`,
        [schema, table],
      )
      return r.rows
    },
    async rows({ schema, table, limit, offset, sort }) {
      const from = `${pgId(schema)}.${pgId(table)}`
      const page = await arr(`select * from ${from}${orderBy(sort, pgId)} limit $1 offset $2`, [clampInt(limit, 5000), clampInt(offset, 1e12)])
      const est = await pool.query(
        `select c.reltuples::bigint as n from pg_class c join pg_namespace n on n.oid = c.relnamespace where n.nspname = $1 and c.relname = $2`,
        [schema, table],
      )
      const guess = Number(est.rows[0]?.n ?? -1)
      if (guess > EXACT_COUNT_MAX) return { ...page, total: guess, estimated: true }
      const exact = await pool.query(`select count(*)::bigint as n from ${from}`)
      return { ...page, total: Number(exact.rows[0].n), estimated: false }
    },
    query: (sql) => arr(sql),
    close: () => pool.end(),
  }
}

/**
 * @param {string} uri
 * @returns {Promise<Engine>}
 */
export async function openMysql(uri) {
  const mysql = await import('mysql2/promise')
  const pool = mysql.createPool({ uri: uri.replace(/^(mariadb|mysql2):/i, 'mysql:'), connectionLimit: 4, dateStrings: true, supportBigNumbers: true, bigNumberStrings: true, connectTimeout: 15_000 })
  await (await pool.getConnection()).release()

  /** @param {string} sql @param {unknown[]} [values] */
  const arr = async (sql, values) => {
    const [rows, fields] = await pool.query({ sql, values, rowsAsArray: true })
    if (!Array.isArray(rows)) {
      const h = /** @type {{ affectedRows?: number }} */ (rows)
      return { columns: [], rows: [], rowCount: h.affectedRows ?? null, command: sql.trim().split(/\s+/)[0].toUpperCase() }
    }
    return {
      columns: (fields ?? []).map((f) => f.name),
      rows: rows.map((row) => /** @type {unknown[]} */ (row).map(cell)),
      rowCount: rows.length,
      command: 'SELECT',
    }
  }

  return {
    engine: 'mysql',
    async info() {
      const [r] = /** @type {any} */ (await pool.query('select database() as db, version() as v'))
      const u = new URL(uri.replace(/^[a-z0-9+.-]+:/i, 'http:'))
      return { engine: 'mysql', database: r[0].db ?? '', host: u.hostname, version: r[0].v }
    },
    async tables() {
      const [r] = /** @type {any} */ (await pool.query(
        `select table_schema as \`schema\`, table_name as name, if(table_type = 'VIEW', 'view', 'table') as kind
         from information_schema.tables
         where table_schema = coalesce(database(), table_schema)
           and table_schema not in ('mysql', 'information_schema', 'performance_schema', 'sys')
         order by 1, 2`,
      ))
      return r
    },
    async columns(schema, table) {
      const [r] = /** @type {any} */ (await pool.query(
        `select column_name as name, column_type as type, is_nullable = 'YES' as nullable, column_key = 'PRI' as primaryKey
         from information_schema.columns where table_schema = ? and table_name = ? order by ordinal_position`,
        [schema, table],
      ))
      return r.map((/** @type {any} */ c) => ({ ...c, nullable: !!c.nullable, primaryKey: !!c.primaryKey }))
    },
    async rows({ schema, table, limit, offset, sort }) {
      const from = `${myId(schema)}.${myId(table)}`
      const page = await arr(`select * from ${from}${orderBy(sort, myId)} limit ? offset ?`, [clampInt(limit, 5000), clampInt(offset, 1e12)])
      const [est] = /** @type {any} */ (await pool.query(
        'select table_rows as n from information_schema.tables where table_schema = ? and table_name = ?',
        [schema, table],
      ))
      const guess = Number(est[0]?.n ?? -1)
      if (guess > EXACT_COUNT_MAX) return { ...page, total: guess, estimated: true }
      const [exact] = /** @type {any} */ (await pool.query(`select count(*) as n from ${from}`))
      return { ...page, total: Number(exact[0].n), estimated: false }
    },
    query: (sql) => arr(sql),
    close: () => pool.end(),
  }
}

/**
 * @param {string} file
 * @returns {Promise<Engine>}
 */
export async function openSqlite(file) {
  /** @type {typeof import('node:sqlite')} */
  let sqlite
  try {
    sqlite = await import('node:sqlite')
  } catch {
    throw new Error(`SQLite needs Node 22.5 or newer (this is ${process.version}). Or open the file in the Stroke app.`)
  }
  const db = new sqlite.DatabaseSync(file)

  /** @param {string} sql @param {unknown[]} [params] */
  const arr = (sql, params = []) => {
    const stmt = db.prepare(sql)
    const cols = typeof stmt.columns === 'function' ? stmt.columns().map((c) => c.name) : null
    const returnsRows = cols ? cols.length > 0 : /^\s*(select|with|pragma|explain|values)\b/i.test(sql)
    if (!returnsRows) {
      const r = stmt.run(.../** @type {any[]} */ (params))
      return { columns: [], rows: [], rowCount: Number(r.changes), command: sql.trim().split(/\s+/)[0].toUpperCase() }
    }
    const objs = /** @type {Record<string, unknown>[]} */ (stmt.all(.../** @type {any[]} */ (params)))
    const columns = cols ?? Object.keys(objs[0] ?? {})
    return { columns, rows: objs.map((o) => columns.map((c) => cell(o[c]))), rowCount: objs.length, command: 'SELECT' }
  }
  /** @param {string} sql */
  const multi = (sql) => {
    // node:sqlite prepares one statement at a time; run all but the last with
    // exec and return the last one's rows, like the other consoles do.
    const parts = splitStatements(sql)
    if (parts.length > 1) db.exec(parts.slice(0, -1).join(';\n'))
    return arr(parts[parts.length - 1] ?? sql)
  }

  return {
    engine: 'sqlite',
    async info() {
      const v = /** @type {any} */ (db.prepare('select sqlite_version() as v').get()).v
      return { engine: 'sqlite', database: file.split(/[\\/]/).pop() ?? file, host: 'local file', version: `SQLite ${v}` }
    },
    async tables() {
      const r = db.prepare(`select name, type from sqlite_schema where type in ('table', 'view') and name not like 'sqlite_%' order by name`).all()
      return r.map((t) => ({ schema: 'main', name: String(t.name), kind: t.type === 'view' ? 'view' : 'table' }))
    },
    async columns(_schema, table) {
      const r = db.prepare('select name, type, "notnull" as nn, pk from pragma_table_info(?)').all(table)
      return r.map((c) => ({ name: String(c.name), type: String(c.type || ''), nullable: !c.nn, primaryKey: Number(c.pk) > 0 }))
    },
    async rows({ table, limit, offset, sort }) {
      const page = arr(`select * from ${pgId(table)}${orderBy(sort, pgId)} limit ? offset ?`, [clampInt(limit, 5000), clampInt(offset, 1e12)])
      const total = Number(/** @type {any} */ (db.prepare(`select count(*) as n from ${pgId(table)}`).get()).n)
      return { ...page, total, estimated: false }
    },
    async query(sql) {
      return multi(sql)
    },
    async close() {
      db.close()
    },
  }
}

/**
 * Split on `;` outside quotes and comments. Enough for a console; not a parser.
 * @param {string} sql
 */
export function splitStatements(sql) {
  /** @type {string[]} */
  const out = []
  let cur = ''
  for (let i = 0; i < sql.length; i++) {
    const c = sql[i]
    if (c === "'" || c === '"' || c === '`') {
      const end = sql.indexOf(c, i + 1)
      const j = end < 0 ? sql.length : end
      cur += sql.slice(i, j + 1)
      i = j
    } else if (c === '-' && sql[i + 1] === '-') {
      const end = sql.indexOf('\n', i)
      i = end < 0 ? sql.length : end - 1
    } else if (c === '/' && sql[i + 1] === '*') {
      const end = sql.indexOf('*/', i + 2)
      i = end < 0 ? sql.length : end + 1
    } else if (c === ';') {
      if (cur.trim()) out.push(cur.trim())
      cur = ''
    } else {
      cur += c
    }
  }
  if (cur.trim()) out.push(cur.trim())
  return out
}

/**
 * @param {'postgres' | 'mysql' | 'sqlite'} engine
 * @param {string} uri
 */
export function openEngine(engine, uri) {
  if (engine === 'postgres') return openPostgres(uri)
  if (engine === 'mysql') return openMysql(uri)
  return openSqlite(uri)
}
