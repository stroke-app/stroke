/**
 * Which schemas are the engine's own, rather than the user's data.
 *
 * The backend returns them all - `pg_catalog` and `information_schema` are real,
 * browsable schemas, and a client that hides the catalog outright cannot answer
 * "what does the server think this table looks like" without dropping to SQL.
 * What they are not is where anyone keeps their tables, so the picker hides them
 * until asked. One list, used by the picker and by anything else that has to
 * make the same call.
 */
const SYSTEM_SCHEMAS = new Set([
  // Postgres / CockroachDB
  'pg_catalog',
  'pg_toast',
  'crdb_internal',
  // Postgres extensions that keep their own bookkeeping in a schema:
  // TimescaleDB (plus every `_timescaledb_*` schema, below) and PostGIS.
  'timescaledb_information',
  'timescaledb_experimental',
  'topology',
  'tiger',
  'tiger_data',
  // Cross-engine (SQL standard)
  'information_schema',
  // MySQL / MariaDB
  'mysql',
  'performance_schema',
  'sys',
  // SQL Server
  'guest',
  // ClickHouse; also Oracle's SYSTEM user
  'system',
  // Oracle: users the database itself creates and maintains
  'anonymous', 'appqossys', 'audsys', 'ctxsys', 'dbsfwuser', 'dbsnmp', 'dgpdb_int', 'dip', 'dvf', 'dvsys',
  'ggsharedcap', 'ggsys', 'gsmadmin_internal', 'gsmcatuser', 'gsmrootuser', 'gsmuser', 'lbacsys', 'mddata',
  'mdsys', 'ojvmsys', 'olapsys', 'opsg_admin', 'oracle_ocm', 'orddata', 'ordplugins', 'ordsys', 'outln',
  'remote_scheduler_agent', 'si_informtn_schema', 'sys$umf', 'sysbackup', 'sysdg', 'syskm', 'sysrac',
  'vecsys', 'wmsys', 'xdb', 'xs$null', 'baasys',
])

/** @param {string} name */
export function isSystemSchema(name) {
  const n = String(name ?? '').toLowerCase()
  if (!n) return false
  if (SYSTEM_SCHEMAS.has(n)) return true
  // Per-session and per-relation storage: generated names, nothing to browse.
  // TimescaleDB's catalog, cache, config and chunk schemas.
  return n.startsWith('pg_temp_') || n.startsWith('pg_toast_') || n.startsWith('_timescaledb_')
}

/**
 * Split a schema list into the user's and the engine's, keeping the order the
 * server returned within each.
 * @param {string[]} schemas
 */
export function splitSchemas(schemas) {
  /** @type {string[]} */ const user = []
  /** @type {string[]} */ const system = []
  for (const s of schemas ?? []) (isSystemSchema(s) ? system : user).push(s)
  return { user, system }
}
