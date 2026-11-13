// Byte sizes - `file_size = 4404019` reads `4.2 MB`.
//
// Only numeric columns whose name says it holds bytes; a `_kb` / `_mb` suffix
// is read as that unit. Binary (KiB = 1024) by default, decimal on request.
import { normalizeColumnType } from '$lib/cell-value.js'

const NUMERIC_RE = /(int|numeric|decimal|real|double|float|number|serial)/i
const NAME_RE = /(bytes?|size|filesize|file_size|byte_size|content_length|length_bytes|storage|disk|_kb|_mb|_gb|kilobytes|megabytes|memory|mem_|rss|heap|bandwidth|upload|download|payload_size|object_size)/i
const NOT_RE = /(^|_)(id|count|num|number|qty|quantity|page_size|font_size|batch_size|pool_size|sample_size|shoe_size|size_id)$/i

const defaultConfig = { units: 'binary' } // 'binary' | 'decimal'

/** @param {number} bytes @param {boolean} binary */
export function formatBytes(bytes, binary = true) {
  const base = binary ? 1024 : 1000
  const units = binary ? ['B', 'KiB', 'MiB', 'GiB', 'TiB', 'PiB'] : ['B', 'KB', 'MB', 'GB', 'TB', 'PB']
  const sign = bytes < 0 ? '-' : ''
  let n = Math.abs(bytes)
  let i = 0
  while (n >= base && i < units.length - 1) { n /= base; i++ }
  const digits = i === 0 ? 0 : n < 10 ? 1 : 0
  return `${sign}${n.toFixed(digits)} ${units[i]}`
}

export const byteSize = {
  id: 'byte-size',
  name: 'Byte Sizes',
  description: 'Show byte counts as KB, MB and GB in size columns (file_size, bytes, storage, ...).',
  kind: 'formatter',
  defaultConfig,

  /** @param {string} type @param {string} name */
  appliesTo(type, name) {
    if (!NUMERIC_RE.test(normalizeColumnType(type))) return false
    if (!name) return true
    return NAME_RE.test(name) && !NOT_RE.test(name)
  },

  /** @param {unknown} value @param {string} _type @param {Record<string, unknown>} config @param {string} name */
  format(value, _type, config, name) {
    const n = typeof value === 'number' ? value : Number(value)
    if (!Number.isFinite(n)) return null
    const scale = /_kb$|kilobytes/i.test(name ?? '') ? 1024 : /_mb$|megabytes/i.test(name ?? '') ? 1024 ** 2 : /_gb$/i.test(name ?? '') ? 1024 ** 3 : 1
    const binary = (config?.units ?? defaultConfig.units) !== 'decimal'
    return { display: formatBytes(n * scale, binary), title: `${n.toLocaleString()}${scale === 1 ? ' bytes' : ''}` }
  },
}
