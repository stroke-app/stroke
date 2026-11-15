import { notifyLocalChange } from '$lib/sync/changes.js'
import { writable } from 'svelte/store'
import { debounce } from '$lib/utils.js'

/**
 * @typedef {{
 *   id: string,
 *   chartId: string | null,
 *   span: 1 | 2 | 3,
 * }} DashItem
 *
 * @typedef {{
 *   id: string,
 *   name: string,
 *   columns: 2 | 3,
 *   items: DashItem[],
 *   createdAt: number,
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

/** @param {string} dashId @param {string} itemId */
export function removeChartFromDashboard(dashId, itemId) {
  dashboards.update(ds => ds.map(d =>
    d.id === dashId
      ? { ...d, items: d.items.filter(i => i.id !== itemId) }
      : d
  ))
}

/** @param {string} dashId @param {string} itemId @param {1|2|3} span */
export function setItemSpan(dashId, itemId, span) {
  dashboards.update(ds => ds.map(d =>
    d.id === dashId
      ? { ...d, items: d.items.map(i => i.id === itemId ? { ...i, span } : i) }
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
