// Listing the OTHER databases reachable from the current connection, and the
// rules for which connections can switch between them at all.
//
// Lifted out of StatusBar so the sidebar's Databases section and the status
// bar's switcher share one implementation - two per-engine dispatches would
// drift the moment an engine gained or lost support.

import { executeSql, cloudflareListD1Databases } from '$lib/api.js'
import { providerListDatabases } from '$lib/providers.js'
import { engineFamily } from '$lib/stores/connections.js'

/** @typedef {{ key: string, label: string, onServer?: boolean }} DatabaseEntry `onServer`: on this provider's server but not in its API; switch by name. */
/** @typedef {import('$lib/stores/connections.js').SavedConnection} Conn */

/**
 * How this connection switches databases, or null when it cannot.
 *
 * Provider is checked first: a Supabase/Neon connection is ALSO postgres, but
 * its sibling databases come from the provider API, not pg_catalog. Redis is
 * excluded outright - it addresses numbered logical DBs, which is not the same
 * operation - and a Redis connection may carry a stale `provider` field from an
 * earlier edit, so it has to be ruled out before the provider check.
 * @param {Conn | null | undefined} conn
 * @returns {'provider' | 'postgres' | 'mysql' | 'mssql' | 'clickhouse' | 'd1' | null}
 */
export function dbSwitchKind(conn) {
  if (!conn) return null
  const family = engineFamily(conn.type)
  if (family === 'redis') return null
  // MySQL-family providers (TiDB, PlanetScale) hand back a server or cluster;
  // the databases worth switching between are the ones inside it, which the
  // provider API does not list. TiDB connects with none selected, so a database
  // made there (by you or the AI) was unreachable from the switcher.
  if (conn.provider && family !== 'mysql') return 'provider'
  if (family === 'postgres') return 'postgres'
  if (family === 'mysql') return 'mysql'
  if (family === 'mssql') return 'mssql'
  if (family === 'clickhouse') return 'clickhouse'
  if (conn.type === 'd1') return conn.accountId ? 'd1' : null
  return null
}

/** @param {Conn | null | undefined} conn */
export function canSwitchDatabase(conn) {
  return dbSwitchKind(conn) !== null
}

/**
 * Key identifying the database currently connected to, matched against
 * `DatabaseEntry.key` to mark the active row. D1 identifies by uuid, everyone
 * else by name.
 * @param {Conn | null | undefined} conn
 */
export function currentDatabaseKey(conn) {
  if (!conn) return ''
  // Provider listings key by the provider's own id (Turso "org/name", Neon a
  // project path), not the database name, so the name never matched and the
  // database you were in offered to switch to itself.
  if (dbSwitchKind(conn) === 'provider' && /** @type {any} */ (conn).providerRef) return /** @type {any} */ (conn).providerRef
  if (conn.type === 'd1') return conn.databaseId ?? ''
  // A blank database connects to the server's default, so that row is current.
  if (conn.type === 'mssql') return conn.database || 'master'
  if (conn.type === 'clickhouse') return conn.database || 'default'
  return conn.database ?? conn.filePath ?? ''
}

/**
 * Whether a listed database is the one this connection is on. Keys first; a
 * provider connection saved before it kept its `providerRef` falls back to the
 * database name.
 * @param {DatabaseEntry} entry @param {Conn | null | undefined} conn
 */
export function isCurrentDatabase(entry, conn) {
  if (!conn) return false
  const key = currentDatabaseKey(conn)
  if (key && entry.key === key) return true
  return dbSwitchKind(conn) === 'provider' && !(/** @type {any} */ (conn).providerRef) && !!conn.database && entry.label === conn.database
}

/**
 * Drop entries that repeat a key.
 *
 * Two databases cannot share a name, so a duplicate here is always a decoding
 * fault upstream - and one that used to crash the whole sidebar: the list is
 * rendered by a keyed `{#each}`, and Svelte throws `each_key_duplicate` rather
 * than render two rows with one key. MySQL's `SHOW DATABASES` typed its column
 * as VARBINARY, so every name decoded to the placeholder `"[18 bytes]"` and the
 * panel died on the second row. The decoder is fixed; this keeps a switcher
 * that merely looks wrong from taking the panel with it.
 * @template {{ key: string }} T
 * @param {T[]} entries
 * @returns {T[]}
 */
/** Databases on a hosted Postgres server that belong to the platform, not the user. */
const SERVER_SYSTEM_DBS = new Set(['postgres', 'rdsadmin', 'azure_maintenance', 'azure_sys', 'cloudsqladmin', '_supabase'])

function dedupeByKey(entries) {
  const seen = new Set()
  return entries.filter((e) => {
    if (seen.has(e.key)) return false
    seen.add(e.key)
    return true
  })
}

/**
 * Databases reachable from this connection, sorted by name. Returns [] rather
 * than throwing: a switcher that cannot list is an empty switcher, not a broken
 * screen, and the caller has no better recovery than showing nothing.
 * @param {Conn | null | undefined} conn
 * @returns {Promise<DatabaseEntry[]>}
 */
export async function listDatabases(conn) {
  const kind = dbSwitchKind(conn)
  if (!kind || !conn) return []
  try {
    if (kind === 'provider' && conn.provider) {
      const dbs = await providerListDatabases(conn.provider)
      /** @type {DatabaseEntry[]} */
      const listed = (dbs ?? []).map((/** @type {{ db_ref: string, name: string }} */ d) => ({ key: d.db_ref, label: d.name }))
      // The provider API only knows the databases made through it. One made
      // with SQL on the same server (by you or the AI) is still reachable over
      // this login; list it too, switched by name on this server.
      if (engineFamily(conn.type) === 'postgres') {
        const names = new Set(listed.map((e) => e.label.split(/[\s/·]+/).pop()))
        try {
          const result = await executeSql(`SELECT datname FROM pg_catalog.pg_database WHERE datistemplate = false AND datallowconn ORDER BY datname`)
          for (const r of result?.rows ?? []) {
            const name = String(r[0])
            if (names.has(name) || name === conn.database || SERVER_SYSTEM_DBS.has(name)) continue
            listed.push({ key: `server:${name}`, label: name, onServer: true })
          }
        } catch { /* no rights to pg_database: the provider's list is what we have */ }
      }
      return sortByLabel(dedupeByKey(listed))
    }
    if (kind === 'postgres') {
      const result = await executeSql(
        `SELECT datname FROM pg_catalog.pg_database WHERE datistemplate = false ORDER BY datname`,
      )
      return dedupeByKey((result?.rows ?? []).map((r) => ({ key: String(r[0]), label: String(r[0]) })))
    }
    if (kind === 'mysql') {
      // pg_catalog does not exist here - MySQL/MariaDB list through SHOW.
      const result = await executeSql('SHOW DATABASES')
      return sortByLabel(dedupeByKey((result?.rows ?? []).map((r) => ({ key: String(r[0]), label: String(r[0]) }))))
    }
    if (kind === 'mssql' || kind === 'clickhouse') {
      const result = await executeSql(
        kind === 'mssql' ? 'SELECT name FROM sys.databases ORDER BY name' : 'SELECT name FROM system.databases ORDER BY name',
      )
      return sortByLabel(dedupeByKey((result?.rows ?? []).map((r) => ({ key: String(r[0]), label: String(r[0]) }))))
    }
    if (kind === 'd1' && conn.accountId) {
      // OAuth D1 connections keep the token in the Cloudflare token store rather
      // than on the connection, so fall back to it when absent.
      let token = conn.apiToken
      if (!token) {
        const { cfGetValidToken } = await import('$lib/cloudflare.js')
        token = await cfGetValidToken()
      }
      const dbs = await cloudflareListD1Databases(token, conn.accountId)
      return sortByLabel((dbs ?? []).map((/** @type {{ uuid: string, name: string }} */ d) => ({ key: d.uuid, label: d.name })))
    }
  } catch {
    return []
  }
  return []
}

/** @param {DatabaseEntry[]} list */
function sortByLabel(list) {
  return [...list].sort((a, b) => a.label.localeCompare(b.label))
}
