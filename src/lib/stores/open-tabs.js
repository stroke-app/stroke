/**
 * The tabs a database had open, so connecting to it again opens them again.
 *
 * Only what it takes to reopen a tab is kept: its kind, and for a table its
 * schema, name, and the sort, filters, search and view it was left on. Rows
 * are never stored; a reopened table fetches them when it comes to the front,
 * as a tab reopened with Mod+Shift+T does. An editor tab is a placeholder for
 * its position: its text is kept by sql-draft.js, as it always was.
 *
 * Tabs that belong to a live session (a terminal, an AI chat, a notebook file,
 * a DDL viewer) are not kept.
 */

const KEY = 'stroke:open-tabs'
/** Databases remembered; the ones used longest ago go first. */
const MAX_DATABASES = 60
/** Tabs kept per database: past this a list is noise, not a workspace. */
const MAX_TABS = 40

/** Pages with no state of their own: reopening one is creating it again. */
export const PAGE_KINDS = new Set([
  'schema', 'erd', 'diagrams', 'reltree', 'backup', 'charts', 'dashboard', 'json',
  'logs', 'insights', 'advisor', 'objects', 'search', 'schema-timeline', 'data-diff',
  'migrations', 'security', 'extensions', 'orm', 'orm-schema', 'map', 'queries',
  'redis', 'dynamodb',
])

/**
 * @typedef {{ kind: string, pinned?: boolean, active?: boolean, title?: string,
 *   schema?: string, table?: string, tableKind?: string, collection?: string,
 *   view?: Record<string, unknown> }} StoredTab
 */

/** The parts of a table tab's state that say how it was being looked at. */
const TABLE_VIEW_KEYS = ['rowSort', 'rowFilters', 'rowSearch', 'pageSize', 'dataViewMode', 'tableViewMode']

/**
 * What to keep of the open tabs. Tabs that can't be reopened are left out.
 * @param {any[]} tabs @param {string | null} activeTabId
 * @returns {StoredTab[]}
 */
export function tabsToStore(tabs, activeTabId) {
  /** @type {StoredTab[]} */
  const out = []
  for (const t of tabs) {
    /** @type {StoredTab | null} */
    let s = null
    if (t.kind === 'table' && t.state?.table) {
      /** @type {Record<string, unknown>} */
      const view = {}
      for (const k of TABLE_VIEW_KEYS) {
        const v = t.state[k]
        if (v != null && v !== '' && !(Array.isArray(v) && v.length === 0)) view[k] = v
      }
      s = { kind: 'table', schema: t.state.schema, table: t.state.table, tableKind: t.state.tableKind ?? 'table', title: t.title }
      if (Object.keys(view).length) s.view = view
    } else if (t.kind === 'sql' && t.draft !== false) {
      s = { kind: 'sql' }
    } else if (t.kind === 'docs') {
      s = { kind: 'docs', collection: t.collection ?? '', title: t.title }
    } else if (PAGE_KINDS.has(t.kind)) {
      s = { kind: t.kind }
    }
    if (!s) continue
    if (t.pinned) s.pinned = true
    if (t.id === activeTabId) s.active = true
    out.push(s)
    if (out.length >= MAX_TABS) break
  }
  return out
}

/** @returns {Record<string, { at: number, tabs: StoredTab[] }>} */
function readAll() {
  try {
    const raw = localStorage.getItem(KEY)
    const all = raw ? JSON.parse(raw) : {}
    return all && typeof all === 'object' && !Array.isArray(all) ? all : {}
  } catch {
    return {}
  }
}

/** @param {string} dbKey @returns {StoredTab[]} */
export function loadOpenTabs(dbKey) {
  if (!dbKey) return []
  const list = readAll()[dbKey]?.tabs
  return Array.isArray(list) ? list.filter((t) => t && typeof t.kind === 'string') : []
}

/** @param {string} dbKey @param {StoredTab[]} tabs */
export function saveOpenTabs(dbKey, tabs) {
  if (!dbKey) return
  const all = readAll()
  all[dbKey] = { at: Date.now(), tabs }
  const keys = Object.keys(all)
  if (keys.length > MAX_DATABASES) {
    keys.sort((a, b) => (all[a].at ?? 0) - (all[b].at ?? 0))
    for (const k of keys.slice(0, keys.length - MAX_DATABASES)) delete all[k]
  }
  try {
    localStorage.setItem(KEY, JSON.stringify(all))
  } catch {
    /* storage full or blocked: the tabs just won't come back */
  }
}
