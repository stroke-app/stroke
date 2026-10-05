/**
 * The note the SQL editor writes after a statement that ran: how long it took
 * and what it returned, `478ms · 12 rows` or `35ms · 3 affected`.
 */
import { formatCompactCount } from './table-list.js'

/** @param {number | null | undefined} ms */
export function formatDuration(ms) {
  const n = Number(ms)
  if (ms == null || !Number.isFinite(n) || n < 0) return ''
  if (n < 1000) return `${Math.round(n)}ms`
  if (n < 60_000) return `${(n / 1000).toFixed(n < 10_000 ? 2 : 1)}s`
  const m = Math.floor(n / 60_000)
  return `${m}m ${Math.round((n % 60_000) / 1000)}s`
}

/**
 * @param {{ ms?: number | null, rows?: number | null, affected?: number | null }} run
 *   `rows` for a statement that returned a result, `affected` for one that
 *   changed rows; neither when the engine said nothing.
 */
export function formatRunInfo({ ms, rows, affected }) {
  const parts = []
  const d = formatDuration(ms)
  if (d) parts.push(d)
  if (typeof rows === 'number' && rows >= 0) parts.push(`${formatCompactCount(rows)} ${rows === 1 ? 'row' : 'rows'}`)
  else if (typeof affected === 'number' && affected >= 0) parts.push(`${formatCompactCount(affected)} affected`)
  return parts.join(' · ')
}
