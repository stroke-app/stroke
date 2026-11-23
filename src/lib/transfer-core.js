/**
 * The parts of transfer.js with no app state: the file format, sealing
 * secrets with a passphrase, and reading what an export file holds.
 */

export const FORMAT = 'stroke-export'
const PBKDF2_ROUNDS = 310_000

/**
 * @typedef {{ connections: boolean, queries: boolean, settings: boolean, snippets: boolean, secrets: boolean }} Parts
 * @typedef {{ salt: string, iv: string, data: string, rounds: number }} Sealed
 */

// ── Encryption ───────────────────────────────────────────────────────────────

const b64 = (/** @type {ArrayBuffer | Uint8Array} */ buf) => {
  const bytes = buf instanceof Uint8Array ? buf : new Uint8Array(buf)
  let s = ''
  for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode.apply(null, /** @type {any} */ (bytes.subarray(i, i + 0x8000)))
  return btoa(s)
}
const unb64 = (/** @type {string} */ s) => Uint8Array.from(atob(s), (c) => c.charCodeAt(0))

/** @param {string} passphrase @param {Uint8Array} salt @param {number} rounds */
async function deriveKey(passphrase, salt, rounds) {
  const base = await crypto.subtle.importKey('raw', new TextEncoder().encode(passphrase), 'PBKDF2', false, ['deriveKey'])
  return crypto.subtle.deriveKey(
    { name: 'PBKDF2', salt: /** @type {any} */ (salt), iterations: rounds, hash: 'SHA-256' },
    base,
    { name: 'AES-GCM', length: 256 },
    false,
    ['encrypt', 'decrypt'],
  )
}

/** @param {unknown} value @param {string} passphrase @returns {Promise<Sealed>} */
export async function seal(value, passphrase) {
  const salt = crypto.getRandomValues(new Uint8Array(16))
  const iv = crypto.getRandomValues(new Uint8Array(12))
  const key = await deriveKey(passphrase, salt, PBKDF2_ROUNDS)
  const data = await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, key, new TextEncoder().encode(JSON.stringify(value)))
  return { salt: b64(salt), iv: b64(iv), data: b64(data), rounds: PBKDF2_ROUNDS }
}

/** The sealed value, or null when the passphrase is wrong. @param {Sealed} sealed @param {string} passphrase */
export async function unseal(sealed, passphrase) {
  try {
    const key = await deriveKey(passphrase, unb64(sealed.salt), sealed.rounds || PBKDF2_ROUNDS)
    const plain = await crypto.subtle.decrypt({ name: 'AES-GCM', iv: unb64(sealed.iv) }, key, unb64(sealed.data))
    return JSON.parse(new TextDecoder().decode(plain))
  } catch {
    return null
  }
}

/**
 * @typedef {{
 *   file: Record<string, any>,
 *   connections: { conn: Record<string, any>, existing: boolean }[],
 *   queries: number,
 *   settings: boolean,
 *   snippets: number,
 *   encrypted: boolean,
 *   exportedAt: string,
 * }} ImportPlan
 */

/**
 * What a file would bring in, without changing anything.
 * @param {string} text @param {Set<string>} here ids of the connections already saved
 * @returns {ImportPlan | { error: string }}
 */
export function planImportAgainst(text, here) {
  let file
  try {
    file = JSON.parse(text)
  } catch {
    return { error: 'That file is not JSON.' }
  }
  if (!file || file.format !== FORMAT) return { error: 'That is not a Stroke export file.' }
  if (typeof file.version !== 'number' || file.version > 1) return { error: 'This export comes from a newer version of Stroke. Update, then import it.' }
  const connections = (Array.isArray(file.connections) ? file.connections : [])
    .filter((c) => c && typeof c.id === 'string' && typeof c.type === 'string')
    .map((c) => ({ conn: c, existing: here.has(c.id) }))
  return {
    file,
    connections,
    queries: Array.isArray(file.savedQueries) ? file.savedQueries.length : 0,
    settings: !!file.settings && typeof file.settings === 'object',
    snippets: Array.isArray(file.snippets) ? file.snippets.length : 0,
    encrypted: !!file.secrets,
    exportedAt: String(file.exportedAt ?? ''),
  }
}

