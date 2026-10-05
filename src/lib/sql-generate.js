/**
 * Statement templates for the "Generate SQL" dialog - skeleton queries for a
 * table with `:name` placeholders where values would go. Identifier quoting
 * and schema qualification reuse the grid's DML rules (dml-preview.js) so the
 * generated SQL matches what the app itself would execute per engine.
 */
import { quoteIdent as dmlQuoteIdent, qualifiedTable as dmlQualifiedTable } from './dml-preview.js'
import { quoteName, tableRef } from './sql-ident.js'

/** @typedef {import('./dml-preview.js').Dialect} Dialect */
/** @typedef {{ name: string, dataType?: string, nullable?: boolean }} GenColumn */
/**
 * @typedef {{
 *   dialect: Dialect,
 *   schema?: string,
 *   table: string,
 *   columns: GenColumn[],
 *   primaryKey: string[],
 *   quote?: import('./sql-ident.js').QuoteMode,
 *   qualify?: boolean,
 * }} GenContext
 * `quote` and `qualify` follow Settings → SQL editor (Quote object names,
 * Qualify tables with their schema). Without `quote`, names are written the
 * way the grid writes them when it saves.
 */

/** @param {string} name @param {GenContext} ctx */
const quoteIdent = (name, ctx) => (ctx.quote ? quoteName(name, ctx.dialect, ctx.quote) : dmlQuoteIdent(name, ctx.dialect))

/**
 * The table, qualified the grid's way: never for the engines whose grid
 * writes take no schema (the SQLite family, DuckDB).
 * @param {GenContext} ctx
 */
function qualifiedTable(ctx) {
  if (!ctx.quote) return dmlQualifiedTable(ctx)
  const gridQualifies = !!ctx.schema && dmlQualifiedTable(ctx) !== dmlQualifiedTable({ ...ctx, schema: '' })
  const schema = gridQualifies ? ctx.schema : ''
  return tableRef({ schema, table: ctx.table, engine: ctx.dialect, mode: ctx.quote, qualify: ctx.qualify !== false })
}

/** @param {string} name */
const ph = (name) => `:${name}`

/** @param {GenContext} ctx */
const pkConditions = (ctx) =>
  ctx.primaryKey.map((k) => `${quoteIdent(k, ctx)} = ${ph(k)}`).join('\n  AND ')

/**
 * WHERE clause targeting the primary key - or the always-false `1 = 0` guard
 * when the table has none, so a pasted statement can't touch every row.
 * @param {GenContext} ctx
 */
const whereClause = (ctx) => (ctx.primaryKey.length ? `WHERE ${pkConditions(ctx)};` : 'WHERE 1 = 0;')

/** @param {GenContext} ctx */
export function genSelectStar(ctx) {
  const tbl = qualifiedTable(ctx)
  if (ctx.dialect === 'mssql') return `SELECT TOP 100 *\nFROM ${tbl};`
  return `SELECT *\nFROM ${tbl}\nLIMIT 100;`
}

/** @param {GenContext} ctx */
export function genSelectFields(ctx) {
  const tbl = qualifiedTable(ctx)
  const cols = ctx.columns.map((c) => `  ${quoteIdent(c.name, ctx)}`).join(',\n')
  const tail = ctx.dialect === 'mssql' ? ';' : '\nLIMIT 100;'
  const head = ctx.dialect === 'mssql' ? 'SELECT TOP 100' : 'SELECT'
  return `${head}\n${cols}\nFROM ${tbl}${tail}`
}

/** @param {GenContext} ctx */
export function genInsert(ctx) {
  const tbl = qualifiedTable(ctx)
  const cols = ctx.columns.map((c) => `  ${quoteIdent(c.name, ctx)}`).join(',\n')
  const vals = ctx.columns.map((c) => `  ${ph(c.name)}`).join(',\n')
  return `INSERT INTO ${tbl} (\n${cols}\n) VALUES (\n${vals}\n);`
}

/** @param {GenContext} ctx */
export function genUpdate(ctx) {
  const tbl = qualifiedTable(ctx)
  const pkSet = new Set(ctx.primaryKey)
  // Non-PK columns go in SET; for pure-key tables (join tables) fall back to all.
  let setCols = ctx.columns.filter((c) => !pkSet.has(c.name))
  if (setCols.length === 0) setCols = ctx.columns
  const sets = setCols.map((c) => `  ${quoteIdent(c.name, ctx)} = ${ph(c.name)}`).join(',\n')
  const where = setCols === ctx.columns ? 'WHERE 1 = 0;' : whereClause(ctx)
  return `UPDATE ${tbl}\nSET\n${sets}\n${where}`
}

/** @param {GenContext} ctx */
export function genDelete(ctx) {
  return `DELETE FROM ${qualifiedTable(ctx)}\n${whereClause(ctx)}`
}

/**
 * Engine-specific upsert - null when the table has no primary key or the
 * dialect has no native upsert form.
 * @param {GenContext} ctx
 */
export function genUpsert(ctx) {
  if (ctx.primaryKey.length === 0) return null
  const tbl = qualifiedTable(ctx)
  const q = (/** @type {string} */ n) => quoteIdent(n, ctx)
  const cols = ctx.columns.map((c) => `  ${q(c.name)}`).join(',\n')
  const vals = ctx.columns.map((c) => `  ${ph(c.name)}`).join(',\n')
  const pkSet = new Set(ctx.primaryKey)
  const updatable = ctx.columns.filter((c) => !pkSet.has(c.name))
  if (updatable.length === 0) return null

  if (ctx.dialect === 'mysql') {
    const sets = updatable.map((c) => `  ${q(c.name)} = VALUES(${q(c.name)})`).join(',\n')
    return `INSERT INTO ${tbl} (\n${cols}\n) VALUES (\n${vals}\n)\nON DUPLICATE KEY UPDATE\n${sets};`
  }
  if (ctx.dialect === 'mssql' || ctx.dialect === 'clickhouse') return null

  // Postgres / SQLite family (sqlite, d1, libsql, duckdb)
  const conflict = ctx.primaryKey.map(q).join(', ')
  const sets = updatable.map((c) => `  ${q(c.name)} = EXCLUDED.${q(c.name)}`).join(',\n')
  return `INSERT INTO ${tbl} (\n${cols}\n) VALUES (\n${vals}\n)\nON CONFLICT (${conflict}) DO UPDATE SET\n${sets};`
}
