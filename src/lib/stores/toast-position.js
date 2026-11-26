/**
 * Which corner notifications appear in (Settings → Appearance). Kept on this
 * device only; a corner is a choice about this screen, not the account.
 */
import { writable } from 'svelte/store'

const KEY = 'stroke:toast-position'

/** @typedef {'top-left' | 'top-right' | 'bottom-left' | 'bottom-right'} ToastPosition */
/** @type {ToastPosition[]} */
export const TOAST_POSITIONS = ['top-left', 'top-right', 'bottom-left', 'bottom-right']

/**
 * Bottom right unless picked: at the top a toast sat over the tab bar and
 * the window controls, the two places most often reached for just after an
 * action that raises one.
 */
const DEFAULT_POSITION = /** @type {ToastPosition} */ ('bottom-right')

/** @returns {ToastPosition} */
function load() {
  try {
    const v = /** @type {any} */ (localStorage.getItem(KEY))
    return TOAST_POSITIONS.includes(v) ? v : DEFAULT_POSITION
  } catch {
    return DEFAULT_POSITION
  }
}

export const toastPosition = writable(load())

toastPosition.subscribe((v) => {
  try {
    localStorage.setItem(KEY, v)
  } catch {}
})
