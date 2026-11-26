/**
 * Persistent store for saved charts, backed by localStorage.
 * Charts are scoped per database connection - switching connections loads
 * that connection's charts. An empty connectionId shows nothing.
 */
import { notifyLocalChange } from '$lib/sync/changes.js'
import { get, writable } from 'svelte/store'
import { debounce } from '$lib/utils.js'

/**
 * @typedef {{
 *   id: string
 *   name: string
 *   group: string
 *   createdAt: number
 *   connectionId: string
 *   sql: string
 *   config: {
 *     type: string
 *     xCol: string
 *     yCol: string
 *     zCol?: string
 *     groupCol?: string
 *     title?: string
 *   }
 *   previewOption?: any
 * }} SavedChart
 */

const CHARTS_KEY = (id) => id ? `stroke:saved-charts:${id}` : 'stroke:saved-charts'
const GROUPS_KEY = (id) => id ? `stroke:chart-groups:${id}` : 'stroke:chart-groups'

// Tracks the currently active connection scope
let _connId = ''

/**
 * Load charts for a connection. If the connection-specific key is empty,
 * migrates charts from the legacy global key that match this connectionId.
 * @param {string} connectionId
 * @returns {SavedChart[]}
 */
function loadCharts(connectionId) {
  if (!connectionId) return []
  try {
    const specific = JSON.parse(localStorage.getItem(CHARTS_KEY(connectionId)) ?? 'null')
    if (Array.isArray(specific)) return specific
    // One-time migration: filter matching charts from the legacy global key
    const global_ = JSON.parse(localStorage.getItem('stroke:saved-charts') ?? '[]')
    return Array.isArray(global_) ? global_.filter(c => c.connectionId === connectionId) : []
  } catch { return [] }
}

/** @param {string} connectionId @returns {string[]} */
function loadGroups(connectionId) {
  if (!connectionId) return ['Default']
  try {
    const specific = JSON.parse(localStorage.getItem(GROUPS_KEY(connectionId)) ?? 'null')
    if (Array.isArray(specific) && specific.length > 0) return specific
    // One-time migration: build groups from migrated charts
    const charts = loadCharts(connectionId)
    const groups = [...new Set(charts.map(c => c.group).filter(Boolean))]
    return groups.length > 0 ? groups : ['Default']
  } catch { return ['Default'] }
}

/** @param {any} v @param {string} key */
function persist(v, key) {
  try { localStorage.setItem(key, JSON.stringify(v)) } catch {}
  notifyLocalChange('library')
}

/** Re-read the open connection's charts and groups (sync wrote new ones). */
export function reloadChartsConnection() {
  if (!_connId) return
  savedCharts.set(loadCharts(_connId))
  chartGroups.set(loadGroups(_connId))
}

export const savedCharts = writable(/** @type {SavedChart[]} */ ([]))
export const chartGroups = writable(/** @type {string[]} */ (['Default']))

// Debounced persistence - chart edits can fire in bursts; avoid a synchronous
// JSON.stringify + localStorage write on every change. Charts and groups use
// separate debouncers so their distinct keys never coalesce/clobber.
const persistCharts = debounce((/** @type {any} */ v, /** @type {string} */ key) => persist(v, key), 300)
const persistGroups = debounce((/** @type {any} */ v, /** @type {string} */ key) => persist(v, key), 300)
savedCharts.subscribe(v => { if (_connId) persistCharts(v, CHARTS_KEY(_connId)) })
chartGroups.subscribe(v => { if (_connId) persistGroups(v, GROUPS_KEY(_connId)) })
if (typeof window !== 'undefined') {
  window.addEventListener('beforeunload', () => { persistCharts.flush(); persistGroups.flush() })
}

/**
 * Switch the active connection. Reloads charts and groups from the
 * connection-specific localStorage key. Call whenever the active DB changes.
 * @param {string} connectionId
 */
export function switchChartsConnection(connectionId) {
  // Write out any pending edit of the OLD connection first - the set()s below
  // re-arm the debouncers with the new connection's data, dropping it otherwise.
  persistCharts.flush()
  persistGroups.flush()
  _connId = connectionId
  savedCharts.set(loadCharts(_connId))
  chartGroups.set(loadGroups(_connId))
}

// ── Mutations ─────────────────────────────────────────────────────────────────

/**
 * Save a new chart. Returns the generated id.
 * @param {Omit<SavedChart, 'id' | 'createdAt'>} chart
 * @returns {string}
 */
export function saveChart(chart) {
  const id = `chart-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`
  const entry = /** @type {SavedChart} */ ({ ...chart, id, createdAt: Date.now() })
  savedCharts.update(cs => [...cs, entry])
  chartGroups.update(gs => gs.includes(entry.group) ? gs : [...gs, entry.group])
  return id
}

/**
 * @param {string} id
 * @returns {(() => boolean) | null} puts it back in its place, for an Undo; false
 *   once the charts on screen belong to another connection
 */
export function deleteChart(id) {
  const list = get(savedCharts)
  const index = list.findIndex(c => c.id === id)
  if (index < 0) return null
  const chart = list[index]
  const conn = _connId
  savedCharts.update(cs => cs.filter(c => c.id !== id))
  return () => {
    if (_connId !== conn || get(savedCharts).some(c => c.id === id)) return false
    savedCharts.update(cs => [...cs.slice(0, index), chart, ...cs.slice(index)])
    chartGroups.update(gs => gs.includes(chart.group) ? gs : [...gs, chart.group])
    return true
  }
}

/** @param {string} id @param {Partial<SavedChart>} patch */
export function updateChart(id, patch) {
  savedCharts.update(cs => cs.map(c => c.id === id ? { ...c, ...patch } : c))
}

/** @param {string} name */
export function addGroup(name) {
  const t = name.trim()
  if (!t) return
  chartGroups.update(gs => gs.includes(t) ? gs : [...gs, t])
}

/** @param {string} name */
export function deleteGroup(name) {
  if (name === 'Default') return
  chartGroups.update(gs => gs.filter(g => g !== name))
  savedCharts.update(cs => cs.map(c => c.group === name ? { ...c, group: 'Default' } : c))
}

/** @param {string} oldName @param {string} newName */
export function renameGroup(oldName, newName) {
  const t = newName.trim()
  if (!t) return
  chartGroups.update(gs => gs.map(g => g === oldName ? t : g))
  savedCharts.update(cs => cs.map(c => c.group === oldName ? { ...c, group: t } : c))
}
