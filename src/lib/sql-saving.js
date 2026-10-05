/**
 * What Save does in the SQL editor, and which editor tabs are kept.
 *
 * A tab opened from a saved query, or saved once already, carries that query's
 * id. Save writes the query in place; Save as is the only way to get a copy.
 * Before this every Save asked for a name and filed a new entry, so one query
 * piled up under Saved once per press.
 */

/** @typedef {import('$lib/stores/query-history.js').SavedQuery} SavedQuery */
/**
 * @typedef {{ kind: 'update', query: SavedQuery }
 *   | { kind: 'unchanged', query: SavedQuery }
 *   | { kind: 'link', query: SavedQuery }
 *   | { kind: 'ask' }} SavePlan
 * @typedef {import('$lib/stores/sql-draft.js').StoredSqlTab} StoredSqlTab
 */

/** Same query, ignoring the whitespace at either end. @param {unknown} a @param {unknown} b */
export const sameSql = (a, b) => String(a ?? '').trim() === String(b ?? '').trim()

/**
 * @param {string} sql the editor's text
 * @param {string | null | undefined} linkedId the saved query the tab belongs to
 * @param {SavedQuery[]} saved this connection's saved queries
 * @returns {SavePlan}
 */
export function planSave(sql, linkedId, saved) {
  const linked = linkedId ? saved.find((q) => q.id === linkedId) : undefined
  if (linked) return sameSql(linked.sql, sql) ? { kind: 'unchanged', query: linked } : { kind: 'update', query: linked }
  // Not linked, or its query was deleted since: the same text saved already is
  // that query, not a reason for a second copy.
  const same = savedQueryFor(sql, saved)
  return same ? { kind: 'link', query: same } : { kind: 'ask' }
}

/**
 * The saved query whose text this is, so a tab that loads it belongs to it.
 * @param {string} sql @param {SavedQuery[]} saved
 */
export function savedQueryFor(sql, saved) {
  if (!String(sql ?? '').trim()) return null
  return saved.find((q) => sameSql(q.sql, sql)) ?? null
}

/**
 * The Query Editor tabs to keep for a connection, in tab order. A DDL viewer
 * (`draft: false`) is a scratch buffer and stays out.
 * @param {Array<{ id: string, kind: string, title?: string, draft?: boolean, savedQueryId?: string | null, state?: any }>} tabs
 * @param {string | null} activeTabId
 * @param {string} liveText the editor's text; the tab in front only copies it into its state when left
 * @returns {StoredSqlTab[]}
 */
export function sqlTabsToStore(tabs, activeTabId, liveText) {
  return tabs
    .filter((t) => t.kind === 'sql' && t.draft !== false)
    .map((t) => ({
      title: t.title || 'Query Editor',
      sql: t.id === activeTabId ? liveText : String(t.state?.sqlText ?? ''),
      savedQueryId: t.savedQueryId ?? null,
      active: t.id === activeTabId,
    }))
}
