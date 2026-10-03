/**
 * Sort a console result held in this window (Settings → Database → Stream
 * query results off) by one column, the way the result store sorts in the
 * backend (result_store.rs): by the column's database type. Numbers by value,
 * timestamps, dates and times by instant, booleans false first, everything else
 * as text compared naturally. NULLs last in either direction; equal values keep
 * the order the rows arrived in.
 *
 * The comparator this replaces stringified and parsed both cells on every
 * comparison (about 100M of them for a 5M-row result), and compared temporal
 * text with numeric collation, which put `10:00:00.500` before
 * `10:00:00.123456`: it read the fractions as the whole numbers 500 and 123456.
 * Here each row's key is computed once and an index is sorted, not the rows.
 */

/** @typedef {'number' | 'decimal' | 'temporal' | 'boolean' | 'text'} KeyKind */

/** @param {string | undefined} dataType @returns {KeyKind} */
export function keyKindOf(dataType) {
  // `_int4` and friends are arrays: they sort as text.
  const t = String(dataType ?? '').toLowerCase().replace(/\(.*$/, '').trim()
  if (/^(int2|int4|int8|oid|smallint|integer|bigint|tinyint|mediumint|int|year|float4|float8|real|double|double precision|float)$/.test(t)) return 'number'
  if (/^(numeric|decimal|money)$/.test(t)) return 'decimal'
  if (/^(timestamptz|timestamp|date|time|datetime)$/.test(t)) return 'temporal'
  if (/^(bool|boolean)$/.test(t)) return 'boolean'
  return 'text'
}

const TEMPORAL =
  /^(-?\d{4,})-(\d{2})-(\d{2})(?:[ T](\d{2}):(\d{2}):(\d{2})(?:\.(\d{1,9}))?)?\s*(UTC|Z|[+-]\d{2}(?::?\d{2})?)?$/i
const TIME_OF_DAY = /^(\d{2}):(\d{2}):(\d{2})(?:\.(\d{1,9}))?$/

/**
 * Microseconds since the epoch (or since midnight for a time of day), or NaN.
 * @param {unknown} v
 */
export function temporalMicros(v) {
  if (typeof v !== 'string') return NaN
  const s = v.trim()
  let m = TEMPORAL.exec(s)
  if (m) {
    const [, y, mo, d, h = '0', mi = '0', se = '0', frac = '', zone] = m
    const ms = Date.UTC(+y, +mo - 1, +d, +h, +mi, +se)
    let micros = ms * 1000 + Number((frac + '000000').slice(0, 6))
    if (zone && !/^(utc|z)$/i.test(zone)) {
      const sign = zone[0] === '-' ? -1 : 1
      const digits = zone.slice(1).replace(':', '')
      const offsetMin = Number(digits.slice(0, 2)) * 60 + Number(digits.slice(2, 4) || 0)
      micros -= sign * offsetMin * 60_000_000
    }
    return micros
  }
  m = TIME_OF_DAY.exec(s)
  if (m) {
    const [, h, mi, se, frac = ''] = m
    return ((+h * 60 + +mi) * 60 + +se) * 1_000_000 + Number((frac + '000000').slice(0, 6))
  }
  return NaN
}

/**
 * Exact order of two decimal texts (`numeric` comes as text so no digits are
 * lost); used where their float keys tie.
 * @param {string} a @param {string} b
 */
export function compareDecimalText(a, b) {
  const parse = (/** @type {string} */ s) => {
    const t = s.trim()
    const neg = t.startsWith('-')
    const body = t.replace(/^[-+]/, '')
    const [int = '', frac = ''] = body.split('.')
    return { neg, int: int.replace(/^0+/, ''), frac: frac.replace(/0+$/, '') }
  }
  const x = parse(a)
  const y = parse(b)
  const zx = !x.int && !x.frac
  const zy = !y.int && !y.frac
  const sx = zx ? 0 : x.neg ? -1 : 1
  const sy = zy ? 0 : y.neg ? -1 : 1
  if (sx !== sy) return sx - sy
  if (sx === 0) return 0
  let mag = x.int.length - y.int.length
  if (!mag) mag = x.int < y.int ? -1 : x.int > y.int ? 1 : 0
  if (!mag) mag = x.frac < y.frac ? -1 : x.frac > y.frac ? 1 : 0
  return sx < 0 ? -mag : mag
}

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' })

/** @param {unknown} v */
function cellText(v) {
  if (v === null || v === undefined) return ''
  if (typeof v === 'object') {
    try { return JSON.stringify(v) } catch { return String(v) }
  }
  return String(v)
}

/**
 * The rows in sorted order, as a new array (the input is not touched).
 * @param {any[][]} rows
 * @param {number} col column index
 * @param {string | undefined} dataType the column's database type
 * @param {boolean} desc
 * @returns {any[][]}
 */
export function sortRowsByColumn(rows, col, dataType, desc) {
  const n = rows.length
  const kind = keyKindOf(dataType)
  const dir = desc ? -1 : 1
  const isNull = new Uint8Array(n)
  /** @type {Float64Array | null} */
  let nums = null
  /** @type {string[] | null} */
  let texts = null
  if (kind === 'text') {
    texts = new Array(n)
    for (let i = 0; i < n; i++) {
      const v = rows[i]?.[col]
      if (v === null || v === undefined) isNull[i] = 1
      else texts[i] = cellText(v)
    }
  } else {
    nums = new Float64Array(n)
    for (let i = 0; i < n; i++) {
      const v = rows[i]?.[col]
      if (v === null || v === undefined) { isNull[i] = 1; continue }
      let k = NaN
      if (kind === 'temporal') k = temporalMicros(v)
      else if (kind === 'boolean') k = v === true || v === 't' || v === 'true' ? 1 : v === false || v === 'f' || v === 'false' ? 0 : NaN
      else if (typeof v === 'number') k = v
      else if (v === 'Infinity') k = Infinity
      else if (v === '-Infinity') k = -Infinity
      else k = Number(v)
      // A value that does not read as its type (or NaN) sorts after the rest.
      nums[i] = k
    }
  }
  const order = new Uint32Array(n)
  for (let i = 0; i < n; i++) order[i] = i
  order.sort((a, b) => {
    const na = isNull[a]
    const nb = isNull[b]
    if (na || nb) return na && nb ? a - b : na ? 1 : -1
    let c = 0
    if (nums) {
      const x = nums[a]
      const y = nums[b]
      const xn = Number.isNaN(x)
      const yn = Number.isNaN(y)
      if (xn || yn) c = xn && yn ? 0 : xn ? 1 : -1
      else if (x !== y) c = x < y ? -1 : 1
      else if (kind === 'decimal') c = compareDecimalText(String(rows[a][col]), String(rows[b][col]))
      c *= xn || yn ? 1 : dir
    } else {
      c = dir * collator.compare(/** @type {string[]} */ (texts)[a], /** @type {string[]} */ (texts)[b])
    }
    return c || a - b
  })
  const out = new Array(n)
  for (let i = 0; i < n; i++) out[i] = rows[order[i]]
  return out
}
