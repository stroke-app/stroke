/**
 * AES-256-GCM sealing for the studio relay: a key from the share link's
 * #fragment, base64url on the wire. Same scheme as strokedb (cli/src/relay.js)
 * and the web studio (stroke-web src/lib/relay/relay-crypto.ts).
 */
/** @param {Uint8Array} bytes */
export function toB64u(bytes) {
  let s = ''
  for (const b of bytes) s += String.fromCharCode(b)
  return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
}
/** @param {string} text */
export function fromB64u(text) {
  const b = atob(text.replace(/-/g, '+').replace(/_/g, '/'))
  const out = new Uint8Array(b.length)
  for (let i = 0; i < b.length; i++) out[i] = b.charCodeAt(i)
  return out
}
export const rand = (/** @type {number} */ n) => crypto.getRandomValues(new Uint8Array(n))
/** @param {CryptoKey} key @param {unknown} value */
export async function seal(key, value) {
  const iv = rand(12)
  const ct = new Uint8Array(await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, key, new TextEncoder().encode(JSON.stringify(value))))
  return { iv: toB64u(iv), ct: toB64u(ct) }
}
/** @param {CryptoKey} key @param {{ iv: string, ct: string }} s */
export async function unseal(key, s) {
  const pt = await crypto.subtle.decrypt({ name: 'AES-GCM', iv: fromB64u(s.iv) }, key, fromB64u(s.ct))
  return JSON.parse(new TextDecoder().decode(pt))
}
