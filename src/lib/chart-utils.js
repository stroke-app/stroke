/**
 * Utilities for converting SQL result sets (columns + rows) into ECharts option objects.
 * All functions return pure JSON - no callbacks - so AI can generate them too.
 *
 * NOTE: echarts-wordcloud is registered lazily at the chart init sites (EChartPanel,
 * AiChartRenderer) so that importing this pure-JSON util module does NOT drag echarts
 * (and the wordcloud plugin) into the startup bundle. See loadEcharts() in those files.
 */

import { resolveColor } from '$lib/geo-basemap.js'

/** @typedef {{ name: string, dataType?: string, data_type?: string }} ColInfo */
/** @typedef {unknown[][]} Rows */

/** @param {ColInfo} col */
export function colType(col) {
  const dt = (col.dataType ?? col.data_type ?? '').toLowerCase().replace(/\(.+\)$/, '').trim()
  if (/^(int|integer|bigint|smallint|numeric|decimal|real|double|float|serial|money|number)/.test(dt)) return 'number'
  if (/^(date|timestamp|timestamptz|timetz|time)/.test(dt)) return 'date'
  if (dt === 'boolean' || dt === 'bool') return 'boolean'
  return 'string'
}

/** @param {ColInfo[]} columns */
export function guessXCol(columns) {
  return (
    columns.find((c) => colType(c) === 'date') ??
    columns.find((c) => colType(c) === 'string') ??
    columns[0]
  )?.name ?? ''
}

/** @param {ColInfo[]} columns @param {string} xCol */
export function guessYCol(columns, xCol) {
  const isIdish = (/** @type {ColInfo} */ c) =>
    /^(id|_id|rowid|oid|pk|key|uuid)$/i.test(c.name) ||
    /_(id|key|pk)$/i.test(c.name)
  return (
    columns.find(c => c.name !== xCol && colType(c) === 'number' && !isIdish(c)) ??
    columns.find(c => c.name !== xCol && colType(c) === 'number') ??
    columns.find(c => c.name !== xCol)
  )?.name ?? ''
}

/**
 * @param {string} chartType
 * @returns {{ x: string, y: string, z?: string, group?: string }}
 */
export function getRequiredAxes(chartType) {
  switch (chartType) {
    case 'scatter':
      return { x: 'X (number)', y: 'Y (number)' }
    case 'bubble':
      return { x: 'X (number)', y: 'Y (number)', z: 'Size (number)' }
    case 'heatmap':
      return { x: 'X category', y: 'Value', group: 'Y category' }
    case 'pie':
    case 'donut':
    case 'funnel':
      return { x: 'Label', y: 'Value' }
    case 'gauge':
      return { x: 'Label', y: 'Value (0-100)' }
    case 'radar':
      return { x: 'Category', y: 'Value' }
    case 'histogram':
      return { x: 'Value (numeric)', y: '' }
    case 'box-plot':
      return { x: 'Group', y: 'Value (numeric)' }
    case 'word-cloud':
      return { x: 'Word', y: 'Size/Count' }
    case 'tree':
    case 'dendrogram':
      return { x: 'Name', y: '', group: 'Parent' }
    case 'choropleth':
      return { x: 'Country / Region', y: 'Value' }
    case 'meter':
      return { x: 'Segment label', y: 'Value', z: 'Total (optional)' }
    case 'sankey':
      return { x: 'Source', y: 'Value', group: 'Target' }
    case 'treemap':
    case 'circle-pack':
      return { x: 'Name', y: 'Value', group: 'Parent (optional)' }
    case 'bullet':
      return { x: 'Category', y: 'Actual', z: 'Target' }
    case 'lollipop-h':
      return { x: 'Category', y: 'Value' }
    case 'combo':
      return { x: 'Category', y: 'Bar series', z: 'Line series' }
    case 'bar-floating':
      return { x: 'Category', y: 'Min', z: 'Max' }
    case 'bar-grouped':
    case 'bar-stacked':
    case 'bar-stacked-100':
    case 'area-stacked':
      return { x: 'Category', y: 'Value', group: 'Series' }
    case 'waterfall': return { x: 'Category', y: 'Value (delta)' }
    case 'pareto':    return { x: 'Category', y: 'Value' }
    case 'step':      return { x: 'Category', y: 'Value', group: 'Group (optional)' }
    case 'sunburst':  return { x: 'Name', y: 'Value', group: 'Parent (optional)' }
    default:
      return { x: 'Category', y: 'Value', group: 'Group (optional)' }
  }
}

// Default categorical palette. The lead color is used for single-series charts;
// buildOption replaces it with the live theme accent (see the `accent` param) so
// charts follow whatever theme/primary color the app is set to.
const DEFAULT_PALETTE = [
  '#6366f1', '#22c55e', '#f59e0b', '#ef4444', '#3b82f6',
  '#a855f7', '#14b8a6', '#f97316', '#ec4899', '#64748b',
]

/** Resolve any CSS custom property that holds a colour into a concrete string
 * echarts can both paint and parse, so charts follow the active theme instead of
 * hardcoding hexes. Returns '' when the token can't resolve to a real colour
 * (unset, or unparseable in this engine - in which case the probe inherits the
 * foreground and would paint bars white); callers then fall back to a literal.
 * @param {string} varName e.g. '--primary', '--success' */
export function resolveCssColor(varName) {
  if (typeof document === 'undefined') return ''
  try {
    const probe = document.createElement('span')
    probe.style.cssText = `position:absolute;visibility:hidden;color:var(${varName})`
    document.body.appendChild(probe)
    const c = getComputedStyle(probe).color
    probe.style.color = 'var(--stroke-nonexistent-token)'
    const fg = getComputedStyle(probe).color
    probe.remove()
    return (!c || c === fg) ? '' : toParseableColor(c)
  } catch { return '' }
}

/** The app's --primary token as a concrete colour, for single-series charts
 * across the app (table view, SQL editor, AI charts, previews). '' when unset -
 * callers fall back to DEFAULT_PALETTE. */
export function resolveChartAccent() {
  return resolveCssColor('--primary')
}

/** Colours echarts can parse itself, not just paint: hex / rgb() / hsl(). */
const ZR_PARSEABLE = /^(#|rgba?\(|hsla?\()/i

/**
 * Re-serialize a CSS colour into a notation echarts (zrender) can PARSE.
 * Theme tokens are authored in `oklch()`, and `getComputedStyle().color` hands
 * that notation straight back. Canvas paints it fine, so bars look right - but
 * zrender's own colour parser only understands hex/rgb/hsl, and its liftColor()
 * (which derives the emphasis/hover fill) returns `undefined` for anything else.
 * The hovered bar then paints with NO fill and vanishes under the axis-pointer
 * shadow. Normalising here keeps every derived state (hover, blur, gradients)
 * working across themes.
 */
function toParseableColor(color) {
  if (!color || ZR_PARSEABLE.test(color.trim())) return color
  try {
    const ctx = document.createElement('canvas').getContext('2d')
    if (!ctx) return ''
    ctx.fillStyle = '#010203' // sentinel: an unparseable value leaves it untouched
    ctx.fillStyle = color
    if (ctx.fillStyle === '#010203') return ''
    if (ZR_PARSEABLE.test(String(ctx.fillStyle))) return String(ctx.fillStyle)
    // Engine echoed a modern notation back (oklch/color()): rasterize one pixel
    // and read the sRGB bytes - the one conversion every engine agrees on.
    ctx.canvas.width = ctx.canvas.height = 1
    ctx.fillStyle = color
    ctx.fillRect(0, 0, 1, 1)
    const [r, g, b, a] = ctx.getImageData(0, 0, 1, 1).data
    return a === 255 ? `rgb(${r}, ${g}, ${b})` : `rgba(${r}, ${g}, ${b}, ${(a / 255).toFixed(3)})`
  } catch { return '' }
}

/**
 * Format a raw value for tooltip display.
 * Timestamps from PostgreSQL arrive as strings like "2025-12-01 00:00:00 UTC"
 * or ISO "2026-01-01T00:00:00.000Z" - convert to "Dec 2025" / "Jan 2026".
 */
function fmtTooltipValue(val) {
  if (Array.isArray(val)) return fmtTooltipValue(val[val.length - 1])
  if (typeof val === 'string' && /^\d{4}-\d{2}-\d{2}/.test(val)) {
    try {
      const d = new Date(val.replace(' UTC', 'Z').replace(' ', 'T'))
      if (!isNaN(d.getTime())) {
        return d.toLocaleDateString('en-US', { month: 'short', year: 'numeric' })
      }
    } catch {}
  }
  if (typeof val === 'number' && !isNaN(val)) {
    if (Math.abs(val) >= 1e6) return val.toLocaleString()
    if (!Number.isInteger(val)) return val.toFixed(2)
    return val.toLocaleString()
  }
  return String(val ?? '')
}

/**
 * Performance options injected into the root ECharts option.
 * ECharts auto-disables animation via animationThreshold, but we explicitly
 * turn it off past 2k rows to avoid the threshold check overhead too.
 * @param {number} n
 */
function animOpts(n) {
  if (n >= 2000) return { animation: false }
  return { animation: true, animationDuration: 400, animationThreshold: 2000 }
}

/** Hover band behind the focused category. The zrender default (30% mid-grey)
 * paints a heavy slab across the whole plot - at that weight the highlight
 * competes with the bars it is meant to point at. A whisper of the foreground
 * reads as "this column" without repainting the chart. */
function shadowPointer(isDark) {
  return { type: 'shadow', shadowStyle: { color: isDark ? 'rgba(255,255,255,0.045)' : 'rgba(0,0,0,0.035)' } }
}

/** @param {boolean} isDark @param {boolean} [noTitle] */
/**
 * Chart text and rules, taken from the theme actually in use.
 *
 * These used to be `isDark ? 'rgba(255,255,255,…)' : 'rgba(0,0,0,…)'`, which
 * knows only two themes. The app ships many - each with its own foreground,
 * muted foreground and border - so on any of them the chart text was a generic
 * white or black wash that matched nothing around it. Reading the real tokens
 * is what makes a chart look like it belongs to the theme it is sitting in.
 *
 * The alpha values are kept: axis labels should be quieter than body text, and
 * grid lines quieter still. What changes is the hue they are quiet *in*.
 *
 * Falls back to the old constants when there is no DOM to read (tests, SSR).
 * @param {boolean} isDark
 */
function themeInk(isDark) {
  const fallback = {
    text: isDark ? 'rgba(255,255,255,0.50)' : 'rgba(0,0,0,0.42)',
    axis: isDark ? 'rgba(255,255,255,0.55)' : 'rgba(0,0,0,0.45)',
    line: isDark ? 'rgba(255,255,255,0.07)' : 'rgba(0,0,0,0.06)',
    strong: isDark ? 'rgba(255,255,255,0.85)' : 'rgba(0,0,0,0.78)',
    bg: isDark ? 'rgba(20,20,23,0.97)' : 'rgba(255,255,255,0.97)',
  }
  if (typeof document === 'undefined' || typeof getComputedStyle !== 'function') return fallback
  try {
    const cs = getComputedStyle(document.documentElement)
    const rgb = (/** @type {string} */ name) => resolveColor(cs.getPropertyValue(name).trim())
    const muted = rgb('--muted-foreground')
    const fg = rgb('--foreground')
    const bg = rgb('--popover') || rgb('--background')
    if (!muted || !fg) return fallback
    return {
      text: `rgba(${muted},0.85)`,
      axis: `rgba(${muted},0.95)`,
      line: `rgba(${muted},0.15)`,
      strong: `rgba(${fg},0.92)`,
      bg: `rgba(${bg},0.97)`,
    }
  } catch {
    return fallback
  }
}

/**
 * The app's own UI font for chart text. Charts hard-coded Inter while the app
 * is set in Geist, so every axis read as a different product. Read once from
 * the theme (it follows a font change in Settings on the next render).
 */
function chartFont() {
  try {
    const f = getComputedStyle(document.documentElement).getPropertyValue('--font-sans').trim()
    if (f) return f
  } catch {}
  return '"Geist Variable", ui-sans-serif, system-ui, sans-serif'
}

function baseOption(isDark, noTitle = false) {
  const textColor = isDark ? 'rgba(255,255,255,0.50)' : 'rgba(0,0,0,0.42)'
  const lineColor = isDark ? 'rgba(255,255,255,0.07)' : 'rgba(0,0,0,0.06)'
  const bg = isDark ? 'rgba(20,20,23,0.97)' : 'rgba(255,255,255,0.97)'
  return {
    backgroundColor: 'transparent',
    textStyle: { color: textColor, fontFamily: chartFont(), fontSize: 11 },
    tooltip: {
      backgroundColor: bg,
      borderColor: isDark ? 'rgba(255,255,255,0.08)' : 'rgba(0,0,0,0.08)',
      borderWidth: 1,
      padding: [8, 12],
      textStyle: { color: themeInk(isDark).strong, fontSize: 12, fontFamily: chartFont() },
      extraCssText: 'border-radius:10px;box-shadow:0 8px 24px rgba(0,0,0,0.20);',
      formatter(params) {
        const items = Array.isArray(params) ? params : [params]
        const header = fmtTooltipValue(items[0]?.axisValue ?? items[0]?.name ?? '')
        const multi = items.length > 1
        const rows = items.map(p => {
          const color = p.color ?? '#6366f1'
          const val = fmtTooltipValue(p.value ?? p.data)
          return `<div style="display:flex;align-items:center;gap:6px;margin-top:3px">
            <span style="display:inline-block;width:8px;height:8px;border-radius:50%;background:${color};flex-shrink:0"></span>
            ${multi ? `<span style="opacity:0.65;font-size:11px">${p.seriesName ?? ''}</span>` : ''}
            <span style="margin-left:auto;padding-left:12px;font-weight:600">${val}</span>
          </div>`
        }).join('')
        return `<div style="font-size:11px;opacity:0.65;margin-bottom:2px">${header}</div>${rows}`
      },
    },
    grid: { top: noTitle ? 12 : 12, right: 16, bottom: 36, left: 12, containLabel: true },
    axisPointer: { lineStyle: { color: lineColor } },
    splitLine: { lineStyle: { color: lineColor } },
  }
}

/** @param {boolean} isDark */
function axisStyle(isDark) {
  const textColor = isDark ? 'rgba(255,255,255,0.55)' : 'rgba(0,0,0,0.5)'
  const lineColor = isDark ? 'rgba(255,255,255,0.07)' : 'rgba(0,0,0,0.07)'
  // Quiet chrome: dashed hairline gridlines, no tick marks, a faint baseline.
  // The data is the subject; the frame should only be found when looked for.
  return {
    axisLabel: { color: textColor, fontSize: 11, margin: 10 },
    axisLine: { lineStyle: { color: isDark ? 'rgba(255,255,255,0.14)' : 'rgba(0,0,0,0.14)' } },
    axisTick: { show: false },
    splitLine: { lineStyle: { color: lineColor, type: [3, 4] } },
  }
}

/** Value labels at the end of each bar, while there are few enough to read. */
function barValueLabel(isDark, position, count) {
  return {
    show: count <= 40,
    position,
    distance: 6,
    color: isDark ? 'rgba(255,255,255,0.72)' : 'rgba(0,0,0,0.66)',
    fontSize: 11,
    fontWeight: 500,
    formatter: (/** @type {any} */ p) => fmtCompact(typeof p.value === 'number' ? p.value : Number(p.value)),
  }
}

/** Hovering one bar dims the rest a little, so the hovered value stands out. */
const BAR_EMPHASIS = { focus: 'self', itemStyle: { opacity: 1 } }
const BAR_BLUR = { itemStyle: { opacity: 0.45 } }

/** Build the unique x-axis category list, sorted chronologically for date columns */
function sortedXData(rows, xi) {
  const vals = [...new Set(rows.map(r => String(r[xi] ?? '')))]
  if (isTimestampAxis(vals)) {
    vals.sort((a, b) => {
      const da = new Date(a.replace(' UTC', 'Z').replace(' ', 'T')).getTime()
      const db = new Date(b.replace(' UTC', 'Z').replace(' ', 'T')).getTime()
      return (isNaN(da) || isNaN(db)) ? 0 : da - db
    })
  }
  return vals
}

/** Aggregate rows by x-key, summing y-values. Replaces the last-wins Object.fromEntries pattern. */
function aggDataMap(rows, xi, yi) {
  /** @type {Record<string, number>} */
  const map = {}
  rows.forEach(r => {
    const key = String(r[xi] ?? '')
    map[key] = (map[key] ?? 0) + (Number(r[yi]) || 0)
  })
  return map
}

/** Aggregate rows into {name,value} pairs by x-key (summing y), sorted by value
 * descending. Used by part-to-whole charts (pie/donut/funnel) so duplicate
 * categories collapse into one slice instead of rendering repeated slices. */
function aggPairs(rows, xi, yi) {
  return Object.entries(aggDataMap(rows, xi, yi))
    .map(([name, value]) => ({ name, value }))
    .sort((a, b) => b.value - a.value)
}

/** Max/min over an array WITHOUT spreading - `Math.max(...arr)` throws
 * `RangeError: Maximum call stack size exceeded` once the array passes ~100k
 * elements, which is exactly the large-result case we must survive. */
function arrMax(arr, seed = -Infinity) {
  let m = seed
  for (let i = 0; i < arr.length; i++) { const v = arr[i]; if (v > m) m = v }
  return m
}
function arrMin(arr, seed = Infinity) {
  let m = seed
  for (let i = 0; i < arr.length; i++) { const v = arr[i]; if (v < m) m = v }
  return m
}

/** Build a strict, acyclic hierarchy (array with a single root) for tree /
 * dendrogram from a name column (xi) and parent column (gi). The old inline
 * build let one node attach to many parents (shared object refs) and had no
 * cycle guard, so A→B / B→A crashed ECharts. Here each node is deduped by name,
 * attached to at most one parent, self/cycle edges are skipped, and the node
 * count is capped. */
function buildTreeData(rows, xi, gi) {
  const MAX_NODES = 2000
  /** @type {Map<string, { name: string, children: any[] }>} */
  const nodeMap = new Map()
  for (const r of rows) {
    const name = String(r[xi] ?? '')
    if (!name || nodeMap.has(name)) continue
    nodeMap.set(name, { name, children: [] })
    if (nodeMap.size >= MAX_NODES) break
  }
  const parentOf = new Map()
  const attached = new Set()
  if (gi >= 0) {
    for (const r of rows) {
      const name = String(r[xi] ?? '')
      const parent = String(r[gi] ?? '')
      if (!name || !parent || parent === name || attached.has(name)) continue
      if (!nodeMap.has(name) || !nodeMap.has(parent)) continue
      // Cycle guard: parent must not already be a descendant of `name`.
      let cur = parent, cyclic = false
      while (cur !== undefined) { if (cur === name) { cyclic = true; break } cur = parentOf.get(cur) }
      if (cyclic) continue
      nodeMap.get(parent).children.push(nodeMap.get(name))
      parentOf.set(name, parent)
      attached.add(name)
    }
  }
  const roots = [...nodeMap.values()].filter((n) => !attached.has(n.name))
  if (roots.length === 0) return [{ name: 'Root', children: [...nodeMap.values()] }]
  if (roots.length === 1) return roots
  return [{ name: 'Root', children: roots }]
}

/** Detect if an array of strings looks like timestamps */
/** Exported so chart previews in ChartsPage can re-apply the formatter after JSON round-trip */
export function isTimestampAxis(xData) {
  const sample = xData.find(v => v && v !== 'null')
  return typeof sample === 'string' && /^\d{4}-\d{2}-\d{2}/.test(sample)
}

/** Format a timestamp x-axis label to a short readable form */
export function fmtAxisLabel(val) {
  if (typeof val !== 'string' || !/^\d{4}-\d{2}-\d{2}/.test(val)) return val
  try {
    const d = new Date(val.replace(' UTC', 'Z').replace(' ', 'T'))
    if (isNaN(d.getTime())) return val
    // Day-level: show "Jan 15", Month-level: show "Jan 2026"
    const hasTime = /\d{2}:\d{2}/.test(val)
    const isMonthStart = d.getDate() === 1 && (!hasTime || val.includes('00:00:00'))
    if (isMonthStart) return d.toLocaleDateString('en-US', { month: 'short', year: 'numeric' })
    return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' })
  } catch { return val }
}

/** @param {boolean} isDark @param {string[]} xData */
function categoryXAxis(isDark, xData) {
  const isTs = isTimestampAxis(xData)
  return {
    type: 'category',
    data: xData,
    ...axisStyle(isDark),
    axisLabel: {
      ...axisStyle(isDark).axisLabel,
      rotate: !isTs && xData.length > 12 ? 30 : 0,
      overflow: isTs ? 'none' : 'truncate',
      ellipsis: '…',
      width: isTs ? undefined : 80,
      ...(isTs ? { formatter: fmtAxisLabel } : {}),
    },
  }
}

/** Compact axis-tick number: 1_200_000 → "1.2M", 3500 → "3.5k". Keeps long value
 * axes (counts, sizes) readable instead of printing raw 7-digit numbers. */
function fmtCompact(v) {
  if (typeof v !== 'number' || !isFinite(v)) return String(v)
  const a = Math.abs(v)
  if (a >= 1e9) return (v / 1e9).toFixed(a % 1e9 === 0 ? 0 : 1) + 'B'
  if (a >= 1e6) return (v / 1e6).toFixed(a % 1e6 === 0 ? 0 : 1) + 'M'
  if (a >= 1e3) return (v / 1e3).toFixed(a % 1e3 === 0 ? 0 : 1) + 'k'
  return String(v)
}

function valueYAxis(isDark) {
  const s = axisStyle(isDark)
  return { type: 'value', ...s, axisLabel: { ...s.axisLabel, formatter: fmtCompact } }
}

/**
 * Build an ECharts option for any supported chart type.
 * @param {{
 *   type: string
 *   columns: ColInfo[]
 *   rows: Rows
 *   xCol: string
 *   yCol: string
 *   zCol?: string
 *   groupCol?: string
 *   isDark?: boolean
 *   title?: string
 *   noTitle?: boolean
 * }} cfg
 * @returns {import('echarts').EChartsOption}
 */
export function buildOption({ type, columns, rows, xCol, yCol, zCol, groupCol, isDark = false, title, noTitle = false, accent = '' }) {
  // Theme-aware palette: lead with the live accent (resolved from the app's
  // --primary token by the caller) so single-series charts and accents follow
  // the current theme; the rest of the categorical ramp stays fixed. Shadows the
  // module DEFAULT_PALETTE so every PALETTE[...] reference below is themed.
  const accentColor = accent || resolveChartAccent()
  const PALETTE = accentColor ? [accentColor, ...DEFAULT_PALETTE.slice(1)] : DEFAULT_PALETTE
  const n = rows.length
  const base = { ...baseOption(isDark, noTitle), ...animOpts(n) }
  const xi = columns.findIndex((c) => c.name === xCol)
  const yi = columns.findIndex((c) => c.name === yCol)
  const zi = zCol ? columns.findIndex((c) => c.name === zCol) : -1
  const gi = groupCol ? columns.findIndex((c) => c.name === groupCol) : -1

  /** @param {string} text */
  const titleOpt = (text) =>
    (!noTitle && text) ? { title: { text, textStyle: { ...base.textStyle, fontSize: 13, fontWeight: 600 }, top: 4, left: 'center' } } : {}

  // ── Pie ────────────────────────────────────────────────────────────────────
  if (type === 'pie') {
    const data = aggPairs(rows, xi, yi)
    return {
      ...base,
      grid: undefined,
      legend: { orient: 'vertical', right: '4%', top: 'center', textStyle: base.textStyle },
      series: [{ type: 'pie', radius: '65%', center: ['42%', '52%'], data, label: { color: base.textStyle.color }, color: PALETTE }],
      ...titleOpt(title ?? ''),
    }
  }

  // ── Donut ──────────────────────────────────────────────────────────────────
  if (type === 'donut') {
    const data = aggPairs(rows, xi, yi)
    return {
      ...base,
      grid: undefined,
      legend: { orient: 'vertical', right: '4%', top: 'center', textStyle: base.textStyle },
      series: [{ type: 'pie', radius: ['40%', '68%'], center: ['42%', '52%'], data, label: { color: base.textStyle.color }, color: PALETTE }],
      ...titleOpt(title ?? ''),
    }
  }

  // ── Funnel ─────────────────────────────────────────────────────────────────
  if (type === 'funnel') {
    const data = aggPairs(rows, xi, yi)
    return {
      ...base,
      grid: undefined,
      tooltip: { ...base.tooltip, trigger: 'item' },
      series: [{
        type: 'funnel',
        left: '10%',
        width: '80%',
        data,
        color: PALETTE,
        label: { color: base.textStyle.color },
      }],
      ...titleOpt(title ?? ''),
    }
  }

  // ── Gauge ──────────────────────────────────────────────────────────────────
  if (type === 'gauge') {
    const val = rows.length > 0 ? Number(rows[0][yi]) || 0 : 0
    const maxVal = Math.max(arrMax(rows.map((r) => Number(r[yi]) || 0)), 100)
    return {
      ...base,
      grid: undefined,
      series: [{
        type: 'gauge',
        max: maxVal,
        data: [{ value: val, name: xCol }],
        detail: { color: base.textStyle.color, fontSize: 18 },
        title: { color: base.textStyle.color, fontSize: 12 },
        axisLabel: { color: base.textStyle.color, fontSize: 10 },
        pointer: { itemStyle: { color: PALETTE[0] } },
        itemStyle: { color: PALETTE[0] },
        progress: { show: true, itemStyle: { color: PALETTE[0] } },
      }],
      ...titleOpt(title ?? ''),
    }
  }

  // ── Scatter ────────────────────────────────────────────────────────────────
  if (type === 'scatter') {
    const data = rows.map((r) => [Number(r[xi]) || 0, Number(r[yi]) || 0])
    const large = n > 2000
    return {
      ...base,
      xAxis: { type: 'value', name: xCol, nameLocation: 'middle', nameGap: 28, nameTextStyle: base.textStyle, ...axisStyle(isDark) },
      yAxis: { type: 'value', name: yCol, nameLocation: 'middle', nameGap: 40, nameTextStyle: base.textStyle, ...axisStyle(isDark) },
      series: [{ type: 'scatter', name: yCol, data, symbolSize: large ? 4 : 7, itemStyle: { color: PALETTE[0] }, ...(large ? { large: true, largeThreshold: 2000 } : {}) }],
      tooltip: {
        ...base.tooltip,
        trigger: 'item',
        formatter(p) {
          const [vx, vy] = Array.isArray(p.value) ? p.value : [0, p.value]
          const bg2 = base.tooltip.backgroundColor
          return `<div style="font-size:11px;opacity:0.65;margin-bottom:4px">${p.seriesName || ''}</div>` +
            `<div style="display:flex;align-items:center;justify-content:space-between;gap:12px;margin-top:2px;font-size:12px">` +
            `<span style="opacity:0.65">${xCol}</span><span style="font-weight:600">${fmtTooltipValue(vx)}</span></div>` +
            `<div style="display:flex;align-items:center;justify-content:space-between;gap:12px;margin-top:2px;font-size:12px">` +
            `<span style="opacity:0.65">${yCol}</span><span style="font-weight:600">${fmtTooltipValue(vy)}</span></div>`
        },
      },
      ...titleOpt(title ?? ''),
    }
  }

  // ── Bubble ─────────────────────────────────────────────────────────────────
  if (type === 'bubble') {
    const data = rows.map((r) => [
      Number(r[xi]) || 0,
      Number(r[yi]) || 0,
      zi >= 0 ? Number(r[zi]) || 0 : 10,
    ])
    const maxZ = Math.max(arrMax(data.map((d) => d[2])), 1)
    return {
      ...base,
      xAxis: { type: 'value', name: xCol, nameLocation: 'middle', nameGap: 28, nameTextStyle: base.textStyle, ...axisStyle(isDark) },
      yAxis: { type: 'value', name: yCol, nameLocation: 'middle', nameGap: 40, nameTextStyle: base.textStyle, ...axisStyle(isDark) },
      series: [{
        type: 'scatter',
        name: yCol,
        data,
        symbolSize: /** @param {number[]} d */ (d) => Math.max(8, (d[2] / maxZ) * 48),
        itemStyle: { color: PALETTE[0], opacity: 0.75 },
        ...(n > 2000 ? { large: true, largeThreshold: 2000 } : {}),
      }],
      tooltip: {
        ...base.tooltip,
        trigger: 'item',
        formatter(p) {
          const [vx, vy, vz] = Array.isArray(p.value) ? p.value : [0, p.value, 0]
          return `<div style="font-size:11px;opacity:0.65;margin-bottom:4px">${p.seriesName || ''}</div>` +
            `<div style="display:flex;align-items:center;justify-content:space-between;gap:12px;margin-top:2px;font-size:12px">` +
            `<span style="opacity:0.65">${xCol}</span><span style="font-weight:600">${fmtTooltipValue(vx)}</span></div>` +
            `<div style="display:flex;align-items:center;justify-content:space-between;gap:12px;margin-top:2px;font-size:12px">` +
            `<span style="opacity:0.65">${yCol}</span><span style="font-weight:600">${fmtTooltipValue(vy)}</span></div>` +
            `<div style="display:flex;align-items:center;justify-content:space-between;gap:12px;margin-top:2px;font-size:12px">` +
            `<span style="opacity:0.65">${zCol || 'size'}</span><span style="font-weight:600">${fmtTooltipValue(vz)}</span></div>`
        },
      },
      ...titleOpt(title ?? ''),
    }
  }

  // ── Heatmap ────────────────────────────────────────────────────────────────
  if (type === 'heatmap') {
    const xVals = [...new Set(rows.map((r) => String(r[xi])))]
    const yVals = gi >= 0 ? [...new Set(rows.map((r) => String(r[gi])))] : [yCol]
    // Index lookups via Map: indexOf per row is O(n²) on high-cardinality data.
    const xIdx = new Map(xVals.map((v, i) => [v, i]))
    const yIdx = new Map(yVals.map((v, i) => [v, i]))
    const data = rows.map((r) => [
      xIdx.get(String(r[xi])) ?? -1,
      gi >= 0 ? (yIdx.get(String(r[gi])) ?? 0) : 0,
      Number(r[yi]) || 0,
    ])
    const lineColor = isDark ? 'rgba(255,255,255,0.08)' : 'rgba(0,0,0,0.07)'
    return {
      ...base,
      tooltip: {
        ...base.tooltip,
        trigger: 'item',
        formatter(p) {
          const [xi2, yi2, val] = Array.isArray(p.value) ? p.value : [0, 0, p.value]
          const xLabel = xVals[xi2] ?? xi2
          const yLabel = yVals[yi2] ?? yi2
          return `<div style="font-size:11px;opacity:0.65;margin-bottom:2px">${xLabel} · ${yLabel}</div>` +
            `<div style="display:flex;align-items:center;gap:6px;margin-top:3px">` +
            `<span style="display:inline-block;width:8px;height:8px;border-radius:2px;background:${p.color ?? '#6366f1'};flex-shrink:0"></span>` +
            `<span style="margin-left:auto;padding-left:12px;font-weight:600;font-size:12px">${fmtTooltipValue(val)}</span></div>`
        },
      },
      xAxis: { type: 'category', data: xVals, splitArea: { show: true }, ...axisStyle(isDark) },
      yAxis: { type: 'category', data: yVals, splitArea: { show: true }, ...axisStyle(isDark) },
      visualMap: { min: 0, max: arrMax(data.map((d) => d[2])), calculable: true, orient: 'horizontal', left: 'center', bottom: 4, textStyle: base.textStyle },
      series: [{ type: 'heatmap', data, label: { show: data.length < 100 } }],
      grid: { ...base.grid, bottom: 70 },
      splitLine: { lineStyle: { color: lineColor } },
      ...titleOpt(title ?? ''),
    }
  }

  // ── Radar ──────────────────────────────────────────────────────────────────
  if (type === 'radar') {
    // Each non-xCol numeric column becomes an indicator; each row becomes a series
    const numCols = columns.filter((c) => c.name !== xCol && colType(c) === 'number')
    const numIdx = numCols.map((c) => columns.findIndex((cc) => cc.name === c.name))
    const indicators = numCols.map((c, k) => ({
      name: c.name,
      max: Math.max(arrMax(rows.map((r) => Number(r[numIdx[k]]) || 0)), 1) * 1.2,
    }))
    const seriesData = rows.map((r) => ({
      name: String(r[xi] ?? ''),
      value: numIdx.map((i) => Number(r[i]) || 0),
    }))
    return {
      ...base,
      grid: undefined,
      legend: { textStyle: base.textStyle, top: 4 },
      radar: {
        indicator: indicators,
        name: { textStyle: { color: base.textStyle.color } },
        axisLine: { lineStyle: { color: isDark ? 'rgba(255,255,255,0.1)' : 'rgba(0,0,0,0.1)' } },
        splitLine: { lineStyle: { color: isDark ? 'rgba(255,255,255,0.06)' : 'rgba(0,0,0,0.06)' } },
      },
      series: [{
        type: 'radar',
        data: seriesData.map((s, i) => ({ ...s, itemStyle: { color: PALETTE[i % PALETTE.length] }, areaStyle: { opacity: 0.15 } })),
      }],
      tooltip: { ...base.tooltip, trigger: 'item' },
      ...titleOpt(title ?? ''),
    }
  }

  // ── Histogram ─────────────────────────────────────────────────────────────
  if (type === 'histogram') {
    const vals = rows.map((r) => Number(r[yi >= 0 ? yi : xi]) || 0).filter(isFinite)
    const bins = 20
    if (vals.length === 0) return { ...base, series: [] }
    const min = arrMin(vals)
    const max = arrMax(vals)
    const step = (max - min) / bins || 1
    const counts = Array(bins).fill(0)
    vals.forEach((v) => {
      const i = Math.min(Math.floor((v - min) / step), bins - 1)
      counts[i]++
    })
    const labels = counts.map((_, i) => {
      const lo = min + i * step
      const hi = lo + step
      return `${lo.toFixed(1)}-${hi.toFixed(1)}`
    })
    return {
      ...base,
      tooltip: { ...base.tooltip, trigger: 'axis' },
      xAxis: categoryXAxis(isDark, labels),
      yAxis: valueYAxis(isDark),
      series: [{ type: 'bar', name: xCol, data: counts, itemStyle: { color: PALETTE[0], borderRadius: [3, 3, 0, 0] }, barCategoryGap: '2%' }],
      ...titleOpt(title ?? ''),
    }
  }

  // ── Box Plot ───────────────────────────────────────────────────────────────
  if (type === 'box-plot') {
    const groups = gi >= 0
      ? [...new Set(rows.map((r) => String(r[gi])))]
      : ['All']
    const boxData = groups.map((grp) => {
      const vals = (gi >= 0 ? rows.filter((r) => String(r[gi]) === grp) : rows)
        .map((r) => Number(r[yi]) || 0)
        .sort((a, b) => a - b)
      if (vals.length === 0) return [0, 0, 0, 0, 0]
      const q = (p) => {
        const idx = (vals.length - 1) * p
        const lo = Math.floor(idx)
        const hi = Math.ceil(idx)
        return vals[lo] + (vals[hi] - vals[lo]) * (idx - lo)
      }
      return [vals[0], q(0.25), q(0.5), q(0.75), vals[vals.length - 1]]
    })
    return {
      ...base,
      tooltip: { ...base.tooltip, trigger: 'item' },
      xAxis: categoryXAxis(isDark, groups),
      yAxis: valueYAxis(isDark),
      series: [{ type: 'boxplot', data: boxData, itemStyle: { color: PALETTE[0], borderColor: PALETTE[0] } }],
      ...titleOpt(title ?? ''),
    }
  }

  // ── Word Cloud ─────────────────────────────────────────────────────────────
  if (type === 'word-cloud') {
    // Aggregate duplicate words (like pie/donut) and cap to the top 150 by
    // weight - the layout is a synchronous canvas packer, so thousands of words
    // would freeze the UI.
    const data = aggPairs(rows, xi, yi).slice(0, 150).map((d) => ({ name: d.name, value: d.value || 1 }))
    return {
      ...base,
      grid: undefined,
      tooltip: { ...base.tooltip, trigger: 'item' },
      series: [{
        type: 'wordCloud',
        shape: 'circle',
        width: '90%',
        height: '90%',
        gridSize: 8,
        sizeRange: [12, 60],
        rotationRange: [-45, 45],
        rotationStep: 45,
        drawOutOfBound: false,
        textStyle: { color: (p) => PALETTE[(p?.dataIndex ?? 0) % PALETTE.length] },
        emphasis: { textStyle: { fontWeight: 'bold' } },
        data,
      }],
      ...titleOpt(title ?? ''),
    }
  }

  // ── Treemap ────────────────────────────────────────────────────────────────
  if (type === 'treemap') {
    const data = rows.map((r) => ({ name: String(r[xi] ?? ''), value: Number(r[yi]) || 0 }))
    return {
      ...base,
      grid: undefined,
      tooltip: { ...base.tooltip, trigger: 'item' },
      series: [{
        type: 'treemap',
        data,
        color: PALETTE,
        label: { color: '#fff', fontSize: 11 },
        breadcrumb: { show: false },
      }],
      ...titleOpt(title ?? ''),
    }
  }

  // ── Circle Pack (treemap approximation) ───────────────────────────────────
  if (type === 'circle-pack') {
    const data = rows.map((r) => ({ name: String(r[xi] ?? ''), value: Number(r[yi]) || 0 }))
    return {
      ...base,
      grid: undefined,
      tooltip: { ...base.tooltip, trigger: 'item' },
      series: [{
        type: 'treemap',
        data,
        color: PALETTE,
        label: { color: '#fff', fontSize: 10 },
        breadcrumb: { show: false },
        itemStyle: { borderRadius: 99, gapWidth: 4 },
        levels: [{ itemStyle: { borderRadius: 99 } }],
      }],
      ...titleOpt(title ?? ''),
    }
  }

  // ── Tree ───────────────────────────────────────────────────────────────────
  if (type === 'tree') {
    // xCol=name, groupCol=parent. buildTreeData dedups, enforces a single parent
    // per node, guards cycles, and caps node count.
    const treeData = buildTreeData(rows, xi, gi)
    return {
      ...base,
      grid: undefined,
      tooltip: { ...base.tooltip, trigger: 'item' },
      series: [{
        type: 'tree',
        data: treeData,
        top: '5%',
        left: '7%',
        bottom: '5%',
        right: '20%',
        symbolSize: 7,
        label: { position: 'left', verticalAlign: 'middle', align: 'right', fontSize: 10, color: base.textStyle.color },
        leaves: { label: { position: 'right', verticalAlign: 'middle', align: 'left' } },
        roam: true,
        expandAndCollapse: true,
        animationDuration: 400,
        lineStyle: { color: isDark ? 'rgba(255,255,255,0.15)' : 'rgba(0,0,0,0.15)' },
      }],
      ...titleOpt(title ?? ''),
    }
  }

  // ── Dendrogram ─────────────────────────────────────────────────────────────
  if (type === 'dendrogram') {
    // Same hierarchy as 'tree' (deduped, single-parent, cycle-guarded, capped),
    // rendered as a radial dendrogram.
    const treeData = buildTreeData(rows, xi, gi)
    return {
      ...base,
      grid: undefined,
      tooltip: { ...base.tooltip, trigger: 'item', formatter: '{b}' },
      series: [{
        type: 'tree',
        data: treeData,
        layout: 'radial',
        top: '5%', left: '5%', bottom: '5%', right: '5%',
        symbolSize: 6,
        symbol: 'circle',
        itemStyle: { color: PALETTE[0], borderWidth: 0 },
        label: { fontSize: 10, color: base.textStyle.color, distance: 8 },
        lineStyle: {
          color: isDark ? 'rgba(255,255,255,0.18)' : 'rgba(0,0,0,0.15)',
          width: 1.5,
          curveness: 0.5,
        },
        roam: true,
        expandAndCollapse: true,
        initialTreeDepth: 3,
        animationDuration: 400,
      }],
      ...titleOpt(title ?? ''),
    }
  }

  // ── Waterfall ─────────────────────────────────────────────────────────────
  if (type === 'waterfall') {
    const xData = rows.map(r => String(r[xi] ?? ''))
    const vals = rows.map(r => Number(r[yi]) || 0)
    let running = 0
    /** @type {any[]} */ const baseData = []
    /** @type {any[]} */ const incrData = []
    vals.forEach(v => {
      if (v >= 0) {
        baseData.push(running)
        incrData.push({ value: v, originalValue: v, itemStyle: { color: '#22c55e', borderRadius: [3, 3, 0, 0] } })
      } else {
        baseData.push(running + v)
        incrData.push({ value: -v, originalValue: v, itemStyle: { color: '#ef4444', borderRadius: [3, 3, 0, 0] } })
      }
      running += v
    })
    return {
      ...base,
      ...animOpts(n),
      tooltip: {
        ...base.tooltip,
        trigger: 'axis',
        axisPointer: shadowPointer(isDark),
        formatter(params) {
          const all = Array.isArray(params) ? params : [params]
          const vis = all.filter(p => !String(p.seriesName ?? '').startsWith('__'))
          const header = fmtTooltipValue(vis[0]?.axisValue ?? all[0]?.axisValue ?? '')
          const rowsHtml = vis.map(p => {
            const orig = p.data?.originalValue ?? p.value
            const color = p.color ?? PALETTE[0]
            const sign = Number(orig) >= 0 ? '+' : ''
            return `<div style="display:flex;align-items:center;gap:6px;margin-top:3px">` +
              `<span style="display:inline-block;width:8px;height:8px;border-radius:50%;background:${color};flex-shrink:0"></span>` +
              `<span style="margin-left:auto;padding-left:12px;font-weight:600">${sign}${fmtTooltipValue(orig)}</span></div>`
          }).join('')
          return `<div style="font-size:11px;opacity:0.65;margin-bottom:2px">${header}</div>${rowsHtml}`
        },
      },
      xAxis: categoryXAxis(isDark, xData),
      yAxis: valueYAxis(isDark),
      series: [
        {
          type: 'bar', name: '__base__', stack: 'wf',
          data: baseData,
          itemStyle: { color: 'transparent', borderColor: 'transparent' },
          emphasis: { itemStyle: { color: 'transparent' } },
          silent: true, tooltip: { show: false },
        },
        { type: 'bar', name: yCol, stack: 'wf', data: incrData, barMaxWidth: 48 },
      ],
      ...titleOpt(title ?? ''),
    }
  }

  // ── Pareto ────────────────────────────────────────────────────────────────
  if (type === 'pareto') {
    const rawPairs = rows.map(r => [String(r[xi] ?? ''), Number(r[yi]) || 0])
    const sorted = [...rawPairs].sort((a, b) => /** @type {number} */ (b[1]) - /** @type {number} */ (a[1]))
    const xData = sorted.map(p => String(p[0]))
    const barData = sorted.map(p => Number(p[1]))
    const total = barData.reduce((s, v) => s + v, 0)
    let cum = 0
    const lineData = barData.map(v => {
      cum += v
      return total > 0 ? parseFloat((cum / total * 100).toFixed(1)) : 0
    })
    return {
      ...base,
      ...animOpts(n),
      tooltip: { ...base.tooltip, trigger: 'axis' },
      legend: { textStyle: base.textStyle, top: 4, data: [yCol, 'Cumulative %'] },
      xAxis: categoryXAxis(isDark, xData),
      yAxis: [
        valueYAxis(isDark),
        {
          ...valueYAxis(isDark),
          name: '%', max: 100,
          axisLabel: { ...axisStyle(isDark).axisLabel, formatter: '{value}%' },
          splitLine: { show: false },
        },
      ],
      series: [
        {
          type: 'bar', name: yCol, data: barData,
          itemStyle: { color: PALETTE[0], borderRadius: [3, 3, 0, 0] },
          barMaxWidth: 48,
        },
        {
          type: 'line', name: 'Cumulative %', data: lineData, yAxisIndex: 1,
          symbol: 'circle', symbolSize: 4, smooth: false,
          lineStyle: { color: PALETTE[2], width: 2 },
          itemStyle: { color: PALETTE[2] },
          markLine: {
            silent: true, symbol: 'none',
            data: [{ yAxis: 80 }],
            lineStyle: { color: PALETTE[2], opacity: 0.35, type: 'dashed', width: 1 },
            label: { formatter: '80%', color: base.textStyle.color, fontSize: 10, position: 'insideEndTop' },
          },
        },
      ],
      ...titleOpt(title ?? ''),
    }
  }

  // ── Sunburst ──────────────────────────────────────────────────────────────
  if (type === 'sunburst') {
    // Same guards as buildTreeData: dedupe by name, attach each node to at most
    // one parent, skip self/cycle edges and cap the count - duplicate rows or
    // cyclic parent data otherwise build a graph that hangs/crashes ECharts.
    const MAX_NODES = 2000
    /** @type {Map<string, any>} */
    const nodeMap = new Map()
    for (const r of rows) {
      const name = String(r[xi] ?? '')
      if (!name || nodeMap.has(name)) continue
      nodeMap.set(name, { name, value: Number(r[yi]) || 0, children: [] })
      if (nodeMap.size >= MAX_NODES) break
    }
    /** @type {any[]} */ let roots = []
    if (gi >= 0) {
      const parentOf = new Map()
      const attached = new Set()
      for (const r of rows) {
        const name = String(r[xi] ?? '')
        const parent = String(r[gi] ?? '')
        if (!name || !parent || parent === name || attached.has(name)) continue
        if (!nodeMap.has(name) || !nodeMap.has(parent)) continue
        // Cycle guard: parent must not already be a descendant of `name`.
        let cur = parent, cyclic = false
        while (cur !== undefined) { if (cur === name) { cyclic = true; break } cur = parentOf.get(cur) }
        if (cyclic) continue
        nodeMap.get(parent).children.push(nodeMap.get(name))
        parentOf.set(name, parent)
        attached.add(name)
      }
      roots = [...nodeMap.values()].filter((n) => !attached.has(n.name))
      roots = roots.length > 0 ? roots : [{ name: 'Root', children: [...nodeMap.values()] }]
    } else {
      roots = [...nodeMap.values()]
    }
    return {
      ...base,
      grid: undefined,
      color: PALETTE,
      tooltip: {
        ...base.tooltip,
        trigger: 'item',
        formatter(p) {
          return `<div style="font-size:11px;opacity:0.65;margin-bottom:2px">${p.name}</div>` +
            `<div style="display:flex;align-items:center;gap:6px;margin-top:3px">` +
            `<span style="display:inline-block;width:8px;height:8px;border-radius:50%;background:${p.color ?? PALETTE[0]};flex-shrink:0"></span>` +
            `<span style="margin-left:auto;padding-left:12px;font-weight:600;font-size:12px">${fmtTooltipValue(p.value)}</span></div>`
        },
      },
      series: [{
        type: 'sunburst',
        data: roots,
        radius: ['15%', '80%'],
        nodeClick: 'rootToNode',
        label: { fontSize: 10, minAngle: 8, overflow: 'truncate' },
        itemStyle: { borderRadius: 4, borderWidth: 1, borderColor: 'transparent' },
        emphasis: { focus: 'ancestor' },
      }],
      ...titleOpt(title ?? ''),
    }
  }

  // ── Choropleth / Meter - handled by dedicated Svelte components ────────────
  // buildOption is not called for these types; return empty so callers get {}
  if (type === 'choropleth' || type === 'meter') return {}

  // ── Sankey ─────────────────────────────────────────────────────────────────
  if (type === 'sankey') {
    // Sankey needs a distinct Target (group) column; without it every link's
    // target is empty and nothing renders.
    if (gi < 0) return {}
    // Aggregate duplicate source→target pairs. The old code emitted one link PER
    // ROW, so N rows produced N overlapping links (8k rows → 8k links = the
    // unreadable hairball). Sum values per unique pair instead.
    const linkMap = new Map()
    for (const r of rows) {
      const s = String(r[xi] ?? ''), t = String(r[gi] ?? '')
      if (!s || !t || s === t) continue
      const key = s + '␟' + t
      linkMap.set(key, (linkMap.get(key) ?? 0) + (Number(r[yi]) || 1))
    }
    // Strongest links first; drop the reverse edge of any 2-cycle (ECharts sankey
    // requires a DAG and throws on cycles) and cap the count so it stays legible.
    const seenPair = new Set()
    const links = [...linkMap.entries()]
      .map(([key, value]) => { const [source, target] = key.split('␟'); return { source, target, value } })
      .sort((a, b) => b.value - a.value)
      .filter((l) => {
        if (seenPair.has(l.target + '␟' + l.source)) return false
        seenPair.add(l.source + '␟' + l.target)
        return true
      })
      .slice(0, 200)
    const nodeSet = new Set()
    for (const l of links) { nodeSet.add(l.source); nodeSet.add(l.target) }
    const nodes = [...nodeSet].map((name) => ({ name }))
    if (nodes.length === 0) return {}
    return {
      ...base,
      grid: undefined,
      tooltip: { ...base.tooltip, trigger: 'item' },
      series: [{
        type: 'sankey',
        data: nodes,
        links,
        emphasis: { focus: 'adjacency' },
        lineStyle: { color: 'gradient', opacity: 0.4 },
        label: { color: base.textStyle.color, fontSize: 11 },
      }],
      ...titleOpt(title ?? ''),
    }
  }

  // ── Lollipop ───────────────────────────────────────────────────────────────
  if (type === 'lollipop') {
    const xData = rows.map((r) => String(r[xi] ?? ''))
    const yData = rows.map((r) => Number(r[yi]) || 0)
    return {
      ...base,
      tooltip: { ...base.tooltip, trigger: 'axis' },
      xAxis: categoryXAxis(isDark, xData),
      yAxis: valueYAxis(isDark),
      series: [
        {
          type: 'line',
          name: yCol,
          data: yData,
          symbol: 'circle',
          symbolSize: 8,
          lineStyle: { width: 0 },
          itemStyle: { color: PALETTE[0] },
          markLine: {
            silent: true,
            symbol: 'none',
            label: { show: false },
            lineStyle: { color: isDark ? 'rgba(255,255,255,0.15)' : 'rgba(0,0,0,0.12)', type: 'solid' },
            data: [{ yAxis: 0 }],
          },
        },
        {
          type: 'bar',
          data: yData,
          barMaxWidth: 2,
          itemStyle: { color: PALETTE[0] },
          silent: true,
        },
      ],
      ...titleOpt(title ?? ''),
    }
  }

  // ── Lollipop Horizontal ──────────────────────────────────────────────────
  if (type === 'lollipop-h') {
    const yData = rows.map((r) => String(r[xi] ?? ''))
    const xData = rows.map((r) => Number(r[yi]) || 0)
    return {
      ...base,
      tooltip: { ...base.tooltip, trigger: 'axis' },
      xAxis: { ...valueYAxis(isDark) },
      yAxis: { type: 'category', data: yData, ...axisStyle(isDark) },
      series: [
        {
          type: 'bar', data: xData, barMaxWidth: 2,
          itemStyle: { color: PALETTE[0] }, silent: true,
        },
        {
          type: 'scatter', name: yCol, data: xData.map((v, i) => [v, i]),
          symbolSize: 10, itemStyle: { color: PALETTE[0] },
        },
      ],
      ...titleOpt(title ?? ''),
    }
  }

  // ── Combo (Bar + Line dual axis) ──────────────────────────────────────────
  if (type === 'combo') {
    const xData = rows.map((r) => String(r[xi] ?? ''))
    const barData = rows.map((r) => Number(r[yi]) || 0)
    const lineData = zi >= 0 ? rows.map((r) => Number(r[zi]) || 0) : []
    const hasLine = lineData.length > 0
    return {
      ...base,
      tooltip: { ...base.tooltip, trigger: 'axis' },
      legend: { textStyle: base.textStyle, top: 4, data: [yCol, ...(hasLine ? [zCol ?? 'line'] : [])] },
      xAxis: categoryXAxis(isDark, xData),
      yAxis: hasLine
        ? [{ ...valueYAxis(isDark), name: yCol }, { ...valueYAxis(isDark), name: zCol ?? '', splitLine: { show: false } }]
        : valueYAxis(isDark),
      series: [
        { type: 'bar', name: yCol, data: barData, itemStyle: { color: PALETTE[0], borderRadius: [3,3,0,0] }, barMaxWidth: 48, yAxisIndex: 0 },
        ...(hasLine ? [{
          type: 'line', name: zCol ?? 'line', data: lineData, smooth: true,
          symbol: 'circle', symbolSize: 5,
          lineStyle: { color: PALETTE[1], width: 2 }, itemStyle: { color: PALETTE[1] },
          yAxisIndex: 1,
        }] : []),
      ],
      ...titleOpt(title ?? ''),
    }
  }

  // ── Bar Floating ──────────────────────────────────────────────────────────
  if (type === 'bar-floating') {
    const xData = rows.map((r) => String(r[xi] ?? ''))
    const data = rows.map((r) => [Number(r[yi]) || 0, zi >= 0 ? Number(r[zi]) || 0 : Number(r[yi]) || 0])
    return {
      ...base,
      tooltip: { ...base.tooltip, trigger: 'axis' },
      xAxis: categoryXAxis(isDark, xData),
      yAxis: valueYAxis(isDark),
      series: [{ type: 'bar', name: yCol, data, itemStyle: { color: PALETTE[0], borderRadius: [3, 3, 0, 0] }, barMaxWidth: 48 }],
      ...titleOpt(title ?? ''),
    }
  }

  // ── Bullet ────────────────────────────────────────────────────────────────
  if (type === 'bullet') {
    const categories = rows.map((r) => String(r[xi] ?? ''))
    const actuals = rows.map((r) => Number(r[yi]) || 0)
    const targets = rows.map((r) => zi >= 0 ? Number(r[zi]) || 0 : 0)
    const maxVal = Math.max(arrMax(actuals), arrMax(targets), 1)
    return {
      ...base,
      tooltip: { ...base.tooltip, trigger: 'axis', axisPointer: shadowPointer(isDark) },
      xAxis: { type: 'value', max: maxVal * 1.1, ...axisStyle(isDark) },
      yAxis: { type: 'category', data: categories, ...axisStyle(isDark) },
      series: [
        {
          type: 'bar',
          name: yCol,
          data: actuals,
          itemStyle: { color: PALETTE[0], borderRadius: [0, 3, 3, 0] },
          barMaxWidth: 20,
        },
        {
          type: 'scatter',
          name: zCol ?? 'Target',
          data: targets.map((v, i) => [v, i]),
          symbol: 'rect',
          symbolSize: [4, 20],
          itemStyle: { color: PALETTE[3] },
          z: 10,
        },
      ],
      legend: { textStyle: base.textStyle, top: 4 },
      ...titleOpt(title ?? ''),
    }
  }

  // ── Bar Horizontal ─────────────────────────────────────────────────────────
  if (type === 'bar-horizontal') {
    let xData = sortedXData(rows, xi)
    const dataMap = aggDataMap(rows, xi, yi)
    // A ranking: largest first (and, with the inverted axis, on top). Dates keep
    // their chronological order, earliest on top.
    if (!isTimestampAxis(xData)) xData = [...xData].sort((a, b) => (dataMap[b] ?? 0) - (dataMap[a] ?? 0))
    // Categories are on the Y axis; past ~25 they overflow vertically. Add a
    // vertical scroll/zoom (slider on the right + wheel/drag inside) so many
    // categories stay usable instead of cramming - the generic dataZoom only
    // covers the horizontal x-axis and this branch never reaches it.
    const needsZoom = xData.length > 25
    const dataZoom = needsZoom ? [
      { type: 'slider', yAxisIndex: 0, right: 4, width: 12, start: 0, end: (25 / xData.length) * 100,
        borderColor: 'transparent', fillerColor: 'rgba(127,127,127,0.14)', handleStyle: { color: PALETTE[0] }, textStyle: { color: 'transparent' } },
      { type: 'inside', yAxisIndex: 0 },
    ] : []
    return {
      ...base,
      tooltip: { ...base.tooltip, trigger: 'axis', axisPointer: shadowPointer(isDark) },
      xAxis: valueYAxis(isDark),
      yAxis: {
        type: 'category',
        data: xData,
        // Largest on top: the eye starts at the top of a ranked list.
        inverse: true,
        ...axisStyle(isDark),
        axisLine: { show: false },
        splitLine: { show: false },
        // Long names were clipped by the window edge: cap the label column and
        // end a long name with an ellipsis (the tooltip has it whole).
        axisLabel: { ...axisStyle(isDark).axisLabel, width: 200, overflow: 'truncate', ellipsis: '…' },
      },
      series: [{
        type: 'bar',
        name: yCol,
        data: xData.map((x) => dataMap[x] ?? null),
        itemStyle: { color: PALETTE[0], borderRadius: [0, 4, 4, 0] },
        barMaxWidth: 22,
        barCategoryGap: '38%',
        // A faint track behind each bar shows how far it is from the maximum.
        showBackground: xData.length <= 40,
        backgroundStyle: { color: isDark ? 'rgba(255,255,255,0.03)' : 'rgba(0,0,0,0.025)', borderRadius: [0, 4, 4, 0] },
        label: barValueLabel(isDark, 'right', xData.length),
        emphasis: BAR_EMPHASIS,
        blur: BAR_BLUR,
      }],
      grid: { ...base.grid, left: 12, right: needsZoom ? 36 : 40 },
      ...(needsZoom ? { dataZoom } : {}),
      ...titleOpt(title ?? ''),
    }
  }

  // ── Bar Grouped ────────────────────────────────────────────────────────────
  if (type === 'bar-grouped') {
    const xData = sortedXData(rows, xi)
    const groups = gi >= 0 ? [...new Set(rows.map((r) => String(r[gi])))] : [yCol]
    const prog = n > 1000 ? { progressive: 400, progressiveThreshold: 1000 } : {}
    const series = groups.map((grp, i) => {
      const dataMap = gi >= 0
        ? aggDataMap(rows.filter(r => String(r[gi]) === grp), xi, yi)
        : aggDataMap(rows, xi, yi)
      return { type: 'bar', name: grp, data: xData.map((x) => dataMap[x] ?? null), itemStyle: { color: PALETTE[i % PALETTE.length], borderRadius: n > 300 ? 0 : [3, 3, 0, 0] }, barMaxWidth: 32, ...prog }
    })
    return {
      ...base,
      tooltip: { ...base.tooltip, trigger: 'axis' },
      legend: { textStyle: base.textStyle, top: 4 },
      xAxis: categoryXAxis(isDark, xData),
      yAxis: valueYAxis(isDark),
      series,
      ...titleOpt(title ?? ''),
    }
  }

  // ── Bar Stacked ────────────────────────────────────────────────────────────
  if (type === 'bar-stacked') {
    const xData = sortedXData(rows, xi)
    const groups = gi >= 0 ? [...new Set(rows.map((r) => String(r[gi])))] : [yCol]
    const series = groups.map((grp, i) => {
      const dataMap = gi >= 0
        ? aggDataMap(rows.filter(r => String(r[gi]) === grp), xi, yi)
        : aggDataMap(rows, xi, yi)
      return { type: 'bar', name: grp, stack: 'total', data: xData.map((x) => dataMap[x] ?? null), itemStyle: { color: PALETTE[i % PALETTE.length] }, barMaxWidth: 48 }
    })
    return {
      ...base,
      tooltip: { ...base.tooltip, trigger: 'axis' },
      legend: { textStyle: base.textStyle, top: 4 },
      xAxis: categoryXAxis(isDark, xData),
      yAxis: valueYAxis(isDark),
      series,
      ...titleOpt(title ?? ''),
    }
  }

  // ── Bar Stacked 100% ───────────────────────────────────────────────────────
  if (type === 'bar-stacked-100') {
    const xData = sortedXData(rows, xi)
    const groups = gi >= 0 ? [...new Set(rows.map((r) => String(r[gi])))] : [yCol]
    // Compute totals per x category
    const totals = Object.fromEntries(xData.map((x) => [x, 0]))
    rows.forEach((r) => { totals[String(r[xi] ?? '')] = (totals[String(r[xi] ?? '')] || 0) + (Number(r[yi]) || 0) })
    const series = groups.map((grp, i) => {
      const dataMap = gi >= 0
        ? aggDataMap(rows.filter(r => String(r[gi]) === grp), xi, yi)
        : aggDataMap(rows, xi, yi)
      return {
        type: 'bar',
        name: grp,
        stack: 'total',
        data: xData.map((x) => totals[x] > 0 ? +((dataMap[x] ?? 0) / totals[x] * 100).toFixed(2) : 0),
        itemStyle: { color: PALETTE[i % PALETTE.length] },
        barMaxWidth: 48,
      }
    })
    return {
      ...base,
      tooltip: {
        ...base.tooltip,
        trigger: 'axis',
        formatter(params) {
          const items = Array.isArray(params) ? params : [params]
          const header = fmtTooltipValue(items[0]?.axisValue ?? items[0]?.name ?? '')
          const multi = items.length > 1
          const rowsHtml = items.map(p => {
            const color = p.color ?? '#6366f1'
            const val = `${fmtTooltipValue(p.value)}%`
            return `<div style="display:flex;align-items:center;gap:6px;margin-top:3px">` +
              `<span style="display:inline-block;width:8px;height:8px;border-radius:50%;background:${color};flex-shrink:0"></span>` +
              (multi ? `<span style="opacity:0.65;font-size:11px">${p.seriesName ?? ''}</span>` : '') +
              `<span style="margin-left:auto;padding-left:12px;font-weight:600">${val}</span></div>`
          }).join('')
          return `<div style="font-size:11px;opacity:0.65;margin-bottom:2px">${header}</div>${rowsHtml}`
        },
      },
      legend: { textStyle: base.textStyle, top: 4 },
      xAxis: categoryXAxis(isDark, xData),
      yAxis: { type: 'value', max: 100, axisLabel: { ...axisStyle(isDark).axisLabel, formatter: '{value}%' }, ...axisStyle(isDark) },
      series,
      ...titleOpt(title ?? ''),
    }
  }

  // ── Area Stacked ──────────────────────────────────────────────────────────
  if (type === 'area-stacked') {
    const xData = sortedXData(rows, xi)
    const groups = gi >= 0 ? [...new Set(rows.map((r) => String(r[gi])))] : [yCol]
    const prog = n > 1000 ? { progressive: 400, progressiveThreshold: 1000 } : {}
    const showSym = n < 200
    const sampling = n > 500 ? { sampling: 'lttb' } : {}
    const series = groups.map((grp, i) => {
      const dataMap = gi >= 0
        ? aggDataMap(rows.filter(r => String(r[gi]) === grp), xi, yi)
        : aggDataMap(rows, xi, yi)
      const color = PALETTE[i % PALETTE.length]
      return {
        type: 'line',
        name: grp,
        stack: 'total',
        data: xData.map((x) => dataMap[x] ?? null),
        smooth: n < 5000,
        symbol: showSym ? 'circle' : 'none',
        symbolSize: 4,
        lineStyle: { color, width: 1.5 },
        itemStyle: { color },
        areaStyle: { opacity: 0.2 },
        ...sampling,
        ...prog,
      }
    })
    return {
      ...base,
      tooltip: { ...base.tooltip, trigger: 'axis' },
      legend: { textStyle: base.textStyle, top: 4 },
      xAxis: categoryXAxis(isDark, xData),
      yAxis: valueYAxis(isDark),
      series,
      ...titleOpt(title ?? ''),
    }
  }

  // ── Bar / Line / Area (with optional group) ────────────────────────────────
  let xData = sortedXData(rows, xi)

  // Plain bar charts read far better sorted by magnitude; ECharts otherwise
  // keeps categorical x in arbitrary first-seen order. Skip for timestamps
  // (chronological order is meaningful) and for line/area/step (sorting would
  // scramble the connected path).
  if (type === 'bar' && !isTimestampAxis(xData)) {
    const totals = aggDataMap(rows, xi, yi)
    xData = [...xData].sort((a, b) => (totals[b] ?? 0) - (totals[a] ?? 0))
  }

  /** @type {any[]} */
  let series
  if (gi >= 0) {
    const groups = [...new Set(rows.map((r) => String(r[gi])))]
    series = groups.map((grp, i) => {
      const grpRows = rows.filter((r) => String(r[gi]) === grp)
      const dataMap = aggDataMap(grpRows, xi, yi)
      return makeSeries(type, grp, xData.map((x) => dataMap[x] ?? null), PALETTE[i % PALETTE.length], n)
    })
  } else {
    const dataMap = aggDataMap(rows, xi, yi)
    series = [makeSeries(type, yCol, xData.map((x) => dataMap[x] ?? null), PALETTE[0], n)]
    if (type === 'bar') series[0].label = barValueLabel(isDark, 'top', xData.length > 24 ? Infinity : xData.length)
  }

  // Add a scroll/zoom viewport when there are many x-axis categories so the
  // chart doesn't cram hundreds of labels into a small width.
  const needsZoom = xData.length > 80
  const dataZoom = needsZoom ? [
    {
      type: 'slider',
      xAxisIndex: 0,
      start: 0,
      end: 100,
      height: 18,
      bottom: 4,
      borderColor: 'transparent',
      fillerColor: 'rgba(99,102,241,0.12)',
      handleStyle: { color: PALETTE[0] },
      textStyle: { color: 'transparent' },
    },
    { type: 'inside', xAxisIndex: 0 },
  ] : []

  return {
    ...base,
    // Bars get a 'shadow' pointer (highlights the whole hovered category column -
    // the expected bar-chart hover); line/area keep the thin crosshair line.
    tooltip: { ...base.tooltip, trigger: 'axis', axisPointer: type === 'bar' ? shadowPointer(isDark) : { type: 'line' } },
    legend: series.length > 1 ? { textStyle: base.textStyle, top: 4 } : undefined,
    xAxis: categoryXAxis(isDark, xData),
    yAxis: valueYAxis(isDark),
    series,
    ...(needsZoom ? { grid: { ...base.grid, bottom: 44 } } : {}),
    ...(needsZoom ? { dataZoom } : {}),
    ...titleOpt(title ?? ''),
  }
}

/**
 * @param {string} type
 * @param {string} name
 * @param {any[]} data
 * @param {string} color
 * @param {number} [n] total row count for perf decisions
 */
function makeSeries(type, name, data, color, n = 0) {
  // Progressive rendering kicks in above this threshold - renders in 400-item
  // chunks per animation frame so the thread stays responsive.
  const prog = n > 1000 ? { progressive: 400, progressiveThreshold: 1000 } : {}

  if (type === 'bar') {
    return {
      type: 'bar', name, data,
      itemStyle: { color, borderRadius: n > 300 ? 0 : [4, 4, 0, 0] },
      barMaxWidth: 40,
      barCategoryGap: '32%',
      ...(n <= 300 ? { emphasis: BAR_EMPHASIS, blur: BAR_BLUR } : {}),
      ...prog,
      ...(n > 5000 ? { large: true, largeThreshold: 5000 } : {}),
    }
  }

  if (type === 'step') {
    return {
      type: 'line', name, data,
      step: 'end',
      smooth: false,
      symbol: n < 200 ? 'circle' : 'none',
      symbolSize: 4,
      lineStyle: { color, width: 2 },
      itemStyle: { color },
      ...(n > 1000 ? { progressive: 400, progressiveThreshold: 1000 } : {}),
    }
  }

  const isArea = type === 'area'
  // Hide per-point symbols when there are many points - they cost more to render
  // than the line itself and become invisible noise past ~200 points anyway.
  const showSymbols = n < 200
  // LTTB (Largest-Triangle-Three-Buckets) down-samples the visible line while
  // preserving shape - the chart looks identical but draws far fewer paths.
  const sampling = n > 500 ? { sampling: 'lttb' } : {}

  return {
    type: 'line',
    name,
    data,
    smooth: n < 5000,
    symbol: showSymbols ? 'circle' : 'none',
    symbolSize: 4,
    lineStyle: { color, width: n > 2000 ? 1.5 : 2 },
    itemStyle: { color },
    ...sampling,
    ...prog,
    ...(isArea ? { areaStyle: { color, opacity: 0.12 } } : {}),
  }
}

/**
 * Detect if a column set is suitable for a chart (at least 2 columns).
 * @param {ColInfo[]} columns
 */
/**
 * Make a saved chart option safe to render right now.
 *
 * `previewOption` is a JSON snapshot of a built option, so it carries whatever
 * was true when it was saved: theme colours baked into every axis label and
 * text style, a legend laid out for a full-size chart, and - because
 * JSON.stringify drops functions - no formatters at all. Rendered months later
 * in the other theme, the axis labels are dark text on a dark card, which is
 * why a saved chart could come back with no readable axes.
 *
 * Rebuilding from the data would be better, but the data is a query result that
 * is not saved with the chart. Re-theming what we have is what can actually be
 * done, and it is enough: colours come from the live theme, the title is
 * dropped (the card already shows it), and a timestamp axis gets its formatter
 * back.
 *
 * @param {any} option A saved previewOption.
 * @param {{ isDark: boolean, compact?: boolean }} ctx
 *   `compact` is for dashboard tiles, where a legend positioned for a full
 *   chart lands on top of the plot.
 */
export function restyleSavedOption(option, { isDark, compact = false }) {
  if (!option || typeof option !== 'object' || !Object.keys(option).length) return option

  const themed = baseOption(isDark, true)
  const axis = axisStyle(isDark)

  /** Re-colour one axis (or an array of them) without touching its data. */
  const reAxis = (a) => {
    if (Array.isArray(a)) return a.map(reAxis)
    if (!a || typeof a !== 'object') return a
    const next = {
      ...a,
      axisLabel: { ...a.axisLabel, color: axis.axisLabel.color },
      axisLine: { ...a.axisLine, lineStyle: { ...a.axisLine?.lineStyle, ...axis.axisLine.lineStyle } },
      splitLine: { ...a.splitLine, lineStyle: { ...a.splitLine?.lineStyle, ...axis.splitLine.lineStyle } },
    }
    // A timestamp axis loses its formatter to JSON.stringify and renders as
    // raw epoch numbers; put it back.
    if (Array.isArray(a.data) && isTimestampAxis(a.data)) {
      next.axisLabel = { ...next.axisLabel, formatter: fmtAxisLabel }
    }
    return next
  }

  const out = {
    ...option,
    title: undefined,
    textStyle: { ...option.textStyle, ...themed.textStyle },
    tooltip: { ...option.tooltip, ...themed.tooltip },
    xAxis: option.xAxis ? reAxis(option.xAxis) : option.xAxis,
    yAxis: option.yAxis ? reAxis(option.yAxis) : option.yAxis,
  }

  if (option.legend) {
    out.legend = { ...option.legend, textStyle: { ...option.legend.textStyle, color: themed.textStyle.color } }
    // A tile is a few hundred pixels wide. A legend that was fine beside a
    // full-size chart covers the plot here, and its labels get clipped - so at
    // this size the chart itself is worth more than its key.
    if (compact) out.legend = { ...out.legend, show: false }
  }

  // Titles were drawn inside the canvas; without one, that reserved space is
  // just a gap at the top of an already small tile.
  if (compact && out.grid && !Array.isArray(out.grid)) {
    out.grid = { ...out.grid, top: 12, left: 8, right: 12, bottom: 8, containLabel: true }
  }

  return out
}

export function isChartable(columns) {
  return columns.length >= 2
}

/**
 * All 26 chart types with metadata for the picker and AI integration.
 */
export const CHART_CATALOG = [
  {
    id: 'bar',
    label: 'Bar',
    group: 'Bar',
    icon: 'bar-chart-2',
    description: 'Vertical bars comparing values across categories',
    axes: { x: 'category', y: 'value', group: 'optional' },
    requires: { x: 'any', y: 'number', group: 'optional-category' },
    aiHint: 'Use GROUP BY with COUNT/SUM/AVG for Y axis',
  },
  {
    id: 'bar-horizontal',
    label: 'H-Bar',
    group: 'Bar',
    icon: 'bar-chart',
    description: 'Horizontal bars, good for long category labels',
    axes: { x: 'category', y: 'value' },
    requires: { x: 'any', y: 'number' },
    aiHint: 'Same as bar but rendered horizontally',
  },
  {
    id: 'bar-grouped',
    label: 'Grouped',
    group: 'Bar',
    icon: 'bar-chart-horizontal-big',
    description: 'Side-by-side bars per group',
    axes: { x: 'category', y: 'value', group: 'required' },
    requires: { x: 'any', y: 'number', group: 'category' },
    aiHint: 'Use GROUP BY category, series with SUM/COUNT',
  },
  {
    id: 'bar-stacked',
    label: 'Stacked',
    group: 'Bar',
    icon: 'layers',
    description: 'Stacked bars showing part-to-whole with absolute values',
    axes: { x: 'category', y: 'value', group: 'required' },
    requires: { x: 'any', y: 'number', group: 'category' },
    aiHint: 'Use GROUP BY category, series with SUM',
  },
  {
    id: 'bar-stacked-100',
    label: '100%',
    group: 'Bar',
    icon: 'bar-chart-big',
    description: '100% stacked bars showing proportions',
    axes: { x: 'category', y: 'value', group: 'required' },
    requires: { x: 'any', y: 'number', group: 'category' },
    aiHint: 'Use GROUP BY category, series; values normalized to 100%',
  },
  {
    id: 'bar-floating',
    label: 'Floating',
    group: 'Bar',
    icon: 'align-vertical-distribute-center',
    description: 'Bars from min to max (Gantt-style)',
    axes: { x: 'category', y: 'min', z: 'max' },
    requires: { x: 'any', y: 'number', z: 'number' },
    aiHint: 'Query min and max values per category',
  },
  {
    id: 'lollipop',
    label: 'Lollipop',
    group: 'Bar',
    icon: 'circle-dot',
    description: 'Dot-on-stick variant of a vertical bar chart',
    axes: { x: 'category', y: 'value' },
    requires: { x: 'any', y: 'number' },
    aiHint: 'Same data shape as a bar chart',
  },
  {
    id: 'lollipop-h',
    label: 'Lollipop H',
    group: 'Bar',
    icon: 'minus',
    description: 'Horizontal lollipop, good for ranked categories',
    axes: { x: 'category', y: 'value' },
    requires: { x: 'any', y: 'number' },
    aiHint: 'ORDER BY value DESC for ranking effect',
  },
  {
    id: 'combo',
    label: 'Combo',
    group: 'Line & Area',
    icon: 'layers',
    description: 'Bar + Line on dual Y axes for two different measures',
    axes: { x: 'category', y: 'bar value', z: 'line value' },
    requires: { x: 'any', y: 'number', z: 'number' },
    aiHint: 'Query two numeric columns per category; y_col=bar, z_col=line',
  },
  {
    id: 'line',
    label: 'Line',
    group: 'Line & Area',
    icon: 'line-chart',
    description: 'Line chart for trends over time or sequence',
    axes: { x: 'date or category', y: 'value', group: 'optional' },
    requires: { x: 'any', y: 'number', group: 'optional-category' },
    aiHint: 'ORDER BY date/sequence column for correct line order',
  },
  {
    id: 'area',
    label: 'Area',
    group: 'Line & Area',
    icon: 'area-chart',
    description: 'Filled area under a line',
    axes: { x: 'date or category', y: 'value', group: 'optional' },
    requires: { x: 'any', y: 'number', group: 'optional-category' },
    aiHint: 'ORDER BY date for time series',
  },
  {
    id: 'area-stacked',
    label: 'Stacked Area',
    group: 'Line & Area',
    icon: 'layers-2',
    description: 'Multiple areas stacked on top of each other',
    axes: { x: 'date or category', y: 'value', group: 'required' },
    requires: { x: 'any', y: 'number', group: 'category' },
    aiHint: 'GROUP BY date, series column with SUM',
  },
  {
    id: 'scatter',
    label: 'Scatter',
    group: 'Correlation',
    icon: 'scatter-chart',
    description: 'Points on X/Y axes to show correlation',
    axes: { x: 'number', y: 'number' },
    requires: { x: 'number', y: 'number' },
    aiHint: 'Both X and Y must be numeric columns',
  },
  {
    id: 'bubble',
    label: 'Bubble',
    group: 'Correlation',
    icon: 'circle',
    description: 'Scatter with bubble size as a third dimension',
    axes: { x: 'number', y: 'number', z: 'size' },
    requires: { x: 'number', y: 'number', z: 'number' },
    aiHint: 'Three numeric columns: X position, Y position, bubble size',
  },
  {
    id: 'heatmap',
    label: 'Heatmap',
    group: 'Correlation',
    icon: 'grid-3x3',
    description: 'Color-encoded grid for dense comparisons',
    axes: { x: 'category', y: 'value', group: 'Y category' },
    requires: { x: 'any', y: 'number', group: 'category' },
    aiHint: 'Use GROUP BY row_label, col_label with AVG/SUM value',
  },
  {
    id: 'radar',
    label: 'Radar',
    group: 'Correlation',
    icon: 'radar',
    description: 'Spider chart comparing multiple numeric dimensions',
    axes: { x: 'category', y: 'multiple numeric columns' },
    requires: { x: 'any', y: 'number' },
    aiHint: 'Each row is one entity; numeric columns become radar axes',
  },
  {
    id: 'histogram',
    label: 'Histogram',
    group: 'Distribution',
    icon: 'chart-no-axes-combined',
    description: 'Frequency distribution of a numeric column',
    axes: { x: 'numeric value', y: 'auto-count' },
    requires: { x: 'number', y: 'number' },
    aiHint: 'Pass raw numeric values; bins are computed automatically',
  },
  {
    id: 'box-plot',
    label: 'Box Plot',
    group: 'Distribution',
    icon: 'box',
    description: 'Shows median, quartiles, and outliers per group',
    axes: { x: 'group', y: 'numeric values' },
    requires: { x: 'any', y: 'number', group: 'optional-category' },
    aiHint: 'Pass raw rows; quartiles are computed from the data',
  },
  {
    id: 'pie',
    label: 'Pie',
    group: 'Part-to-Whole',
    icon: 'pie-chart',
    description: 'Classic pie chart for proportions',
    axes: { x: 'label', y: 'value' },
    requires: { x: 'any', y: 'number' },
    aiHint: 'Use GROUP BY label with SUM/COUNT for values',
  },
  {
    id: 'donut',
    label: 'Donut',
    group: 'Part-to-Whole',
    icon: 'donut',
    description: 'Donut chart with center hole',
    axes: { x: 'label', y: 'value' },
    requires: { x: 'any', y: 'number' },
    aiHint: 'Same as pie; center hole is decorative',
  },
  {
    id: 'gauge',
    label: 'Gauge',
    group: 'Part-to-Whole',
    icon: 'gauge',
    description: 'Speedometer-style gauge for a single KPI',
    axes: { x: 'label', y: 'value (0-max)' },
    requires: { x: 'any', y: 'number' },
    aiHint: 'First row\'s Y value is displayed on the gauge',
  },
  {
    id: 'funnel',
    label: 'Funnel',
    group: 'Part-to-Whole',
    icon: 'filter',
    description: 'Ordered funnel from largest to smallest',
    axes: { x: 'stage label', y: 'value' },
    requires: { x: 'any', y: 'number' },
    aiHint: 'Query stage names and counts; sorted by value descending automatically',
  },
  {
    id: 'bullet',
    label: 'Bullet',
    group: 'Part-to-Whole',
    icon: 'target',
    description: 'Bar with a target line for actual-vs-target comparison',
    axes: { x: 'category', y: 'actual', z: 'target' },
    requires: { x: 'any', y: 'number', z: 'number' },
    aiHint: 'Query category, actual_value, target_value columns',
  },
  {
    id: 'treemap',
    label: 'Treemap',
    group: 'Hierarchical',
    icon: 'layout-grid',
    description: 'Nested rectangles sized by value',
    axes: { x: 'name', y: 'value', group: 'parent (optional)' },
    requires: { x: 'any', y: 'number' },
    aiHint: 'Query name and numeric size/value columns',
  },
  {
    id: 'tree',
    label: 'Tree',
    group: 'Hierarchical',
    icon: 'network',
    description: 'Hierarchical tree diagram from parent-child data',
    axes: { x: 'name', group: 'parent' },
    requires: { x: 'any', group: 'category' },
    aiHint: 'Query id/name and parent_id/parent_name for adjacency list',
  },
  {
    id: 'circle-pack',
    label: 'Circle Pack',
    group: 'Hierarchical',
    icon: 'circles',
    description: 'Packed circles sized by value',
    axes: { x: 'name', y: 'value' },
    requires: { x: 'any', y: 'number' },
    aiHint: 'Query name and numeric size columns; nesting is optional',
  },
  {
    id: 'sankey',
    label: 'Sankey',
    group: 'Flow',
    icon: 'git-merge',
    description: 'Flow diagram between nodes with proportional links',
    axes: { x: 'source', group: 'target', y: 'value' },
    requires: { x: 'any', group: 'category', y: 'number' },
    aiHint: 'Query source, target, and flow_value columns',
  },
  {
    id: 'word-cloud',
    label: 'Word Cloud',
    group: 'Other',
    icon: 'type',
    description: 'Word cloud sized by frequency or weight',
    axes: { x: 'word/term', y: 'count/weight' },
    requires: { x: 'any', y: 'number' },
    aiHint: 'Query term and COUNT(*) or weight columns',
  },
  {
    id: 'dendrogram',
    label: 'Dendrogram',
    group: 'Hierarchical',
    icon: 'git-fork',
    description: 'Radial tree dendrogram from parent-child data',
    axes: { x: 'name', group: 'parent' },
    requires: { x: 'any', group: 'category' },
    aiHint: 'Query id/name and parent_id/parent_name for adjacency list',
  },
  {
    id: 'choropleth',
    label: 'Choropleth',
    group: 'Geographic',
    icon: 'map',
    description: 'World map colored by numeric value per country',
    axes: { x: 'country name', y: 'value' },
    requires: { x: 'any', y: 'number' },
    aiHint: 'Query country name (English) and a numeric metric column',
  },
  {
    id: 'meter',
    label: 'Meter',
    group: 'Part-to-Whole',
    icon: 'gauge',
    description: 'Proportional meter showing segments of a total',
    axes: { x: 'segment label', y: 'value', z: 'total (optional)' },
    requires: { x: 'any', y: 'number' },
    aiHint: 'Query segment name and numeric value columns; optionally a total column',
  },
  {
    id: 'waterfall',
    label: 'Waterfall',
    group: 'Bar',
    icon: 'trending-up',
    description: 'Incremental bar chart showing how values add up (gains in green, losses in red)',
    axes: { x: 'category', y: 'delta value' },
    requires: { x: 'any', y: 'number' },
    aiHint: 'Values represent increments (positive or negative); sorted by sequence',
  },
  {
    id: 'pareto',
    label: 'Pareto',
    group: 'Bar',
    icon: 'chart-no-axes-combined',
    description: 'Sorted bar + cumulative % line, identifies top contributors (80/20 rule)',
    axes: { x: 'category', y: 'value' },
    requires: { x: 'any', y: 'number' },
    aiHint: 'Query category and count/value; chart auto-sorts descending and adds cumulative line',
  },
  {
    id: 'step',
    label: 'Step',
    group: 'Line & Area',
    icon: 'step-forward',
    description: 'Step-interpolated line: ideal for inventory, prices, or status changes',
    axes: { x: 'date or category', y: 'value', group: 'optional' },
    requires: { x: 'any', y: 'number', group: 'optional-category' },
    aiHint: 'Same data as a line chart; ORDER BY date for correct sequence',
  },
  {
    id: 'sunburst',
    label: 'Sunburst',
    group: 'Hierarchical',
    icon: 'sun',
    description: 'Radial hierarchical chart showing part-to-whole at multiple levels',
    axes: { x: 'name', y: 'value', group: 'parent (optional)' },
    requires: { x: 'any', y: 'number' },
    aiHint: 'Query name, value, and optional parent_name for multi-level hierarchy',
  },
]
