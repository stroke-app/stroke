/** The export / import dialog (transfer.js): mounted once by the shell, opened from Settings and the palette. */
import { writable } from 'svelte/store'

/** @type {import('svelte/store').Writable<{ open: boolean, mode: 'export' | 'import' }>} */
export const transferDialog = writable({ open: false, mode: 'export' })

/** @param {'export' | 'import'} mode */
export function openTransfer(mode) {
  transferDialog.set({ open: true, mode })
}
