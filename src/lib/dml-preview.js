/**
 * DML preview builders.
 *
 * The backend executes table edits/inserts/deletes with parameterized queries
 * (values are bound, never string-interpolated). For a human-readable "review
 * before applying" preview we inline the values here so the user can see exactly
 * what each write will do. The identifier quoting and statement shapes mirror
 * `src-tauri/src/db/*` per engine family; the inlined literals are for display
 * only and are NOT what gets sent to the database.
 *
 * @typedef {'postgres' | 'mysql' | 'sqlite' | 'd1' | 'libsql' | 'duckdb' | 'clickhouse' | 'mssql' | 'oracle'} Dialect
 *
 * @typedef {{ name: string, [k: string]: unknown }} Column
 *
 * @typedef {{
 *   dialect: Dialect,
 *   schema?: string,
 *   table: string,
 *   columns: Column[],
 *   primaryKey: string[],
 * }} DmlContext
 */

/** Engine families that don't qualify grid writes with a schema. */
const NO_SCHEMA = new Set(['sqlite', 'd1', 'libsql', 'duckdb'])

/**
 * Quote an identifier for the given dialect.
 * @param {string} name @param {Dialect} dialect
 */
export function quoteIdent(name, dialect) {
  const s = String(name)
  if (dialect === 'mysql') return '`' + s.replace(/`/g, '``') + '`'
  return '"' + s.replace(/"/g, '""') + '"'
}

/**
 * Schema-qualified, quoted table name.
 * @param {DmlContext} ctx
 */
export function qualifiedTable(ctx) {
  const t = quoteIdent(ctx.table, ctx.dialect)
  if (ctx.schema && !NO_SCHEMA.has(ctx.dialect)) {
    return quoteIdent(ctx.schema, ctx.dialect) + '.' + t
  }
  return t
}

/** Single-quote a string literal, doubling embedded quotes. */
function quoteString(/** @type {string} */ s) {
  return "'" + s.replace(/'/g, "''") + "'"
}

/**
 * Render a JS value as a SQL literal for display.
 * @param {unknown} value @param {Dialect} dialect
 */
export function sqlLiteral(value, dialect) {
  if (value === null || value === undefined) return 'NULL'
  if (typeof value === 'number') return Number.isFinite(value) ? String(value) : 'NULL'
  if (typeof value === 'bigint') return String(value)
  if (typeof value === 'boolean') {
    // SQLite/D1/libsql/MySQL have no boolean literal - they store 1/0.
    if (dialect === 'sqlite' || dialect === 'd1' || dialect === 'libsql' || dialect === 'mysql') {
      return value ? '1' : '0'
    }
    return value ? 'TRUE' : 'FALSE'
  }
  if (typeof value === 'object') {
    try { return quoteString(JSON.stringify(value)) } catch { return quoteString(String(value)) }
  }
  return quoteString(String(value))
}

/**
 * Map column name → index, for reading primary-key values out of a row array.
 * @param {Column[]} columns
 */
function nameIndex(columns) {
  /** @type {Map<string, number>} */
  const m = new Map()
  columns.forEach((c, i) => m.set(c.name, i))
  return m
}

/**
 * Build the `WHERE` clause that targets a single row by its primary key.
 * Returns null when the table has no primary key (writes aren't row-addressable).
 * @param {unknown[]} row @param {DmlContext} ctx @param {Map<string, number>} idx
 */
function pkWhere(row, ctx, idx) {
  if (!ctx.primaryKey.length) return null
  const parts = ctx.primaryKey.map((pk) => {
    const ci = idx.get(pk)
    const val = ci == null ? null : row?.[ci]
    const col = quoteIdent(pk, ctx.dialect)
    return val === null || val === undefined
      ? `${col} IS NULL`
      : `${col} = ${sqlLiteral(val, ctx.dialect)}`
  })
  return parts.join(' AND ')
}

/**
 * One UPDATE statement per staged cell edit - matching the backend, which
 * applies edits one cell at a time.
 * @param {{ rowIdx: number, colIdx: number, value: unknown }[]} edits
 * @param {unknown[][]} rows
 * @param {DmlContext} ctx
 * @returns {string[]}
 */
export function buildUpdateStatements(edits, rows, ctx) {
  const idx = nameIndex(ctx.columns)
  const tbl = qualifiedTable(ctx)
  /** @type {string[]} */
  const out = []
  for (const e of edits) {
    const col = ctx.columns[e.colIdx]
    if (!col) continue
    const where = pkWhere(rows[e.rowIdx], ctx, idx)
    const setClause = `${quoteIdent(col.name, ctx.dialect)} = ${sqlLiteral(e.value, ctx.dialect)}`
    out.push(where
      ? `UPDATE ${tbl} SET ${setClause} WHERE ${where};`
      : `UPDATE ${tbl} SET ${setClause} WHERE /* no primary key */;`)
  }
  return out
}

/**
 * One UPDATE per edited row, setting all of that row's changed columns -
 * matching the atomic apply (`pg_apply_table_changes`), which writes a row in a
 * single statement. Rows keep their first-edit order; columns are sorted by
 * name, as the backend sorts them.
 * @param {{ rowIdx: number, colIdx: number, value: unknown }[]} edits
 * @param {unknown[][]} rows
 * @param {DmlContext} ctx
 * @returns {string[]}
 */
export function buildRowUpdateStatements(edits, rows, ctx) {
  const idx = nameIndex(ctx.columns)
  const tbl = qualifiedTable(ctx)
  /** @type {Map<number, Map<string, unknown>>} */
  const byRow = new Map()
  for (const e of edits) {
    const col = ctx.columns[e.colIdx]
    if (!col) continue
    let sets = byRow.get(e.rowIdx)
    if (!sets) byRow.set(e.rowIdx, (sets = new Map()))
    sets.set(col.name, e.value)
  }
  /** @type {string[]} */
  const out = []
  for (const [rowIdx, sets] of byRow) {
    const setClause = [...sets.keys()]
      .sort()
      .map((name) => `${quoteIdent(name, ctx.dialect)} = ${sqlLiteral(sets.get(name), ctx.dialect)}`)
      .join(', ')
    const where = pkWhere(rows[rowIdx], ctx, idx)
    out.push(`UPDATE ${tbl} SET ${setClause} WHERE ${where ?? '/* no primary key */'};`)
  }
  return out
}

/**
 * Per-row DELETEs, one statement per key - matching the atomic apply, which
 * deletes row by row so it can check each one still exists.
 * @param {number[]} rowIndices
 * @param {unknown[][]} rows
 * @param {DmlContext} ctx
 * @returns {string[]}
 */
export function buildRowDeleteStatements(rowIndices, rows, ctx) {
  const idx = nameIndex(ctx.columns)
  const tbl = qualifiedTable(ctx)
  return rowIndices.map((ri) => `DELETE FROM ${tbl} WHERE ${pkWhere(rows[ri], ctx, idx) ?? '/* no primary key */'};`)
}

/**
 * DELETE statements for the given row indices. Collapses to a single `IN (…)`
 * when the table has a single-column primary key (matching the backend).
 * @param {number[]} rowIndices
 * @param {unknown[][]} rows
 * @param {DmlContext} ctx
 * @returns {string[]}
 */
export function buildDeleteStatements(rowIndices, rows, ctx) {
  const idx = nameIndex(ctx.columns)
  const tbl = qualifiedTable(ctx)
  if (!ctx.primaryKey.length) {
    return [`DELETE FROM ${tbl} WHERE /* no primary key */;`]
  }
  if (ctx.primaryKey.length === 1) {
    const pk = ctx.primaryKey[0]
    const ci = idx.get(pk)
    const vals = rowIndices.map((ri) => sqlLiteral(ci == null ? null : rows[ri]?.[ci], ctx.dialect))
    return [`DELETE FROM ${tbl} WHERE ${quoteIdent(pk, ctx.dialect)} IN (${vals.join(', ')});`]
  }
  return rowIndices.map((ri) => `DELETE FROM ${tbl} WHERE ${pkWhere(rows[ri], ctx, idx)};`)
}

/**
 * INSERT statement for a new-row draft (record of column name → value).
 * @param {Record<string, unknown>} values
 * @param {DmlContext} ctx
 * @returns {string[]}
 */
export function buildInsertStatements(values, ctx) {
  const tbl = qualifiedTable(ctx)
  const names = Object.keys(values)
  if (!names.length) return [`INSERT INTO ${tbl} DEFAULT VALUES;`]
  const cols = names.map((n) => quoteIdent(n, ctx.dialect)).join(', ')
  const vals = names.map((n) => sqlLiteral(values[n], ctx.dialect)).join(', ')
  return [`INSERT INTO ${tbl} (${cols}) VALUES (${vals});`]
}
