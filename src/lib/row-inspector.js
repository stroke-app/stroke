import { oversizeCellInfo, oversizeCellText } from './cell-value.js'
import { layoutJson } from './json-layout.js'

/**
 * @param {unknown} value
 * @returns {unknown}
 */
export function normalizeCellValue(value) {
  if (value === undefined) return null
  return value
}

/**
 * `v` with oversize sentinels swapped for their marker text, so copied rows and
 * detail views show the cut instead of the wrapper. Sentinels only ever sit at
 * a cell (or a record's cell), so only the top two levels are looked at.
 * @param {unknown} v @param {number} [depth]
 */
function withoutOversize(v, depth = 0) {
  const over = oversizeCellInfo(v)
  if (over) return oversizeCellText(over)
  if (depth >= 1 || v === null || typeof v !== 'object' || Array.isArray(v)) return v
  /** @type {Record<string, unknown>} */
  const out = {}
  for (const [k, x] of Object.entries(/** @type {Record<string, unknown>} */ (v))) out[k] = withoutOversize(x, depth + 1)
  return out
}

/**
 * JSON for reading: short lines at any size (see json-layout.js). Indented
 * stringify put a 263,000-integer byte array on 263,000 lines.
 * @param {unknown} v
 */
function readableJson(v) {
  return layoutJson(withoutOversize(v))
}

/**
 * @param {unknown} value
 */
export function formatNormalValue(value) {
  const v = normalizeCellValue(value)
  if (v === null) return 'NULL'
  if (typeof v === 'string') return v
  if (typeof v === 'number' || typeof v === 'boolean') return String(v)
  if (typeof v === 'object') {
    const over = oversizeCellInfo(v)
    if (over) return oversizeCellText(over)
    try {
      return readableJson(v)
    } catch {
      return String(v)
    }
  }
  return String(v)
}

/**
 * @param {unknown} value
 */
export function formatJsonValue(value) {
  try {
    return readableJson(normalizeCellValue(value))
  } catch {
    return 'null'
  }
}

/**
 * @param {Record<string, unknown>} record
 */
export function formatNormalRecord(record) {
  return Object.entries(record)
    .map(([key, value]) => `${key}: ${formatNormalValue(value)}`)
    .join('\n')
}

/**
 * @param {Record<string, unknown>[]} records
 */
export function formatNormalRecords(records) {
  return records.map((r) => formatNormalRecord(r)).join('\n\n')
}

/**
 * @param {{ name: string, dataType?: string, data_type?: string }[]} columns
 * @param {unknown[][]} row
 * @param {Set<string>} [hidden] Column names to omit (e.g. columns hidden in the UI).
 */
export function rowToRecord(columns, row, hidden) {
  /** @type {Record<string, unknown>} */
  const record = {}
  columns.forEach((col, i) => {
    if (hidden && hidden.has(col.name)) return
    record[col.name] = normalizeCellValue(row[i])
  })
  return record
}

/**
 * @param {{ name: string }[]} columns
 * @param {unknown[][]} rows
 * @param {number[]} indices
 */
export function rowsToJsonPayload(columns, rows, indices) {
  if (indices.length === 1) {
    return rowToRecord(columns, rows[indices[0]])
  }
  return indices.map((idx) => rowToRecord(columns, rows[idx]))
}
