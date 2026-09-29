import { writable, get } from 'svelte/store'
import { setMode } from 'mode-watcher'
import {
  DEFAULT_THEME_ID,
  isDarkTheme,
  THEME_IDS,
  CYCLE_THEME_IDS,
  normalizeThemeId,
  getThemeDefinition,
} from '$lib/themes/registry.js'
import { zoomState, ZOOM_MIN, ZOOM_MAX } from '$lib/stores/canvas-zoom.svelte.js'
import { detectOs } from '$lib/platform.js'
import {
  UI_TYPE_SCALE,
  TYPE_SCALE_REF,
  rootPxFor,
  ROOT_BASE_PX,
  ZOOM_STEPS,
  buildTypeScale,
} from '$lib/type-scale.js'
import { SQL_FORMAT_DEFAULTS, normalizeSqlFormat, setSqlFormatOptions } from '$lib/sql-format-options.js'

const STORAGE_KEY = 'stroke:settings'

/**
 * Whether the OS should own scrolling, when the user has expressed no preference.
 *
 * Eased scrolling is ours: wheel deltas are accumulated and `scrollTop` is walked
 * toward the target one frame at a time. That is a real improvement over a
 * discrete mouse wheel, which otherwise jumps a fixed notch with no motion at all.
 *
 * It is the wrong thing on macOS. The OS has ALREADY applied momentum and rubber
 * banding to a trackpad's deltas by the time they reach us, so easing them again
 * is a second filter on top of a filter: every gesture trails its fingers by the
 * length of our ease, which is exactly what "the scrolling feels laggy" describes.
 * It also costs a non-passive wheel listener, which puts the main thread in front
 * of every tick on the one platform whose compositor did not need it.
 *
 * So: OS-owned on macOS, eased elsewhere. Either way Settings → Appearance →
 * Native scrolling is the override, and an explicit choice always wins over this.
 */
export function defaultNativeScroll() {
  return detectOs() === 'macos'
}

/** One-shot marker for the migration in `loadSettings` (see there for why). */
const SCROLL_DEFAULT_KEY = 'stroke:scroll-default-v2'
const scrollDefaultApplied = () => {
  try { return localStorage.getItem(SCROLL_DEFAULT_KEY) === '1' } catch { return true }
}
const markScrollDefaultApplied = () => {
  try { localStorage.setItem(SCROLL_DEFAULT_KEY, '1') } catch {}
}

/**
 * One-shot marker for the monospace-default migration in `loadSettings`.
 *
 * A stored blob from before this change carries `font: "geist"` - which is
 * indistinguishable from someone having chosen Geist on purpose - so the switch
 * cannot be inferred from the value. It runs once per install: the first load
 * after the update rewrites a font nobody changed, sets this key, and never
 * touches the setting again.
 */
const FONT_DEFAULT_KEY = 'stroke:font-default-mono'
const fontDefaultApplied = () => {
  try { return localStorage.getItem(FONT_DEFAULT_KEY) === '1' } catch { return true }
}
const markFontDefaultApplied = () => {
  try { localStorage.setItem(FONT_DEFAULT_KEY, '1') } catch {}
}
/** Same one-shot marker for the move from the mono default to the Claude one. */
const FONT_DEFAULT_CLAUDE_KEY = 'stroke:font-default-claude'
const fontClaudeDefaultApplied = () => {
  try { return localStorage.getItem(FONT_DEFAULT_CLAUDE_KEY) === '1' } catch { return true }
}
const markFontClaudeDefaultApplied = () => {
  try { localStorage.setItem(FONT_DEFAULT_CLAUDE_KEY, '1') } catch {}
}
/** And for the move back to Geist, from either of the two defaults before it. */
const FONT_DEFAULT_GEIST_KEY = 'stroke:font-default-geist'
const fontGeistDefaultApplied = () => {
  try { return localStorage.getItem(FONT_DEFAULT_GEIST_KEY) === '1' } catch { return true }
}
const markFontGeistDefaultApplied = () => {
  try { localStorage.setItem(FONT_DEFAULT_GEIST_KEY, '1') } catch {}
}

/** @typedef {import('$lib/themes/registry.js').ThemeId} ThemeId */
/** @typedef {'claude' | 'geist' | 'serif' | 'apple' | 'inter' | 'mono' | 'fira' | 'plex' | 'space' | 'source'} FontId */
/** @typedef {'regular' | 'light' | 'bold'} IconStyleId */
/** @typedef {'lucide' | 'hugeicons' | 'phosphor'} IconSetId */
/** @typedef {{ theme: ThemeId, zoom: number, font: FontId, iconStyle: IconStyleId, iconSet: IconSetId, tableStyle: TableStyleId, jsonTheme: JsonThemeId, mcpAutoStart: boolean, launchAtLogin: boolean, autoReconnectOnStartup: boolean, previewDmlBeforeApply: boolean, defaultDataView: string, paginationMode: string, maxQueryHistory: number, connectTimeoutMs: number, socketTimeoutMs: number, maxAllowedPacket: number, sessionTimezone: string, vimMode: boolean, cmdkAiEnabled: boolean, liveModeEnabled: boolean, lazyWideColumns: boolean, nullSortOrder: string, agentChatFontSize: number, agentCodeFontSize: number, agentThinkingStyle: string, agentShowQueryCards: boolean, agentWebAccess: boolean, tableTextAlign: string, telemetry: boolean, jsonWordWrap: boolean, nativeScroll: boolean, rowSpacing: RowSpacingId, motion: MotionId, zebraRows: boolean, showRowNumbers: boolean, showMenuBar: boolean, numberGrouping: boolean, imagePreview: boolean, openUrlsOnClick: boolean, highlightActiveRow: boolean, gridFontSize: number, autoSaveQueries: boolean, sqlFormat: import('$lib/sql-format-options.js').SqlFormatOptions }} AppSettings */

/**
 * UI type scale in design pixels: `[step, font-size, line-height?]`, matching
 * DESIGN_SYSTEM.md §4. applySettings() rounds each step to a whole pixel for the
 * active base size and zoom, then publishes it as `--fs-<step>` / `--lh-<step>`;
 * app.css's `.text-ui-*` classes read those vars. The rounding lives here rather
 * than in a CSS calc so a non-14px base can never push a step off the pixel grid.
 * The `15` step exists only to back the legacy `text-[15px]` compatibility class.
 */

export { ZOOM_STEPS }
const DEFAULT_ZOOM = 1

/**
 * Selectable font stacks. Each sets the UI (`--font-sans`) and data/SQL/grid
 * (`--font-mono`) families. Stacks fall back gracefully when a font isn't
 * installed, so an unavailable option degrades instead of breaking. `heading`
 * (optional) sets `--font-heading` for dialog titles; without it headings
 * follow the sans.
 * @type {Record<FontId, { label: string, description: string, sans: string, mono: string, heading?: string }>}
 */
export const FONT_PRESETS = {
  // The claude.ai look with open faces: its own Anthropic Sans/Serif are not
  // licensed for reuse, so Inter carries the UI and Source Serif 4 the
  // headings. Data and SQL stay in JetBrains Mono. All three ship with the app.
  claude: {
    label: 'Claude',
    description: 'Inter + Source Serif headings',
    sans: '"Inter Variable", ui-sans-serif, system-ui, sans-serif',
    mono: '"JetBrains Mono Variable", ui-monospace, monospace',
    heading: '"Source Serif 4 Variable", ui-serif, Georgia, serif',
  },
  geist: {
    label: 'Geist',
    description: 'Clean variable sans',
    sans: '"Geist Variable", ui-sans-serif, system-ui, sans-serif',
    mono: '"Geist Mono Variable", ui-monospace, monospace',
  },
  serif: {
    label: 'Serif',
    description: 'Editorial serif UI',
    sans: 'ui-serif, "New York", "Iowan Old Style", Georgia, "Times New Roman", serif',
    mono: 'ui-monospace, "SF Mono", Menlo, Monaco, Consolas, monospace',
  },
  apple: {
    label: 'Apple',
    description: 'San Francisco + SF Mono',
    sans: '-apple-system, BlinkMacSystemFont, "SF Pro Text", "SF Pro Display", system-ui, sans-serif',
    mono: 'ui-monospace, "SF Mono", "SFMono-Regular", Menlo, Monaco, Consolas, monospace',
  },
  // Both faces below ship with the app (fontsource imports in app.css), so
  // these presets render identically on every platform - no system fallback.
  inter: {
    label: 'Inter',
    description: 'Inter + JetBrains Mono',
    sans: '"Inter Variable", ui-sans-serif, system-ui, sans-serif',
    mono: '"JetBrains Mono Variable", ui-monospace, monospace',
  },
  mono: {
    label: 'Mono',
    description: 'All-monospace terminal feel',
    sans: '"JetBrains Mono Variable", ui-monospace, monospace',
    mono: '"JetBrains Mono Variable", ui-monospace, monospace',
  },
  // The four below also ship with the app (fontsource imports in app.css).
  fira: {
    label: 'Fira Code',
    description: 'Inter + Fira Code ligatures',
    sans: '"Inter Variable", ui-sans-serif, system-ui, sans-serif',
    mono: '"Fira Code Variable", ui-monospace, monospace',
  },
  plex: {
    label: 'IBM Plex',
    description: 'IBM Plex Sans + Mono',
    sans: '"IBM Plex Sans", ui-sans-serif, system-ui, sans-serif',
    mono: '"IBM Plex Mono", ui-monospace, monospace',
  },
  space: {
    label: 'Space Grotesk',
    description: 'Geometric sans + JetBrains Mono',
    sans: '"Space Grotesk Variable", ui-sans-serif, system-ui, sans-serif',
    mono: '"JetBrains Mono Variable", ui-monospace, monospace',
  },
  source: {
    label: 'Source Code Pro',
    description: 'Inter + Source Code Pro',
    sans: '"Inter Variable", ui-sans-serif, system-ui, sans-serif',
    mono: '"Source Code Pro Variable", ui-monospace, monospace',
  },
}
/**
 * The app defaults to Geist: Geist for the UI, Geist Mono for the data (grid,
 * SQL, identifiers).
 *
 * It is the third default. The all-mono one set JetBrains Mono for the chrome
 * too and read as a terminal rather than a desktop app; the Claude preset that
 * replaced it (Inter + serif titles) stays available in Settings → Appearance.
 *
 * Existing installs move with each default change exactly once, through the
 * FONT_DEFAULT_* keys above: an update should land the new look, and someone
 * who has since picked their own font should keep it.
 * @type {FontId}
 */
export const DEFAULT_FONT = 'geist'
/** @returns {FontId} */
function normalizeFont(/** @type {unknown} */ id) {
  return FONT_PRESETS[/** @type {FontId} */ (id)] ? /** @type {FontId} */ (id) : DEFAULT_FONT
}

/**
 * Selectable icon weights. Applied globally as the `stroke-width` of every Lucide
 * icon via a `[data-icon-style]` rule in app.css - no per-component changes. Bold
 * also aids low-vision readability of small nav/toolbar glyphs.
 * @type {Record<IconStyleId, { label: string, description: string, strokeWidth: number }>}
 */
export const ICON_STYLES = {
  regular: { label: 'Regular', description: 'Balanced default weight', strokeWidth: 2 },
  light:   { label: 'Light',   description: 'Thin and minimal',        strokeWidth: 1.5 },
  bold:    { label: 'Bold',    description: 'Heavier, high-visibility', strokeWidth: 2.5 },
}
/** @type {IconStyleId} */
export const DEFAULT_ICON_STYLE = 'regular'
/** @returns {IconStyleId} */
function normalizeIconStyle(/** @type {unknown} */ id) {
  return ICON_STYLES[/** @type {IconStyleId} */ (id)] ? /** @type {IconStyleId} */ (id) : DEFAULT_ICON_STYLE
}

/**
 * Selectable icon families. `lucide` is the built-in stroke set (also honors the
 * icon-weight setting above). `hugeicons` swaps in the Hugeicons stroke set wherever
 * a component renders through the shared `Icon` wrapper; unmapped glyphs fall back
 * to Lucide, so coverage can grow without ever breaking the UI.
 * @type {Record<IconSetId, { label: string, description: string }>}
 */
export const ICON_SETS = {
  lucide:    { label: 'Lucide',    description: 'Crisp, minimal built-in set' },
  hugeicons: { label: 'Hugeicons', description: 'Rounded, expressive premium set' },
  phosphor:  { label: 'Phosphor',  description: 'Friendly, geometric open set' },
}
/**
 * Hugeicons is the app's look. Lucide remains the safety net rather than the
 * default: `Icon.svelte` falls back to it per NAME, so the two glyphs this
 * registry has not mapped yet still render - switching the default can add
 * coverage gaps over time but never holes.
 *
 * Only new installs land here. `iconSet` is persisted, so anyone who has already
 * run the app keeps whatever is in their settings until they change it.
 * @type {IconSetId}
 */
export const DEFAULT_ICON_SET = 'hugeicons'
/** @returns {IconSetId} */
function normalizeIconSet(/** @type {unknown} */ id) {
  return ICON_SETS[/** @type {IconSetId} */ (id)] ? /** @type {IconSetId} */ (id) : DEFAULT_ICON_SET
}

/**
 * @typedef {'lines'|'double'|'hairline'|'none'|'ledger'|'graph'|'bands'|'ticks'
 *   |'dotted'|'dots'|'minimal'|'bordered'|'striped'|'dashed'|'columns'} TableStyleId
 * @typedef {{ label: string, description: string,
 *   rows: boolean, cols: boolean, dash: number[]|null, dots: boolean, strong?: boolean, zebra?: boolean,
 *   double?: boolean, ticks?: boolean, groupEvery?: number }} TableStyleDef
 */

/**
 * Data-grid style presets for the canvas table. Each preset only changes how the
 * per-row grid pass draws separators - it's applied in DataTable's virtualized
 * draw(), so it costs O(visible cells) and never scales with total row count.
 *   - rows/cols: draw horizontal / vertical separators
 *   - dash:      canvas setLineDash pattern (null = solid)
 *   - dots:      draw a small dot at each cell join instead of lines
 * @type {Record<TableStyleId, TableStyleDef>}
 */
export const TABLE_STYLES = {
  lines:    { label: 'Lines',    description: 'Solid grid lines (classic)',        rows: true,  cols: true,  dash: null,   dots: false },
  double:   { label: 'Double',   description: 'Twin rules - a ledger/print feel',  rows: true,  cols: true,  dash: null,   dots: false, double: true },
  hairline: { label: 'Hairline', description: 'The finest dash - barely there',    rows: true,  cols: true,  dash: [1, 5], dots: false },
  none:     { label: 'None',     description: 'No rules at all - text only',       rows: false, cols: false, dash: null,   dots: false },
  // `groupEvery` draws a stronger rule every Nth row. Paired with rows:true it is
  // ruled paper; with rows:false it is the only horizontal line on screen, which
  // is the quietest way to keep a long page countable.
  ledger:   { label: 'Ledger',   description: 'Row rules, heavier every 5th',     rows: true,  cols: false, dash: null,   dots: false, groupEvery: 5 },
  graph:    { label: 'Graph',    description: 'Fine grid, heavier every 5th row', rows: true,  cols: true,  dash: null,   dots: false, groupEvery: 5 },
  bands:    { label: 'Bands',    description: 'One rule every 5 rows, nothing else', rows: false, cols: false, dash: null, dots: false, groupEvery: 5 },
  ticks:    { label: 'Ticks',    description: 'Row rules with short column ticks', rows: true,  cols: true,  dash: null,   dots: false, ticks: true },
  bordered: { label: 'Bordered', description: 'Bold high-contrast grid lines',     rows: true,  cols: true,  dash: null,   dots: false, strong: true },
  striped:  { label: 'Striped',  description: 'Alternating even/odd row shading',  rows: true,  cols: false, dash: null,   dots: false, zebra: true },
  dotted:   { label: 'Dotted',   description: 'Fine dotted grid, softer feel',     rows: true,  cols: true,  dash: [1, 3], dots: false },
  dots:     { label: 'Dots',     description: 'Corner dots + soft row shading',    rows: false, cols: false, dash: null,   dots: true,  zebra: true },
  minimal:  { label: 'Minimal',  description: 'Row separators only, no columns',   rows: true,  cols: false, dash: null,   dots: false },
  dashed:   { label: 'Dashed',   description: 'Dashed grid, drafting-table feel',  rows: true,  cols: true,  dash: [5, 4], dots: false },
  columns:  { label: 'Columns',  description: 'Vertical rails only, open rows',    rows: false, cols: true,  dash: null,   dots: false },
}
/** @type {TableStyleId} */
export const DEFAULT_TABLE_STYLE = 'lines'

/**
 * Row spacing for the data grid, as a row height in CSS pixels at 100% zoom.
 * The grid multiplies by the canvas zoom, so these stay proportional.
 * Compact fits roughly a third more rows on screen; relaxed is easier to track
 * across wide tables and kinder at small font sizes.
 * @typedef {'compact' | 'standard' | 'relaxed'} RowSpacingId
 * @type {Record<RowSpacingId, { label: string, height: number }>}
 */
export const ROW_SPACINGS = {
  // 19px is the floor that still clears a 13px glyph's descenders. Below it the
  // text starts touching the rule beneath it, which reads as a rendering fault
  // rather than as density.
  dense: { label: 'Dense', height: 19 },
  compact: { label: 'Compact', height: 22 },
  standard: { label: 'Standard', height: 28 },
  relaxed: { label: 'Relaxed', height: 36 },
  // Headroom for the top of the grid-text-size range: an 18px value needs a row
  // taller than `relaxed` before it stops feeling cramped.
  spacious: { label: 'Spacious', height: 44 },
}
/**
 * How much motion the interface is allowed.
 *
 * `system` follows `prefers-reduced-motion`, which is the right default and what
 * the app did before. The override exists because the OS setting is one switch
 * for every app on the machine: somebody who wants animation in their window
 * manager but not in a tool they stare at all day had no way to say so, and
 * somebody on a locked-down machine could not turn it back on.
 * @typedef {'system' | 'reduced' | 'full'} MotionId
 * @type {Record<MotionId, { label: string, description: string }>}
 */
export const MOTION_MODES = {
  system:  { label: 'System',  description: 'Follow the OS reduced-motion setting' },
  reduced: { label: 'Reduced', description: 'Transitions and animations off' },
  full:    { label: 'Full',    description: 'Always animate, whatever the OS says' },
}
/** @type {MotionId} */
export const DEFAULT_MOTION = 'system'
export const MOTION_IDS = /** @type {MotionId[]} */ (Object.keys(MOTION_MODES))
/** @param {unknown} id @returns {MotionId} */
function normalizeMotion(id) {
  return MOTION_MODES[/** @type {MotionId} */ (id)] ? /** @type {MotionId} */ (id) : DEFAULT_MOTION
}

// NULL rendering deliberately does NOT live here. The "Empty & NULL Markers"
// extension already owns it - DataTable draws ∅ instead of NULL when that
// extension is on (see `c.nullishOn`) - and SettingsDialog already surfaces that
// toggle for discoverability. A second control here would be two switches for
// one behaviour, which is the trap that comment in SettingsDialog calls out.

// Boolean rendering deliberately does NOT live here either. The "Boolean Glyphs"
// extension owns it, and it wins by construction: per-cell formatters run AFTER
// formatCell and replace its output, so a setting here would silently do nothing
// whenever that extension was on. Surfaced in SettingsDialog as a toggle instead,
// exactly like the NULL markers above.

/**
 * Grid text size in px at 100% zoom. The canvas multiplies by the zoom rung, so
 * this is the base, not the rendered size.
 *
 * Bounded rather than free: below 10 the monospace glyphs stop resolving on the
 * pixel grid, and above 18 the fixed row heights in ROW_SPACINGS clip the
 * descenders. Anyone wanting more than this wants the app zoom.
 */
export const GRID_FONT_MIN = 10
export const GRID_FONT_MAX = 18
export const DEFAULT_GRID_FONT_SIZE = 13
/** @param {unknown} n */
function normalizeGridFontSize(n) {
  const v = Math.round(Number(n))
  if (!Number.isFinite(v)) return DEFAULT_GRID_FONT_SIZE
  return Math.min(GRID_FONT_MAX, Math.max(GRID_FONT_MIN, v))
}

/** @type {RowSpacingId} */
export const DEFAULT_ROW_SPACING = 'standard'
export const ROW_SPACING_IDS = /** @type {RowSpacingId[]} */ (Object.keys(ROW_SPACINGS))
/** @param {unknown} id @returns {RowSpacingId} */
export function normalizeRowSpacing(id) {
  return ROW_SPACING_IDS.includes(/** @type {any} */ (id)) ? /** @type {RowSpacingId} */ (id) : DEFAULT_ROW_SPACING
}
/** Row height in px at 100% zoom for a spacing id. @param {unknown} id */
export function rowSpacingHeight(id) {
  return ROW_SPACINGS[normalizeRowSpacing(id)].height
}
/** @returns {TableStyleId} */
export function normalizeTableStyle(/** @type {unknown} */ id) {
  return TABLE_STYLES[/** @type {TableStyleId} */ (id)] ? /** @type {TableStyleId} */ (id) : DEFAULT_TABLE_STYLE
}

/**
 * Colour palettes for JSON - the expanded row, the cell dock and every
 * highlighted value in the app.
 *
 * These were one hardcoded set on `html` with a single light override, so all
 * 26 app themes rendered JSON in the same four colours. `auto` keeps that
 * behaviour (it follows light and dark); the rest are the palettes people
 * already know from their editors, chosen so a theme the app palette clashes
 * with has somewhere to go.
 *
 * @typedef {'auto'|'vivid'|'ocean'|'solarized'|'github'|'monochrome'} JsonThemeId
 */
export const JSON_THEMES = {
  auto:       { label: 'Auto',       description: 'Follows the app theme' },
  vivid:      { label: 'Vivid',      description: 'High-chroma, maximum separation' },
  ocean:      { label: 'Ocean',      description: 'Cool blues and teals' },
  solarized:  { label: 'Solarized',  description: 'The classic low-contrast set' },
  github:     { label: 'GitHub',     description: "GitHub's syntax colours" },
  monochrome: { label: 'Monochrome', description: 'Weight and shade only, no hue' },
}
/** @type {JsonThemeId} */
export const DEFAULT_JSON_THEME = 'auto'
export const JSON_THEME_IDS = /** @type {JsonThemeId[]} */ (Object.keys(JSON_THEMES))

export function normalizeJsonTheme(/** @type {unknown} */ id) {
  return JSON_THEMES[/** @type {JsonThemeId} */ (id)] ? /** @type {JsonThemeId} */ (id) : DEFAULT_JSON_THEME
}

// ── Query & connection defaults ──────────────────────────────────────────────
// Numeric/text knobs surfaced under Settings → Database. `maxQueryHistory` is
// consumed by the query-history store; the connector values (packet/timeouts/
// timezone) are persisted as MySQL connection defaults.
/**
 * Grid cell alignment.
 * 'numbers' is the convention every spreadsheet and DB client uses: digits line
 * up by place value so you can compare magnitudes down a column, while prose
 * stays left where the eye finds the start of each line.
 */
export const TABLE_ALIGN_OPTIONS = /** @type {const} */ ([
  { id: 'left', label: 'Left' },
  { id: 'numbers', label: 'Numbers right' },
  { id: 'right', label: 'Right' },
])
export const TABLE_ALIGN_IDS = TABLE_ALIGN_OPTIONS.map((o) => o.id)
export const DEFAULT_TABLE_ALIGN = 'left'

// Must stay in step with DATA_VIEW_MODES in TableToolbar.svelte - that list is
// what a tab can actually switch to, this one is what you may pick as the
// default. 'erd' was added to the toolbar without being added here, so it was
// the one view you could open but never default to.
export const DATA_VIEW_IDS = /** @type {const} */ (['table', 'json', 'record', 'text', 'chart', 'erd'])
export const DEFAULT_DATA_VIEW = 'table'

// How the grid pages through rows.
//  • offset   - LIMIT/OFFSET; random access (jump to any page) but O(offset) deep.
//  • cursor   - keyset by primary key (opaque cursor); O(1) next/prev, no page jump.
//  • keyset   - same engine as cursor (keyset on the PK); listed separately for clarity.
//  • temporal - keyset on a timestamp column (newest-first); great for logs/events.
// All non-offset modes fall back to offset when their preconditions aren't met
// (no single-column PK, a multi-column sort, or a jump to an arbitrary page).
export const PAGINATION_MODE_IDS = /** @type {const} */ (['offset', 'cursor', 'keyset', 'temporal'])
/** Null placement for quick-query ORDER BY (dialects that support it). */
export const NULL_SORT_IDS = /** @type {const} */ (['unset', 'first', 'last'])
export const DEFAULT_NULL_SORT = 'unset'

/** Selectable font sizes (px) for the AI/agent chat + code blocks. */
export const AGENT_FONT_SIZES = /** @type {const} */ ([12, 13, 14, 15, 16])
export const DEFAULT_AGENT_CHAT_FONT = 16
export const DEFAULT_AGENT_CODE_FONT = 16

/**
 * Chat and code both used to default to the app's 14/13px UI scale, which is the
 * right size for dense chrome and the wrong one for prose you actually read. The
 * defaults below moved to 16px; these are what they used to be, so a stored value
 * that still matches can be recognised as "never chosen" and moved up with them.
 * @see migrateAgentFont
 */
const LEGACY_AGENT_CHAT_FONT = 14
const LEGACY_AGENT_CODE_FONT = 13

/**
 * Existing installs have the old default written into localStorage - not because
 * anyone picked it, but because saving any unrelated setting persists the whole
 * object. Left alone they would keep 14/13 forever and never see the new default.
 * A value that still equals the old default is treated as unset; anything else is
 * a real choice and is left exactly as it is.
 * @param {unknown} stored @param {number} legacy @param {number} next
 */
function migrateAgentFont(stored, legacy, next) {
  if (!AGENT_FONT_SIZES.includes(/** @type {never} */ (stored))) return next
  return stored === legacy ? next : /** @type {number} */ (stored)
}
/** Thinking-indicator visual styles for the agent chat. */
export const THINKING_STYLES = /** @type {const} */ ([
  { id: 'shimmer', label: 'Shimmer' },
  { id: 'pulse', label: 'Pulse' },
  { id: 'static', label: 'Static' },
])
export const THINKING_STYLE_IDS = THINKING_STYLES.map((s) => s.id)
export const DEFAULT_THINKING_STYLE = 'shimmer'
export const DEFAULT_PAGINATION_MODE = 'offset'
export const DEFAULT_MAX_QUERY_HISTORY = 100
export const DEFAULT_CONNECT_TIMEOUT_MS = 60000
export const DEFAULT_SOCKET_TIMEOUT_MS = 600000
export const DEFAULT_MAX_ALLOWED_PACKET = 1073741824
export const DEFAULT_SESSION_TIMEZONE = 'SYSTEM'

/**
 * Coerce a persisted value to an integer within [min, max], falling back to
 * `def` when it isn't a finite number.
 * @param {unknown} value @param {number} def @param {number} [min] @param {number} [max]
 */
function normalizeInt(value, def, min = 0, max = Number.MAX_SAFE_INTEGER) {
  const n = Math.round(Number(value))
  if (!Number.isFinite(n)) return def
  return Math.min(max, Math.max(min, n))
}

/** @type {AppSettings} */
export const DEFAULT_SETTINGS = {
  theme: DEFAULT_THEME_ID,
  zoom: DEFAULT_ZOOM,
  font: DEFAULT_FONT,
  iconStyle: DEFAULT_ICON_STYLE,
  iconSet: DEFAULT_ICON_SET,
  tableStyle: DEFAULT_TABLE_STYLE,
  jsonTheme: DEFAULT_JSON_THEME,
  mcpAutoStart: false,
  launchAtLogin: false,
  autoReconnectOnStartup: true,
  previewDmlBeforeApply: true,
  defaultDataView: DEFAULT_DATA_VIEW,
  paginationMode: DEFAULT_PAGINATION_MODE,
  maxQueryHistory: DEFAULT_MAX_QUERY_HISTORY,
  connectTimeoutMs: DEFAULT_CONNECT_TIMEOUT_MS,
  socketTimeoutMs: DEFAULT_SOCKET_TIMEOUT_MS,
  maxAllowedPacket: DEFAULT_MAX_ALLOWED_PACKET,
  sessionTimezone: DEFAULT_SESSION_TIMEZONE,
  vimMode: false,
  cmdkAiEnabled: false,
  // Soft-wrap in every JSON viewer. Off by default: unwrapped keeps the
  // structure scannable down the left edge, and one embedding value can run to
  // tens of thousands of characters - wrapped, it buries every row around it.
  jsonWordWrap: false,
  // Platform default, not a fixed one - see `defaultNativeScroll()`.
  nativeScroll: defaultNativeScroll(),
  rowSpacing: DEFAULT_ROW_SPACING,
  motion: DEFAULT_MOTION,
  // SQL formatter preferences. Defaults live with the formatter (format-sql.js)
  // so there is one source for what a valid option set is.
  sqlFormat: { ...SQL_FORMAT_DEFAULTS },
  // Independent of the grid-style preset: two of those presets (Striped, Dots)
  // shade alternate rows as part of their look, and this turns the same shading
  // on for any of the others without changing the separators you picked.
  zebraRows: false,
  showRowNumbers: false,
  showMenuBar: true,
  numberGrouping: false,
  imagePreview: true,
  openUrlsOnClick: true,
  highlightActiveRow: true,
  gridFontSize: DEFAULT_GRID_FONT_SIZE,
  // Off by default: every executed statement is already in Query History, and
  // saving each one would bury the handful you deliberately kept. On, a run that
  // succeeded is filed under Saved Queries too, deduplicated by its SQL.
  autoSaveQueries: false,
  // On by default, and stated plainly in Settings. What it sends is a fixed
  // list of event names, the version and the OS - never a query, a table name
  // or anything about a connection. See src/lib/telemetry.js.
  telemetry: true,
  liveModeEnabled: false,
  // On by default. A column averaging half a megabyte a row is fetched as a
  // size, and its value is loaded per cell on demand - the difference between a
  // table opening in a second and in eleven. Off restores the old behaviour:
  // every value on the page, whatever it costs.
  lazyWideColumns: true,
  nullSortOrder: DEFAULT_NULL_SORT,
  agentChatFontSize: DEFAULT_AGENT_CHAT_FONT,
  agentCodeFontSize: DEFAULT_AGENT_CODE_FONT,
  agentThinkingStyle: DEFAULT_THINKING_STYLE,
  // On by default: seeing the SQL the agent ran, and what it returned, is how
  // you tell a right answer from a confident one.
  agentShowQueryCards: true,
  // Off by default. Everything else the agent does stays between the app and
  // your database; searching sends your question to a third party, so it is a
  // decision the user makes rather than one they discover afterwards.
  agentWebAccess: false,
  tableTextAlign: DEFAULT_TABLE_ALIGN,
}

/** Reactive app font id (synced by applySettings). */
export const appFont = writable(/** @type {FontId} */ (DEFAULT_FONT))

/** Reactive app icon style (synced by applySettings). */
export const appIconStyle = writable(/** @type {IconStyleId} */ (DEFAULT_ICON_STYLE))
/** Reactive motion preference. */
export const appMotion = writable(/** @type {MotionId} */ (DEFAULT_MOTION))

/** Reactive app icon set / family (synced by applySettings). */
export const appIconSet = writable(/** @type {IconSetId} */ (DEFAULT_ICON_SET))

/** Reactive app zoom scale (synced by applySettings). Monaco editors subscribe
 *  to this to rescale their font/line-height in lockstep with the rest of the UI. */
export const appZoom = writable(DEFAULT_ZOOM)

/** Reactive app theme id (synced by applySettings). */
export const appThemeId = writable(/** @type {ThemeId} */ (DEFAULT_THEME_ID))

/** Reactive: show a SQL preview/confirm before applying grid writes (synced by applySettings). */
export const appPreviewDml = writable(true)

/** Reactive: experimental app-wide Vim mode enabled (synced by applySettings). */
export const appVimMode = writable(false)

/** Reactive: experimental ⌘K "Ask AI" enabled (off by default; synced by applySettings). */
export const appCmdkAi = writable(false)

/** Reactive JSON soft-wrap preference (synced by applySettings).
 *  Every JSON viewer subscribes, so flipping it in Settings - or from the Wrap
 *  button on any one of them - reflows all of them at once instead of leaving
 *  each open view on whatever it happened to be created with. */
export const appJsonWordWrap = writable(false)

/** Reactive data-grid row spacing (synced by applySettings); DataTable derives its
 *  row height from it and repaints. */
export const appRowSpacing = writable(/** @type {RowSpacingId} */ (DEFAULT_ROW_SPACING))

/** Reactive: shade alternate grid rows regardless of the style preset. */
export const appZebraRows = writable(false)
/** Row-number gutter in the data grid. Off by default: it is a reading aid, not
 *  data, and it costs horizontal space on every table. */
export const appRowNumbers = writable(false)
/** File/Edit/View/Tools/Help in the title bar. On by default; off gives the
 *  window title bar back to the drag region and the tab strip. */
export const appMenuBar = writable(true)
/** Grid value rendering - read by the canvas renderer on every paint. */
export const appNumberGrouping = writable(false)
/** Fetch and draw thumbnails for image-URL cells. Off also stops the FETCH. */
export const appImagePreview = writable(true)
/** Whether a click on a URL cell leaves the app to open it. */
export const appOpenUrlsOnClick = writable(true)
export const appHighlightActiveRow = writable(true)
export const appGridFontSize = writable(DEFAULT_GRID_FONT_SIZE)

/** Reactive: file every successful run under Saved Queries as well as History. */
export const appAutoSaveQueries = writable(false)

/** Reactive: use the OS's native scrolling instead of the app's eased scrolling
 *  (off by default). The grid and the sidebar both subscribe, so flipping it
 *  applies without reopening anything. */
export const appNativeScroll = writable(false)

/** Reactive: experimental Live mode (auto-refresh) status-bar toggle enabled (off by default). */
export const appLiveMode = writable(false)

/** Whether the agent transcript shows a card per executed query. */
export const appAgentQueryCards = writable(true)

/** Whether the agent may search the web and read pages. */
export const appAgentWebAccess = writable(false)

/** Grid cell alignment - the canvas table subscribes and repaints on change. */
export const appTableAlign = writable(/** @type {string} */ (DEFAULT_TABLE_ALIGN))

/** Reactive pagination strategy (offset | cursor | keyset | temporal), synced by applySettings. */
export const appPaginationMode = writable(/** @type {string} */ (DEFAULT_PAGINATION_MODE))

/** Reactive canvas-table grid style preset (synced by applySettings). DataTable
 *  subscribes to repaint when it changes. */
export const appTableStyle = writable(/** @type {TableStyleId} */ (DEFAULT_TABLE_STYLE))
export const appJsonTheme = writable(/** @type {JsonThemeId} */ (DEFAULT_JSON_THEME))

const LAST_DARK_KEY  = 'stroke:last-dark-theme'
const LAST_LIGHT_KEY = 'stroke:last-light-theme'

/** @param {ThemeId} id */
function saveLastForMode(id) {
  // A hidden theme is never remembered. The easter egg is a dark theme, so
  // wearing it once made it the theme ⌘M returned to every time you toggled back
  // to dark - you would have to escape it twice. You can still switch INTO it
  // from Appearance and toggle away from it; it just is not what "dark" means.
  if (getThemeDefinition(id)?.hidden) return
  try {
    if (isDarkTheme(id)) localStorage.setItem(LAST_DARK_KEY, id)
    else                  localStorage.setItem(LAST_LIGHT_KEY, id)
  } catch {}
}

/** @returns {{ dark: ThemeId, light: ThemeId }} */
function loadLastForMode() {
  try {
    const dark  = normalizeThemeId(localStorage.getItem(LAST_DARK_KEY)  ?? 'dark')
    const light = normalizeThemeId(localStorage.getItem(LAST_LIGHT_KEY) ?? 'light')
    return { dark, light }
  } catch {
    return { dark: 'dark', light: 'light' }
  }
}

/** @type {ThemeId[]} */
let themeHistoryStack = []
let restoringTheme = false

/** @param {ThemeId} theme */
function recordThemeBeforeChange(theme) {
  const top = themeHistoryStack[themeHistoryStack.length - 1]
  if (top !== theme) themeHistoryStack.push(theme)
  if (themeHistoryStack.length > 32) themeHistoryStack.shift()
}

// In-memory cache of the normalized settings. All reads go through loadSettings
// and all writes through saveSettings, so this stays authoritative for the app's
// (single) window and lets the zoom/pinch hot path skip a localStorage getItem +
// JSON.parse (× the two loadSettings calls each updateSettings made) per step.
/** @type {AppSettings | null} */
let _settingsCache = null

/**
 * On a fresh install (no saved settings) pick the theme to match the OS
 * appearance: Light Studio for a light system, Dark Studio otherwise. Only ever
 * consulted on the very first load - once the user has any saved settings their
 * chosen theme wins and this is never used again.
 * @returns {ThemeId}
 */
function systemPreferredTheme() {
  try {
    if (
      typeof window !== 'undefined' &&
      typeof window.matchMedia === 'function' &&
      window.matchMedia('(prefers-color-scheme: light)').matches
    ) {
      return 'light'
    }
  } catch {}
  return DEFAULT_THEME_ID
}

/** @returns {AppSettings} */
export function loadSettings() {
  if (_settingsCache) return { ..._settingsCache }
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) {
      markScrollDefaultApplied()
      markFontDefaultApplied()
      markFontClaudeDefaultApplied()
      markFontGeistDefaultApplied()
      _settingsCache = {
        ...DEFAULT_SETTINGS,
        theme: systemPreferredTheme(),
        // 100% is already the comfortable size on every platform (see
        // ROOT_BASE_PX in type-scale.js), so nobody starts off-rung.
        zoom: DEFAULT_ZOOM,
      }
      return { ..._settingsCache }
    }
    const parsed = JSON.parse(raw)
    const theme = normalizeThemeId(parsed.theme)
    let zoom = DEFAULT_ZOOM
    if (parsed.zoom != null) {
      zoom = Number(parsed.zoom)
    } else if (parsed.fontSize != null) {
      const fs = Number(parsed.fontSize)
      if (Number.isFinite(fs) && fs >= 10 && fs <= 24) zoom = fs / 14
    }
    if (!Number.isFinite(zoom)) zoom = DEFAULT_ZOOM
    if (!ZOOM_STEPS.includes(zoom)) {
      zoom = ZOOM_STEPS.reduce((prev, step) =>
        Math.abs(step - zoom) < Math.abs(prev - zoom) ? step : prev,
      )
    }
    const mcpAutoStart = parsed.mcpAutoStart === true
    const launchAtLogin = parsed.launchAtLogin === true
    const autoReconnectOnStartup = parsed.autoReconnectOnStartup !== false
    const previewDmlBeforeApply = parsed.previewDmlBeforeApply !== false
    let font = normalizeFont(parsed.font)
    if (!fontDefaultApplied()) {
      // Only the OLD default is rewritten. Any other value is a choice, and the
      // whole point of the marker is that this cannot run twice - so someone who
      // sets Geist after the update keeps it.
      if (font === 'geist') font = DEFAULT_FONT
      markFontDefaultApplied()
    }
    // Set when the Claude move rewrites the font, so the new value is written
    // straight back. Left only in the cache, the next save from anything still
    // holding the old settings object put `mono` back after the marker was set.
    let fontMigrated = false
    if (!fontClaudeDefaultApplied()) {
      // Same rule for the next default: only the previous one (mono) moves.
      if (font === 'mono') { font = DEFAULT_FONT; fontMigrated = true }
      markFontClaudeDefaultApplied()
    }
    if (!fontGeistDefaultApplied()) {
      // Back to Geist: both earlier defaults move, because an install that
      // skipped the Claude move (marker set, value still mono) is still on a
      // default nobody picked.
      if (font === 'claude' || font === 'mono') { font = DEFAULT_FONT; fontMigrated = true }
      markFontGeistDefaultApplied()
    }
    const iconStyle = normalizeIconStyle(parsed.iconStyle)
    const iconSet = normalizeIconSet(parsed.iconSet)
    const tableStyle = normalizeTableStyle(parsed.tableStyle)
    const jsonTheme = normalizeJsonTheme(parsed.jsonTheme)
    const defaultDataView = DATA_VIEW_IDS.includes(parsed.defaultDataView) ? parsed.defaultDataView : DEFAULT_DATA_VIEW
    const paginationMode = PAGINATION_MODE_IDS.includes(parsed.paginationMode) ? parsed.paginationMode : DEFAULT_PAGINATION_MODE
    const maxQueryHistory = normalizeInt(parsed.maxQueryHistory, DEFAULT_MAX_QUERY_HISTORY, 1, 100000)
    const connectTimeoutMs = normalizeInt(parsed.connectTimeoutMs, DEFAULT_CONNECT_TIMEOUT_MS, 0)
    const socketTimeoutMs = normalizeInt(parsed.socketTimeoutMs, DEFAULT_SOCKET_TIMEOUT_MS, 0)
    const maxAllowedPacket = normalizeInt(parsed.maxAllowedPacket, DEFAULT_MAX_ALLOWED_PACKET, 1024)
    const sessionTimezone =
      typeof parsed.sessionTimezone === 'string' && parsed.sessionTimezone.trim()
        ? parsed.sessionTimezone.trim()
        : DEFAULT_SESSION_TIMEZONE
    const vimMode = parsed.vimMode === true
    // Absent means on: only an explicit false opts out.
    const telemetry = parsed.telemetry !== false
    const cmdkAiEnabled = parsed.cmdkAiEnabled === true
    const jsonWordWrap = parsed.jsonWordWrap === true
    // `saveSettings` writes the whole object, so an existing install has the OLD
    // default (`false`) stored as if it were a choice - there is no way to tell
    // "the user picked eased" from "eased is what shipped". The marker below is
    // what distinguishes them: it is stamped the first time this build resolves
    // the setting, so the platform default is applied exactly once, and anything
    // the user picks after that is a real choice and is left alone.
    let nativeScroll = typeof parsed.nativeScroll === 'boolean' ? parsed.nativeScroll : defaultNativeScroll()
    if (!scrollDefaultApplied()) {
      nativeScroll = defaultNativeScroll()
      markScrollDefaultApplied()
    }
    const rowSpacing = normalizeRowSpacing(parsed.rowSpacing)
    const motion = normalizeMotion(parsed.motion)
    const sqlFormat = normalizeSqlFormat(parsed.sqlFormat)
    const zebraRows = parsed.zebraRows === true
    const showRowNumbers = parsed.showRowNumbers === true
    const showMenuBar = parsed.showMenuBar !== false
    const numberGrouping = parsed.numberGrouping === true
    // Both default ON - this is what the grid already did - so an absent key must
    // read as true, not false.
    const imagePreview = parsed.imagePreview !== false
    const openUrlsOnClick = parsed.openUrlsOnClick !== false
    // Defaults true, so an absent key must not read as false.
    const highlightActiveRow = parsed.highlightActiveRow !== false
    const gridFontSize = normalizeGridFontSize(parsed.gridFontSize)
    const autoSaveQueries = parsed.autoSaveQueries === true
    const liveModeEnabled = parsed.liveModeEnabled === true
    const lazyWideColumns = parsed.lazyWideColumns !== false
    const nullSortOrder = NULL_SORT_IDS.includes(parsed.nullSortOrder) ? parsed.nullSortOrder : DEFAULT_NULL_SORT
    const agentChatFontSize = migrateAgentFont(parsed.agentChatFontSize, LEGACY_AGENT_CHAT_FONT, DEFAULT_AGENT_CHAT_FONT)
    const agentCodeFontSize = migrateAgentFont(parsed.agentCodeFontSize, LEGACY_AGENT_CODE_FONT, DEFAULT_AGENT_CODE_FONT)
    const agentThinkingStyle = THINKING_STYLE_IDS.includes(parsed.agentThinkingStyle) ? parsed.agentThinkingStyle : DEFAULT_THINKING_STYLE
    const agentShowQueryCards = parsed.agentShowQueryCards !== false
    const agentWebAccess = parsed.agentWebAccess === true
    const tableTextAlign = TABLE_ALIGN_IDS.includes(parsed.tableTextAlign) ? parsed.tableTextAlign : DEFAULT_TABLE_ALIGN
    _settingsCache = { theme, zoom, font, iconStyle, iconSet, tableStyle, jsonTheme, mcpAutoStart, launchAtLogin, autoReconnectOnStartup, previewDmlBeforeApply, defaultDataView, paginationMode, maxQueryHistory, connectTimeoutMs, socketTimeoutMs, maxAllowedPacket, sessionTimezone, vimMode, cmdkAiEnabled, liveModeEnabled, lazyWideColumns, nullSortOrder, agentChatFontSize, agentCodeFontSize, agentThinkingStyle, agentShowQueryCards, agentWebAccess, tableTextAlign, telemetry, jsonWordWrap, nativeScroll, rowSpacing, motion, zebraRows, showRowNumbers, showMenuBar, numberGrouping, imagePreview, openUrlsOnClick, highlightActiveRow, gridFontSize, autoSaveQueries, sqlFormat }
    if (fontMigrated) {
      try { localStorage.setItem(STORAGE_KEY, JSON.stringify(_settingsCache)) } catch {}
    }
    return { ..._settingsCache }
  } catch {
    return { ...DEFAULT_SETTINGS }
  }
}

/** @param {AppSettings} settings */
export function saveSettings(settings) {
  // Keep the in-memory cache authoritative even if the localStorage write throws
  // (quota/private-mode) - the running app should still reflect the new settings.
  _settingsCache = { ...settings }
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings))
  } catch (err) {
    console.error('Failed to persist settings:', err)
  }
}

// ── Change-aware appliers ─────────────────────────────────────────────────────
// applySettings runs on EVERY updateSettings call (any toggle in the Settings
// dialog). Unconditional root style/attribute writes fire the canvas table's
// MutationObserver (colour-reader rebuild + font re-measure + full repaint) and
// unconditional store.set() re-notifies every subscriber (Monaco updateOptions
// on all editors) - that was the Settings-page lag. Guard every write so only
// values that actually changed touch the DOM or notify subscribers.
/** @param {HTMLElement} el @param {string} prop @param {string} value */
function setStyleVar(el, prop, value) {
  if (el.style.getPropertyValue(prop) !== value) el.style.setProperty(prop, value)
}
/** @param {HTMLElement} el @param {string} name @param {string} value */
function setAttr(el, name, value) {
  // null removes the attribute: "follow the system" has to be the ABSENCE of an
  // override, not a third value CSS would have to know about.
  if (value === null) {
    if (el.hasAttribute(name)) el.removeAttribute(name)
    return
  }
  if (el.getAttribute(name) !== value) el.setAttribute(name, value)
}
/** @param {import('svelte/store').Writable<any>} store @param {any} value */
function setStore(store, value) {
  if (get(store) !== value) store.set(value)
}

let _lastAppliedZoom = /** @type {number | null} */ (null)

/** Read by index.html before the bundle loads. Keep the key in sync there. */
const BOOT_SURFACE_KEY = 'stroke:boot-surface'

/**
 * Remember this theme's real background so the next launch paints it from the
 * first frame.
 *
 * index.html runs before any stylesheet exists, so it could only paint one of
 * two hard-coded colours (#080808 / #f7f7f7). Any theme whose --background is
 * not exactly those - most of them - came up in the wrong shade and switched
 * when the CSS arrived: the dark-then-different flash on the splash. It also
 * replaces index.html's inline colour now, which would otherwise outlive the
 * theme change because an inline style beats the stylesheet.
 * @param {HTMLElement} root @param {string} theme
 */
function rememberBootSurface(root, theme) {
  try {
    const color = getComputedStyle(root).getPropertyValue('--background').trim()
    if (!color) return
    root.style.backgroundColor = color
    localStorage.setItem(BOOT_SURFACE_KEY, JSON.stringify({ theme, color }))
  } catch { /* no storage: the boot falls back to the base light/dark colour */ }
}

/** @param {AppSettings} settings */
export function applySettings(settings) {
  const root = document.documentElement
  const theme = normalizeThemeId(settings.theme)
  const zoom = settings.zoom
  const dark = isDarkTheme(theme)

  setAttr(root, 'data-theme', theme)
  if (root.classList.contains('dark') !== dark) root.classList.toggle('dark', dark)
  rememberBootSurface(root, theme)
  setMode(dark ? 'dark' : 'light')
  setStore(appThemeId, theme)
  setStore(isCurrentThemeDark, dark)
  const rootPx = rootPxFor(zoom)

  // Every type step is rounded to a whole pixel against that root. Resolving the
  // scale in CSS as `calc(N / 14 * 1rem)` only landed on whole pixels when the
  // root happened to be 14px; anywhere else a 13px caption came out fractional
  // and WebKit rasterised it off the pixel grid, which is what made UI text look
  // soft. The canvas table reads --app-font-size, so it follows automatically.
  const scale = buildTypeScale(rootPx)
  setStyleVar(root, '--app-zoom', String(zoom))
  setStyleVar(root, '--app-font-size', `${rootPx}px`)
  // The ratio the UI actually renders at, which is the rounded root over the
  // base - not `zoom`, which is the rung's nominal label and can sit a few
  // tenths of a pixel away from it. Draggable panel widths are stored as px at
  // 100% and multiplied by this, so a sidebar grows with the text inside it
  // instead of clipping its own labels at the high rungs.
  const appScale = rootPx / ROOT_BASE_PX
  setStyleVar(root, '--app-scale', String(appScale))
  for (const [step, size] of scale) setStyleVar(root, `--fs-${step}`, `${size}px`)
  for (const [step, , leading] of UI_TYPE_SCALE) {
    if (!leading) continue
    const lh = Math.max(1, Math.round((leading * rootPx) / TYPE_SCALE_REF))
    setStyleVar(root, `--lh-${step}`, `${lh}px`)
  }

  // Monaco editors read --editor-font-size / --editor-line-height directly (Monaco
  // takes pixel values, not CSS units, so it can't inherit --app-font-size). Scale
  // them off the same base + zoom so the editor grows in lockstep with the UI.
  // The appZoom subscription in monaco-env.js pushes these to live editor instances.
  setStyleVar(root, '--editor-font-size', `${rootPx}px`)
  setStyleVar(root, '--editor-line-height', `${Math.round(rootPx * 1.5)}px`)
  setStore(appZoom, zoom)

  // Font family - overrides the stylesheet :root defaults inline (inline style
  // wins), so the whole UI + canvas grid pick it up. The canvas re-measures its
  // font metrics via the documentElement style MutationObserver.
  const font = normalizeFont(settings.font)
  setStyleVar(root, '--font-sans', FONT_PRESETS[font].sans)
  setStyleVar(root, '--font-mono', FONT_PRESETS[font].mono)
  setStyleVar(root, '--heading-font', FONT_PRESETS[font].heading ?? FONT_PRESETS[font].sans)
  setStore(appFont, font)

  // AI/agent chat typography - consumed by the chat surfaces (AiMarkdown, code blocks).
  const chatFont = AGENT_FONT_SIZES.includes(settings.agentChatFontSize) ? settings.agentChatFontSize : DEFAULT_AGENT_CHAT_FONT
  const codeFont = AGENT_FONT_SIZES.includes(settings.agentCodeFontSize) ? settings.agentCodeFontSize : DEFAULT_AGENT_CODE_FONT
  setStyleVar(root, '--ai-chat-font-size', `${chatFont}px`)
  setStyleVar(root, '--ai-code-font-size', `${codeFont}px`)
  const thinkStyle = THINKING_STYLE_IDS.includes(settings.agentThinkingStyle) ? settings.agentThinkingStyle : DEFAULT_THINKING_STYLE
  if (root.getAttribute('data-thinking-style') !== thinkStyle) root.setAttribute('data-thinking-style', thinkStyle)

  // Icon weight - a single [data-icon-style] attribute drives the global Lucide
  // stroke-width rule in app.css. No per-icon or per-component changes needed.
  // Motion preference. `system` leaves the attribute off so only the media
  // query in app.css decides; the other two override it in either direction.
  const motion = normalizeMotion(settings.motion)
  setAttr(root, 'data-motion', motion === 'system' ? null : motion)
  setStore(appMotion, motion)

  const iconStyle = normalizeIconStyle(settings.iconStyle)
  setAttr(root, 'data-icon-style', iconStyle)
  setStore(appIconStyle, iconStyle)

  // Icon family - the shared <Icon> wrapper subscribes to appIconSet and swaps
  // between icon families. data-icon-set is exposed for any CSS hooks.
  const iconSet = normalizeIconSet(settings.iconSet)
  setAttr(root, 'data-icon-set', iconSet)
  setStore(appIconSet, iconSet)

  // Grid-write DML preview toggle - DataTable subscribes to gate its confirm dialog.
  setStore(appPreviewDml, settings.previewDmlBeforeApply !== false)
  setStore(appVimMode, settings.vimMode === true)
  setStore(appCmdkAi, settings.cmdkAiEnabled === true)
  setStore(appJsonWordWrap, settings.jsonWordWrap === true)
  setStore(appNativeScroll, settings.nativeScroll === true)
  setStore(appRowSpacing, normalizeRowSpacing(settings.rowSpacing))
  // Push formatter prefs into the shared option holder that format-sql.js reads.
  setSqlFormatOptions(settings.sqlFormat)
  setStore(appZebraRows, settings.zebraRows === true)
  setStore(appRowNumbers, settings.showRowNumbers === true)
  setStore(appMenuBar, settings.showMenuBar !== false)
  setStore(appNumberGrouping, settings.numberGrouping === true)
  setStore(appImagePreview, settings.imagePreview !== false)
  setStore(appOpenUrlsOnClick, settings.openUrlsOnClick !== false)
  setStore(appHighlightActiveRow, settings.highlightActiveRow !== false)
  setStore(appGridFontSize, normalizeGridFontSize(settings.gridFontSize))
  setStore(appAutoSaveQueries, settings.autoSaveQueries === true)
  setStore(appLiveMode, settings.liveModeEnabled === true)
  setStore(appAgentQueryCards, settings.agentShowQueryCards !== false)
  setStore(appAgentWebAccess, settings.agentWebAccess === true)
  setStore(appTableAlign, TABLE_ALIGN_IDS.includes(settings.tableTextAlign) ? settings.tableTextAlign : DEFAULT_TABLE_ALIGN)
  setStore(appPaginationMode, PAGINATION_MODE_IDS.includes(settings.paginationMode) ? settings.paginationMode : DEFAULT_PAGINATION_MODE)

  // Canvas table grid style - data attribute for any CSS hooks; DataTable reads
  // the store and repaints the virtualized grid pass.
  const tableStyle = normalizeTableStyle(settings.tableStyle)
  setAttr(root, 'data-table-style', tableStyle)
  setStore(appTableStyle, tableStyle)

  // JSON colours. A data attribute only - every consumer reads the
  // --json-* custom properties, so the palette swaps with no component
  // re-rendering anything.
  const jsonTheme = normalizeJsonTheme(settings.jsonTheme)
  setAttr(root, 'data-json-theme', jsonTheme)
  setStore(appJsonTheme, jsonTheme)

  // Keep the canvas-table zoom in lockstep with the app zoom so Cmd +/-/0 (and
  // the zoom buttons) scale the grid alongside the rest of the UI. The canvas
  // renderer reads zoomState directly and repaints on change.
  // Drop the legacy per-table key - it drifted from settings.zoom and made only
  // the grid look huge/blurry while the sidebar stayed at normal scale.
  try { localStorage.removeItem('stroke:canvas-zoom') } catch {}
  // `appScale`, NOT `zoom`. The canvas takes its FONTS from a DOM probe, which
  // resolves against --app-font-size, i.e. the rounded root over the base. Its
  // GEOMETRY (row height, padding, icons) scales by this number. Feeding it the
  // rung's nominal label instead made the two disagree wherever rounding moved
  // the root off the label: at the 90% rung the root is 14px, so text rendered
  // at 87.5% inside rows that shrank to 90%, and at 110% text grew 12.5% inside
  // rows that grew 10% - text gaining on its row in BOTH directions, which is
  // what made zooming look asymmetric.
  const canvasZoom = Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, appScale))
  if (zoomState.value !== canvasZoom) zoomState.value = canvasZoom
  // Webview page-zoom reset is an IPC round-trip - only needed when zoom changed
  // (or on the first apply, to undo any stale native zoom from a prior session).
  if (_lastAppliedZoom !== zoom) {
    _lastAppliedZoom = zoom
    resetWebviewZoom()
  }
}

/** Snap native webview page-zoom back to 1 (macOS pinch / Tauri polyfill leak). */
export function resetWebviewZoom() {
  void import('@tauri-apps/api/webview')
    .then(({ getCurrentWebview }) => getCurrentWebview().setZoom(1))
    .catch(() => {})
}

let zoomListenerInstalled = false

/** @param {KeyboardEvent} e */
function handleZoomKeydown(e) {
  if (!e.ctrlKey && !e.metaKey) return
  if (e.altKey) return

  const { key, code } = e

  if (key === '0' || code === 'Digit0' || code === 'Numpad0') {
    e.preventDefault()
    e.stopPropagation()
    resetZoom()
    return
  }

  if (
    key === '=' ||
    key === '+' ||
    code === 'Equal' ||
    code === 'NumpadAdd' ||
    (e.shiftKey && code === 'Equal')
  ) {
    e.preventDefault()
    e.stopPropagation()
    increaseZoom()
    return
  }

  if (key === '-' || key === '_' || code === 'Minus' || code === 'NumpadSubtract') {
    e.preventDefault()
    e.stopPropagation()
    decreaseZoom()
    return
  }

}

/**
 * Surfaces that do their own zooming.
 *
 * The blockers below run in the CAPTURE phase and call
 * `stopImmediatePropagation()`, so anything inside the app that wants to zoom
 * its own content never sees the event at all - it cannot opt out from its own
 * handler, because its handler does not run. `.mermaid-canvas` was named here
 * for exactly that reason; `[data-zoom-surface]` is the same escape hatch
 * without a component's class name in a store (the media lightbox needs it for
 * Ctrl+scroll and for trackpad pinch, which is what made zooming an image
 * preview do nothing at all).
 * @param {Event} e
 */
function ownsItsZoom(e) {
  const t = /** @type {Element | null} */ (e.target)
  return !!t?.closest?.('[data-zoom-surface], .mermaid-canvas')
}

/**
 * Block every Ctrl/Cmd + scroll zoom path. Zoom is keyboard-only (Cmd +/-/0).
 * macOS trackpad pinch arrives as ctrl+wheel near column resize handles and
 * page-zooms the webview (devicePixelRatio drift → canvas looks huge/blurry
 * while the sidebar, scaled via --app-font-size, stays normal).
 * @param {Event} e
 */
function blockNativeScrollZoom(e) {
  const we = /** @type {WheelEvent} */ (e)
  if (!(we.ctrlKey || we.metaKey)) return
  if (ownsItsZoom(e)) return
  e.preventDefault()
  e.stopImmediatePropagation()
  resetWebviewZoom()
}

/**
 * Block macOS WebKit (WKWebView) native trackpad pinch-magnification.
 * @param {Event} e
 */
function handleZoomGesture(e) {
  if (ownsItsZoom(e)) return
  e.preventDefault()
  e.stopImmediatePropagation()
  resetWebviewZoom()
}

export function installZoomShortcuts() {
  if (zoomListenerInstalled || typeof window === 'undefined') return
  zoomListenerInstalled = true
  window.addEventListener('keydown', handleZoomKeydown, true)
  window.addEventListener('wheel', blockNativeScrollZoom, { capture: true, passive: false })
  // Legacy event - Tauri's zoom polyfill listens on this, not `wheel`.
  window.addEventListener('mousewheel', blockNativeScrollZoom, { capture: true, passive: false })
  // WebKit-only pinch magnification (macOS). No-op on Chromium.
  window.addEventListener('gesturestart', handleZoomGesture, { capture: true, passive: false })
  window.addEventListener('gesturechange', handleZoomGesture, { capture: true, passive: false })
  window.addEventListener('gestureend', handleZoomGesture, { capture: true, passive: false })
}

/** @param {Partial<AppSettings>} patch */
export function updateSettings(patch) {
  const current = loadSettings()
  const next = { ...current, ...patch }

  if (!restoringTheme && patch.theme != null && patch.theme !== current.theme) {
    recordThemeBeforeChange(current.theme)
    saveLastForMode(patch.theme)
  }

  saveSettings(next)
  applySettings(next)
  return next
}

export function increaseZoom() {
  const current = loadSettings()
  const idx = ZOOM_STEPS.indexOf(current.zoom)
  if (idx < ZOOM_STEPS.length - 1) {
    return updateSettings({ zoom: ZOOM_STEPS[idx + 1] })
  }
  return current
}

export function decreaseZoom() {
  const current = loadSettings()
  const idx = ZOOM_STEPS.indexOf(current.zoom)
  if (idx > 0) {
    return updateSettings({ zoom: ZOOM_STEPS[idx - 1] })
  }
  return current
}

export function resetZoom() {
  // Also clear any webview zoom. That is a second, independent scale (WKWebView
  // pinch magnification, or Tauri's page-zoom polyfill) which the app never sets
  // on purpose but can drift into. Resetting only the app zoom left the window
  // still magnified with no way back, and Cmd+0 is the one gesture that has to
  // always mean "put it back".
  resetWebviewZoom()
  return updateSettings({ zoom: DEFAULT_ZOOM })
}

/** Cycle only through themes of the same darkness as the current theme. */
export function cycleTheme() {
  const current = loadSettings()
  const dark = isDarkTheme(current.theme)
  // `CYCLE_THEME_IDS`, not every theme: the hidden one is an easter egg, and
  // cycling used to deal it out like any other. Standing on it still works -
  // `indexOf` returns -1 and the next step lands on the first real theme, which
  // is the way out.
  const sameMode = CYCLE_THEME_IDS.filter(id => isDarkTheme(id) === dark)
  if (!sameMode.length) return current
  const idx = sameMode.indexOf(current.theme)
  const next = sameMode[(idx + 1) % sameMode.length]
  return updateSettings({ theme: next })
}

/** Toggle between the user's last-used dark theme and last-used light theme. */
export function toggleLightDark() {
  const current = loadSettings()
  // Record the theme being *left* before reading the other mode's memory.
  // `updateSettings` only ever records the theme being set, so a theme that was
  // chosen in an earlier session - or before this per-mode memory existed, or on
  // a first launch where the default came from the OS appearance - was never
  // written down. Toggling away from it then fell back to plain Dark/Light
  // Studio and the choice was lost on the way back. Recording here makes the
  // round-trip lossless: whatever you are looking at is what you return to.
  saveLastForMode(current.theme)
  const { dark, light } = loadLastForMode()
  const target = isDarkTheme(current.theme) ? light : dark
  return updateSettings({ theme: target })
}

/** Whether the current active theme is dark (reactive). */
export const isCurrentThemeDark = writable(isDarkTheme(DEFAULT_THEME_ID))

/** Revert to the theme used before the most recent change (⌘/Ctrl+Shift+M). */
export function restorePreviousTheme() {
  const current = loadSettings()
  const prev = themeHistoryStack.pop()
  if (!prev || prev === current.theme) return current

  restoringTheme = true
  try {
    return updateSettings({ theme: prev })
  } finally {
    restoringTheme = false
  }
}

export function canIncreaseZoom(zoom) {
  return ZOOM_STEPS.indexOf(zoom) < ZOOM_STEPS.length - 1
}

export function canDecreaseZoom(zoom) {
  return ZOOM_STEPS.indexOf(zoom) > 0
}
