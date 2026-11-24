/**
 * Which app this is: Stroke or Stroke Nightly. Nightly's version carries a
 * build stamp, `3.7.7-nightly.202611240456`; Stable's is plain `3.7.7`.
 */
import { derived, writable } from 'svelte/store'

/** The running app's version, '' until Tauri answers (and in browser dev). */
export const appVersion = writable('')

if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) {
  void import('@tauri-apps/api/app')
    .then((m) => m.getVersion())
    .then((v) => appVersion.set(v))
    .catch(() => {})
}

/** @param {string} version */
export function isNightlyVersion(version) {
  return /-nightly\.\d+/.test(version)
}

export const isNightly = derived(appVersion, (v) => isNightlyVersion(v))

