// ID decoder - ids that carry their creation time: UUIDv7, UUIDv1, ULID,
// KSUID, MongoDB ObjectId, and (with an epoch set) Snowflake ids.
//
// The tooltip says what the id is and when it was made; optionally the grid
// shows the age after it. Pure and synchronous: nothing is fetched. A decoded
// time outside 2000..tomorrow is treated as "not that kind of id", which keeps
// random hex and counters from being read as dates.
import { normalizeColumnType } from '$lib/cell-value.js'

const NAME_RE = /(^|_)(id|uuid|guid|ulid|ksuid|key|oid|ref)$|^_id$/i
const UUID_RE = /^[0-9a-f]{8}-?[0-9a-f]{4}-?([0-9a-f])[0-9a-f]{3}-?[0-9a-f]{4}-?[0-9a-f]{12}$/i
const ULID_RE = /^[0-7][0-9A-HJKMNP-TV-Z]{25}$/i
const KSUID_RE = /^[0-9A-Za-z]{27}$/
const OBJECTID_RE = /^[0-9a-f]{24}$/i
const CROCKFORD = '0123456789ABCDEFGHJKMNPQRSTVWXYZ'
const B62 = '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz'
const MIN_MS = Date.UTC(2000, 0, 1)

/** Known Snowflake epochs, by config value. */
export const SNOWFLAKE_EPOCHS = /** @type {Record<string, number>} */ ({
  twitter: 1288834974657,
  discord: 1420070400000,
  instagram: 1314220021721,
})

const defaultConfig = { inline: false, snowflake: 'none' }

/** @param {number} ms */
function plausible(ms) {
  return Number.isFinite(ms) && ms >= MIN_MS && ms <= Date.now() + 86_400_000
}

/** @param {string} s @returns {{ kind: string, ms: number } | null} */
export function decodeId(s, snowflake = '') {
  const t = s.trim()
  const u = UUID_RE.exec(t)
  if (u) {
    const hex = t.replace(/-/g, '')
    if (u[1] === '7') {
      const ms = parseInt(hex.slice(0, 12), 16)
      return plausible(ms) ? { kind: 'UUIDv7', ms } : null
    }
    if (u[1] === '1' || u[1] === '6') {
      // 60-bit count of 100ns since 1582-10-15.
      const ts = u[1] === '1'
        ? BigInt('0x' + hex.slice(13, 16) + hex.slice(8, 12) + hex.slice(0, 8))
        : BigInt('0x' + hex.slice(0, 12) + hex.slice(13, 16))
      const ms = Number(ts / 10000n) - 12219292800000
      return plausible(ms) ? { kind: `UUIDv${u[1]}`, ms } : null
    }
    return null
  }
  if (ULID_RE.test(t)) {
    let ms = 0
    for (const ch of t.slice(0, 10).toUpperCase()) ms = ms * 32 + CROCKFORD.indexOf(ch)
    return plausible(ms) ? { kind: 'ULID', ms } : null
  }
  if (OBJECTID_RE.test(t)) {
    const ms = parseInt(t.slice(0, 8), 16) * 1000
    return plausible(ms) ? { kind: 'ObjectId', ms } : null
  }
  if (KSUID_RE.test(t) && /[A-Za-z]/.test(t)) {
    let n = 0n
    for (const ch of t) n = n * 62n + BigInt(B62.indexOf(ch))
    // 20 bytes: a 4-byte timestamp (seconds since 2014-05-13) then 16 random.
    const secs = Number(n >> 128n)
    const ms = (secs + 1400000000) * 1000
    return plausible(ms) ? { kind: 'KSUID', ms } : null
  }
  const epoch = SNOWFLAKE_EPOCHS[snowflake]
  if (epoch && /^\d{15,20}$/.test(t)) {
    const ms = Number(BigInt(t) >> 22n) + epoch
    return plausible(ms) ? { kind: 'Snowflake', ms } : null
  }
  return null
}

/** "3d ago", "5 min ago". @param {number} ms */
export function ago(ms) {
  const s = Math.max(0, Math.round((Date.now() - ms) / 1000))
  if (s < 60) return 'just now'
  if (s < 3600) return `${Math.floor(s / 60)} min ago`
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`
  if (s < 86400 * 60) return `${Math.floor(s / 86400)}d ago`
  if (s < 86400 * 730) return `${Math.floor(s / (86400 * 30))}mo ago`
  return `${Math.floor(s / (86400 * 365))}y ago`
}

export const idDecoder = {
  id: 'id-decoder',
  name: 'ID Decoder',
  description: 'When a UUIDv7, ULID, KSUID, ObjectId or Snowflake id was created, from the id itself.',
  kind: 'formatter',
  defaultConfig,

  /** @param {string} type @param {string} name */
  appliesTo(type, name) {
    const t = normalizeColumnType(type)
    return /uuid|objectid/i.test(t) || (!!name && NAME_RE.test(name)) || (!name && /char|text/i.test(t))
  },

  /** @param {unknown} value @param {string} _type @param {Record<string, unknown>} config */
  format(value, _type, config) {
    if (typeof value !== 'string' && typeof value !== 'number' && typeof value !== 'bigint') return null
    const cfg = { ...defaultConfig, ...config }
    const d = decodeId(String(value), String(cfg.snowflake || ''))
    if (!d) return null
    const when = new Date(d.ms)
    const title = `${d.kind} · created ${when.toLocaleString()} (${ago(d.ms)})`
    return cfg.inline ? { trail: { text: ago(d.ms) }, title } : { title }
  },
}
