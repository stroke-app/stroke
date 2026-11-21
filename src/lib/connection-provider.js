/**
 * Which hosting provider a saved connection belongs to, and a short engine name
 * for its chip in the connection list.
 *
 * A connection made through a provider's sign-in carries `provider`. One made by
 * pasting a URL does not, so the host is the fallback: `*.db.thenile.dev` is Nile
 * whether it came through the Nile card or a clipboard.
 */

/** @type {Array<[RegExp, string]>} host pattern → provider id (a DbIcon id) */
const HOSTS = [
  [/\.neon\.(tech|build)$/i, 'neon'],
  [/(\.supabase\.(co|com)|pooler\.supabase\.com)$/i, 'supabase'],
  [/\.prisma-data\.(net|com)$/i, 'prisma'],
  [/(\.psdb\.cloud|\.planetscale\.(com|sh))$/i, 'planetscale'],
  [/\.tidbcloud\.com$/i, 'tidb'],
  [/\.turso\.io$/i, 'turso'],
  [/(\.rlwy\.net|\.railway\.app|\.railway\.internal)$/i, 'railway'],
  [/\.thenile\.dev$/i, 'nile'],
  [/\.upstash\.io$/i, 'upstash'],
  [/\.xata\.tech$/i, 'xata'],
  [/\.svc\.singlestore\.com$/i, 'singlestore'],
]

/** Older saved connections used these ids before the cards settled on theirs. */
const ALIASES = { 'prisma-postgres': 'prisma' }

/** @param {string | undefined} url */
function hostOfUrl(url) {
  try {
    return new URL(String(url ?? '')).hostname
  } catch {
    return ''
  }
}

/**
 * @param {{ type?: string, provider?: string, host?: string, url?: string }} conn
 * @returns {string | null} provider id, or null for a self-hosted / local connection
 */
export function providerOf(conn) {
  if (!conn) return null
  if (conn.provider) return ALIASES[/** @type {keyof ALIASES} */ (conn.provider)] ?? conn.provider
  if (conn.type === 'd1') return 'd1'
  if (conn.type === 'posthog') return 'posthog'
  const host = conn.host || hostOfUrl(conn.url)
  if (!host) return null
  for (const [re, id] of HOSTS) if (re.test(host)) return id
  return null
}

import { ODBC_PRESETS, odbcDriverId } from '$lib/odbc-presets.js'

/** @type {Record<string, string>} */
const ENGINE_SHORT = {
  postgres: 'Postgres',
  cockroachdb: 'CockroachDB',
  yugabytedb: 'YugabyteDB',
  alloydb: 'AlloyDB',
  mysql: 'MySQL',
  mariadb: 'MariaDB',
  mssql: 'SQL Server',
  oracle: 'Oracle',
  sqlite: 'SQLite',
  'sqlite-memory': 'SQLite',
  libsql: 'libSQL',
  duckdb: 'DuckDB',
  'duckdb-memory': 'DuckDB',
  clickhouse: 'ClickHouse',
  redis: 'Redis',
  valkey: 'Valkey',
  dynamodb: 'DynamoDB',
  couchdb: 'CouchDB',
  elasticsearch: 'Elasticsearch',
  opensearch: 'OpenSearch',
  cassandra: 'Cassandra',
  scylladb: 'ScyllaDB',
  firestore: 'Firestore',
  spanner: 'Spanner',
  bigquery: 'BigQuery',
  snowflake: 'Snowflake',
  odbc: 'ODBC',
  ...Object.fromEntries(ODBC_PRESETS.map((p) => [`odbc-${p.id}`, p.label])),
  mongodb: 'MongoDB',
  d1: 'D1',
  posthog: 'HogQL',
}

/**
 * The engine a saved connection shows as: its type, except a Cassandra
 * connection made for ScyllaDB. ScyllaDB speaks CQL and runs on the Cassandra
 * engine end to end, so the type stays `cassandra` and only the name and mark
 * say ScyllaDB.
 * @param {{ type?: string, flavor?: string } | null | undefined} c
 * @returns {string}
 */
export function engineId(c) {
  if (c?.type === 'cassandra' && c?.flavor === 'scylladb') return 'scylladb'
  // ODBC connections show as the engine they reach: `odbc-db2`, `odbc-hana`.
  if (c?.type === 'odbc') return odbcDriverId(/** @type {any} */ (c).preset)
  return c?.type ?? ''
}

/** Short engine name for the list chip ("Postgres", "Redis"). @param {string | undefined} type */
export function engineLabel(type) {
  return ENGINE_SHORT[String(type ?? '')] ?? String(type ?? '')
}
