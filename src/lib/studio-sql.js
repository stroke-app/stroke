/**
 * The SQL a shared web studio runs, per engine: which engines it shares,
 * how each quotes a name, and how each pages. Kept free of the app's API so
 * it can be tested on its own.
 */
import { odbcQuote } from '$lib/sql-ident.js'

/**
 * The engine a shared studio reports, by the app's connection types: every
 * SQL engine. The studio page quotes, pages and highlights by this. Document,
 * key-value and search engines have no SQL to share this way.
 */
export function studioEngine(/** @type {string} */ type) {
  const t = String(type || '').toLowerCase()
  if (['postgres', 'postgresql', 'cockroach', 'cockroachdb', 'supabase', 'neon', 'timescale', 'yugabyte'].includes(t)) return 'postgres'
  if (['mysql', 'mariadb', 'tidb', 'planetscale'].includes(t)) return 'mysql'
  if (['sqlite', 'd1', 'libsql'].includes(t)) return 'sqlite'
  if (['mssql', 'oracle', 'duckdb', 'clickhouse', 'spanner', 'bigquery', 'snowflake', 'odbc'].includes(t)) return t
  return null
}

/** How an engine quotes a name. @param {string} engine @param {string} id */
export function studioQuote(engine, id) {
  const ch = engine === 'odbc' ? odbcQuote() : ['mysql', 'spanner', 'bigquery'].includes(engine) ? '`' : engine === 'mssql' ? '[' : '"'
  if (ch === '[') return '[' + id.replace(/]/g, ']]') + ']'
  return ch + id.split(ch).join(ch + ch) + ch
}

/**
 * The page query for an engine. LIMIT/OFFSET is the common form; SQL Server,
 * Oracle and ODBC databases that refuse it take OFFSET/FETCH, which SQL Server
 * wants under an ORDER BY.
 * @param {string} engine @param {string} base SELECT ... FROM ... WHERE ... @param {string} order
 * @param {number} limit @param {number} offset @param {boolean} [fetchForm]
 */
export function studioPageSql(engine, base, order, limit, offset, fetchForm = false) {
  if (engine === 'mssql' || engine === 'oracle' || fetchForm) {
    const by = order || (engine === 'mssql' || fetchForm ? ' ORDER BY (SELECT NULL)' : '')
    return `${base}${by} OFFSET ${offset} ROWS FETCH NEXT ${limit} ROWS ONLY`
  }
  return `${base}${order} LIMIT ${limit} OFFSET ${offset}`
}
