// Row-to-row change - in a time series, each value shows how it moved from the
// row above: `▲ 12%`, `▼ 3.4%`, `=`. Green up and red down by default; flip it
// for columns where lower is better (latency, errors, cost).
//
// Uses the row drawn above (ctx.prev), so it reads in the grid's current sort.
// Numeric columns only, never ids or keys.
import { normalizeColumnType } from '$lib/cell-value.js'

const NUMERIC_RE = /(int|numeric|decimal|real|double|float|money|number)/i
const NOT_RE = /(^|_)(id|uuid|key|pk|fk|code|year|month|day|hour|minute|second|zip|postal|phone|version|port|lat|lng|lon|latitude|longitude)$|_id$|^id$/i
const LOWER_IS_BETTER = /(latency|duration|elapsed|error|errors|fail|failures|cost|spend|churn|bounce|lag|wait|queue|p9[59]|p50|ms$|_ms$|time_ms|response_time)/i

const defaultConfig = { mode: 'percent' } // 'percent' | 'absolute'

/** @param {number} n */
function compact(n) {
  const a = Math.abs(n)
  if (a >= 1e9) return (n / 1e9).toFixed(1) + 'B'
  if (a >= 1e6) return (n / 1e6).toFixed(1) + 'M'
  if (a >= 1e4) return (n / 1e3).toFixed(1) + 'k'
  return Number.isInteger(n) ? String(n) : n.toFixed(Math.abs(n) < 1 ? 3 : 2)
}

export const rowChange = {
  id: 'row-change',
  name: 'Row-to-Row Change',
  description: 'Show how each number moved from the row above: ▲ 12%, ▼ 3%. For time series and logs.',
  kind: 'formatter',
  defaultConfig,

  /** @param {string} type @param {string} name */
  appliesTo(type, name) {
    return NUMERIC_RE.test(normalizeColumnType(type)) && !(name && NOT_RE.test(name))
  },

  /**
   * @param {unknown} value @param {string} _type @param {Record<string, unknown>} config @param {string} name
   * @param {{ prev?: unknown[], colIdx?: number } | undefined} ctx
   */
  format(value, _type, config, name, ctx) {
    if (!ctx?.prev || ctx.colIdx == null) return null
    const cur = typeof value === 'number' ? value : Number(value)
    const before = ctx.prev[ctx.colIdx]
    if (before == null) return null
    const prev = typeof before === 'number' ? before : Number(before)
    if (!Number.isFinite(cur) || !Number.isFinite(prev)) return null
    const diff = cur - prev
    if (diff === 0) return { trail: { text: '=' } }
    const up = diff > 0
    const good = LOWER_IS_BETTER.test(name ?? '') ? !up : up
    const arrow = up ? '▲' : '▼'
    const mode = config?.mode ?? defaultConfig.mode
    const pct = prev !== 0 ? (diff / Math.abs(prev)) * 100 : null
    const text = mode === 'absolute' || pct === null
      ? `${arrow} ${compact(Math.abs(diff))}`
      : `${arrow} ${Math.abs(pct) >= 100 ? Math.round(Math.abs(pct)) : Math.abs(pct).toFixed(1)}%`
    return {
      trail: { text, color: good ? 'success' : 'destructive' },
      title: `${up ? '+' : '−'}${compact(Math.abs(diff))} from the row above (${compact(prev)})${pct !== null ? `, ${up ? '+' : '−'}${Math.abs(pct).toFixed(1)}%` : ''}`,
    }
  },
}
