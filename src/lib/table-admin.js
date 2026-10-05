// Dropping and truncating a table or view, spelled per engine.
//
// The backend builds and runs the real statement (src-tauri/src/db/admin.rs);
// this is the same spelling for the confirm dialog, so what it shows is what
// runs. The two are tested against the same cases: change one, change both.

import { engineFamily } from '$lib/stores/connections.js'

/** @typedef {'postgres' | 'mysql' | 'sqlite' | 'duckdb' | 'mssql' | 'clickhouse'} TableDialect */
/** @typedef {'table' | 'view' | 'materialized_view'} ObjectKind */

/**
 * The dialect a connection's DDL is written in, or null where there is no DDL:
 * Redis lists keys, and PostHog is read-only.
 * @param {string | null | undefined} type a saved connection's `type`
 * @returns {TableDialect | null}
 */
export function tableDialect(type) {
  const family = engineFamily(type)
  if (family === 'postgres' || family === 'mysql' || family === 'duckdb' || family === 'mssql' || family === 'clickhouse') return family
  if (family === 'sqlite' || family === 'd1' || family === 'libsql') return 'sqlite'
  return null
}

/** CASCADE does something on Postgres and DuckDB only. @param {TableDialect | null} d */
export function supportsCascade(d) {
  return d === 'postgres' || d === 'duckdb'
}

/** @param {TableDialect} d @param {string} ident */
function quote(d, ident) {
  const s = String(ident)
  if (d === 'mysql' || d === 'clickhouse') return `\`${s.replace(/`/g, '``')}\``
  if (d === 'mssql') return `[${s.replace(/]/g, ']]')}]`
  return `"${s.replace(/"/g, '""')}"`
}

/** SQLite and DuckDB connections only list `main`, so their names stay bare.
 *  @param {TableDialect} d @param {string} schema @param {string} name */
function qualified(d, schema, name) {
  const s = String(schema ?? '').trim()
  if (d === 'sqlite' || d === 'duckdb' || !s) return quote(d, name)
  return `${quote(d, s)}.${quote(d, name)}`
}

/**
 * @param {TableDialect} d @param {string} schema @param {string} name
 * @param {{ kind?: ObjectKind, cascade?: boolean }} [opts]
 */
export function dropObjectSql(d, schema, name, { kind = 'table', cascade = false } = {}) {
  let noun = kind === 'view' ? 'VIEW' : 'TABLE'
  if (kind === 'materialized_view') {
    if (d === 'postgres') noun = 'MATERIALIZED VIEW'
    else if (d === 'clickhouse') noun = 'VIEW'
    else throw new Error('This database has no materialized views')
  }
  return `DROP ${noun} ${qualified(d, schema, name)}${cascade && supportsCascade(d) ? ' CASCADE' : ''}`
}

/** SQLite has no TRUNCATE; DuckDB's is the same DELETE.
 *  @param {TableDialect} d @param {string} schema @param {string} table */
export function truncateTableSql(d, schema, table) {
  const target = qualified(d, schema, table)
  return d === 'sqlite' || d === 'duckdb' ? `DELETE FROM ${target}` : `TRUNCATE TABLE ${target}`
}
