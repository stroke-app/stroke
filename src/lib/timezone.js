/**
 * Showing absolute timestamps in a chosen zone (Settings > General > Timezone).
 *
 * Only values that name their instant are converted: Postgres `timestamptz`
 * arrives as `2026-06-05 10:07:11.618398 UTC`, other engines as an ISO string
 * with `Z` or an offset. A plain `timestamp` has no zone to convert from and
 * is shown as stored. The result keeps every fractional digit (a Date holds
 * only milliseconds) and says its offset: `2026-06-05 15:52:11.618398 +05:45`.
 */

/** `YYYY-MM-DD HH:MM:SS[.fff…]` then ` UTC`, `Z` or `±HH[:]MM`. */
const INSTANT_RE = /^(\d{4})-(\d{2})-(\d{2})[ T](\d{2}):(\d{2}):(\d{2})(\.\d{1,9})?(?: ?UTC|Z|([+-])(\d{2}):?(\d{2}))$/

/** The zone a setting means: 'auto' is this computer's. @param {string} setting */
export function resolveZone(setting) {
  if (!setting || setting === 'utc') return 'UTC'
  if (setting === 'auto') {
    try { return Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC' } catch { return 'UTC' }
  }
  return setting
}

/** Every zone the runtime knows, for the picker. */
export function listTimezones() {
  try {
    const all = /** @type {any} */ (Intl).supportedValuesOf?.('timeZone')
    if (Array.isArray(all) && all.length) return all
  } catch { /* older runtime */ }
  return ['UTC', 'America/New_York', 'America/Los_Angeles', 'Europe/London', 'Europe/Berlin', 'Asia/Kolkata', 'Asia/Kathmandu', 'Asia/Tokyo', 'Australia/Sydney']
}

/** @type {Map<string, Intl.DateTimeFormat>} */
const formatters = new Map()
/** @param {string} zone */
function formatterFor(zone) {
  let f = formatters.get(zone)
  if (!f) {
    f = new Intl.DateTimeFormat('en-US', {
      timeZone: zone, hourCycle: 'h23',
      year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', second: '2-digit',
    })
    formatters.set(zone, f)
  }
  return f
}

/** `+05:45` for an offset in minutes. @param {number} min */
const offsetText = (min) => `${min < 0 ? '-' : '+'}${String(Math.floor(Math.abs(min) / 60)).padStart(2, '0')}:${String(Math.abs(min) % 60).padStart(2, '0')}`

/** Converted values, by zone and text: a grid redraws the same cells every frame. */
const cache = new Map()

/**
 * `text` shown in `zone`, or null when it is not an absolute timestamp (or the
 * zone is unknown). In UTC it is returned unchanged: that is how it arrives.
 * @param {string} text @param {string} zone an IANA name, from resolveZone
 * @returns {string | null}
 */
export function instantIn(text, zone) {
  if (typeof text !== 'string' || text.length < 19 || text.length > 40) return null
  const key = `${zone}\u0000${text}`
  const hit = cache.get(key)
  if (hit !== undefined) return hit
  const m = INSTANT_RE.exec(text)
  /** @type {string | null} */
  let out = null
  if (m) {
    if (zone === 'UTC') {
      out = text
    } else {
      const [, y, mo, d, h, mi, sec, frac = '', sign, oh, om] = m
      const offsetMin = sign ? (sign === '-' ? -1 : 1) * (Number(oh) * 60 + Number(om)) : 0
      const ms = Date.UTC(+y, +mo - 1, +d, +h, +mi, +sec) - offsetMin * 60_000
      try {
        const p = Object.fromEntries(formatterFor(zone).formatToParts(new Date(ms)).map((x) => [x.type, x.value]))
        const wall = Date.UTC(+p.year, +p.month - 1, +p.day, +p.hour % 24, +p.minute, +p.second)
        const zoneOffset = Math.round((wall - ms) / 60_000)
        out = `${p.year}-${p.month}-${p.day} ${String(+p.hour % 24).padStart(2, '0')}:${p.minute}:${p.second}${frac} ${offsetText(zoneOffset)}`
      } catch {
        out = null
      }
    }
  }
  if (cache.size > 20_000) cache.clear()
  cache.set(key, out)
  return out
}

/** The zone copies and exports use, kept current by the settings store. */
let currentZone = 'UTC'
/** @param {string} setting */
export function setDisplayZone(setting) {
  currentZone = resolveZone(setting)
}
/** A cell's text in the display zone, when it is an absolute timestamp. @param {string} text */
export function displayInstant(text) {
  return currentZone === 'UTC' ? text : instantIn(text, currentZone) ?? text
}
