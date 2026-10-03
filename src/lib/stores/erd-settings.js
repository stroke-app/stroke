const STORAGE_KEY = 'stroke:erd'

/**
 * @typedef {'all' | 'keys'} ColumnMode
 * @typedef {'compact' | 'comfortable' | 'spacious'} Spacing
 * @typedef {'smart' | 'direct'} Routing
 * @typedef {'badges' | 'lines'} HubLinks
 * @typedef {{ columnMode: ColumnMode, showTypes: boolean, connectedOnly: boolean,
 *   spacing: Spacing, routing: Routing, hubLinks: HubLinks, grid: boolean, touched: string[] }} ErdSettings
 */

/**
 * `touched` lists the keys the user has actually chosen. Anything not in it
 * follows the view's own default - a per-table diagram wants different spacing
 * and line routing than a whole-schema one - and stops following it the moment
 * the user picks a value.
 * @type {ErdSettings}
 */
export const DEFAULT_ERD_SETTINGS = {
  columnMode: 'all',
  showTypes: true,
  connectedOnly: false,
  spacing: 'comfortable',
  routing: 'smart',
  // A table most of the schema points at (tenants, users) is named on each
  // referencing row instead of drawn as a line from every card.
  hubLinks: 'badges',
  grid: true,
  touched: [],
}

/** Defaults per view: a focused table diagram is small and wide-open, a whole
 *  schema is dense and needs its lines routed around the cards. */
export const SCOPE_DEFAULTS = {
  focused: { spacing: /** @type {Spacing} */ ('spacious'), routing: /** @type {Routing} */ ('direct') },
  schema:  { spacing: /** @type {Spacing} */ ('comfortable'), routing: /** @type {Routing} */ ('smart') },
}

/**
 * Layout gutters per spacing preset. `rankSep`/`nodeSep` feed Dagre; the rest
 * drive the focus packer and the shelf packing between components.
 *
 * `rankSep` used to be wider than a card, back when an orthogonal router needed
 * a corridor of stacked lanes between every pair of ranks. Curves along Dagre's
 * own corridors need far less, and the gutter was most of the empty space that
 * made a whole-schema diagram unreadable at fit-zoom.
 */
export const SPACING_PRESETS = {
  compact:     { rankSep: 110, nodeSep: 36,  colGap: 80,  rowGap: 44,  sideGap: 220 },
  comfortable: { rankSep: 150, nodeSep: 52,  colGap: 110, rowGap: 64,  sideGap: 300 },
  spacious:    { rankSep: 240, nodeSep: 88,  colGap: 170, rowGap: 108, sideGap: 430 },
}

/** @returns {ErdSettings} */
export function loadErdSettings() {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return { ...DEFAULT_ERD_SETTINGS }
    const p = JSON.parse(raw)
    return {
      columnMode: p.columnMode === 'keys' ? 'keys' : 'all',
      showTypes: p.showTypes !== false,
      connectedOnly: p.connectedOnly === true,
      spacing: p.spacing in SPACING_PRESETS ? p.spacing : DEFAULT_ERD_SETTINGS.spacing,
      routing: p.routing === 'direct' ? 'direct' : 'smart',
      hubLinks: p.hubLinks === 'lines' ? 'lines' : 'badges',
      grid: p.grid !== false,
      touched: Array.isArray(p.touched) ? p.touched.filter(k => typeof k === 'string') : [],
    }
  } catch {
    return { ...DEFAULT_ERD_SETTINGS }
  }
}

/** @param {Partial<ErdSettings>} patch */
export function saveErdSettings(patch) {
  const prev = loadErdSettings()
  const keys = Object.keys(patch).filter(k => k !== 'touched')
  const next = { ...prev, ...patch, touched: [...new Set([...prev.touched, ...keys])] }
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(next))
  } catch (err) {
    console.error('Failed to persist ERD settings:', err)
  }
  return next
}
