import { get, writable } from 'svelte/store'

const STORAGE_KEY = 'stroke:layout'

/** @typedef {'normal' | 'json'} InspectorView */

/** @typedef {{ navSidebarWidth: number, navSidebarOpen: boolean, navSidebarSide: 'left' | 'right', navSidebarPanel: string, inspectorWidth: number, inspectorView: InspectorView, sqlEditorHeight: number, logPanelWidth: number, logPanelOpen: boolean, aiSidebarWidth: number, aiSidebarOpen: boolean, statusBarVisible: boolean, tabBarVisible: boolean, tableToolbarVisible: boolean }} PanelLayout */

export const DEFAULT_LAYOUT = {
  navSidebarWidth: 320, // NAV_SIDEBAR_MIN: the header row fits whole
  navSidebarOpen: true,
  navSidebarSide: /** @type {'left' | 'right'} */ ('left'),
  navSidebarPanel: 'tables',
  inspectorWidth: 300,
  inspectorView: 'normal',
  sqlEditorHeight: 320,
  logPanelWidth: 300,
  logPanelOpen: false,
  aiSidebarWidth: 400,
  aiSidebarOpen: false,
  statusBarVisible: true,
  tabBarVisible: true,
  tableToolbarVisible: true,
}

/**
 * Which side the nav sidebar sits on, as a store.
 *
 * The rest of the layout is read once at mount and written on drag, which is
 * fine for sizes nobody changes from two places. The side is different: it can
 * be set from the sidebar's own context menu AND from Settings → Appearance, so
 * it needs a single value both can write and the shell can react to. Seeded from
 * the persisted layout; `setSidebarSide` is the only writer and it persists.
 * @type {import('svelte/store').Writable<'left' | 'right'>}
 */
export const sidebarSideStore = writable(loadLayout().navSidebarSide)

/** @param {'left' | 'right'} side */
export function setSidebarSide(side) {
  const next = side === 'right' ? 'right' : 'left'
  if (get(sidebarSideStore) !== next) sidebarSideStore.set(next)
  saveLayout({ navSidebarSide: next })
}

// Narrow enough to stay out of the way, wide enough for the header row whole:
// 6 tabs at 32 + their 4px gaps (212), 3 actions at 28 + 2px gaps (88), the
// 4px between them and 8px padding each side - 320 at a 16px rem (the width
// is scaled by --app-scale, so it holds at every zoom). Any narrower and the
// tab strip clips its last icons under the actions.
export const NAV_SIDEBAR_MIN = 320
// Long, prefix-heavy table names (django_/invoicing_/bots_…) truncate well past
// 420px, so the nav sidebar can be dragged much wider.
export const NAV_SIDEBAR_MAX = 720
export const INSPECTOR_MIN = 220
export const INSPECTOR_MAX = 640
export const SQL_EDITOR_MIN = 120
export const SQL_EDITOR_RESULTS_MIN = 120
export const LOG_PANEL_MIN = 220
export const LOG_PANEL_MAX = 600
export const AI_SIDEBAR_MIN = 320
export const AI_SIDEBAR_MAX = 720

/** @param {number} width */
export function clampNavSidebarWidth(width) {
  return Math.round(Math.min(NAV_SIDEBAR_MAX, Math.max(NAV_SIDEBAR_MIN, width)))
}

/** @param {number} width */
export function clampInspectorWidth(width) {
  return Math.round(Math.min(INSPECTOR_MAX, Math.max(INSPECTOR_MIN, width)))
}

/** @param {number} width */
export function clampLogPanelWidth(width) {
  return Math.round(Math.min(LOG_PANEL_MAX, Math.max(LOG_PANEL_MIN, width)))
}

/** @param {number} width */
export function clampAiSidebarWidth(width) {
  return Math.round(Math.min(AI_SIDEBAR_MAX, Math.max(AI_SIDEBAR_MIN, width)))
}

/** @param {number} height @param {number} [containerHeight] */
export function clampSqlEditorHeight(height, containerHeight = 0) {
  const max =
    containerHeight > 0
      ? Math.max(SQL_EDITOR_MIN, containerHeight - SQL_EDITOR_RESULTS_MIN)
      : 720
  return Math.round(Math.min(max, Math.max(SQL_EDITOR_MIN, height)))
}

/** @returns {PanelLayout} */
export function loadLayout() {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return { ...DEFAULT_LAYOUT }
    const parsed = JSON.parse(raw)
    let navSidebarWidth = Number(parsed.navSidebarWidth)
    let inspectorWidth = Number(parsed.inspectorWidth)
    let sqlEditorHeight = Number(parsed.sqlEditorHeight)
    let logPanelWidth = Number(parsed.logPanelWidth)
    let aiSidebarWidth = Number(parsed.aiSidebarWidth)
    if (!Number.isFinite(navSidebarWidth)) navSidebarWidth = DEFAULT_LAYOUT.navSidebarWidth
    if (!Number.isFinite(inspectorWidth)) inspectorWidth = DEFAULT_LAYOUT.inspectorWidth
    if (!Number.isFinite(sqlEditorHeight)) sqlEditorHeight = DEFAULT_LAYOUT.sqlEditorHeight
    if (!Number.isFinite(logPanelWidth)) logPanelWidth = DEFAULT_LAYOUT.logPanelWidth
    if (!Number.isFinite(aiSidebarWidth)) aiSidebarWidth = DEFAULT_LAYOUT.aiSidebarWidth
    const inspectorView = parsed.inspectorView === 'json' ? 'json' : 'normal'
    const navSidebarSide = parsed.navSidebarSide === 'right' ? 'right' : 'left'
    const navSidebarPanel = typeof parsed.navSidebarPanel === 'string' ? parsed.navSidebarPanel : 'tables'
    const navSidebarOpen = parsed.navSidebarOpen !== false
    const logPanelOpen = parsed.logPanelOpen === true
    const aiSidebarOpen = parsed.aiSidebarOpen === true
    const statusBarVisible = parsed.statusBarVisible !== false
    const tabBarVisible = parsed.tabBarVisible !== false
    const tableToolbarVisible = parsed.tableToolbarVisible !== false
    return {
      navSidebarWidth: clampNavSidebarWidth(navSidebarWidth),
      navSidebarOpen,
      navSidebarSide,
      navSidebarPanel,
      inspectorWidth: clampInspectorWidth(inspectorWidth),
      inspectorView,
      sqlEditorHeight: clampSqlEditorHeight(sqlEditorHeight),
      logPanelWidth: clampLogPanelWidth(logPanelWidth),
      logPanelOpen,
      aiSidebarWidth: clampAiSidebarWidth(aiSidebarWidth),
      aiSidebarOpen,
      statusBarVisible,
      tabBarVisible,
      tableToolbarVisible,
    }
  } catch {
    return { ...DEFAULT_LAYOUT }
  }
}

/** @param {Partial<PanelLayout>} patch */
export function saveLayout(patch) {
  const next = { ...loadLayout(), ...patch }
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(next))
  } catch (err) {
    console.error('Failed to persist layout:', err)
  }
  return next
}
