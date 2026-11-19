// The three engines the bridge speaks, behind one interface the web studio
// calls: tables, structure, a page of rows, edits by primary key, and a SQL
// console.

/**
 * @typedef {{ schema: string, name: string, kind: 'table' | 'view', rows: number | null }} TableRef
 * @typedef {{ name: string, type: string, nullable: boolean, primaryKey: boolean, default: string | null }} Column
 * @typedef {{ name: string, columns: string[], unique: boolean, primary: boolean }} Index
 * @typedef {{ name: string, columns: string[], refSchema: string, refTable: string, refColumns: string[] }} ForeignKey
 * @typedef {{ columns: Column[], indexes: Index[], foreignKeys: ForeignKey[] }} Structure
 * @typedef {{ columns: string[], rows: unknown[][], rowCount?: number | null, command?: string }} Result
 * @typedef {{ column: string, dir: 'asc' | 'desc' }} Sort
 * @typedef {{ schema: string, table: string, limit: number, offset: number, sort?: Sort | null, where?: string }} RowsQuery
 * @typedef {Record<string, unknown>} Values
 * @typedef {{ sql: string, params: unknown[], atMostOne?: boolean }} Step
 * @typedef {{
 *   engine: 'postgres' | 'mysql' | 'sqlite',
 *   info(): Promise<{ engine: string, database: string, host: string, version: string }>,
 *   tables(): Promise<TableRef[]>,
 *   columns(schema: string, table: string): Promise<Column[]>,
 *   structure(schema: string, table: string): Promise<Structure>,
 *   rows(o: RowsQuery): Promise<Result & { total: number | null, estimated: boolean }>,
 *   update(o: { schema: string, table: string, key: Values, set: Values }): Promise<{ rowCount: number }>,
 *   insert(o: { schema: string, table: string, values: Values }): Promise<{ rowCount: number }>,
 *   remove(o: { schema: string, table: string, keys: Values[] }): Promise<{ rowCount: number }>,
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

/**
 * A value from the page as a driver parameter: objects (json columns) as JSON
 * text, everything else as is - the database casts text to the column's type.
 * @param {unknown} v
 */
export function param(v) {
  if (v === undefined) return null
  if (v !== null && typeof v === 'object') return JSON.stringify(v)
  return v
}

/** @param {string} id */
const dq = (id) => `"${String(id).replace(/"/g, '""')}"`
/** @param {string} id */
const bq = (id) => `\`${String(id).replace(/`/g, '``')}\``

/** @param {number} n @param {number} max */
const clampInt = (n, max) => Math.max(0, Math.min(max, Math.floor(Number(n) || 0)))

/**
 * SQL builders for one dialect.
 * @param {{ q: (id: string) => string, ph: (i: number) => string, from: (schema: string, table: string) => string }} d
 */
export function dialect(d) {
  /** @param {Values} key @param {number} start */
  const whereKey = (key, start) => {
    const cols = Object.keys(key)
    if (!cols.length) throw new Error('No primary key: this table can only be changed with SQL.')
    return {
      sql: cols.map((c, i) => `${d.q(c)} = ${d.ph(start + i)}`).join(' AND '),
      params: cols.map((c) => param(key[c])),
    }
  }
  return {
    ...d,
    /** @param {RowsQuery} o */
    page(o) {
      const where = o.where?.trim() ? ` WHERE (${o.where})` : ''
      const order = o.sort?.column ? ` ORDER BY ${d.q(o.sort.column)} ${o.sort.dir === 'desc' ? 'DESC' : 'ASC'}` : ''
      return {
        sql: `SELECT * FROM ${d.from(o.schema, o.table)}${where}${order} LIMIT ${clampInt(o.limit, 5000)} OFFSET ${clampInt(o.offset, 1e12)}`,
        count: `SELECT count(*) AS n FROM ${d.from(o.schema, o.table)}${where}`,
      }
    },
    /** @param {string} schema @param {string} table @param {Values} key @param {Values} set @returns {Step} */
    update(schema, table, key, set) {
      const cols = Object.keys(set)
      if (!cols.length) throw new Error('Nothing to change.')
      const w = whereKey(key, cols.length + 1)
      return {
        sql: `UPDATE ${d.from(schema, table)} SET ${cols.map((c, i) => `${d.q(c)} = ${d.ph(i + 1)}`).join(', ')} WHERE ${w.sql}`,
        params: [...cols.map((c) => param(set[c])), ...w.params],
        atMostOne: true,
      }
    },
    /** @param {string} schema @param {string} table @param {Values} values @returns {Step} */
    insert(schema, table, values) {
      const cols = Object.keys(values)
      if (!cols.length) return { sql: d.emptyInsert(d.from(schema, table)), params: [] }
      return {
        sql: `INSERT INTO ${d.from(schema, table)} (${cols.map(d.q).join(', ')}) VALUES (${cols.map((_, i) => d.ph(i + 1)).join(', ')})`,
        params: cols.map((c) => param(values[c])),
      }
    },
    /** @param {string} schema @param {string} table @param {Values} key @returns {Step} */
    remove(schema, table, key) {
      const w = whereKey(key, 1)
      return { sql: `DELETE FROM ${d.from(schema, table)} WHERE ${w.sql}`, params: w.params, atMostOne: true }
    },
    emptyInsert: d.emptyInsert,
  }
}

/** @param {number} n @param {Step} step */
function checkOne(n, step) {
  if (step.atMostOne && n > 1) throw new Error(`That would change ${n} rows, not one. Nothing was changed.`)
}

/**
 * @param {string} uri
 * @returns {Promise<Engine>}
 */
export async function openPostgres(uri, { readOnly = false } = {}) {
  const { default: pg } = await import('pg')
  // Dates and timestamps as text, exactly as stored: no timezone shifts.
  for (const oid of [1082, 1114, 1184, 1083, 1266]) pg.types.setTypeParser(oid, (v) => v)
  const pool = new pg.Pool({ connectionString: uri, max: 4, connectionTimeoutMillis: 15_000 })
  await (await pool.connect()).release()
  /**
   * Every query this engine runs. Read-only (a shared studio): each statement
   * runs alone, extended protocol (one statement, so no `COMMIT; DELETE`), in
   * its own `BEGIN READ ONLY` that is always rolled back, so even a SET it
   * makes is undone.
   * @param {any} cfg @param {unknown[]} [values]
   */
  const pq = async (cfg, values) => {
    if (!readOnly) return pool.query(cfg, values)
    const c = await pool.connect()
    try {
      await c.query('BEGIN READ ONLY')
      const q = typeof cfg === 'string' ? { text: cfg, values } : { ...cfg, values: cfg.values ?? values }
      return await c.query({ ...q, queryMode: 'extended' })
    } finally {
      await c.query('ROLLBACK').catch(() => {})
      c.release()
    }
  }
  const sql = dialect({
    q: dq,
    ph: (i) => `$${i}`,
    from: (s, t) => `${dq(s)}.${dq(t)}`,
    emptyInsert: (from) => `INSERT INTO ${from} DEFAULT VALUES`,
  })

  /** @param {string} text @param {unknown[]} [values] */
  const arr = async (text, values) => {
    const r = await pq({ text, values, rowMode: 'array' })
    const last = Array.isArray(r) ? r[r.length - 1] : r
    return {
      columns: (last.fields ?? []).map((/** @type {{ name: string }} */ f) => f.name),
      rows: (last.rows ?? []).map((/** @type {unknown[]} */ row) => row.map(cell)),
      rowCount: last.rowCount ?? null,
      command: last.command ?? '',
    }
  }
  /** @param {Step[]} steps */
  const tx = async (steps) => {
    if (readOnly) throw new Error(READ_ONLY)
    const c = await pool.connect()
    let total = 0
    try {
      await c.query('BEGIN')
      for (const s of steps) {
        const r = await c.query(s.sql, s.params)
        checkOne(r.rowCount ?? 0, s)
        total += r.rowCount ?? 0
      }
      await c.query('COMMIT')
      return { rowCount: total }
    } catch (e) {
      await c.query('ROLLBACK').catch(() => {})
      throw e
    } finally {
      c.release()
    }
  }

  /** @param {string} schema @param {string} table */
  const columns = async (schema, table) => {
    const r = await pq(
      `select a.attname as name, format_type(a.atttypid, a.atttypmod) as type, not a.attnotnull as nullable,
              coalesce(a.attnum = any(i.indkey), false) as "primaryKey",
              pg_get_expr(d.adbin, d.adrelid) as default
       from pg_attribute a
       join pg_class c on c.oid = a.attrelid
       join pg_namespace n on n.oid = c.relnamespace
       left join pg_index i on i.indrelid = c.oid and i.indisprimary
       left join pg_attrdef d on d.adrelid = c.oid and d.adnum = a.attnum
       where n.nspname = $1 and c.relname = $2 and a.attnum > 0 and not a.attisdropped
       order by a.attnum`,
      [schema, table],
    )
    return r.rows
  }

  return {
    engine: 'postgres',
    async info() {
      const r = await pq('select current_database() as db, version() as v')
      const u = new URL(uri.replace(/^[a-z0-9+.-]+:/i, 'http:'))
      return { engine: 'postgres', database: r.rows[0].db, host: u.hostname, version: String(r.rows[0].v).split(' on ')[0] }
    },
    async tables() {
      const r = await pq(`
        select n.nspname as schema, c.relname as name,
               case when c.relkind in ('v', 'm') then 'view' else 'table' end as kind,
               case when c.relkind in ('r', 'p') then coalesce(nullif(c.reltuples, -1)::bigint, st.n_live_tup) end as rows
        from pg_class c join pg_namespace n on n.oid = c.relnamespace
        left join pg_stat_user_tables st on st.relid = c.oid
        where c.relkind in ('r', 'p', 'v', 'm', 'f')
          and n.nspname not in ('pg_catalog', 'information_schema') and n.nspname not like 'pg_toast%' and n.nspname not like 'pg_temp%'
          and not c.relispartition
        order by 1, 2`)
      return r.rows.map((t) => ({ ...t, rows: t.rows == null ? null : Number(t.rows) }))
    },
    columns,
    async structure(schema, table) {
      const idx = await pq(
        `select i.relname as name, ix.indisunique as unique, ix.indisprimary as primary,
                array(select a.attname from unnest(ix.indkey) with ordinality k(n, o) join pg_attribute a on a.attrelid = t.oid and a.attnum = k.n order by k.o)::text[] as columns
         from pg_index ix join pg_class t on t.oid = ix.indrelid join pg_class i on i.oid = ix.indexrelid join pg_namespace n on n.oid = t.relnamespace
         where n.nspname = $1 and t.relname = $2 order by ix.indisprimary desc, i.relname`,
        [schema, table],
      )
      const fks = await pq(
        `select con.conname as name, rn.nspname as "refSchema", rt.relname as "refTable",
                array(select a.attname from unnest(con.conkey) with ordinality k(n, o) join pg_attribute a on a.attrelid = con.conrelid and a.attnum = k.n order by k.o)::text[] as columns,
                array(select a.attname from unnest(con.confkey) with ordinality k(n, o) join pg_attribute a on a.attrelid = con.confrelid and a.attnum = k.n order by k.o)::text[] as "refColumns"
         from pg_constraint con join pg_class t on t.oid = con.conrelid join pg_namespace n on n.oid = t.relnamespace
         join pg_class rt on rt.oid = con.confrelid join pg_namespace rn on rn.oid = rt.relnamespace
         where con.contype = 'f' and n.nspname = $1 and t.relname = $2 order by con.conname`,
        [schema, table],
      )
      return { columns: await columns(schema, table), indexes: idx.rows, foreignKeys: fks.rows }
    },
    async rows(o) {
      const q = sql.page(o)
      const page = await arr(q.sql)
      if (!o.where?.trim()) {
        const est = await pq(
          `select c.reltuples::bigint as n from pg_class c join pg_namespace n on n.oid = c.relnamespace where n.nspname = $1 and c.relname = $2`,
          [o.schema, o.table],
        )
        const guess = Number(est.rows[0]?.n ?? -1)
        if (guess > EXACT_COUNT_MAX) return { ...page, total: guess, estimated: true }
      }
      const exact = await pq(q.count)
      return { ...page, total: Number(exact.rows[0].n), estimated: false }
    },
    update: async (o) => tx([sql.update(o.schema, o.table, o.key, o.set)]),
    insert: async (o) => tx([sql.insert(o.schema, o.table, o.values)]),
    remove: async (o) => tx(o.keys.map((k) => sql.remove(o.schema, o.table, k))),
    query: (text) => arr(text),
    close: () => pool.end(),
  }
}

/**
 * @param {string} uri
 * @returns {Promise<Engine>}
 */
export async function openMysql(uri, { readOnly = false } = {}) {
  const mysql = await import('mysql2/promise')
  const pool = mysql.createPool({ uri: uri.replace(/^(mariadb|mysql2):/i, 'mysql:'), connectionLimit: 4, dateStrings: true, supportBigNumbers: true, bigNumberStrings: true, connectTimeout: 15_000 })
  await (await pool.getConnection()).release()
  /**
   * Every query this engine runs. Read-only (a shared studio): one statement
   * per call (mysql2 keeps multipleStatements off) inside its own
   * `START TRANSACTION READ ONLY`, always rolled back.
   * @param {any} cfg @param {unknown[]} [values]
   */
  const pq = async (cfg, values) => {
    if (!readOnly) return pool.query(cfg, values)
    const c = await pool.getConnection()
    try {
      await c.query('START TRANSACTION READ ONLY')
      return await c.query(cfg, values)
    } finally {
      await c.query('ROLLBACK').catch(() => {})
      c.release()
    }
  }
  const sql = dialect({
    q: bq,
    ph: () => '?',
    from: (s, t) => `${bq(s)}.${bq(t)}`,
    emptyInsert: (from) => `INSERT INTO ${from} () VALUES ()`,
  })

  /** @param {string} text @param {unknown[]} [values] */
  const arr = async (text, values) => {
    const [rows, fields] = await pq({ sql: text, values, rowsAsArray: true })
    if (!Array.isArray(rows)) {
      const h = /** @type {{ affectedRows?: number }} */ (rows)
      return { columns: [], rows: [], rowCount: h.affectedRows ?? null, command: text.trim().split(/\s+/)[0].toUpperCase() }
    }
    return {
      columns: (fields ?? []).map((f) => f.name),
      rows: rows.map((row) => /** @type {unknown[]} */ (row).map(cell)),
      rowCount: rows.length,
      command: 'SELECT',
    }
  }
  /** @param {Step[]} steps */
  const tx = async (steps) => {
    if (readOnly) throw new Error(READ_ONLY)
    const c = await pool.getConnection()
    let total = 0
    try {
      await c.beginTransaction()
      for (const s of steps) {
        const [r] = /** @type {any} */ (await c.query(s.sql, s.params))
        checkOne(r.affectedRows ?? 0, s)
        total += r.affectedRows ?? 0
      }
      await c.commit()
      return { rowCount: total }
    } catch (e) {
      await c.rollback().catch(() => {})
      throw e
    } finally {
      c.release()
    }
  }
  /** @param {string} schema @param {string} table */
  const columns = async (schema, table) => {
    const [r] = /** @type {any} */ (await pq(
      `select column_name as name, column_type as type, is_nullable = 'YES' as nullable, column_key = 'PRI' as primaryKey, column_default as \`default\`
       from information_schema.columns where table_schema = ? and table_name = ? order by ordinal_position`,
      [schema, table],
    ))
    return r.map((/** @type {any} */ c) => ({ ...c, nullable: !!c.nullable, primaryKey: !!c.primaryKey, default: c.default === 'NULL' ? null : c.default }))
  }

  return {
    engine: 'mysql',
    async info() {
      const [r] = /** @type {any} */ (await pq('select database() as db, version() as v'))
      const u = new URL(uri.replace(/^[a-z0-9+.-]+:/i, 'http:'))
      return { engine: 'mysql', database: r[0].db ?? '', host: u.hostname, version: r[0].v }
    },
    async tables() {
      const [r] = /** @type {any} */ (await pq(
        `select table_schema as \`schema\`, table_name as name, if(table_type = 'VIEW', 'view', 'table') as kind, table_rows as \`rows\`
         from information_schema.tables
         where table_schema = coalesce(database(), table_schema)
           and table_schema not in ('mysql', 'information_schema', 'performance_schema', 'sys')
         order by 1, 2`,
      ))
      return r.map((/** @type {any} */ t) => ({ ...t, rows: t.rows == null ? null : Number(t.rows) }))
    },
    columns,
    async structure(schema, table) {
      const [idx] = /** @type {any} */ (await pq(
        `select index_name as name, non_unique = 0 as \`unique\`, index_name = 'PRIMARY' as \`primary\`, group_concat(column_name order by seq_in_index separator '\\n') as cols
         from information_schema.statistics where table_schema = ? and table_name = ? group by index_name, non_unique order by \`primary\` desc, name`,
        [schema, table],
      ))
      const [fks] = /** @type {any} */ (await pq(
        `select constraint_name as name, referenced_table_schema as refSchema, referenced_table_name as refTable,
                group_concat(column_name order by ordinal_position separator '\\n') as cols,
                group_concat(referenced_column_name order by ordinal_position separator '\\n') as refCols
         from information_schema.key_column_usage
         where table_schema = ? and table_name = ? and referenced_table_name is not null
         group by constraint_name, referenced_table_schema, referenced_table_name order by name`,
        [schema, table],
      ))
      return {
        columns: await columns(schema, table),
        indexes: idx.map((/** @type {any} */ i) => ({ name: i.name, columns: String(i.cols).split('\n'), unique: !!i.unique, primary: !!i.primary })),
        foreignKeys: fks.map((/** @type {any} */ f) => ({ name: f.name, refSchema: f.refSchema, refTable: f.refTable, columns: String(f.cols).split('\n'), refColumns: String(f.refCols).split('\n') })),
      }
    },
    async rows(o) {
      const q = sql.page(o)
      const page = await arr(q.sql)
      if (!o.where?.trim()) {
        const [est] = /** @type {any} */ (await pq(
          'select table_rows as n from information_schema.tables where table_schema = ? and table_name = ?',
          [o.schema, o.table],
        ))
        const guess = Number(est[0]?.n ?? -1)
        if (guess > EXACT_COUNT_MAX) return { ...page, total: guess, estimated: true }
      }
      const [exact] = /** @type {any} */ (await pq(q.count))
      return { ...page, total: Number(exact[0].n), estimated: false }
    },
    update: async (o) => tx([sql.update(o.schema, o.table, o.key, o.set)]),
    insert: async (o) => tx([sql.insert(o.schema, o.table, o.values)]),
    remove: async (o) => tx(o.keys.map((k) => sql.remove(o.schema, o.table, k))),
    query: (text) => arr(text),
    close: () => pool.end(),
  }
}

/**
 * @param {string} file
 * @returns {Promise<Engine>}
 */
export async function openSqlite(file, { readOnly = false } = {}) {
  /** @type {typeof import('node:sqlite')} */
  let sqlite
  try {
    sqlite = await import('node:sqlite')
  } catch {
    throw new Error(`SQLite needs Node 22.5 or newer (this is ${process.version}). Or open the file in the Stroke app.`)
  }
  // Read-only (a shared studio): opened read-only at the file level, which no
  // statement can undo (PRAGMA query_only can be switched back off; this can't).
  const db = readOnly ? new sqlite.DatabaseSync(file, { readOnly: true }) : new sqlite.DatabaseSync(file)
  // One schema per file; the page still sends "main".
  const sql = dialect({ q: dq, ph: () => '?', from: (_s, t) => dq(t), emptyInsert: (from) => `INSERT INTO ${from} DEFAULT VALUES` })
  /** @param {unknown} v */
  const bind = (v) => (typeof v === 'boolean' ? Number(v) : v)

  /** @param {string} text @param {unknown[]} [params] */
  const arr = (text, params = []) => {
    const stmt = db.prepare(text)
    const cols = typeof stmt.columns === 'function' ? stmt.columns().map((c) => c.name) : null
    const returnsRows = cols ? cols.length > 0 : /^\s*(select|with|pragma|explain|values)\b/i.test(text)
    if (!returnsRows) {
      const r = stmt.run(.../** @type {any[]} */ (params.map(bind)))
      return { columns: [], rows: [], rowCount: Number(r.changes), command: text.trim().split(/\s+/)[0].toUpperCase() }
    }
    const objs = /** @type {Record<string, unknown>[]} */ (stmt.all(.../** @type {any[]} */ (params.map(bind))))
    const columns = cols ?? Object.keys(objs[0] ?? {})
    return { columns, rows: objs.map((o) => columns.map((c) => cell(o[c]))), rowCount: objs.length, command: 'SELECT' }
  }
  /** @param {Step[]} steps */
  const tx = async (steps) => {
    if (readOnly) throw new Error(READ_ONLY)
    let total = 0
    db.exec('BEGIN')
    try {
      for (const s of steps) {
        const n = Number(db.prepare(s.sql).run(.../** @type {any[]} */ (s.params.map(bind))).changes)
        checkOne(n, s)
        total += n
      }
      db.exec('COMMIT')
      return { rowCount: total }
    } catch (e) {
      try { db.exec('ROLLBACK') } catch { /* already ended */ }
      throw e
    }
  }
  /** @param {string} _schema @param {string} table */
  const columns = async (_schema, table) => {
    const r = db.prepare('select name, type, "notnull" as nn, pk, dflt_value as dflt from pragma_table_info(?)').all(table)
    return r.map((c) => ({ name: String(c.name), type: String(c.type || ''), nullable: !c.nn, primaryKey: Number(c.pk) > 0, default: c.dflt == null ? null : String(c.dflt) }))
  }

  return {
    engine: 'sqlite',
    async info() {
      const v = /** @type {any} */ (db.prepare('select sqlite_version() as v').get()).v
      return { engine: 'sqlite', database: file.split(/[\\/]/).pop() ?? file, host: 'local file', version: `SQLite ${v}` }
    },
    async tables() {
      const r = db.prepare(`select name, type from sqlite_schema where type in ('table', 'view') and name not like 'sqlite_%' order by name`).all()
      return r.map((t) => {
        const kind = t.type === 'view' ? 'view' : 'table'
        let rows = null
        if (kind === 'table') {
          try { rows = Number(/** @type {any} */ (db.prepare(`select count(*) as n from ${dq(String(t.name))}`).get()).n) } catch { /* virtual table */ }
        }
        return { schema: 'main', name: String(t.name), kind, rows }
      })
    },
    columns,
    async structure(schema, table) {
      const list = db.prepare('select name, "unique" as u, origin from pragma_index_list(?)').all(table)
      const indexes = list.map((i) => ({
        name: String(i.name),
        unique: !!i.u,
        primary: i.origin === 'pk',
        columns: db.prepare('select name from pragma_index_info(?) order by seqno').all(String(i.name)).map((c) => String(c.name)),
      }))
      /** @type {Map<number, any>} */
      const byId = new Map()
      for (const f of db.prepare('select id, "table" as t, "from" as f, "to" as c from pragma_foreign_key_list(?) order by id, seq').all(table)) {
        const id = Number(f.id)
        const fk = byId.get(id) ?? { name: `fk_${id}`, refSchema: 'main', refTable: String(f.t), columns: [], refColumns: [] }
        fk.columns.push(String(f.f))
        fk.refColumns.push(f.c == null ? '' : String(f.c))
        byId.set(id, fk)
      }
      return { columns: await columns(schema, table), indexes, foreignKeys: [...byId.values()] }
    },
    async rows(o) {
      const q = sql.page(o)
      const page = arr(q.sql)
      const total = Number(/** @type {any} */ (db.prepare(q.count).get()).n)
      return { ...page, total, estimated: false }
    },
    update: async (o) => tx([sql.update(o.schema, o.table, o.key, o.set)]),
    insert: async (o) => tx([sql.insert(o.schema, o.table, o.values)]),
    remove: async (o) => tx(o.keys.map((k) => sql.remove(o.schema, o.table, k))),
    async query(text) {
      // node:sqlite prepares one statement at a time; run all but the last
      // with exec and return the last one's rows, like the other consoles do.
      const parts = splitStatements(text)
      if (parts.length > 1 && readOnly) throw new Error('Run one statement at a time in a shared studio.')
      if (parts.length > 1) db.exec(parts.slice(0, -1).join(';\n'))
      return arr(parts[parts.length - 1] ?? text)
    },
    async close() {
      db.close()
    },
  }
}

/**
 * Split on `;` outside quotes and comments. Enough for a console; not a parser.
 * @param {string} text
 */
export function splitStatements(text) {
  /** @type {string[]} */
  const out = []
  let cur = ''
  for (let i = 0; i < text.length; i++) {
    const c = text[i]
    if (c === "'" || c === '"' || c === '`') {
      const end = text.indexOf(c, i + 1)
      const j = end < 0 ? text.length : end
      cur += text.slice(i, j + 1)
      i = j
    } else if (c === '-' && text[i + 1] === '-') {
      const end = text.indexOf('\n', i)
      i = end < 0 ? text.length : end - 1
    } else if (c === '/' && text[i + 1] === '*') {
      const end = text.indexOf('*/', i + 2)
      i = end < 0 ? text.length : end + 1
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
 * @param {{ readOnly?: boolean }} [opts]
 */
export function openEngine(engine, uri, opts = {}) {
  if (engine === 'postgres') return openPostgres(uri, opts)
  if (engine === 'mysql') return openMysql(uri, opts)
  return openSqlite(uri, opts)
}

const READ_ONLY = 'This shared studio is read-only.'
