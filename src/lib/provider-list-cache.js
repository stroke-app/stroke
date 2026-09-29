/**
 * Last-known provider listings (Cloudflare accounts + D1 databases, Neon /
 * Supabase / PlanetScale / Prisma databases), so the connect picker opens on the
 * list it showed last time and refreshes it in the background instead of sitting
 * on a spinner through a keychain read and two or three API round-trips.
 *
 * Only names, ids and regions are kept - never a token or a password. Persisted
 * to localStorage so the first open after a restart is instant too.
 */

const STORAGE_KEY = 'stroke.providerListCache.v1'

/** @type {Record<string, unknown> | null} */
let _mem = null

function load() {
  if (_mem) return _mem
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    const parsed = raw ? JSON.parse(raw) : null
    _mem = parsed && typeof parsed === 'object' ? parsed : {}
  } catch {
    _mem = {}
  }
  return _mem
}

function save() {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(_mem ?? {}))
  } catch { /* storage full or blocked: the in-memory copy still serves this session */ }
}

/**
 * @template T
 * @param {string} key
 * @returns {T | undefined}
 */
export function readProviderList(key) {
  return /** @type {T | undefined} */ (load()[key])
}

/** @param {string} key @param {unknown} value */
export function writeProviderList(key, value) {
  load()[key] = value
  save()
}

/** Drop every entry whose key starts with `prefix` (a sign-out). */
export function clearProviderLists(/** @type {string} */ prefix) {
  const mem = load()
  for (const k of Object.keys(mem)) if (k.startsWith(prefix)) delete mem[k]
  save()
}

/** Test hook: forget the in-memory copy so the next read goes back to storage. */
export function _resetProviderListCache() {
  _mem = null
}
