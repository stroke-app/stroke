// Experimental app-wide Vim mode - shared modal state.
//
// This is intentionally decentralized: each surface (the data grid, the Monaco
// editors, the global/tab layer) interprets Vim keys for its own context and
// reports the active sub-mode here, so the status-bar indicator reflects whoever
// currently owns focus. Only meaningful while `appVimMode` (settings) is on.

import { writable } from 'svelte/store'

/** @typedef {'normal' | 'insert' | 'visual' | 'command'} VimSubMode */

/** Current Vim sub-mode, shared across surfaces for the status-bar indicator. */
export const vimSubMode = writable(/** @type {VimSubMode} */ ('normal'))

/** Status-bar label per sub-mode. */
export const VIM_MODE_LABEL = /** @type {Record<VimSubMode, string>} */ ({
  normal: 'NORMAL',
  insert: 'INSERT',
  visual: 'VISUAL',
  command: 'COMMAND',
})

/** @param {VimSubMode} m */
export function setVimSubMode(m) {
  vimSubMode.set(m)
}

/**
 * True when `el` is a live text-entry target where Vim normal-mode keys must NOT
 * be swallowed: native inputs/textareas/selects, contenteditable, and anything
 * inside an editor that runs Vim itself (CodeMirror's vim, marked `data-vim-editor`).
 * @param {Element | EventTarget | null} el
 */
export function isTextEntryTarget(el) {
  if (!el || !(el instanceof Element)) return false
  const tag = el.tagName
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true
  if (el instanceof HTMLElement && el.isContentEditable) return true
  // An editor with its own Vim owns its modes - never intercept its keys here.
  if (el.closest('[data-vim-editor]')) return true
  return false
}
