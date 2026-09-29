/**
 * Per-engine feature capabilities.
 *
 * Several introspection features (enums, triggers, sequences, functions, RLS)
 * are PostgreSQL-only. The Rust backend returns an empty list for those on
 * other engines rather than erroring - which, on its own, leaves the UI showing
 * a blank pane that's indistinguishable from "this Postgres schema genuinely has
 * no triggers." These flags let the UI say *why* a section is empty ("Triggers
 * require PostgreSQL") and let callers skip pointless round-trips entirely.
 *
 * @typedef {'postgres'|'mysql'|'sqlite'|'d1'|'libsql'|'clickhouse'|'duckdb'|'mssql'|'redis'|'posthog'} Engine
 */

/**
 * Feature → set of engines that support it. Kept in sync with the backend
 * dispatch in `src-tauri/src/db/schema.rs`.
 * @type {Record<string, Set<string>>}
 */
const SUPPORT = {
  // Index introspection has a backend branch for every engine.
  indexes:   new Set(['postgres', 'mysql', 'sqlite', 'd1', 'libsql', 'clickhouse', 'duckdb', 'mssql']),
  // Views are derived from table `kind`, available wherever tables are listed.
  views:     new Set(['postgres', 'mysql', 'sqlite', 'd1', 'libsql', 'clickhouse', 'duckdb', 'mssql', 'posthog']),
  // PostgreSQL-only introspection (pg_catalog / pg_proc / pg_policy …).
  triggers:  new Set(['postgres']),
  sequences: new Set(['postgres']),
  enums:     new Set(['postgres']),
  functions: new Set(['postgres']),
  matviews:  new Set(['postgres']),
  roles:     new Set(['postgres']),
  rls:       new Set(['postgres']),
  // Live mode (change notifications / polling) has backend support here.
  liveMode:  new Set(['postgres', 'sqlite']),
  // Inline structure editing (ALTER COLUMN TYPE … USING, COMMENT ON COLUMN,
  // double-quoted identifiers) generates PostgreSQL-only DDL. Other engines use
  // incompatible syntax (MySQL MODIFY COLUMN; SQLite has no ALTER COLUMN), so we
  // gate the editing affordances there and point users to the SQL console.
  editStructure: new Set(['postgres']),
}

/**
 * Saved-connection type aliases → the backend driver the capabilities map keys
 * on. MariaDB is driven by the MySQL backend, and CockroachDB by the Postgres
 * one, but the frontend keeps their distinct type so the UI can label them -
 * so map them back here before any capability lookup.
 * @type {Record<string, string>}
 */
const ENGINE_ALIAS = {
  mariadb: 'mysql',
  cockroachdb: 'postgres',
}

/** @param {string | null | undefined} engine */
export function normalizeEngine(engine) {
  if (!engine) return engine
  return ENGINE_ALIAS[engine] ?? engine
}

/**
 * Is `feature` available on the given engine?
 * Unknown/missing engine → false (fail closed, so we never call an unsupported command).
 * @param {string} feature
 * @param {string | null | undefined} engine
 * @returns {boolean}
 */
export function engineSupports(feature, engine) {
  const e = normalizeEngine(engine)
  if (!e) return false
  return SUPPORT[feature]?.has(e) ?? false
}

/** Human-friendly engine name for messages. */
const ENGINE_LABEL = /** @type {Record<string, string>} */ ({
  postgres: 'PostgreSQL',
  mysql: 'MySQL',
  sqlite: 'SQLite',
  d1: 'Cloudflare D1',
  libsql: 'libSQL',
  clickhouse: 'ClickHouse',
  posthog: 'PostHog',
  duckdb: 'DuckDB',
  mssql: 'SQL Server',
  mariadb: 'MariaDB',
  cockroachdb: 'CockroachDB',
  redis: 'Redis',
})

/** @param {string | null | undefined} engine */
export function engineLabel(engine) {
  return (engine && ENGINE_LABEL[engine]) || 'this database'
}

/**
 * A one-line "why is this empty/unavailable" message for an unsupported feature.
 * @param {string} featureLabel  user-facing feature name, e.g. "Triggers"
 * @param {string} feature       capability key, e.g. "triggers"
 */
export function unsupportedReason(featureLabel, feature) {
  const engines = [...(SUPPORT[feature] ?? [])].map((e) => ENGINE_LABEL[e] ?? e)
  if (engines.length === 0) return `${featureLabel} are not available here.`
  if (engines.length === 1) return `${featureLabel} require ${engines[0]}.`
  const last = engines.pop()
  return `${featureLabel} are only available on ${engines.join(', ')} and ${last}.`
}

/**
 * Key-value (non-SQL) engines. These sit outside the SUPPORT feature sets so
 * SQL-oriented features auto-disable; later phases key their key-value UI off
 * this set.
 * @type {Set<string>}
 */
export const KEY_VALUE_ENGINES = new Set(['redis'])

/** @param {string | null | undefined} engine */
export function isKeyValue(engine) {
  return KEY_VALUE_ENGINES.has(engine)
}
