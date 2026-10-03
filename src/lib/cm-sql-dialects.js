import { SQLDialect, PostgreSQL, MySQL, SQLite, MSSQL, StandardSQL } from '@codemirror/lang-sql'
import { PG_KEYWORDS } from '$lib/sql-complete-data.js'

/**
 * lang-sql's dialects, with the keyword list narrowed to the words that are
 * keywords in practice. Its own lists include every non-reserved word -
 * `id`, `name`, `type`, `user`, `value` - so ordinary column names came out
 * coloured as keywords. Type names keep their own colour.
 */
const HIGHLIGHT_KEYWORDS = [
  ...PG_KEYWORDS,
  'CONFLICT', 'DO', 'NOTHING', 'EXCLUDED', 'INTERVAL', 'IF', 'ONLY', 'REPLACE', 'SCHEMA', 'COLUMN',
  'ADD', 'RENAME', 'TO', 'GRANT', 'REVOKE', 'CASCADE', 'RESTRICT', 'LOCK', 'FOR', 'SHOW', 'DESCRIBE',
  'PRAGMA', 'AUTO_INCREMENT', 'AUTOINCREMENT', 'SERIAL', 'IDENTITY', 'GENERATED', 'ALWAYS', 'ANY', 'SOME',
  'MATERIALIZED', 'REFRESH', 'TRIGGER', 'FUNCTION', 'PROCEDURE', 'RETURNS', 'LANGUAGE', 'DECLARE',
].join(' ').toLowerCase()
/** @param {SQLDialect} base */
const narrowed = (base) => SQLDialect.define({ ...base.spec, keywords: HIGHLIGHT_KEYWORDS })
const PG = narrowed(PostgreSQL)
const MY = narrowed(MySQL)
const LITE = narrowed(SQLite)
const narrowedStandard = narrowed(StandardSQL)
/** The app's Dialect ids, onto those dialects. */
const SQL_DIALECTS = /** @type {Record<string, SQLDialect>} */ ({
  postgres: PG, duckdb: PG,
  mysql: MY, mariadb: MY,
  sqlite: LITE, d1: LITE, libsql: LITE,
  mssql: narrowed(MSSQL),
})

/** The highlighting dialect for an app Dialect id. @param {string} id */
export function sqlDialectFor(id) {
  return SQL_DIALECTS[id] ?? narrowedStandard
}
