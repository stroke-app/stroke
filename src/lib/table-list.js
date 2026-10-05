/**
 * Compact count: 999, 1.5k, 10k, 1.2M
 * @param {number | string | null | undefined} count
 */
export function formatCompactCount(count) {
  // null = count still being resolved in the background (see normalizeTableRowCount)
  if (count === null) return '…'
  const n = Number(count)
  if (!Number.isFinite(n)) return '—'

  const abs = Math.abs(n)
  const sign = n < 0 ? '-' : ''

  if (abs < 1000) return sign + abs.toLocaleString('en-US')

  /**
   * @param {number} value
   * @param {number} divisor
   * @param {string} suffix
   */
  function withSuffix(value, divisor, suffix) {
    const v = value / divisor
    if (v >= 100) return sign + Math.round(v) + suffix
    if (v >= 10) return sign + Math.round(v) + suffix
    const rounded = Math.round(v * 10) / 10
    const text = Number.isInteger(rounded) ? String(rounded) : rounded.toFixed(1)
    return sign + text + suffix
  }

  // The unit follows the rounded value: 999,999 reads 1M, not 1000k.
  if (abs < 999_500) return withSuffix(abs, 1000, 'k')
  if (abs < 999_500_000) return withSuffix(abs, 1_000_000, 'M')
  return withSuffix(abs, 1_000_000_000, 'B')
}

/** @param {number | string | null | undefined} count */
export function formatTableRowCount(count) {
  return formatCompactCount(count)
}

/**
 * Backend reports -1 when a table's row count wasn't resolved yet (it arrives
 * later via getTableRowCounts). Map that to null so the UI can tell
 * "still counting" apart from a genuinely empty table.
 * @param {number | string | null | undefined} count
 * @returns {number | null}
 */
export function normalizeTableRowCount(count) {
  const n = Number(count)
  if (!Number.isFinite(n) || n < 0) return null
  return n
}

/**
 * Oldest first by when a table was created. A real creation time (MySQL,
 * SQL Server) wins; engines without one hand over a creation order instead
 * (Postgres OID, SQLite catalog rowid, DuckDB object id). Tables with neither
 * go last, and the name breaks ties so the order is stable.
 * @param {{ name: string, createdAt?: string | null, createOrder?: number | null }} a
 * @param {{ name: string, createdAt?: string | null, createOrder?: number | null }} b
 */
export function compareCreated(a, b) {
  const at = a.createdAt ?? null, bt = b.createdAt ?? null
  if (at !== null || bt !== null) {
    if (at === null) return 1
    if (bt === null) return -1
    if (at !== bt) return at < bt ? -1 : 1
  } else {
    const ao = a.createOrder ?? null, bo = b.createOrder ?? null
    if (ao === null && bo !== null) return 1
    if (bo === null && ao !== null) return -1
    if (ao !== null && bo !== null && ao !== bo) return ao - bo
  }
  return a.name.localeCompare(b.name)
}
