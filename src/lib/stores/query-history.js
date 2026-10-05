import { getStudioDb, STORES } from '$lib/stores/studio-db.js'
import { loadSettings, DEFAULT_MAX_QUERY_HISTORY } from '$lib/stores/settings.js'

const HISTORY_STORE = STORES.queryHistory
const SAVED_STORE = STORES.savedQueries
/** How far back a re-run looks for its earlier row before filing a new one. */
const DEDUPE_WINDOW = 200

/** How many history entries to keep per connection - configurable in Settings → Database. */
function maxHistoryPerConnection() {
  const n = loadSettings().maxQueryHistory
  return Number.isFinite(n) && n > 0 ? n : DEFAULT_MAX_QUERY_HISTORY
}

/**
 * @typedef {{
 *   id: string
 *   connectionId: string
 *   sql: string
 *   title: string
 *   executedAt: number
 *   queryMs?: number
 *   runCount?: number
 *   favorite?: boolean
 *   success?: boolean
 *   error?: string
 * }} QueryHistoryEntry
 */

/**
 * @typedef {{
 *   id: string
 *   connectionId: string
 *   name: string
 *   sql: string
 *   createdAt: number
 *   updatedAt: number
 *   folderId?: string | null
 * }} SavedQuery
 */

/** @param {string} sql */
export function queryTitle(sql) {
  const line =
    sql
      .split('\n')
      .find((l) => l.trim() && !l.trim().startsWith('--')) ?? sql
  const t = line.trim().replace(/\s+/g, ' ')
  return t.slice(0, 80) + (t.length > 80 ? '…' : '')
}

/**
 * @param {string} connectionId
 * @param {string} sql
 * @param {{ queryMs?: number, success?: boolean, error?: string }} [meta] a failed run
 *   is recorded too, with `success: false` and the database's message
 */
export async function recordQueryExecution(connectionId, sql, meta = {}) {
  const trimmed = sql.trim()
  if (!connectionId || !trimmed) return

  const db = await getStudioDb()

  // The same query run again moves up and counts the run, instead of filing a
  // second row: switching between two queries listed each once per switch.
  // Newest first via a reverse cursor on the executedAt index, and only the
  // connection's recent stretch, so a run never loads or walks a history the
  // cap lets grow to 100k.
  /** @type {QueryHistoryEntry | null} */
  let match = null
  let seen = 0
  for (
    let cursor = await db.transaction(HISTORY_STORE).store.index('executedAt').openCursor(null, 'prev');
    cursor;
    cursor = await cursor.continue()
  ) {
    if (cursor.value.connectionId !== connectionId) continue
    if (cursor.value.sql === trimmed) {
      match = cursor.value
      break
    }
    if (++seen >= DEDUPE_WINDOW) break
  }

  if (match) {
    // The last run's outcome wins: an error from an earlier failed run does not
    // stick to a run that worked.
    const { error: _earlier, ...prev } = match
    await db.put(HISTORY_STORE, {
      ...prev,
      executedAt: Date.now(),
      title: queryTitle(trimmed),
      runCount: (match.runCount ?? 1) + 1,
      ...meta,
    })
    return
  }

  const entry = /** @type {QueryHistoryEntry} */ ({
    id: crypto.randomUUID(),
    connectionId,
    sql: trimmed,
    title: queryTitle(trimmed),
    executedAt: Date.now(),
    runCount: 1,
    ...meta,
  })
  await db.put(HISTORY_STORE, entry)

  // Cap the ring buffer - but favorites are pinned and never evicted. Loading
  // the full list is fine here: this branch only runs when a distinct new
  // statement was inserted, not on every re-run of the same one.
  const all = await db.getAllFromIndex(HISTORY_STORE, 'connectionId', connectionId)
  const evictable = all.sort((a, b) => b.executedAt - a.executedAt).filter((e) => !e.favorite)
  const cap = maxHistoryPerConnection()
  if (evictable.length > cap) {
    await Promise.all(
      evictable.slice(cap).map((stale) => db.delete(HISTORY_STORE, stale.id)),
    )
  }
}

/** @param {string} id @param {boolean} favorite */
export async function setQueryHistoryFavorite(id, favorite) {
  const db = await getStudioDb()
  const entry = await db.get(HISTORY_STORE, id)
  if (entry) await db.put(HISTORY_STORE, { ...entry, favorite })
}

/** @param {string} connectionId @returns {Promise<QueryHistoryEntry[]>} */
export async function listQueryHistory(connectionId) {
  if (!connectionId) return []
  const db = await getStudioDb()
  const all = await db.getAllFromIndex(HISTORY_STORE, 'connectionId', connectionId)
  return all.sort((a, b) => b.executedAt - a.executedAt)
}

/** @param {string} id */
export async function deleteQueryHistoryEntry(id) {
  const db = await getStudioDb()
  await db.delete(HISTORY_STORE, id)
}

/**
 * @param {string} connectionId
 * @param {{ keepStarred?: boolean }} [opts] `keepStarred` spares the starred
 *   entries: the Clear button in the list. Deleting the connection clears all.
 */
export async function clearQueryHistory(connectionId, { keepStarred = false } = {}) {
  if (!connectionId) return
  const db = await getStudioDb()
  const all = await db.getAllFromIndex(HISTORY_STORE, 'connectionId', connectionId)
  await Promise.all(all.filter((e) => !(keepStarred && e.favorite)).map((e) => db.delete(HISTORY_STORE, e.id)))
}

/** Put back a history entry removed a moment ago (Undo). @param {QueryHistoryEntry} entry */
export async function restoreQueryHistoryEntry(entry) {
  const db = await getStudioDb()
  await db.put(HISTORY_STORE, entry)
}

/**
 * @param {string} connectionId
 * @param {string} name
 * @param {string} sql
 * @param {{ folderId?: string | null, allowEmpty?: boolean }} [opts] `allowEmpty`:
 *   a new query from the sidebar starts with no text and is filled on Save
 * @returns {Promise<SavedQuery>}
 */
export async function createSavedQuery(connectionId, name, sql, { folderId = null, allowEmpty = false } = {}) {
  const trimmed = sql.trim()
  if (!connectionId || (!trimmed && !allowEmpty)) throw new Error('Connection and SQL are required')

  const now = Date.now()
  const saved = /** @type {SavedQuery} */ ({
    id: crypto.randomUUID(),
    connectionId,
    name: name.trim() || queryTitle(trimmed) || 'Untitled query',
    sql: trimmed,
    createdAt: now,
    updatedAt: now,
    ...(folderId ? { folderId } : {}),
  })
  const db = await getStudioDb()
  await db.put(SAVED_STORE, saved)
  return saved
}

/**
 * File a query under Saved Queries unless the same SQL is already there.
 *
 * Used by the auto-save setting, where the same statement is typically run many
 * times over a session: without the dedupe the list would fill with copies of
 * whatever you were iterating on and become useless for the queries you kept on
 * purpose. Returns the existing entry when there is one, so callers can't tell
 * (or need to care) which happened.
 *
 * @param {string} connectionId @param {string} sql
 * @returns {Promise<SavedQuery | null>}
 */
export async function saveQueryOnce(connectionId, sql) {
  const trimmed = sql.trim()
  if (!connectionId || !trimmed) return null
  const existing = await listSavedQueries(connectionId)
  const match = existing.find((q) => q.sql.trim() === trimmed)
  if (match) return match
  return createSavedQuery(connectionId, '', trimmed)
}

/** @param {string} connectionId @returns {Promise<SavedQuery[]>} */
export async function listSavedQueries(connectionId) {
  if (!connectionId) return []
  const db = await getStudioDb()
  const all = await db.getAllFromIndex(SAVED_STORE, 'connectionId', connectionId)
  return all.sort((a, b) => b.updatedAt - a.updatedAt)
}

/**
 * Write new text, a new name, a folder, or several into a saved query,
 * keeping its id: the Save of a tab that belongs to it, a rename, a move.
 * @param {string} id
 * @param {{ sql?: string, name?: string, folderId?: string | null }} patch
 *   `folderId: null` moves it to the root
 * @returns {Promise<SavedQuery | null>} null when it was deleted meanwhile
 */
export async function updateSavedQuery(id, patch) {
  const db = await getStudioDb()
  const current = /** @type {SavedQuery | undefined} */ (await db.get(SAVED_STORE, id))
  if (!current) return null
  const sql = patch.sql?.trim()
  const name = patch.name?.trim()
  /** @type {SavedQuery} */
  const next = { ...current, ...(sql ? { sql } : {}), ...(name ? { name } : {}), updatedAt: Date.now() }
  if (patch.folderId !== undefined) {
    if (patch.folderId) next.folderId = patch.folderId
    else delete next.folderId
  }
  await db.put(SAVED_STORE, next)
  return next
}

/** @param {string} id */
export async function deleteSavedQuery(id) {
  const db = await getStudioDb()
  await db.delete(SAVED_STORE, id)
}

/** Put back a saved query removed a moment ago (Undo). @param {SavedQuery} entry */
export async function restoreSavedQuery(entry) {
  const db = await getStudioDb()
  await db.put(SAVED_STORE, entry)
}
