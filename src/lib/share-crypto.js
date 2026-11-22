// The share-link format, the app's half. The viewer's half is stroke-web
// src/lib/shares/crypto.ts; keep the two in step.
//
//   blob = 0x01 · iv (12 bytes) · AES-256-GCM(gzip(JSON payload))
//   link = https://stroke.click/s/<id>#<key, 32 bytes base64url>
//   key  = the link key, or with a password
//          SHA-256(link key · PBKDF2-SHA256(password, salt, 310,000, 32 bytes))
//
// The key only ever lives in the link's #fragment, which browsers never send,
// so stroke.click stores bytes it cannot read.

export const SHARE_FORMAT = 1
export const PBKDF2_ITERATIONS = 310_000

/** @param {Uint8Array} bytes */
export function b64url(bytes) {
  let s = ''
  for (const b of bytes) s += String.fromCharCode(b)
  return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
}

/** @param {string} s */
export function b64urlDecode(s) {
  const b = atob(s.replace(/-/g, '+').replace(/_/g, '/') + '==='.slice((s.length + 3) % 4))
  return Uint8Array.from(b, (c) => c.charCodeAt(0))
}

/** @param {string} text */
async function gzip(text) {
  const stream = new Blob([new TextEncoder().encode(text)]).stream().pipeThrough(new CompressionStream('gzip'))
  return new Uint8Array(await new Response(stream).arrayBuffer())
}

/** @param {ArrayBuffer} data */
async function gunzip(data) {
  const stream = new Blob([data]).stream().pipeThrough(new DecompressionStream('gzip'))
  return await new Response(stream).text()
}

/**
 * The AES key for a link key and, with a password, its PBKDF2 stretch.
 * @param {Uint8Array} linkKey @param {string | null} password @param {string | null} saltB64
 * @param {KeyUsage[]} usages
 */
export async function deriveKey(linkKey, password, saltB64, usages) {
  let raw = linkKey
  if (password && saltB64) {
    const base = await crypto.subtle.importKey('raw', new TextEncoder().encode(password), 'PBKDF2', false, ['deriveBits'])
    const salt = b64urlDecode(saltB64.replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, ''))
    const bits = new Uint8Array(await crypto.subtle.deriveBits({ name: 'PBKDF2', hash: 'SHA-256', salt, iterations: PBKDF2_ITERATIONS }, base, 256))
    const both = new Uint8Array(linkKey.length + bits.length)
    both.set(linkKey)
    both.set(bits, linkKey.length)
    raw = new Uint8Array(await crypto.subtle.digest('SHA-256', both))
  }
  return crypto.subtle.importKey('raw', raw, 'AES-GCM', false, usages)
}

/**
 * Encrypt a payload for sharing. Returns the bytes to upload, the link key
 * (for the #fragment) and, with a password, the salt the server stores so
 * the viewer can stretch the same key.
 * @param {object} payload
 * @param {{ password?: string | null }} [opts]
 * @returns {Promise<{ blob: Uint8Array, linkKey: string, passwordSalt: string | null }>}
 */
export async function sealShare(payload, opts = {}) {
  const linkKey = crypto.getRandomValues(new Uint8Array(32))
  const password = opts.password?.length ? opts.password : null
  const passwordSalt = password ? b64url(crypto.getRandomValues(new Uint8Array(16))) : null
  const key = await deriveKey(linkKey, password, passwordSalt, ['encrypt'])
  const iv = crypto.getRandomValues(new Uint8Array(12))
  const plain = await gzip(JSON.stringify(payload))
  const sealed = new Uint8Array(await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, key, plain))
  const blob = new Uint8Array(1 + iv.length + sealed.length)
  blob[0] = SHARE_FORMAT
  blob.set(iv, 1)
  blob.set(sealed, 1 + iv.length)
  return { blob, linkKey: b64url(linkKey), passwordSalt }
}

/**
 * Decrypt a share (the app opening its own links, and the tests).
 * @param {ArrayBuffer | Uint8Array} blob @param {string} linkKey @param {string | null} password @param {string | null} passwordSalt
 */
export async function openShare(blob, linkKey, password, passwordSalt) {
  const bytes = blob instanceof Uint8Array ? blob : new Uint8Array(blob)
  if (bytes[0] !== SHARE_FORMAT) throw new Error('Unknown share format')
  const key = await deriveKey(b64urlDecode(linkKey), password, passwordSalt, ['decrypt'])
  const plain = await crypto.subtle.decrypt({ name: 'AES-GCM', iv: bytes.slice(1, 13) }, key, bytes.slice(13))
  return JSON.parse(await gunzip(plain))
}

// ── What gets shared ────────────────────────────────────────────────────────

/** Column names that usually hold personal or secret data. */
const PII_NAME = /(^|_)(e_?mail|phone|mobile|tel|ssn|passport|national_id|tax_id|iban|card|cc_num|cvv|dob|birth(day|date)?|address|street|zip|postcode|ip(_?addr(ess)?)?|password|passwd|pwd|secret|token|api_?key|access_?key|private_?key|salt|hash|session|otp|first_?name|last_?name|full_?name|surname)($|_)/i
const EMAIL = /^[^\s@]+@[^\s@]+\.[^\s@]{2,}$/
const PHONE = /^\+?[\d\s().-]{8,}$/
/** Dates (2024-01-03, 03.01.2024) fit PHONE's characters, so a join date was
 *  masked as a phone number; a bare run of digits is more often an id. */
const DATEISH = /^(\d{4}[-./]\d{1,2}[-./]\d{1,2}|\d{1,2}[-./]\d{1,2}[-./]\d{4})$/
const looksLikePhone = (/** @type {string} */ v) =>
  PHONE.test(v) && /\d{7,}/.test(v.replace(/\D/g, '')) && !DATEISH.test(v.trim()) && /^\+|[\s().-]/.test(v.trim())
const TOKENISH = /^(?:sk|pk|rk|ghp|gho|xox[abp]|eyJ)[\w.-]{16,}|^[A-Za-z0-9_-]{32,}$/

/**
 * Columns that probably hold personal data or secrets, judged by name and by
 * a sample of their values. The share dialog offers to mask these.
 * @param {{ name: string }[]} columns @param {unknown[][]} rows
 * @returns {{ index: number, name: string, why: string }[]}
 */
export function detectSensitiveColumns(columns, rows) {
  const sample = rows.slice(0, 200)
  /** @type {{ index: number, name: string, why: string }[]} */
  const out = []
  columns.forEach((c, i) => {
    if (PII_NAME.test(c.name)) { out.push({ index: i, name: c.name, why: 'name' }); return }
    let emails = 0, phones = 0, tokens = 0, seen = 0
    for (const r of sample) {
      const v = r[i]
      if (typeof v !== 'string' || !v) continue
      seen++
      if (EMAIL.test(v)) emails++
      else if (looksLikePhone(v)) phones++
      else if (TOKENISH.test(v)) tokens++
    }
    if (!seen) return
    if (emails / seen > 0.5) out.push({ index: i, name: c.name, why: 'emails' })
    else if (phones / seen > 0.5) out.push({ index: i, name: c.name, why: 'phone numbers' })
    else if (tokens / seen > 0.5) out.push({ index: i, name: c.name, why: 'tokens' })
  })
  return out
}

/**
 * A masked value that keeps its shape (so the column still reads as emails or
 * phone numbers) without the content: `ada@example.com` -> `a••@e••••••.com`.
 * @param {unknown} v
 */
export function maskValue(v) {
  if (v === null || v === undefined) return v
  const s = typeof v === 'string' ? v : JSON.stringify(v)
  if (EMAIL.test(s)) {
    const [user, domain] = s.split('@')
    const dot = domain.lastIndexOf('.')
    const host = dot > 0 ? domain.slice(0, dot) : domain
    const tld = dot > 0 ? domain.slice(dot) : ''
    return `${user[0] ?? ''}${'•'.repeat(Math.max(2, user.length - 1))}@${host[0] ?? ''}${'•'.repeat(Math.max(2, host.length - 1))}${tld}`
  }
  if (s.length <= 4) return '••••'
  return s.slice(0, 2) + '•'.repeat(Math.min(12, s.length - 4)) + s.slice(-2)
}
