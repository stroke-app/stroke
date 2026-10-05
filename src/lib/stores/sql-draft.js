/**
 * Per-connection SQL editor draft.
 *
 * Persists the Query Editor buffer so closing and reopening the tab - or
 * restarting the app - restores the last query text for that connection instead
 * of resetting to the default. Keyed by connection id, with defensive try/catch
 * so a quota/serialization failure never throws into the editor flow.
 */

const KEY = 'stroke:sql-drafts'

/** @returns {Record<string, string>} */
function loadAll() {
  try {
    const raw = localStorage.getItem(KEY)
    if (!raw) return {}
    const parsed = JSON.parse(raw)
    return parsed && typeof parsed === 'object' ? parsed : {}
  } catch {
    return {}
  }
}

/**
 * The saved draft for a connection, or null when there's nothing meaningful to
 * restore (missing or whitespace-only).
 * @param {string | null | undefined} connId
 * @returns {string | null}
 */
export function loadSqlDraft(connId) {
  const all = loadAll()
  const v = all[connId || '_default']
  return typeof v === 'string' && v.trim() ? v : null
}

/**
 * Persist (or clear, when blank) the draft for a connection.
 * @param {string | null | undefined} connId
 * @param {string} text
 */
export function saveSqlDraft(connId, text) {
  try {
    const all = loadAll()
    const key = connId || '_default'
    if (text && text.trim()) all[key] = text
    else delete all[key]
    localStorage.setItem(KEY, JSON.stringify(all))
  } catch {
    // Quota/serialization failure must not throw into the editor flow.
  }
}

// ── Every editor tab ─────────────────────────────────────────────────────────
// The draft above holds one buffer: the tab in front. With three editor tabs
// open, the other two were gone after a restart or a switch to another
// database and back. These keep each tab: its title, its text, the saved query
// it belongs to, and which one was in front.

const TABS_KEY = 'stroke:sql-tabs'

/** @typedef {{ title: string, sql: string, savedQueryId: string | null, active: boolean }} StoredSqlTab */

/** @returns {Record<string, unknown>} */
function loadAllTabs() {
  try {
    const raw = localStorage.getItem(TABS_KEY)
    if (!raw) return {}
    const parsed = JSON.parse(raw)
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? parsed : {}
  } catch {
    return {}
  }
}

/**
 * A connection's editor tabs, oldest first. Anything malformed is dropped
 * rather than restored half-built.
 * @param {string | null | undefined} connId
 * @returns {StoredSqlTab[]}
 */
export function loadSqlTabs(connId) {
  const list = loadAllTabs()[connId || '_default']
  if (!Array.isArray(list)) return []
  return list
    .filter((t) => t && typeof t === 'object' && typeof t.sql === 'string')
    .map((t) => ({
      title: typeof t.title === 'string' && t.title.trim() ? t.title : 'Query Editor',
      sql: t.sql,
      savedQueryId: typeof t.savedQueryId === 'string' && t.savedQueryId ? t.savedQueryId : null,
      active: t.active === true,
    }))
}

/**
 * Keep (or forget, when empty) a connection's editor tabs.
 * @param {string | null | undefined} connId
 * @param {StoredSqlTab[]} tabs
 */
export function saveSqlTabs(connId, tabs) {
  try {
    const all = loadAllTabs()
    const key = connId || '_default'
    if (tabs.length) all[key] = tabs
    else delete all[key]
    localStorage.setItem(TABS_KEY, JSON.stringify(all))
  } catch {
    // Quota/serialization failure must not throw into the editor flow.
  }
}
