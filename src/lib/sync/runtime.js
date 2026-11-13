// Sync wiring: the real stores as engine adapters, the status the UI shows,
// and the triggers (launch, window focus ≤ 1/min, 2 s after a local change,
// "Sync now"). See engine.js for the algorithm and Appendix B of
// docs/accounts-and-sync.md for the contract.

import { invoke } from '@tauri-apps/api/core'
import { writable, get } from 'svelte/store'
import { account } from '$lib/stores/account.js'
import { workspaces, replaceWorkspaces, DEFAULT_WORKSPACE_ID } from '$lib/stores/workspaces.js'
import { loadSavedConnections, upsertConnection, removeConnection, newConnectionId } from '$lib/stores/connections.js'
import { loadSettings, updateSettings } from '$lib/stores/settings.js'
import { aiProfiles, activeProfileId, replaceProfiles } from '$lib/stores/ai-settings.js'
import { keybindings, replaceKeybindings } from '$lib/stores/keybindings.js'
import { onLocalChange } from './changes.js'
import { createSyncEngine, resetSyncBookkeeping, clearSyncBookkeeping } from './engine.js'
import { pickSyncedSettings } from './policy.js'

export { syncLabel, SYNCED_SETTINGS } from './policy.js'

/** @type {import('./engine.js').Adapters} */
const adapters = {
  workspace: {
    list: () => get(workspaces).map((w) => ({ id: w.id, data: { id: w.id, name: w.name, color: w.color, createdAt: w.createdAt } })),
    upsert: (data) => {
      const list = get(workspaces)
      const i = list.findIndex((w) => w.id === data.id)
      replaceWorkspaces(i >= 0 ? list.map((w) => (w.id === data.id ? { ...w, ...data } : w)) : [...list, data])
    },
    remove: (id) => {
      if (id !== DEFAULT_WORKSPACE_ID) replaceWorkspaces(get(workspaces).filter((w) => w.id !== id))
    },
  },
  connection: {
    // Locally discovered entries (Docker, ORM studios) belong to this machine.
    // `lastConnectedAt` changes on every connect and means "on this device".
    list: () =>
      loadSavedConnections()
        .filter((c) => !c.origin)
        .map(({ lastConnectedAt: _t, ...data }) => ({ id: data.id, data })),
    upsert: (data) => {
      const prev = loadSavedConnections().find((c) => c.id === data.id)
      const conn = prev?.lastConnectedAt != null ? { ...data, lastConnectedAt: prev.lastConnectedAt } : data
      // Through the normal save path, so secrets go to the keychain.
      upsertConnection(conn, { revive: true })
    },
    remove: (id) => void removeConnection(id),
    conflictCopy: (data) =>
      upsertConnection({ ...data, id: newConnectionId(), name: `${data.name || 'Connection'} (conflicted copy)` }),
  },
  settings: {
    list: () => [
      {
        id: 'settings',
        data: {
          app: pickSyncedSettings(loadSettings()),
          ai: { profiles: get(aiProfiles), activeProfileId: get(activeProfileId) },
          // Remapped shortcuts. Combos say `Mod`, so they mean the same on
          // every platform.
          keybindings: get(keybindings),
        },
      },
    ],
    upsert: (data) => {
      const app = pickSyncedSettings(data?.app ?? {})
      if (Object.keys(app).length) updateSettings(app)
      if (data?.ai && Array.isArray(data.ai.profiles)) void replaceProfiles(data.ai.profiles, data.ai.activeProfileId ?? null)
      // Absent from an older client's copy: leave this device's as it is.
      if (data?.keybindings && typeof data.keybindings === 'object') replaceKeybindings(data.keybindings)
    },
    remove: () => {},
  },
}

const engine = createSyncEngine({
  invoke,
  adapters,
  onApplied: (kinds) => {
    // StudioShell and the connection picker read the saved list on demand.
    if (kinds.has('connection')) {
      try { window.dispatchEvent(new CustomEvent('stroke:connections-synced')) } catch { /* no window */ }
    }
  },
})

// ── Status ────────────────────────────────────────────────────────────────────

/**
 * @typedef {'signed-out' | 'checking' | 'setup' | 'locked' | 'ready' | 'syncing' | 'offline' | 'error'} SyncPhase
 * @typedef {{ phase: SyncPhase, lastSyncedAt: number, items: number, error: string | null,
 *   keyChanged: boolean, undecryptable: number }} SyncStatus
 */

const LAST_KEY = 'stroke:sync:last'

function loadLast() {
  try {
    const v = JSON.parse(localStorage.getItem(LAST_KEY) ?? '{}')
    return { lastSyncedAt: Number(v.lastSyncedAt) || 0, items: Number(v.items) || 0 }
  } catch {
    return { lastSyncedAt: 0, items: 0 }
  }
}

/** @type {import('svelte/store').Writable<SyncStatus>} */
export const syncStatus = writable({
  phase: 'signed-out',
  ...(typeof localStorage === 'undefined' ? { lastSyncedAt: 0, items: 0 } : loadLast()),
  error: null,
  keyChanged: false,
  undecryptable: 0,
})

/** @param {Partial<SyncStatus>} patch */
function setStatus(patch) {
  syncStatus.update((s) => ({ ...s, ...patch }))
}

/** Ask the backend whether sync is set up and unlocked here. */
export async function refreshSyncState() {
  if (!get(account)?.signed_in) {
    setStatus({ phase: 'signed-out', error: null })
    return get(syncStatus)
  }
  if (get(syncStatus).phase === 'signed-out') setStatus({ phase: 'checking' })
  try {
    /** @type {{ signed_in: boolean, unlocked: boolean, server_has_key: boolean | null, key_changed: boolean, offline: boolean, error: string | null }} */
    const st = await invoke('sync_state')
    if (st.key_changed) resetSyncBookkeeping(localStorage)
    /** @type {SyncPhase} */
    let phase
    if (!st.signed_in) phase = 'signed-out'
    else if (st.unlocked) phase = st.offline ? 'offline' : 'ready'
    else if (st.server_has_key === false) phase = 'setup'
    else if (st.server_has_key === true) phase = 'locked'
    else phase = st.offline ? 'offline' : 'error'
    setStatus({ phase, error: st.error ?? null, keyChanged: !!st.key_changed || (phase === 'locked' && get(syncStatus).keyChanged) })
  } catch (e) {
    setStatus({ phase: 'error', error: String(e) })
  }
  return get(syncStatus)
}

let running = /** @type {Promise<void> | null} */ (null)
let again = false

/**
 * Pull, then push dirty records. Never runs signed out or locked; a run asked
 * for while one is going runs once more after it.
 */
export function syncNow() {
  if (running) {
    again = true
    return running
  }
  running = (async () => {
    try {
      do {
        again = false
        await runOnce()
      } while (again)
    } finally {
      running = null
    }
  })()
  return running
}

/** After a 429, no automatic sync until then (ms). */
let backoffUntil = 0
const BACKOFF_MS = 60_000

async function runOnce() {
  if (!get(account)?.signed_in) return
  if (Date.now() < backoffUntil) return
  // Pull and push re-check the server's key themselves (Rust), so a ready
  // device skips the extra state call; anything else asks again.
  const phase = get(syncStatus).phase
  if (phase !== 'ready' && phase !== 'syncing') {
    const st = await refreshSyncState()
    if (st.phase !== 'ready') return
  }
  setStatus({ phase: 'syncing', error: null })
  try {
    const r = await engine.run()
    const last = { lastSyncedAt: Date.now(), items: r.items }
    try { localStorage.setItem(LAST_KEY, JSON.stringify(last)) } catch { /* shown, not kept */ }
    setStatus({ phase: 'ready', ...last, undecryptable: r.undecryptable, keyChanged: false })
  } catch (e) {
    const msg = String(e)
    if (msg === 'locked') {
      setStatus({ phase: 'locked' })
      return
    }
    if (msg === 'key_changed') {
      // Reset or turned off on another device: the key here was dropped, and
      // what this device knew about the server no longer holds.
      resetSyncBookkeeping(localStorage)
      setStatus({ phase: 'checking', keyChanged: true })
      await refreshSyncState()
      setStatus({ keyChanged: true })
      return
    }
    if (msg === 'rate_limited') {
      backoffUntil = Date.now() + BACKOFF_MS
      setStatus({ phase: 'ready', error: 'The server asked Stroke to slow down. Sync resumes in a minute.' })
      return
    }
    const offline = /reach stroke\.click/i.test(msg)
    setStatus({ phase: offline ? 'offline' : 'error', error: offline ? null : msg })
  }
}

// ── Set up / unlock / reset / lock ───────────────────────────────────────────

/** First device: create the key, then send this device's data. @param {string} passphrase */
export async function setupSync(passphrase) {
  await invoke('sync_setup', { passphrase })
  resetSyncBookkeeping(localStorage)
  await syncNow()
}

/** Another device: open the account's key with the passphrase, then sync. @param {string} passphrase */
export async function unlockSync(passphrase) {
  await invoke('sync_unlock', { passphrase })
  // What this device knew about the server may predate a reset elsewhere.
  resetSyncBookkeeping(localStorage)
  setStatus({ keyChanged: false })
  await syncNow()
}

/**
 * Lost passphrase: a new key, every server record deleted, then this device's
 * data pushed as the new starting point.
 * @param {string} passphrase
 */
export async function resetSync(passphrase) {
  await invoke('sync_reset', { passphrase })
  resetSyncBookkeeping(localStorage)
  await syncNow()
}

/** Drop the key from this device. Server data stays. */
export async function lockSync() {
  await invoke('sync_forget')
  await refreshSyncState()
}

// ── Triggers ──────────────────────────────────────────────────────────────────

const FOCUS_THROTTLE_MS = 60_000
const CHANGE_DEBOUNCE_MS = 2_000
let started = false

/** Start the triggers. Call once, after the account store has started. */
export function startSync() {
  if (started) return
  started = true

  /** @type {ReturnType<typeof setTimeout> | null} */
  let debounce = null
  onLocalChange(() => {
    if (debounce) clearTimeout(debounce)
    debounce = setTimeout(() => {
      debounce = null
      if (get(syncStatus).phase === 'ready' || get(syncStatus).phase === 'offline') void syncNow()
    }, CHANGE_DEBOUNCE_MS)
  })

  let lastFocus = 0
  try {
    window.addEventListener('focus', () => {
      if (Date.now() - lastFocus < FOCUS_THROTTLE_MS) return
      lastFocus = Date.now()
      if (get(account)?.signed_in) void syncNow()
    })
  } catch { /* no window */ }

  // Launch, sign-in and sign-out all arrive through the account store.
  let wasSignedIn = /** @type {boolean | null} */ (null)
  account.subscribe((a) => {
    if (!a) return
    const signedIn = !!a.signed_in
    if (signedIn === wasSignedIn) return
    const before = wasSignedIn
    wasSignedIn = signedIn
    if (signedIn) {
      lastFocus = Date.now()
      void syncNow()
    } else {
      // Signed out (the Rust side dropped the key too): another account on
      // this device starts clean.
      if (before) clearSyncBookkeeping(localStorage)
      setStatus({ phase: 'signed-out', error: null, lastSyncedAt: 0, items: 0 })
      try { localStorage.removeItem(LAST_KEY) } catch { /* ignore */ }
    }
  })
}
