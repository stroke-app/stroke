// Live dashboard cards: a saved chart's query run again, turned into a chart
// option, or into one big number when the result is a single value.

import { buildOption } from '$lib/chart-utils.js'

/**
 * One number to show large: a single row whose only numeric cell is the value
 * (an optional text cell beside it becomes the label).
 * @param {{ name: string }[]} columns @param {unknown[][]} rows
 * @returns {{ value: number, label: string } | null}
 */
export function kpiOf(columns, rows) {
  if (rows.length !== 1 || columns.length === 0 || columns.length > 2) return null
  const row = rows[0]
  const nums = row.map((v, i) => ({ v: typeof v === 'number' ? v : typeof v === 'string' && v.trim() !== '' && Number.isFinite(Number(v)) ? Number(v) : null, i })).filter((x) => x.v !== null)
  if (nums.length !== 1) return null
  const other = columns.length === 2 ? row[1 - nums[0].i] : null
  return { value: /** @type {number} */ (nums[0].v), label: other != null ? String(other) : columns[nums[0].i].name }
}

/** A number for a big readout: grouped, at most two decimals. @param {number} n */
export function formatKpi(n) {
  const abs = Math.abs(n)
  if (abs >= 1e9) return `${(n / 1e9).toFixed(abs >= 1e10 ? 0 : 1)}B`
  if (abs >= 1e6) return `${(n / 1e6).toFixed(abs >= 1e7 ? 0 : 1)}M`
  return n.toLocaleString('en-US', { maximumFractionDigits: Number.isInteger(n) ? 0 : 2 })
}

/** "just now", "3m ago". @param {number} at @param {number} [now] */
export function agoText(at, now = Date.now()) {
  const s = Math.max(0, Math.round((now - at) / 1000))
  if (s < 10) return 'just now'
  if (s < 60) return `${s}s ago`
  if (s < 3600) return `${Math.floor(s / 60)}m ago`
  return `${Math.floor(s / 3600)}h ago`
}

/**
 * Run tasks with at most `limit` at once, so a dashboard of twenty charts
 * doesn't put twenty queries on the database together.
 * @param {number} limit
 */
export function createQueue(limit = 2) {
  let running = 0
  /** @type {(() => void)[]} */
  const waiting = []
  /** @template T @param {() => Promise<T>} task @returns {Promise<T>} */
  return function run(task) {
    return new Promise((resolve, reject) => {
      const go = () => {
        running++
        task().then(resolve, reject).finally(() => {
          running--
          waiting.shift()?.()
        })
      }
      if (running < limit) go()
      else waiting.push(go)
    })
  }
}

/**
 * A result as what a card draws.
 * @param {import('$lib/stores/saved-charts.js').SavedChart} chart
 * @param {{ columns?: any[], rows?: unknown[][] }} r
 * @param {boolean} isDark
 */
export function cardView(chart, r, isDark) {
  const columns = (r?.columns ?? []).map((c) => ({ name: c.name ?? String(c), type: c.dataType ?? c.data_type ?? c.type }))
  const rows = r?.rows ?? []
  const kpi = kpiOf(columns, rows)
  if (kpi) return { kpi, option: null, rows: rows.length }
  const option = buildOption({
    type: chart.config.type,
    columns,
    rows: rows.slice(0, 2000),
    xCol: chart.config.xCol,
    yCol: chart.config.yCol,
    zCol: chart.config.zCol,
    groupCol: chart.config.groupCol,
    isDark,
    noTitle: true,
  })
  return { kpi: null, option, rows: rows.length }
}
