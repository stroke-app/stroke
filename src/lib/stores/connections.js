import { invoke } from '@tauri-apps/api/core'
import { saveSqlDraft, saveSqlTabs } from '$lib/stores/sql-draft.js'
import { notifyLocalChange, recordDeletion } from '$lib/sync/changes.js'
import { activeWorkspaceId, DEFAULT_WORKSPACE_ID } from '$lib/stores/workspaces.js'
import { get } from 'svelte/store'

const STORAGE_KEY = 'stroke:connections'
/**
 * Ids of deleted connections. A delete used to be undone by the next write that
 * carried the old connection - the shell re-saves the active connection on
 * every connect, reconnect and database switch, and `upsertConnection` inserts
 * whatever it is handed - so deleting the connection you were on brought it
 * back. An id listed here is never re-added unless the caller says so.
 */
const DELETED_KEY = 'stroke:connections:deleted'
/** Deleted ids are UUIDs and never reused; the cap only bounds the list. */
const MAX_DELETED_IDS = 500
const LAST_ID_KEY  = 'stroke:last-connection-id'
const DISCONNECTED_KEY = 'stroke:disconnected'

/**
 * @typedef {'postgres' | 'sqlite' | 'd1' | 'mysql' | 'mariadb' | 'cockroachdb' | 'yugabytedb' | 'alloydb' | 'libsql' | 'clickhouse' | 'duckdb' | 'mssql' | 'oracle' | 'spanner' | 'bigquery' | 'snowflake' | 'odbc' | 'redis' | 'valkey' | 'dynamodb' | 'couchdb' | 'elasticsearch' | 'opensearch' | 'cassandra' | 'firestore' | 'convex' | 'neo4j' | 'influxdb' | 'mongodb' | 'posthog'} DbType
 *
 * @typedef {{ host: string, port?: number, username: string, privateKeyPath?: string }} SshConfig
 *
 * @typedef {{
 *   id: string
 *   type: DbType
 *   name: string
 *   lastConnectedAt?: number
 *   host?: string
 *   port?: number
 *   database?: string
 *   db?: number
 *   tls?: boolean
 *   user?: string
 *   password?: string
 *   ssl?: boolean
 *   secure?: boolean
 *   encrypt?: boolean
 *   trustCert?: boolean
 *   serviceName?: string  - Oracle service name
 *   sid?: string  - Oracle SID, instead of a service name
 *   connectString?: string  - Oracle Easy Connect / TNS descriptor / tnsnames alias; overrides host, port, service and SID
 *   clientLibDir?: string  - folder holding Oracle Instant Client
 *   filePath?: string
 *   accountId?: string
 *   databaseId?: string
 *   apiToken?: string
 *   oauth?: boolean
 *   ssh?: SshConfig
 *   readOnly?: boolean
 *   environment?: 'prod' | 'staging' | 'dev' | null
 *   provider?: 'neon' | 'supabase' | 'planetscale' | 'prisma' | 'tidb' | 'turso' | 'nile' | 'railway' | 'upstash' | 'posthog' | 'xata' | 'singlestore' | 'convex'
 *   projectId?: string  - PostHog project
 *   apiKey?: string     - PostHog personal API key, Convex deploy or admin key
 *   providerRef?: string  - the provider's own id for this database (its db_ref), so reopening it from the provider picker connects with this entry instead of minting new credentials
 *   group?: string | null
 *   workspaceId?: string | null  - the workspace it belongs to (stores/workspaces.js); missing = the default one
 *   color?: string | null  - a colour id from stores/workspaces.js COLORS, shown as a dot wherever it appears
 *   pinned?: boolean  - kept at the top of the connection lists (set through setConnectionMeta)
 *   origin?: 'studio' | 'docker'  - discovered locally rather than typed in
 *   tool?: 'prisma' | 'drizzle'
 *   toolLabel?: string
 * }} SavedConnection
 */

/** Providers whose databases can be Redis; any other `provider` on a Redis entry is stale. */
const REDIS_PROVIDERS = new Set(['upstash', 'railway'])

export function newConnectionId() {
  return crypto.randomUUID()
}

/**
 * Maps a saved connection `type` to the underlying engine family that drives
 * capability flags and dialect-specific behavior. Wire-compatible aliases
 * (MariaDB → MySQL, CockroachDB → PostgreSQL) collapse to their base so every
 * `dbType === 'mysql'` / `'postgres'` check keeps working unchanged.
 * @param {string | undefined | null} type
 * @returns {DbType}
 */
export function engineFamily(type) {
  if (type === 'mariadb') return 'mysql'
  if (type === 'cockroachdb' || type === 'yugabytedb' || type === 'alloydb') return 'postgres'
  if (type === 'valkey') return 'redis'
  if (type?.startsWith('odbc-')) return 'odbc'
  return /** @type {DbType} */ (type ?? 'postgres')
}

/** @returns {SavedConnection[]} */
export function loadSavedConnections() {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []
    return parsed.map((c) => {
      // Guard per element so one corrupt/null entry is dropped, not the whole
      // list (a throw here would fall through to the catch and wipe everything).
      if (!c || typeof c !== 'object') return null
      const type = c.type ?? 'postgres'
      // Secrets come back from the keychain cache; an inline copy (not yet
      // migrated, or the keychain unavailable) wins over it.
      const sec = typeof c.id === 'string' ? _secretCache.get(c.id) : undefined
      const conn = {
        ...(sec ?? {}),
        ...c,
        id:   c.id   ?? newConnectionId(),
        type,
        // Only Postgres-family connections get 5432 as a fallback; for other
        // engines the per-engine normalizers in api.js fill the right default
        // (3306/6379/8123/1433) when port is left undefined.
        port: c.port != null ? Number(c.port) : (engineFamily(type) === 'postgres' ? 5432 : undefined),
      }
      // Redis connections can carry stale Postgres-ish fields from an earlier
      // edit/clone (a `provider` and a non-numeric `db` like "postgres"), which
      // made the status bar show `db postgres` and open a Postgres db switcher.
      // Normalize once on load so the UI and backend agree on a numeric logical DB.
      if (engineFamily(type) === 'redis') {
        conn.db = Number(conn.db) || 0
        // Only a provider that doesn't serve Redis is stale. Upstash and
        // Railway do, and dropping theirs turned a saved Upstash database into
        // an anonymous one after every restart.
        if (!REDIS_PROVIDERS.has(conn.provider)) delete conn.provider
      }
      return conn
    }).filter((c) => c != null)
  } catch (err) {
    // The stored value exists but did not parse. Returning [] is safe by itself,
    // but the very next upsert would write that empty list straight back over a
    // payload that was merely unreadable, not gone. Move it aside first so it
    // stays recoverable by hand, and let the app carry on from a clean slate.
    console.error('Saved connections were unreadable:', err)
    try {
      const raw = localStorage.getItem(STORAGE_KEY)
      if (raw) localStorage.setItem(`${STORAGE_KEY}:unreadable`, raw)
    } catch {}
    return []
  }
}

/**
 * @param {SavedConnection[]} connections
 * @returns {boolean} false when the write was rejected (quota exhausted), so the
 * caller can tell the user rather than let them believe the save landed.
 */
export function saveConnections(connections) {
  try {
    let stored = connections
    if (_vaultReady) {
      // Secrets go to the keychain; the list on disk keeps everything else.
      const next = new Map()
      stored = connections.map((c) => {
        const { pub, sec } = splitSecrets(c)
        if (Object.keys(sec).length) next.set(c.id, sec)
        return pub
      })
      _secretCache = next
      queueSecretsWrite(connections.map((c) => c.id))
    }
    localStorage.setItem(STORAGE_KEY, JSON.stringify(stored))
    mirrorToDisk()
    notifyLocalChange('connection')
    return true
  } catch (err) {
    // Quota/serialization failure must not throw into connect/disconnect flows.
    console.error('Failed to persist connections:', err)
    return false
  }
}

/**
 * True when the last `saveConnections` call was rejected. Read it straight after
 * an upsert: a silently dropped write is indistinguishable from a successful one
 * until the app restarts and the connection is gone.
 */
let _persistFailed = false
export function lastPersistFailed() {
  return _persistFailed
}

/** @returns {Set<string>} */
function loadDeletedIds() {
  try {
    const parsed = JSON.parse(localStorage.getItem(DELETED_KEY) ?? '[]')
    return new Set(Array.isArray(parsed) ? parsed.filter((id) => typeof id === 'string') : [])
  } catch {
    return new Set()
  }
}

/** @param {Set<string>} ids */
function saveDeletedIds(ids) {
  try {
    localStorage.setItem(DELETED_KEY, JSON.stringify([...ids].slice(-MAX_DELETED_IDS)))
  } catch { /* the delete itself already landed; this only guards against revival */ }
}

// ── Secrets in the keychain ──────────────────────────────────────────────────
// Passwords, tokens and keys never sit in localStorage or connections.json:
// `saveConnections` splits them out into an in-memory cache that is persisted to
// the OS keychain vault (secrets.rs), and `loadSavedConnections` merges them
// back, so callers still see whole connections. Until the vault has been read
// at startup (or when the keychain is unavailable) secrets stay inline, exactly
// as before: a failed keychain write can never lose a password.

/** Fields that hold credentials. MongoDB's `uri` and an ODBC connection string can carry a password. */
export const SECRET_FIELDS = ['password', 'token', 'apiToken', 'apiKey', 'authToken', 'secretAccessKey', 'sessionToken', 'uri', 'connectionString']

/** @type {Map<string, Record<string, string>>} */
let _secretCache = new Map()
/** True once secrets live in the keychain; false keeps them inline. */
let _vaultReady = false

/**
 * A connection's public fields and its secrets, separately.
 * @param {Record<string, any>} c
 */
export function splitSecrets(c) {
  const pub = { ...c }
  /** @type {Record<string, string>} */
  const sec = {}
  for (const f of SECRET_FIELDS) {
    if (typeof c[f] === 'string' && c[f] !== '') sec[f] = c[f]
    delete pub[f]
  }
  return { pub, sec }
}

let _secretsChain = Promise.resolve()

/**
 * Merge this app's secrets into the keychain vault, which Stroke and Stroke
 * Nightly share: every id of `ids` is written from the cache (an empty object
 * clears it), deleted ids are removed, and ids this app doesn't have are left
 * as the other app saved them. Writing the whole vault from this app's cache
 * used to drop the other track's passwords. On failure the secrets go back
 * inline so none is lost.
 * @param {Iterable<string>} ids
 */
function queueSecretsWrite(ids) {
  /** @type {Record<string, Record<string, string>>} */
  const set = {}
  for (const id of ids) if (typeof id === 'string') set[id] = _secretCache.get(id) ?? {}
  const payload = { set: JSON.stringify(set), removed: [...loadDeletedIds()] }
  _secretsChain = _secretsChain
    .then(() => invoke('conn_secrets_merge', payload))
    .catch((err) => {
      console.error('Keychain write failed; keeping connection secrets inline:', err)
      _vaultReady = false
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(loadSavedConnections()))
        mirrorToDisk()
      } catch {}
    })
}

/**
 * Read the keychain's connection secrets and move any inline ones into it.
 * Inline copies are stripped only after the keychain write succeeded. Runs at
 * startup after the disk copy is loaded; outside Tauri it leaves things inline.
 */
export async function hydrateConnectionSecrets() {
  try {
    const json = /** @type {string | null} */ (await invoke('conn_secrets_read'))
    /** @type {Record<string, Record<string, string>>} */
    const stored = json ? JSON.parse(json) : {}
    const cache = new Map(Object.entries(stored ?? {}))
    const raw = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '[]')
    const list = Array.isArray(raw) ? raw : []
    let inline = false
    for (const c of list) {
      if (!c || typeof c !== 'object' || typeof c.id !== 'string') continue
      const { sec } = splitSecrets(c)
      if (Object.keys(sec).length) {
        cache.set(c.id, { ...(cache.get(c.id) ?? {}), ...sec })
        inline = true
      }
    }
    // Secrets of deleted connections don't come back. Only an id with a
    // tombstone counts as deleted: one missing from this list may belong to
    // the other track (Stroke and Stroke Nightly share the vault), and
    // dropping those wiped its passwords every time the other app started.
    const deleted = loadDeletedIds()
    let pruned = false
    for (const id of [...cache.keys()]) if (deleted.has(id)) { cache.delete(id); pruned = true }
    if (inline || pruned) {
      /** @type {Record<string, Record<string, string>>} */
      const set = {}
      for (const c of list) if (c && typeof c.id === 'string') set[c.id] = cache.get(c.id) ?? {}
      await invoke('conn_secrets_merge', { set: JSON.stringify(set), removed: [...deleted] })
    }
    _secretCache = cache
    _vaultReady = true
    if (inline) {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(list.map((c) => (c && typeof c === 'object' ? splitSecrets(c).pub : c))))
      mirrorToDisk()
    }
  } catch (err) {
    _vaultReady = false
    console.warn('Connection secrets stay in local storage (keychain unavailable):', err)
  }
}

// ── Durable copy on disk ─────────────────────────────────────────────────────
// localStorage reaches disk whenever the webview gets round to it, and WebView2
// can lose the last writes when the app closes right after them - a deleted
// connection was back on the next launch. Every change is also written to
// `connections.json` in the app data folder (fsynced by the backend), and that
// file is loaded into localStorage before anything reads the list.

let _mirrorChain = Promise.resolve()
/** Ids re-added after a delete this session. The backend merges each write
 *  with the file, where a tombstone otherwise wins over a copy of the row. */
const _revived = new Set()

/** Queue a write of the current list and deleted ids. Writes run one at a time,
 *  so an older payload can never land after a newer one. */
function mirrorToDisk() {
  let json
  try {
    json = JSON.stringify({
      version: 1,
      connections: JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '[]'),
      deleted: [...loadDeletedIds()],
      revived: [..._revived],
    })
  } catch {
    return
  }
  _mirrorChain = _mirrorChain
    .then(() => invoke('connections_store_write', { json }))
    .catch((err) => console.error('Failed to save connections to disk:', err))
}

/**
 * Load the on-disk copy into localStorage. Call once at startup, before the app
 * reads any connection. Without a file yet (first launch after this change) the
 * current localStorage list seeds it. Outside Tauri (browser dev) or with an
 * unreadable file, localStorage stays the source.
 */
/** The keychain read started by hydrateConnectionsFromDisk; see secretsReady. */
let _secretsPromise = Promise.resolve()

/**
 * Resolves once saved passwords have been read from the OS keychain. Startup
 * does not wait for it (see hydrateConnectionsFromDisk); anything that is
 * about to connect does, through withSecrets.
 * @returns {Promise<void>}
 */
export function secretsReady() {
  return _secretsPromise
}

/**
 * A connection with its saved secrets filled in, once the keychain has
 * answered. A connection read from the list before then has none, and would
 * fail its login. Fields the connection already carries win.
 * @template {{ id?: string }} T
 * @param {T} conn
 * @returns {Promise<T>}
 */
export async function withSecrets(conn) {
  await _secretsPromise
  const sec = conn && typeof conn.id === 'string' ? _secretCache.get(conn.id) : undefined
  return sec ? { ...sec, ...conn } : conn
}

/**
 * Load the saved list from its on-disk copy, then read the passwords from the
 * OS keychain.
 *
 * `waitForSecrets: false` returns once the list is in and leaves the keychain
 * read running (secretsReady). Startup uses it: the keychain answers over
 * D-Bus on Linux and through the system on macOS and Windows, and it was
 * measured taking 119ms, 1179ms, and once never answering, while the whole
 * window waited for it before mounting anything. The list needs only the
 * file; only a connect needs the passwords.
 * @param {{ waitForSecrets?: boolean }} [opts]
 */
export async function hydrateConnectionsFromDisk({ waitForSecrets = true } = {}) {
  let seedFile = false
  try {
    const json = /** @type {string | null} */ (await invoke('connections_store_read'))
    if (json == null) {
      // No file yet: write it after the secrets have moved to the keychain
      // (below), so a first launch never puts a password on disk.
      seedFile = true
      return
    }
    const data = JSON.parse(json)
    if (Array.isArray(data?.connections)) localStorage.setItem(STORAGE_KEY, JSON.stringify(data.connections))
    if (Array.isArray(data?.deleted)) localStorage.setItem(DELETED_KEY, JSON.stringify(data.deleted))
  } catch (err) {
    console.warn('Saved connections: using browser storage only.', err)
  } finally {
    _secretsPromise = hydrateConnectionSecrets().then(() => {
      if (seedFile) mirrorToDisk()
    })
    if (waitForSecrets) await _secretsPromise
  }
}

/** JSON with object keys sorted, for comparing two payloads by value. @param {unknown} v @returns {string} */
function canonical(v) {
  if (Array.isArray(v)) return `[${v.map(canonical).join(',')}]`
  if (v && typeof v === 'object') {
    return `{${Object.keys(v).sort().map((k) => `${JSON.stringify(k)}:${canonical(/** @type {Record<string, unknown>} */ (v)[k])}`).join(',')}}`
  }
  return JSON.stringify(v) ?? 'null'
}

/**
 * Pick up what the other track saved. Stroke and Stroke Nightly share
 * connections.json and the keychain vault, and both can be open: call this
 * when the window comes back to the front. Returns true when the list changed,
 * after reading the passwords fresh so a connection added over there connects
 * here without asking again.
 * @returns {Promise<boolean>}
 */
export async function refreshConnectionsFromDisk() {
  try {
    // Let this app's own pending writes land first, so the file includes them.
    await _mirrorChain
    await _secretsChain
    const json = /** @type {string | null} */ (await invoke('connections_store_read'))
    if (json == null) return false
    const data = JSON.parse(json)
    if (!Array.isArray(data?.connections)) return false
    const list = JSON.stringify(data.connections)
    const deleted = JSON.stringify(Array.isArray(data.deleted) ? data.deleted : [])
    // The backend may write keys in another order: compare values, not text.
    const same = (/** @type {string} */ a, /** @type {string | null} */ b) => {
      try { return canonical(JSON.parse(a)) === canonical(JSON.parse(b ?? '[]')) } catch { return false }
    }
    if (same(list, localStorage.getItem(STORAGE_KEY)) && same(deleted, localStorage.getItem(DELETED_KEY))) return false
    if (_vaultReady) {
      try {
        const sec = /** @type {string | null} */ (await invoke('conn_secrets_read_fresh'))
        const parsed = sec ? JSON.parse(sec) : {}
        _secretCache = new Map(Object.entries(parsed ?? {}))
      } catch { /* keep the cache: a connection without its password asks for it */ }
    }
    localStorage.setItem(STORAGE_KEY, list)
    localStorage.setItem(DELETED_KEY, deleted)
    return true
  } catch {
    return false
  }
}

/**
 * Where a connection is filed and how it looks, set by the person rather than
 * by whatever dialled it. The one-click paths (a provider's database, a Docker
 * container, a local server, D1) build a fresh row on every connect, so writing
 * that row over the saved one dropped these and sent the connection back to
 * the default workspace. A caller that names one of them (the edit form always
 * does, even to clear it) sets it; one that leaves it out keeps the saved value.
 */
const FILING_FIELDS = /** @type {const} */ (['workspaceId', 'color', 'group'])

/**
 * The workspace a brand-new row lands in: the one on screen, so connecting to
 * a provider's database from the "Work" workspace files it under Work.
 * @returns {string | undefined}
 */
function workspaceForNew() {
  const id = get(activeWorkspaceId)
  return id && id !== DEFAULT_WORKSPACE_ID ? id : undefined
}

/**
 * @param {SavedConnection} conn
 * @param {{ revive?: boolean, exact?: boolean }} [opts] `revive` lets an
 *   explicit re-add (the Sample Database button) bring back an id that was
 *   deleted. Everything else that writes an existing connection back is a
 *   no-op for a deleted one. `exact` stores the row as given, with no filing
 *   kept or added: sync, whose record is the truth from another device.
 */
export function upsertConnection(conn, { revive = false, exact = false } = {}) {
  const deleted = loadDeletedIds()
  if (deleted.has(conn.id)) {
    if (!revive) {
      _persistFailed = false
      return loadSavedConnections()
    }
    deleted.delete(conn.id)
    saveDeletedIds(deleted)
    _revived.add(conn.id)
  }
  const list = loadSavedConnections()
  const idx  = list.findIndex((c) => c.id === conn.id)
  if (idx >= 0 && targetChanged(list[idx], conn)) closeParkedFor(conn.id)
  if (idx >= 0) {
    // A pin is set only through setConnectionMeta; a live copy written back
    // here (connect, database switch, edit) must not drop or revive it.
    const { pinned: _stale, ...rest } = conn
    /** @type {SavedConnection} */
    const next = list[idx].pinned ? { ...rest, pinned: true } : rest
    if (!exact) {
      const prev = /** @type {Record<string, any>} */ (list[idx])
      for (const f of FILING_FIELDS) {
        if (!(f in conn) && prev[f] != null) /** @type {Record<string, any>} */ (next)[f] = prev[f]
      }
    }
    list[idx] = next
  } else if (exact || 'workspaceId' in conn) list.push(conn)
  else {
    const ws = workspaceForNew()
    list.push(ws ? { ...conn, workspaceId: ws } : conn)
  }
  _persistFailed = !saveConnections(list)
  return list
}

/**
 * Fields that decide which server a connection reaches, and as whom. The
 * database itself is not here: it is part of a set-aside pool's key, so
 * switching databases is a different pool, not a changed one.
 */
const TARGET_FIELDS = ['type', 'host', 'port', 'db', 'tls', 'user', 'url', 'ssl', 'secure', 'encrypt', 'trustCert', 'serviceName', 'sid', 'connectString', 'clientLibDir', 'filePath', 'accountId', 'oauth', 'ssh', 'readOnly', 'projectId']

/**
 * True when an edit moves a saved connection to another server, login or
 * mode. Name, colour, group, pin and the connect time don't count, so the
 * write-back on every connect leaves its pools alone. A secret missing from
 * the incoming copy is unchanged (a live copy may not carry it).
 * @param {any} prev @param {any} next
 */
export function targetChanged(prev, next) {
  const same = (/** @type {any} */ a, /** @type {any} */ b) => JSON.stringify(a ?? null) === JSON.stringify(b ?? null)
  if (TARGET_FIELDS.some((f) => !same(prev[f], next[f]))) return true
  return SECRET_FIELDS.some((f) => next[f] !== undefined && !same(prev[f], next[f]))
}

/**
 * Labs, several connections at once: close every pool set aside for this
 * saved connection (db/parked.rs), so switching back can't resume one that
 * points at the old server or login. Nothing set aside: nothing happens.
 * @param {string} id
 */
function closeParkedFor(id) {
  try {
    void invoke('close_parked', { key: null, connection: id }).catch(() => {})
  } catch {
    // Outside the desktop app there are no pools to close.
  }
}

/** How long a deleted connection can be brought back with Undo. */
export const UNDO_REMOVE_MS = 8000

/**
 * Deletions still inside their undo window: the row is gone from the list,
 * but its charts, history and conversations are not purged and the sync
 * tombstone is not written until the window closes.
 * @type {Map<string, { conn: SavedConnection, wasLast: boolean, timer: ReturnType<typeof setTimeout> }>}
 */
const _undoable = new Map()

/** Purge and tombstone a deletion whose undo window has closed. @param {string} id */
function finalizeRemoval(id) {
  const entry = _undoable.get(id)
  if (!entry) return
  clearTimeout(entry.timer)
  _undoable.delete(id)
  // Restored in the meantime by something other than Undo (sync, re-add).
  if (!loadDeletedIds().has(id)) return
  recordDeletion('connection', id)
  purgeConnectionData(id)
}

/** Close every open undo window now, so a quit never strands a half-deleted row. */
export function flushPendingRemovals() {
  for (const id of [..._undoable.keys()]) finalizeRemoval(id)
}
if (typeof window !== 'undefined') window.addEventListener('pagehide', flushPendingRemovals)

/**
 * @param {string} id
 * @param {{ undoable?: boolean }} [opts] `undoable` holds the purge and the sync
 *   tombstone for UNDO_REMOVE_MS so `restoreConnection` can bring the row back.
 *   A delete that arrives from sync is final at once.
 */
export function removeConnection(id, { undoable = false } = {}) {
  const conn = loadSavedConnections().find((c) => c.id === id)
  closeParkedFor(id)
  const deleted = loadDeletedIds()
  deleted.add(id)
  _revived.delete(id)
  // Before saveConnections, so the disk copy it writes carries the id too.
  saveDeletedIds(deleted)
  const list = loadSavedConnections().filter((c) => c.id !== id)
  saveConnections(list)
  const wasLast = getLastConnectionId() === id
  if (wasLast) setLastConnectionId(null)
  if (undoable && conn) {
    const prev = _undoable.get(id)
    if (prev) clearTimeout(prev.timer)
    _undoable.set(id, { conn, wasLast, timer: setTimeout(() => finalizeRemoval(id), UNDO_REMOVE_MS) })
    return list
  }
  // Sync carries the delete to other devices (a no-op while sync applies one).
  recordDeletion('connection', id)
  purgeConnectionData(id)
  return list
}

/**
 * Undo an undoable `removeConnection` while its window is open: the row comes
 * back with its secrets, nothing was purged and no tombstone was written.
 * Lists that keep their own copy reload on `stroke:connections-synced`.
 * @param {string} id
 * @returns {boolean} false once the window has closed
 */
export function restoreConnection(id) {
  const entry = _undoable.get(id)
  if (!entry) return false
  clearTimeout(entry.timer)
  _undoable.delete(id)
  upsertConnection(entry.conn, { revive: true, exact: true })
  if (entry.wasLast) setLastConnectionId(id)
  try { window.dispatchEvent(new CustomEvent('stroke:connections-synced')) } catch { /* no window in tests */ }
  return true
}

/**
 * Remove every per-connection artifact a deleted connection leaves behind.
 * Without this, its recents/charts/dashboards/diagrams and per-table prefs
 * accumulate in localStorage forever (eventually exhausting the quota, which
 * makes unrelated persists start failing), and IndexedDB keeps its query
 * history, saved queries, conversations and schema snapshots.
 * @param {string} id
 */
export function purgeConnectionData(id) {
  if (!id) return
  try {
    for (const key of [
      `stroke:recent-tabs:${id}`,
      `stroke:saved-charts:${id}`,
      `stroke:chart-groups:${id}`,
      `stroke:dashboards:${id}`,
      `stroke:active-dashboard:${id}`,
      `stroke:saved-diagrams:${id}`,
      `stroke:last-schema:${id}`,
      `stroke:query-folders:${id}`,
    ]) localStorage.removeItem(key)
    // Per-table keys carry a `:<schema>.<table>` suffix - match by prefix,
    // iterating backwards because removeItem reindexes localStorage.
    for (let i = localStorage.length - 1; i >= 0; i--) {
      const key = localStorage.key(i)
      if (key?.startsWith(`stroke:hidden-cols:${id}:`) || key?.startsWith(`stroke:table-views:${id}:`)) {
        localStorage.removeItem(key)
      }
    }
  } catch { /* storage failure must not block deleting the connection */ }
  try { saveSqlDraft(id, ''); saveSqlTabs(id, []) } catch { /* ditto */ }
  // IndexedDB rows are cleared fire-and-forget (dynamic imports keep this module
  // free of a static cycle via schema-snapshots -> api -> connections). A
  // failure only leaves orphaned rows behind, never an error in the delete flow.
  void (async () => {
    try {
      const { clearQueryHistory, listSavedQueries, deleteSavedQuery } = await import('$lib/stores/query-history.js')
      await clearQueryHistory(id)
      await Promise.all((await listSavedQueries(id)).map((q) => deleteSavedQuery(q.id)))
    } catch { /* ignore */ }
    try {
      const { clearConversations } = await import('$lib/stores/conversations.js')
      await clearConversations(id)
      await clearConversations(id, 'sidebar')
    } catch { /* ignore */ }
    try {
      const { listSnapshots, deleteSnapshot } = await import('$lib/stores/schema-snapshots.js')
      await Promise.all((await listSnapshots(id)).map((s) => deleteSnapshot(s.id)))
    } catch { /* ignore */ }
  })()
}

/**
 * Assigns (or clears) the free-text group/folder a saved connection belongs to.
 * Pass `null`/empty to move it back to Ungrouped. Absent `group` = ungrouped, so
 * connections saved before groups existed need no migration. Returns the full
 * updated list (like `removeConnection`) so callers can refresh their view.
 * @param {string} id
 * @param {string | null} group
 * @returns {SavedConnection[]}
 */
export function setConnectionGroup(id, group) {
  const g = group && String(group).trim() ? String(group).trim() : null
  const list = loadSavedConnections()
  const idx  = list.findIndex((c) => c.id === id)
  if (idx >= 0) {
    if (g) {
      list[idx] = { ...list[idx], group: g }
    } else {
      const { group: _drop, ...rest } = list[idx]
      list[idx] = rest
    }
    saveConnections(list)
  }
  return list
}

/**
 * Set a connection's workspace, colour and/or pin (null clears the colour; a
 * null workspace means the default one).
 * @param {string} id
 * @param {{ workspaceId?: string | null, color?: string | null, pinned?: boolean }} patch
 */
export function setConnectionMeta(id, patch) {
  const list = loadSavedConnections()
  const idx = list.findIndex((c) => c.id === id)
  if (idx >= 0) {
    const next = { ...list[idx] }
    if ('workspaceId' in patch) {
      if (patch.workspaceId) next.workspaceId = patch.workspaceId
      else delete next.workspaceId
    }
    if ('color' in patch) {
      if (patch.color) next.color = patch.color
      else delete next.color
    }
    if ('pinned' in patch) {
      if (patch.pinned) next.pinned = true
      else delete next.pinned
    }
    list[idx] = next
    saveConnections(list)
  }
  return list
}

/**
 * Move every connection of one workspace to the default one (the workspace is
 * being deleted). Returns the updated list.
 * @param {string} workspaceId
 */
export function releaseWorkspace(workspaceId) {
  const list = loadSavedConnections().map((c) => {
    if (c.workspaceId !== workspaceId) return c
    const { workspaceId: _drop, ...rest } = c
    return rest
  })
  saveConnections(list)
  return list
}

/**
 * What a connection actually dials, as one comparable string. Name is not part
 * of it: "prod" and "prod copy" pointing at the same database on the same
 * server as the same user are the case worth catching, and two rows on one
 * database under different logins are genuinely different connections.
 *
 * @param {any} conn
 * @returns {string} '' when there is not enough of a target to compare
 */
export function connectionTargetKey(conn) {
  if (!conn) return ''
  const type = String(conn.type ?? '').toLowerCase()
  const norm = (v) => String(v ?? '').trim()
  if (type === 'sqlite' || type === 'duckdb') {
    const path = norm(conn.filePath)
    // Two `:memory:` databases are two databases - nothing is shared between them.
    return path && path !== ':memory:' ? `${type}|${path}` : ''
  }
  if (type === 'libsql') {
    const url = norm(conn.url).toLowerCase()
    return url ? `libsql|${url}` : ''
  }
  if (type === 'd1') {
    const db = norm(conn.databaseId)
    return db ? `d1|${norm(conn.accountId)}|${db}` : ''
  }
  const host = norm(conn.host).toLowerCase()
  if (!host) return ''
  return [type, host, norm(conn.port), norm(conn.database), norm(conn.user)].join('|')
}

/**
 * The saved connection `conn` would duplicate, or null. `skipId` is the row
 * being edited - a connection is never a duplicate of itself.
 * @param {any} conn
 * @param {any[]} list
 * @param {string | null} [skipId]
 */
export function findDuplicateConnection(conn, list, skipId = null) {
  const key = connectionTargetKey(conn)
  if (!key) return null
  return list.find((c) => c.id !== skipId && connectionTargetKey(c) === key) ?? null
}

// ── Last-connection helpers ───────────────────────────────────────────────────

/** @returns {string | null} */
export function getLastConnectionId() {
  try { return localStorage.getItem(LAST_ID_KEY) } catch { return null }
}

/** @param {string | null} id */
export function setLastConnectionId(id) {
  try {
    if (id) localStorage.setItem(LAST_ID_KEY, id)
    else    localStorage.removeItem(LAST_ID_KEY)
  } catch {}
}

/**
 * Did the last session end with a deliberate Disconnect?
 *
 * The last-connection id outlives a disconnect on purpose - the modal still
 * highlights where you were, and reconnecting is one click. But auto-reconnect
 * read that id as "resume this", so quitting while disconnected came back
 * connected on the next launch, which is the one thing Disconnect was asked to
 * prevent. This flag is what tells the two apart.
 * @returns {boolean}
 */
export function wasDisconnected() {
  try { return localStorage.getItem(DISCONNECTED_KEY) === '1' } catch { return false }
}

/** @param {boolean} v */
export function setWasDisconnected(v) {
  try {
    if (v) localStorage.setItem(DISCONNECTED_KEY, '1')
    else   localStorage.removeItem(DISCONNECTED_KEY)
  } catch {}
}

/**
 * Last active schema per connection, so reconnecting lands where the user
 * left off instead of resetting to `public`.
 * @param {string} connectionId
 * @returns {string | null}
 */
export function getLastSchema(connectionId) {
  if (!connectionId) return null
  try { return localStorage.getItem(`stroke:last-schema:${connectionId}`) } catch { return null }
}

/**
 * @param {string} connectionId
 * @param {string} schema
 */
export function setLastSchema(connectionId, schema) {
  if (!connectionId || !schema) return
  try { localStorage.setItem(`stroke:last-schema:${connectionId}`, schema) } catch {}
}

/** Returns the last-used connection if it still exists in the saved list. */
export function getLastConnection() {
  const id = getLastConnectionId()
  if (!id) return null
  return loadSavedConnections().find((c) => c.id === id) ?? null
}
