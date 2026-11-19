import { notifyLocalChange } from '$lib/sync/changes.js'
import { writable } from 'svelte/store'
import { debounce } from '$lib/utils.js'

/**
 * @typedef {{
 *   id: string,
 *   chartId: string | null,
 *   span: number,
 *   size?: 's' | 'm' | 'l',
 *   h?: number,
 *   query?: { sql: string, name: string, type?: string },
 * }} DashItem
 *
 * A card shows a saved chart (`chartId`), or a saved query (`query`) drawn as
 * a number, a chart or a table. `h` is its height in pixels once resized by
 * hand; `size` is the older s/m/l preset it falls back to.
 *
 * @typedef {{
 *   id: string,
 *   name: string,
 *   columns: number,
 *   items: DashItem[],
 *   createdAt: number,
 *   refreshSec?: number,
 * }} Dashboard
 */

const KEY = (id) => id ? `stroke:dashboards:${id}` : 'stroke:dashboards'
const ACTIVE_KEY = (id) => id ? `stroke:active-dashboard:${id}` : 'stroke:active-dashboard'

// Tracks the currently active connection scope
let _connId = ''

/** @param {string} connectionId @returns {Dashboard[]} */
function loadDashboards(connectionId) {
  if (!connectionId) return []
  try {
    const specific = JSON.parse(localStorage.getItem(KEY(connectionId)) ?? 'null')
    if (Array.isArray(specific)) return specific
    // One-time migration: load from legacy global key
    const global_ = JSON.parse(localStorage.getItem('stroke:dashboards') ?? '[]')
    return Array.isArray(global_) ? global_ : []
  } catch { return [] }
}

/** @param {string} connectionId @returns {string | null} */
function loadActiveId(connectionId) {
  if (!connectionId) return null
  try {
    return localStorage.getItem(ACTIVE_KEY(connectionId))
      ?? localStorage.getItem('stroke:active-dashboard')
  } catch { return null }
}

export const dashboards = writable(/** @type {Dashboard[]} */ ([]))
export const activeDashboardId = writable(/** @type {string | null} */ (null))

// Dashboard configs can be large; debounce the serialize so rapid edits (drag,
// resize, add) don't JSON.stringify + write on every change.
const persistDashboards = debounce((/** @type {Dashboard[]} */ v, /** @type {string} */ connId) => {
  try { localStorage.setItem(KEY(connId), JSON.stringify(v)) } catch {}
  notifyLocalChange('library')
}, 300)
dashboards.subscribe(v => {
  if (!_connId) return
  persistDashboards(v, _connId)
})
if (typeof window !== 'undefined') {
  window.addEventListener('beforeunload', () => persistDashboards.flush())
}
activeDashboardId.subscribe(v => {
  if (!_connId) return
  try {
    if (v) localStorage.setItem(ACTIVE_KEY(_connId), v)
    else localStorage.removeItem(ACTIVE_KEY(_connId))
  } catch {}
})

/**
 * Switch the active connection. Reloads dashboards from the
 * connection-specific localStorage key. Call whenever the active DB changes.
 * @param {string} connectionId
 */
export function switchDashboardsConnection(connectionId) {
  // Write out any pending edit of the OLD connection first - the set() below
  // re-arms the debounce with the new connection's data, dropping it otherwise.
  persistDashboards.flush()
  _connId = connectionId
  dashboards.set(loadDashboards(_connId))
  activeDashboardId.set(loadActiveId(_connId))
}

/** Write pending edits now (another window is about to read them). */
export function flushDashboards() {
  persistDashboards.flush()
}

/** The connection key the dashboards are stored under right now. */
export function dashboardsConnectionId() {
  return _connId
}

/**
 * The connection key whose dashboards hold `dashId`, `preferred` first. A
 * dashboard window opened with a key the tab didn't store under (an older
 * build, a connection matched another way) still finds its dashboard.
 * @param {string} dashId @param {string} preferred @returns {string | null}
 */
export function findDashboardScope(dashId, preferred) {
  /** @param {string | null} raw */
  const has = (raw) => {
    try {
      const list = JSON.parse(raw ?? 'null')
      return Array.isArray(list) && list.some((d) => d?.id === dashId)
    } catch { return false }
  }
  try {
    if (preferred && has(localStorage.getItem(KEY(preferred)))) return preferred
    for (let i = 0; i < localStorage.length; i++) {
      const k = localStorage.key(i)
      if (k?.startsWith('stroke:dashboards:') && has(localStorage.getItem(k))) return k.slice('stroke:dashboards:'.length)
    }
  } catch { /* storage unavailable */ }
  return null
}

/** @param {string} name @returns {Dashboard} */
export function createDashboard(name) {
  const id = `dash-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`
  const dash = /** @type {Dashboard} */ ({ id, name, columns: 3, items: [], createdAt: Date.now() })
  dashboards.update(ds => [...ds, dash])
  activeDashboardId.set(id)
  return dash
}

/** @param {string} id */
export function deleteDashboard(id) {
  dashboards.update(ds => ds.filter(d => d.id !== id))
  activeDashboardId.update(cur => cur === id ? null : cur)
}

/** @param {string} id @param {Partial<Dashboard>} patch */
export function updateDashboard(id, patch) {
  dashboards.update(ds => ds.map(d => d.id === id ? { ...d, ...patch } : d))
}

/**
 * @param {string} dashId
 * @param {string} chartId
 */
export function addChartToDashboard(dashId, chartId) {
  const itemId = `item-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`
  dashboards.update(ds => ds.map(d =>
    d.id === dashId
      ? { ...d, items: [...d.items, { id: itemId, chartId, span: 1 }] }
      : d
  ))
}

/**
 * A saved query as a card: it runs on the dashboard and is drawn by what it
 * returns (one value as a number, rows as a chart or a table).
 * @param {string} dashId @param {{ sql: string, name: string }} query
 */
export function addQueryToDashboard(dashId, query) {
  const itemId = `item-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`
  dashboards.update(ds => ds.map(d =>
    d.id === dashId
      ? { ...d, items: [...d.items, { id: itemId, chartId: null, span: 1, query: { sql: query.sql, name: query.name } }] }
      : d
  ))
}

/** @param {string} dashId @param {string} itemId @param {Partial<DashItem>} patch */
export function updateItem(dashId, itemId, patch) {
  dashboards.update(ds => ds.map(d =>
    d.id === dashId
      ? { ...d, items: d.items.map(i => i.id === itemId ? { ...i, ...patch } : i) }
      : d
  ))
}

/** @param {string} dashId @param {string} itemId */
export function removeChartFromDashboard(dashId, itemId) {
  dashboards.update(ds => ds.map(d =>
    d.id === dashId
      ? { ...d, items: d.items.filter(i => i.id !== itemId) }
      : d
  ))
}

/** @param {string} dashId @param {string} itemId @param {number} span */
export function setItemSpan(dashId, itemId, span) {
  dashboards.update(ds => ds.map(d =>
    d.id === dashId
      ? { ...d, items: d.items.map(i => i.id === itemId ? { ...i, span } : i) }
      : d
  ))
}

/** Card height. @param {string} dashId @param {string} itemId @param {'s'|'m'|'l'} size */
export function setItemSize(dashId, itemId, size) {
  dashboards.update(ds => ds.map(d =>
    d.id === dashId
      ? { ...d, items: d.items.map(i => i.id === itemId ? { ...i, size } : i) }
      : d
  ))
}

/** @param {string} dashId @param {string[]} newOrder new array of item ids in desired order */
export function reorderItems(dashId, newOrder) {
  dashboards.update(ds => ds.map(d => {
    if (d.id !== dashId) return d
    const itemMap = Object.fromEntries(d.items.map(i => [i.id, i]))
    const reordered = newOrder.map(id => itemMap[id]).filter(Boolean)
    const rest = d.items.filter(i => !newOrder.includes(i.id))
    return { ...d, items: [...reordered, ...rest] }
  }))
}

/** Re-read the open connection's dashboards (sync wrote new ones). */
export function reloadDashboards() {
  if (!_connId) return
  dashboards.set(loadDashboards(_connId))
}
