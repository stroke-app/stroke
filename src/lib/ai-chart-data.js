/**
 * The rows of a chart the AI asked for, whatever shape it sent them in.
 *
 * render_chart wants `data` as row objects - the shape execute_sql hands back -
 * but models send what they have: the array as a JSON string, execute_sql's
 * whole `{ columns, rows }` result, rows as arrays (with a header row or
 * without), or one array per column. A string passed the "has data" check on
 * its length and then crashed the chart view on `data.find`. Every shape a
 * chart can be drawn from comes back as row objects; anything else is an
 * empty list, which the caller reports as "no data" instead of crashing.
 */

/** @param {unknown} v */
const isRowObject = (v) => !!v && typeof v === 'object' && !Array.isArray(v)

/** Column names a spec may carry, as plain strings. @param {unknown} columns */
function columnNames(columns) {
  if (!Array.isArray(columns)) return null
  const names = columns.map((c) => (typeof c === 'string' ? c : isRowObject(c) ? /** @type {any} */ (c).name : null))
  return names.every((n) => typeof n === 'string' && n) ? /** @type {string[]} */ (names) : null
}

/**
 * @param {unknown} data the spec's `data`, as the model sent it
 * @param {{ columns?: unknown, x_col?: string, y_col?: string, z_col?: string }} [spec]
 * @returns {Record<string, unknown>[]}
 */
export function chartRows(data, spec = {}) {
  let d = data
  if (typeof d === 'string') {
    try { d = JSON.parse(d) } catch { return [] }
  }
  if (Array.isArray(d)) {
    if (!d.length) return []
    if (d.every(isRowObject)) return /** @type {Record<string, unknown>[]} */ (d)
    if (!d.every(Array.isArray)) return []
    /** @type {unknown[][]} */
    let body = d
    let header = columnNames(spec.columns)
    // A first row of names over rows that are not all names is a header.
    if (!header && d.length > 1 && d[0].every((v) => typeof v === 'string') && !d[1].every((v) => typeof v === 'string')) {
      header = /** @type {string[]} */ (d[0])
      body = d.slice(1)
    }
    const named = [spec.x_col, spec.y_col, spec.z_col]
    const keys = header ?? d[0].map((_, i) => named[i] || `col${i + 1}`)
    return body.map((row) => Object.fromEntries(keys.map((k, i) => [String(k), row[i]])))
  }
  if (isRowObject(d)) {
    const obj = /** @type {Record<string, unknown>} */ (d)
    // execute_sql's own result: { columns, rows }.
    if (Array.isArray(obj.rows)) return chartRows(obj.rows, { ...spec, columns: obj.columns ?? spec.columns })
    if (Array.isArray(obj.data)) return chartRows(obj.data, spec)
    // One array per column: { month: [...], total: [...] }.
    const entries = Object.entries(obj)
    if (entries.length && entries.every(([, v]) => Array.isArray(v))) {
      const n = Math.max(...entries.map(([, v]) => /** @type {unknown[]} */ (v).length))
      return Array.from({ length: n }, (_, i) => Object.fromEntries(entries.map(([k, v]) => [k, /** @type {unknown[]} */ (v)[i]])))
    }
  }
  return []
}
