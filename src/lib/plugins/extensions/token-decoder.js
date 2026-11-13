// Token decoder - JWTs say who they are for and when they expire; base64 and
// hex values show what they decode to.
//
// A JWT gets a short trail (`expires in 2h`, `expired 3d ago`) and its claims
// in the tooltip. Nothing is verified: the signature needs a key this app does
// not have, and a decoded claim set is only what the token says about itself.
// Base64 and hex are decoded only when the result is readable text, and only in
// columns whose name suggests encoded data, so ordinary strings are left alone.
import { ago } from './id-decoder.js'

const JWT_RE = /^eyJ[\w-]+\.eyJ[\w-]*\.[\w-]*$/
const B64_RE = /^(?:[A-Za-z0-9+/]{4})+(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$|^[A-Za-z0-9_-]{16,}$/
const HEX_RE = /^(?:0x|\\x)?((?:[0-9a-f]{2}){8,})$/i
const ENCODED_NAME_RE = /(token|jwt|payload|data|b64|base64|encoded|blob|body|content|cookie|state|cursor|hex|bytes|raw)/i

/** @param {string} s base64 or base64url */
function b64decode(s) {
  const norm = s.replace(/-/g, '+').replace(/_/g, '/')
  const pad = norm + '==='.slice((norm.length + 3) % 4)
  const bin = atob(pad)
  const bytes = Uint8Array.from(bin, (c) => c.charCodeAt(0))
  return new TextDecoder('utf-8', { fatal: true }).decode(bytes)
}

/** Mostly printable, no control bytes beyond whitespace. @param {string} s */
function readable(s) {
  if (!s) return false
  let ok = 0
  for (const ch of s) if (/[\p{L}\p{N}\p{P}\p{S}\s]/u.test(ch) && !/[\x00-\x08\x0e-\x1f]/.test(ch)) ok++
  return ok / [...s].length > 0.95
}

/** @param {string} s @param {number} n */
const clip = (s, n) => (s.length > n ? s.slice(0, n - 1) + '…' : s)

/** @param {string} t @returns {{ header: any, claims: any } | null} */
export function decodeJwt(t) {
  if (!JWT_RE.test(t)) return null
  try {
    const [h, p] = t.split('.')
    return { header: JSON.parse(b64decode(h)), claims: JSON.parse(b64decode(p)) }
  } catch {
    return null
  }
}

/** "in 2h" for a future time. @param {number} ms */
function inTime(ms) {
  const s = Math.round((ms - Date.now()) / 1000)
  if (s < 60) return 'in under a minute'
  if (s < 3600) return `in ${Math.round(s / 60)} min`
  if (s < 86400) return `in ${Math.round(s / 3600)}h`
  return `in ${Math.round(s / 86400)}d`
}

export const tokenDecoder = {
  id: 'token-decoder',
  name: 'Token Decoder',
  description: 'JWT expiry and claims, and readable base64 / hex values, without leaving the grid.',
  kind: 'formatter',
  defaultConfig: {},

  /** @param {string} type */
  appliesTo(type) {
    return !type || /char|text|string|json|bytea|blob|binary/i.test(type)
  },

  /** @param {unknown} value @param {string} _type @param {Record<string, unknown>} _config @param {string} name */
  format(value, _type, _config, name) {
    if (typeof value !== 'string' || value.length < 16 || value.length > 16384) return null
    const t = value.trim()
    const jwt = decodeJwt(t)
    if (jwt) {
      const c = jwt.claims ?? {}
      const lines = [`JWT · ${jwt.header?.alg ?? '?'}${jwt.header?.kid ? ` · kid ${jwt.header.kid}` : ''} (signature not verified)`]
      for (const k of ['sub', 'iss', 'aud', 'email', 'role', 'roles', 'scope']) {
        if (c[k] != null) lines.push(`${k}: ${clip(typeof c[k] === 'string' ? c[k] : JSON.stringify(c[k]), 80)}`)
      }
      if (typeof c.iat === 'number') lines.push(`issued ${new Date(c.iat * 1000).toLocaleString()}`)
      /** @type {{ text: string, color?: string } | undefined} */
      let trail
      let warn = false
      if (typeof c.exp === 'number') {
        const ms = c.exp * 1000
        lines.push(`expires ${new Date(ms).toLocaleString()}`)
        if (ms <= Date.now()) { trail = { text: `expired ${ago(ms)}`, color: 'destructive' }; warn = true }
        else trail = { text: `expires ${inTime(ms)}` }
      } else {
        trail = { text: 'no expiry' }
      }
      const others = Object.keys(c).filter((k) => !['sub', 'iss', 'aud', 'email', 'role', 'roles', 'scope', 'iat', 'exp', 'nbf', 'jti'].includes(k))
      if (others.length) lines.push(`also: ${clip(others.join(', '), 80)}`)
      return { trail, title: lines.join('\n'), ...(warn ? { warn: true } : {}) }
    }
    if (!name || !ENCODED_NAME_RE.test(name)) return null
    const hex = HEX_RE.exec(t)
    if (hex) {
      try {
        const bytes = Uint8Array.from(hex[1].match(/../g) ?? [], (b) => parseInt(b, 16))
        const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes)
        if (readable(text)) return { title: `hex → ${clip(text, 400)}` }
      } catch { /* not UTF-8 */ }
      return null
    }
    if (B64_RE.test(t) && t.length % 4 !== 1) {
      try {
        const text = b64decode(t)
        if (readable(text)) return { title: `base64 → ${clip(text, 400)}`, trail: { text: 'b64' } }
      } catch { /* not base64 text */ }
    }
    return null
  },
}
