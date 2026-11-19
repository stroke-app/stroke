import { invoke } from '@tauri-apps/api/core'
import { writable, derived } from 'svelte/store'

/**
 * @typedef {{ status: 'Valid', email: string, plan: string, issued_at: number, expires_at: number|null }
 *          | { status: 'Trial', days_remaining: number }
 *          | { status: 'TrialExpired' }
 *          | { status: 'Error', message: string }
 *          | null} LicenseStatus
 */

/** @type {import('svelte/store').Writable<LicenseStatus>} */
export const licenseStatus = writable(null)

export const isTrialActive = derived(
  licenseStatus,
  ($s) => $s?.status === 'Trial',
)

export const isBlocked = derived(
  licenseStatus,
  ($s) => $s !== null && $s.status === 'TrialExpired',
)

/** Fetch the current status from Tauri and update the store. */
export async function refreshLicenseStatus() {
  try {
    const status = await invoke('check_license_status')
    licenseStatus.set(/** @type {LicenseStatus} */ (status))
    return status
  } catch (e) {
    // Keep the last known status - flipping to Error here would drop Pro
    // access (trial or paid) over a transient IPC failure.
    console.error('[license] status check failed:', e)
    return null
  }
}

// ── Free-tier feature gating ─────────────────────────────────────────────────

export const FREE_CONNECTION_LIMIT = 3

/**
 * Reactive: true when the account has Pro access (paid, or its trial is running).
 *
 * Fails OPEN: only a definitive `TrialExpired` verdict locks Pro features.
 * `null` (status still loading) and `Error` (data dir inaccessible, transient
 * check failure) must never lock out an active-trial or paid user - an
 * expired trial is the only state where we positively know there's no Pro.
 */
export const hasPro = derived(
  licenseStatus,
  ($s) => $s?.status !== 'TrialExpired',
)

/**
 * Reactive: Pro that was paid for. The trial does not count: sync and
 * scheduled backups need this (the server checks it too).
 */
export const hasPaidPro = derived(
  licenseStatus,
  ($s) => $s?.status === 'Valid',
)
