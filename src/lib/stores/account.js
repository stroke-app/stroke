import { invoke } from '@tauri-apps/api/core'
import { writable, derived, get } from 'svelte/store'
import { refreshLicenseStatus, licenseStatus } from './license.js'

/**
 * Stroke account (sign in with GitHub/Google through stroke.click). The token
 * lives in the OS keychain on the Rust side (src-tauri/src/account.rs); this
 * store only holds what the UI shows. The license gates read `licenseStatus`,
 * which the Rust side already merges with the account's entitlement.
 *
 * @typedef {{ id: string, email: string, name?: string|null, image?: string|null }} AccountUser
 * @typedef {{ plan: string, source: string,
 *   license: { plan: string, expires_at: number, key_hint?: string|null } | null,
 *   trial: { started_at: number, ends_at: number } | null,
 *   issued_at: number, valid_until: number }} Entitlement
 * @typedef {{ device_id: string, name: string, os: string, last_seen_at: number, current: boolean }} AccountDevice
 * @typedef {{ signed_in: boolean, user: AccountUser|null, entitlement: Entitlement|null,
 *   entitlement_fresh: boolean, offline_grace_until: number, devices: AccountDevice[], fetched_at: number, offline: boolean,
 *   needs_onboarding: boolean, has_license_key: boolean, link_error: string|null,
 *   error: string|null, manage_url: string }} AccountStatus
 * @typedef {{ phase: 'idle'|'waiting'|'error', provider: string|null, error: string|null, url: string|null }} SignInState
 */

export const MANAGE_URL_FALLBACK = 'https://stroke.click/app'
const PROMPT_KEY = 'stroke:account-link-prompt-shown'
const CONNECTIONS_KEY = 'stroke:connections'
/** Refresh on window focus at most this often. */
const FOCUS_REFRESH_MS = 5 * 60 * 1000

/** @type {import('svelte/store').Writable<AccountStatus|null>} */
export const account = writable(null)

/** @type {import('svelte/store').Writable<SignInState>} */
export const signInState = writable({ phase: 'idle', provider: null, error: null, url: null })

export const isSignedIn = derived(account, ($a) => !!$a?.signed_in)

/** True when there are saved connections in localStorage (an existing user). */
function hasSavedConnections() {
  try {
    const raw = localStorage.getItem(CONNECTIONS_KEY)
    if (!raw) return false
    const list = JSON.parse(raw)
    return Array.isArray(list) && list.length > 0
  } catch {
    return false
  }
}

/**
 * Sign in first: everyone who is not signed in sees the sign-in screen before
 * the app. Two exceptions keep existing customers working exactly as before:
 * an activated license key on this device, and any doubt (status not loaded
 * yet, an error) - an unknown state never locks anyone out.
 */
export const needsOnboarding = derived(
  account,
  ($a) => !!$a && !$a.error && !$a.signed_in && !$a.has_license_key,
)

/** A returning user (saved connections on this device) gets "welcome back" copy. */
export const isReturningUser = derived(account, () => hasSavedConnections())

/**
 * Plan label for the account menu and settings.
 * @param {AccountStatus|null} acc
 * @param {import('./license.js').LicenseStatus} lic
 * @param {number} [now] unix seconds
 * @returns {{ label: string, tone: 'pro'|'trial'|'free' }}
 */
export function planLabel(acc, lic, now = Math.floor(Date.now() / 1000)) {
  if (lic?.status === 'Valid') return { label: 'Pro', tone: 'pro' }
  const ent = acc?.signed_in && acc.entitlement_fresh ? acc.entitlement : null
  if (ent?.plan === 'pro' && ent.source !== 'trial') return { label: 'Pro', tone: 'pro' }
  let days = lic?.status === 'Trial' ? lic.days_remaining : 0
  if (!days && ent?.plan === 'pro' && ent.trial) days = Math.max(0, Math.ceil((ent.trial.ends_at - now) / 86400))
  if (days > 0) return { label: `Trial · ${days} ${days === 1 ? 'day' : 'days'}`, tone: 'trial' }
  return { label: 'Free', tone: 'free' }
}

/**
 * Paid Pro running on the offline grace (stroke.click unreachable past the
 * entitlement's window): how long until the app has to get online once.
 * @param {AccountStatus|null} acc
 * @param {number} [now] unix seconds
 * @returns {{ days: number, text: string } | null}
 */
export function offlineGraceNote(acc, now = Math.floor(Date.now() / 1000)) {
  const until = acc?.signed_in ? acc.offline_grace_until : 0
  if (!until || until <= now) return null
  const days = Math.max(1, Math.ceil((until - now) / 86400))
  const span = `${days} ${days === 1 ? 'day' : 'days'}`
  return { days, text: `Pro is working offline. Connect to the internet within ${span} so Stroke can confirm your plan.` }
}

/** @param {AccountUser|null|undefined} user */
export function initials(user) {
  const src = (user?.name || user?.email || '').trim()
  if (!src) return '?'
  const parts = src.split(/[\s@._-]+/).filter(Boolean)
  const a = parts[0]?.[0] ?? '?'
  const b = user?.name && parts.length > 1 ? parts[parts.length - 1][0] : ''
  return (a + b).toUpperCase()
}

/**
 * Load the account. `network` asks the server first (when signed in); a
 * failure keeps the last known state.
 * @param {{ network?: boolean }} [opts]
 */
export async function refreshAccount({ network = true } = {}) {
  try {
    const before = get(account)
    const status = /** @type {AccountStatus} */ (await invoke('account_status', { refresh: network }))
    account.set(status)
    if (network) scheduleOfflineRetry(status)
    // The entitlement feeds the license status; re-read it when it may have moved.
    if (network || before?.signed_in !== status.signed_in) void refreshLicenseStatus()
    return status
  } catch (e) {
    console.error('[account] status failed:', e)
    return null
  }
}

/** @param {unknown} e */
function signInError(e) {
  const msg = String(e)
  if (msg === 'timeout') return 'Sign-in timed out. Try again.'
  return msg
}

/**
 * Open the browser sign-in and wait for it. Resolves `true` when signed in.
 * @param {'github'|'google'|null} [provider] a hint for the sign-in page
 */
let signInSeq = 0

export async function signIn(provider = null) {
  const seq = ++signInSeq
  signInState.set({ phase: 'waiting', provider, error: null, url: null })
  try {
    const status = /** @type {AccountStatus} */ (await invoke('account_sign_in', { provider }))
    account.set(status)
    await refreshLicenseStatus()
    signInState.set({ phase: 'idle', provider: null, error: null, url: null })
    return true
  } catch (e) {
    // A newer sign-in replaced this one: leave its state alone.
    if (seq !== signInSeq) return false
    if (String(e) === 'cancelled') {
      signInState.set({ phase: 'idle', provider: null, error: null, url: null })
      return false
    }
    signInState.set({ phase: 'error', provider, error: signInError(e), url: null })
    return false
  }
}

/**
 * The code in `stroke-code:<code>` or a bare code, or null. Mirrors
 * `parse_pasted_code` in account.rs (which is what decides).
 * @param {string} text
 */
export function parseSignInCode(text) {
  let t = String(text ?? '').trim().replace(/^["'`]+|["'`]+$/g, '').trim()
  if (/^stroke-code:/i.test(t)) t = t.slice('stroke-code:'.length).trim()
  return /^[A-Za-z0-9_-]{16,256}$/.test(t) ? t : null
}

/**
 * Finish the sign-in in flight with a code copied from the website (when the
 * redirect back to the app can't reach it). The waiting `signIn()` resolves
 * as signed in too.
 * @param {string} text
 * @returns {Promise<{ ok: boolean, error?: string }>}
 */
export async function pasteSignInCode(text) {
  if (!parseSignInCode(text)) return { ok: false, error: "That doesn't look like a Stroke sign-in code." }
  try {
    const status = /** @type {AccountStatus} */ (await invoke('account_paste_code', { text }))
    account.set(status)
    await refreshLicenseStatus()
    return { ok: true }
  } catch (e) {
    return { ok: false, error: String(e) }
  }
}

export async function cancelSignIn() {
  signInState.set({ phase: 'idle', provider: null, error: null, url: null })
  try { await invoke('account_cancel_sign_in') } catch { /* nothing to cancel */ }
}

export async function signOut() {
  try {
    const status = /** @type {AccountStatus} */ (await invoke('account_sign_out'))
    account.set(status)
    await refreshLicenseStatus()
    return { ok: true }
  } catch (e) {
    return { ok: false, error: String(e) }
  }
}

/** The new-install screen was passed with a license key. */
export async function skipOnboarding() {
  try { await invoke('account_skip_onboarding') } catch { /* best effort */ }
  account.update((a) => (a ? { ...a, needs_onboarding: false } : a))
}

export async function openManageAccount() {
  const url = get(account)?.manage_url || MANAGE_URL_FALLBACK
  try {
    const { openUrl } = await import('@tauri-apps/plugin-opener')
    await openUrl(url)
  } catch {
    window.open(url, '_blank', 'noopener,noreferrer')
  }
}

/**
 * Sign in from places with no room for a waiting panel (the account menu, the
 * key-holder prompt): progress and the outcome go to toasts.
 */
export async function signInWithToast() {
  const { toast } = await import('$lib/components/ui/sonner/toast.svelte.js')
  const id = toast.info('Finish signing in in your browser', {
    description: 'Stroke opened stroke.click. Come back here when it says you are signed in.',
    duration: Infinity,
    action: { label: 'Cancel', onClick: () => void cancelSignIn() },
  })
  const ok = await signIn(null)
  toast.dismiss(id)
  if (ok) {
    const email = get(account)?.user?.email
    toast.success('Signed in', { description: email ? `Signed in as ${email}.` : undefined })
  } else {
    const s = get(signInState)
    if (s.phase === 'error') toast.error('Sign-in failed', { description: s.error ?? undefined })
  }
}

/**
 * One-time prompt for existing key holders who haven't signed in. Shown once,
 * ever; the key keeps working whatever they choose.
 */
async function maybePromptKeyHolder() {
  const acc = get(account)
  if (!acc || acc.signed_in || !acc.has_license_key) return
  if (get(licenseStatus)?.status !== 'Valid') return
  try {
    if (localStorage.getItem(PROMPT_KEY)) return
    localStorage.setItem(PROMPT_KEY, '1')
  } catch {
    return
  }
  const { toast } = await import('$lib/components/ui/sonner/toast.svelte.js')
  toast.info('Stroke now has accounts', {
    description: 'Sign in to sync your connections; your license comes with you.',
    duration: 20000,
    action: { label: 'Sign in', onClick: () => void signInWithToast() },
  })
}

let started = false
let lastRefresh = 0

/** While stroke.click can't be reached, re-check on this schedule (then every 15 min). */
const OFFLINE_RETRY_MS = [30_000, 60_000, 2 * 60_000, 5 * 60_000, 15 * 60_000]
let retryTimer = /** @type {ReturnType<typeof setTimeout>|null} */ (null)
let retryStep = 0

/**
 * The app keeps running on the cached plan while the server is unreachable;
 * this notices when it is back (an outage ends without an `online` event) and
 * picks up the server's verdict, including a refund or a revoked session.
 * @param {AccountStatus} status
 */
function scheduleOfflineRetry(status) {
  if (retryTimer) clearTimeout(retryTimer)
  retryTimer = null
  if (!status?.signed_in || !status.offline) {
    retryStep = 0
    return
  }
  const ms = OFFLINE_RETRY_MS[Math.min(retryStep++, OFFLINE_RETRY_MS.length - 1)]
  retryTimer = setTimeout(() => {
    retryTimer = null
    if (get(signInState).phase === 'waiting') return scheduleOfflineRetry(status)
    lastRefresh = Date.now()
    void refreshAccount({ network: true })
  }, ms)
}

/** Once per launch: a paid user close to the end of the offline grace is told to connect. */
async function maybeWarnOfflineGrace() {
  const note = offlineGraceNote(get(account))
  if (!note || note.days > 7) return
  const { toast } = await import('$lib/components/ui/sonner/toast.svelte.js')
  toast.warning('Connect to keep Stroke Pro', { description: note.text, duration: 15000 })
}

/**
 * Load the cached account at launch, then refresh it from the server, and again
 * on window focus (at most every FOCUS_REFRESH_MS). Call once.
 */
export async function startAccountSync() {
  if (started) return
  started = true
  await refreshAccount({ network: false })
  lastRefresh = Date.now()
  void refreshAccount({ network: true }).then(() => {
    void maybeWarnOfflineGrace()
    setTimeout(() => void maybePromptKeyHolder(), 4000)
  })

  try {
    // Back online: re-check straight away. Launching offline runs on the
    // signed cache; this is the moment the server can confirm (or revoke) it,
    // and reset the clock guard to real time.
    window.addEventListener('online', () => {
      if (get(signInState).phase === 'waiting') return
      lastRefresh = Date.now()
      void refreshAccount({ network: true })
    })
    // And while the app stays open for days, every few hours.
    setInterval(() => {
      if (!navigator.onLine || get(signInState).phase === 'waiting') return
      lastRefresh = Date.now()
      if (get(account)?.signed_in) void refreshAccount({ network: true })
    }, 6 * 60 * 60 * 1000)
    window.addEventListener('focus', () => {
      if (Date.now() - lastRefresh < FOCUS_REFRESH_MS) return
      if (get(signInState).phase === 'waiting') return
      lastRefresh = Date.now()
      if (get(account)?.signed_in) void refreshAccount({ network: true })
    })
    const { listen } = await import('@tauri-apps/api/event')
    await listen('account-sign-in-url', (e) => {
      const url = /** @type {{ url?: string }} */ (e.payload)?.url ?? null
      signInState.update((s) => (s.phase === 'waiting' ? { ...s, url } : s))
    })
  } catch {
    /* outside Tauri */
  }
}
