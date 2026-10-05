// Listing the OTHER databases reachable from the current connection, and the
// rules for which connections can switch between them at all.
//
// Lifted out of StatusBar so the sidebar's Databases section and the status
// bar's switcher share one implementation - two per-engine dispatches would
// drift the moment an engine gained or lost support.

import { executeSql, cloudflareListD1Databases } from '$lib/api.js'
import { providerListDatabases } from '$lib/providers.js'
import { engineFamily } from '$lib/stores/connections.js'

/** @typedef {{ key: string, label: string }} DatabaseEntry */
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
  if (conn.provider) return 'provider'
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
  if (conn.type === 'd1') return conn.databaseId ?? ''
  // A blank database connects to the server's default, so that row is current.
  if (conn.type === 'mssql') return conn.database || 'master'
  if (conn.type === 'clickhouse') return conn.database || 'default'
  return conn.database ?? conn.filePath ?? ''
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
      return sortByLabel((dbs ?? []).map((/** @type {{ db_ref: string, name: string }} */ d) => ({ key: d.db_ref, label: d.name })))
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
