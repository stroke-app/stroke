/**
 * An "are you sure?" that a script can await.
 *
 * `window.confirm` is not the browser's in the Tauri webview: the dialog plugin
 * replaces it with an async function, so `if (!confirm(...)) return` tests a
 * Promise, which is always truthy, and the guard never stops anything. That is
 * how "Delete all conversations?" deleted them without asking. This store is
 * what those call sites await instead; `ConfirmHost.svelte`, mounted once in
 * App.svelte, draws the request with the app's own ConfirmDialog.
 */
import { writable } from 'svelte/store'

/**
 * @typedef {{
 *   title: string,
 *   description?: string,
 *   confirmLabel?: string,
 *   destructive?: boolean,
 * }} ConfirmRequest
 * @typedef {ConfirmRequest & { resolve: (ok: boolean) => void }} PendingConfirm
 */

/** @type {import('svelte/store').Writable<PendingConfirm | null>} */
export const pendingConfirm = writable(null)

/**
 * Ask, and resolve true only when the confirm button is pressed. A second ask
 * while one is open answers the first with false.
 * @param {ConfirmRequest} req
 * @returns {Promise<boolean>}
 */
export function askConfirm(req) {
  return new Promise((resolve) => {
    pendingConfirm.update((prev) => {
      prev?.resolve(false)
      return { ...req, resolve }
    })
  })
}

/** Answer the open request (the host calls this). @param {boolean} ok */
export function answerConfirm(ok) {
  pendingConfirm.update((prev) => {
    prev?.resolve(ok)
    return null
  })
}
