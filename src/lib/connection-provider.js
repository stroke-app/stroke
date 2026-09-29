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

/** @type {Record<string, string>} */
const ENGINE_SHORT = {
  postgres: 'Postgres',
  cockroachdb: 'CockroachDB',
  mysql: 'MySQL',
  mariadb: 'MariaDB',
  mssql: 'SQL Server',
  sqlite: 'SQLite',
  'sqlite-memory': 'SQLite',
  libsql: 'libSQL',
  duckdb: 'DuckDB',
  'duckdb-memory': 'DuckDB',
  clickhouse: 'ClickHouse',
  redis: 'Redis',
  d1: 'D1',
  posthog: 'HogQL',
}

/** Short engine name for the list chip ("Postgres", "Redis"). @param {string | undefined} type */
export function engineLabel(type) {
  return ENGINE_SHORT[String(type ?? '')] ?? String(type ?? '')
}
