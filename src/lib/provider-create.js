/**
 * "Create database" from the provider panel: what each provider asks for, name
 * checks, the Supabase password, and the wait while a cluster provisions.
 * Pure logic, no Tauri imports, so it is unit-tested on its own; the invoke
 * wrappers live in `providers.js` (and `api.js` for Cloudflare D1).
 */

/**
 * @typedef {{ id: string, name: string }} Choice
 * @typedef {{ scopes: Choice[], scope_label: string, scope: string|null, regions: Choice[], region_label: string, default_region: string|null }} CreateOptions
 * @typedef {{ db_ref: string, name: string, ready: boolean }} CreatedDatabase
 * @typedef {{ ready: boolean, failed: boolean, state: string }} DatabaseStatus
 * @typedef {{
 *   noun: string,
 *   defaultName: string,
 *   pattern?: RegExp,
 *   rule?: string,
 *   maxLength?: number,
 *   password?: boolean,
 *   slow?: boolean,
 * }} CreateSpec
 */

/**
 * Providers whose API lets Stroke's existing sign-in create a database.
 * `noun` is what the provider calls the thing the picker lists. `slow` ones
 * provision for a minute or more, so the form says so up front.
 * @type {Record<string, CreateSpec>}
 */
export const CREATE_SPECS = {
  neon: { noun: 'project', defaultName: 'stroke-db', maxLength: 64 },
  supabase: { noun: 'project', defaultName: 'stroke-db', maxLength: 256, password: true, slow: true },
  prisma: { noun: 'project', defaultName: 'stroke-db', maxLength: 64 },
  tidb: { noun: 'cluster', defaultName: 'stroke-db', maxLength: 64, slow: true },
  turso: {
    noun: 'database',
    defaultName: 'stroke-db',
    pattern: /^[a-z0-9-]+$/,
    rule: 'Use lowercase letters, numbers and dashes.',
    maxLength: 64,
  },
  nile: {
    noun: 'database',
    defaultName: 'stroke_db',
    pattern: /^[A-Za-z_][A-Za-z0-9_]*$/,
    rule: 'Use letters, numbers and underscores, starting with a letter.',
    maxLength: 63,
  },
  upstash: { noun: 'database', defaultName: 'stroke-cache', maxLength: 64 },
  d1: {
    noun: 'database',
    defaultName: 'stroke-db',
    pattern: /^[A-Za-z0-9][A-Za-z0-9_-]*$/,
    rule: 'Use letters, numbers, dashes and underscores.',
    maxLength: 64,
  },
}

/**
 * Providers where Stroke's sign-in can't create a database, with the reason
 * (shown in the empty state) and the dashboard page that can.
 * @type {Record<string, { reason: string, url: string }>}
 */
export const CREATE_ELSEWHERE = {
  planetscale: {
    reason: "Stroke's PlanetScale sign-in can't create databases, and new ones need a paid cluster size. Create one in the PlanetScale dashboard, then refresh.",
    url: 'https://app.planetscale.com',
  },
  railway: {
    reason: "Stroke's Railway sign-in only sees the projects you shared with it. Add a database in the Railway dashboard, then refresh.",
    url: 'https://railway.com/new',
  },
}

/** @param {string} provider @returns {CreateSpec | null} */
export function createSpec(provider) {
  return CREATE_SPECS[provider] ?? null
}

/**
 * The problem with a name, in words, or '' when it is fine.
 * @param {string} name @param {CreateSpec | null} spec
 */
export function nameProblem(name, spec) {
  const n = String(name ?? '').trim()
  if (!n) return 'Enter a name.'
  if (spec?.maxLength && n.length > spec.maxLength) return `Use ${spec.maxLength} characters or fewer.`
  if (spec?.pattern && !spec.pattern.test(n)) return spec.rule ?? 'That name is not allowed here.'
  return ''
}

/**
 * The default name, made unique against names already on the account:
 * `stroke-db`, then `stroke-db-2`, `stroke-db-3`… (underscore for Nile).
 * @param {CreateSpec | null} spec @param {string[]} taken
 */
export function suggestName(spec, taken = []) {
  const base = spec?.defaultName ?? 'stroke-db'
  const used = new Set(taken.map((t) => t.toLowerCase()))
  if (!used.has(base.toLowerCase())) return base
  const sep = base.includes('_') ? '_' : '-'
  for (let i = 2; i < 1000; i++) {
    const next = `${base}${sep}${i}`
    if (!used.has(next.toLowerCase())) return next
  }
  return base
}

const PASSWORD_ALPHABET = 'ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789'

/**
 * A strong database password from the platform CSPRNG. Letters and digits
 * only: a URL-shaped character in a password needs escaping in every
 * connection string it ends up in. 24 characters of a 57-letter alphabet is
 * about 140 bits. Rejection sampling keeps every letter equally likely.
 * @param {number} [length]
 * @param {(buf: Uint8Array) => Uint8Array} [fill]
 */
export function generatePassword(length = 24, fill = (b) => crypto.getRandomValues(b)) {
  const n = PASSWORD_ALPHABET.length
  const limit = 256 - (256 % n)
  let out = ''
  while (out.length < length) {
    for (const byte of fill(new Uint8Array(length * 2))) {
      if (byte < limit) out += PASSWORD_ALPHABET[byte % n]
      if (out.length === length) break
    }
  }
  return out
}

/** @param {number} ms @param {AbortSignal} [signal] */
function sleep(ms, signal) {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) return reject(new DOMException('Stopped waiting', 'AbortError'))
    const t = setTimeout(resolve, ms)
    signal?.addEventListener('abort', () => {
      clearTimeout(t)
      reject(new DOMException('Stopped waiting', 'AbortError'))
    }, { once: true })
  })
}

/**
 * Poll a new database until it is ready.
 *
 * Resolves with the last status once `ready`; rejects when the provider says
 * it failed, after `timeoutMs`, or with an AbortError when `signal` aborts
 * (the user stopped waiting; the database keeps provisioning on the provider).
 * A status call that errors is retried: one flaky request mid-provision is not
 * a failed database. Three in a row is.
 * @param {{
 *   check: () => Promise<DatabaseStatus>,
 *   onstate?: (s: DatabaseStatus) => void,
 *   signal?: AbortSignal,
 *   intervalMs?: number,
 *   timeoutMs?: number,
 *   wait?: (ms: number, signal?: AbortSignal) => Promise<void>,
 *   now?: () => number,
 * }} opts
 * @returns {Promise<DatabaseStatus>}
 */
export async function waitUntilReady({
  check,
  onstate = () => {},
  signal,
  intervalMs = 3000,
  timeoutMs = 10 * 60_000,
  wait = sleep,
  now = () => Date.now(),
}) {
  const started = now()
  let errors = 0
  for (;;) {
    if (signal?.aborted) throw new DOMException('Stopped waiting', 'AbortError')
    try {
      const s = await check()
      errors = 0
      onstate(s)
      if (s.ready) return s
      if (s.failed) throw Object.assign(new Error(`The provider reports it as ${s.state.toLowerCase()}.`), { final: true })
    } catch (e) {
      if (/** @type {any} */ (e)?.final) throw e
      if (++errors >= 3) throw e
    }
    if (now() - started >= timeoutMs) {
      throw new Error('It is taking longer than usual. It will show in the list once the provider finishes.')
    }
    await wait(intervalMs, signal)
  }
}
