/**
 * Uniform text-editing shortcuts for every plain input/textarea in the app.
 *
 * macOS:
 *   Option+Backspace          → delete previous word
 *   Option+Delete             → delete next word
 *   Cmd+Backspace             → delete to start of line
 *   Cmd+Delete                → delete to end of line
 *   Cmd+Shift+Backspace       → clear the whole field
 *   Cmd+Z / Cmd+Shift+Z       → undo / redo
 * Windows / Linux:
 *   Ctrl+Backspace            → delete previous word
 *   Ctrl+Delete               → delete next word
 *   Alt+Backspace / Alt+Delete → delete previous / next word
 *   Ctrl+Shift+Backspace      → clear the whole field
 *   Ctrl+Z / Ctrl+Shift+Z     → undo / redo
 *   Ctrl+Y                    → redo
 *
 * Why this module exists:
 *  1. The Tauri webview does not give Svelte-bound inputs a usable native undo
 *     stack - reassigning `value` from a binding clobbers it - so undo/redo is
 *     backed by a per-element stack here.
 *  2. Word/line deletion is inconsistent across the platform webviews, so it is
 *     implemented directly.
 *  3. The global hotkey layer (@tanstack/hotkeys) binds keydown on `document`
 *     in the BUBBLE phase and, by default, does NOT ignore inputs for Ctrl/Cmd
 *     combos. These listeners run in the CAPTURE phase and stopPropagation() the
 *     editing chords, so the hotkey layer can never steal them from a focused
 *     field. Non-editing chords (Cmd+K, Cmd+F, …) are untouched and still fire.
 *
 * CodeMirror editors are skipped - they ship their own editing model.
 * contenteditable is shielded from the hotkey layer but left to native editing.
 */

import { detectOs } from './platform.js'

const IS_MAC = detectOs() === 'macos'
const MAX_STACK = 200
const COALESCE_MS = 350

/** @type {WeakMap<HTMLInputElement | HTMLTextAreaElement, { stack: string[], idx: number, t: number, suppress: boolean }>} */
const histories = new WeakMap()

const TEXT_INPUT_TYPES = new Set(['text', 'search', 'url', 'tel', 'email', 'password', ''])

/** 'field' = plain input/textarea we fully manage; 'ce' = contenteditable (native); null = not editable. */
function editableKind(/** @type {Element} */ el) {
  if (el instanceof HTMLTextAreaElement) return 'field'
  if (el instanceof HTMLInputElement) return TEXT_INPUT_TYPES.has((el.type || 'text').toLowerCase()) ? 'field' : null
  if (el instanceof HTMLElement && el.isContentEditable) return 'ce'
  return null
}

function inManagedEditor(/** @type {Element} */ el) {
  // CodeMirror manages its text surface (.cm-content), not the inputs in its
  // panels: matching all of .cm-editor left the find/replace fields with no
  // word delete, no undo and no shield from the global hotkeys.
  // xterm.js's hidden textarea is a terminal's keyboard, not a text field: its
  // keys belong to the client in the terminal (psql does its own line editing).
  // Managing it here swallowed Ctrl+Backspace, Ctrl+Z and the rest before
  // xterm ever saw them.
  return !!el.closest?.('.cm-content, .xterm')
}

/** @param {HTMLInputElement | HTMLTextAreaElement} el */
function getHistory(el) {
  let h = histories.get(el)
  if (!h) {
    h = { stack: [el.value], idx: 0, t: 0, suppress: false }
    histories.set(el, h)
  }
  return h
}

/**
 * Capture the element's current value as an undo step.
 * Bursts of typing coalesce into one step; `discrete` forces a fresh step.
 */
function record(/** @type {HTMLInputElement | HTMLTextAreaElement} */ el, discrete = false) {
  const h = getHistory(el)
  if (h.suppress) return
  const val = el.value
  if (val === h.stack[h.idx]) return
  const now = performance.now()
  if (!discrete && now - h.t < COALESCE_MS && h.idx === h.stack.length - 1 && h.idx > 0) {
    h.stack[h.idx] = val // coalesce a burst of typing into one step
  } else {
    h.stack.length = h.idx + 1 // drop any redo tail
    h.stack.push(val)
    if (h.stack.length > MAX_STACK) h.stack.shift()
    h.idx = h.stack.length - 1
  }
  h.t = now
}

/** Programmatically set the value + caret and notify bindings, without re-recording. */
function applyValue(/** @type {HTMLInputElement | HTMLTextAreaElement} */ el, value, caret) {
  const h = getHistory(el)
  h.suppress = true
  el.value = value
  const pos = caret ?? value.length
  try { el.setSelectionRange(pos, pos) } catch { /* number/unsupported */ }
  el.dispatchEvent(new Event('input', { bubbles: true }))
  h.suppress = false
}

function undo(/** @type {HTMLInputElement | HTMLTextAreaElement} */ el) {
  const h = getHistory(el)
  if (el.value !== h.stack[h.idx]) record(el) // commit any pending edit first
  if (h.idx <= 0) return
  h.idx--
  applyValue(el, h.stack[h.idx])
  h.t = 0
}

function redo(/** @type {HTMLInputElement | HTMLTextAreaElement} */ el) {
  const h = getHistory(el)
  if (h.idx >= h.stack.length - 1) return
  h.idx++
  applyValue(el, h.stack[h.idx])
  h.t = 0
}

const isWs = (/** @type {string} */ c) => /\s/.test(c)

/**
 * Delete text relative to the caret.
 * @param {HTMLInputElement | HTMLTextAreaElement} el
 * @param {'word' | 'line' | 'all'} mode
 * @param {boolean} forward  true = delete toward end (Delete), false = toward start (Backspace)
 */
function deleteBy(el, mode, forward) {
  const v = el.value
  const start = el.selectionStart ?? v.length
  const end = el.selectionEnd ?? start

  // A non-empty selection is removed wholesale, matching native behaviour.
  if (start !== end) {
    applyValue(el, v.slice(0, start) + v.slice(end), start)
    record(el, true)
    return
  }

  let from = start
  let to = start
  if (mode === 'all') {
    from = 0
    to = v.length
  } else if (mode === 'word') {
    if (forward) {
      let j = start
      while (j < v.length && isWs(v[j])) j++ // eat leading whitespace
      while (j < v.length && !isWs(v[j])) j++ // eat the word
      to = j
    } else {
      let i = start
      while (i > 0 && isWs(v[i - 1])) i-- // eat trailing whitespace
      while (i > 0 && !isWs(v[i - 1])) i-- // eat the word
      from = i
    }
  } else { // line
    if (forward) {
      const nl = v.indexOf('\n', start)
      to = nl === -1 ? v.length : nl
    } else {
      const nl = v.lastIndexOf('\n', start - 1)
      from = nl === -1 ? 0 : nl + 1
    }
  }

  if (from === to) return
  applyValue(el, v.slice(0, from) + v.slice(to), from)
  record(el, true)
}

/** @param {KeyboardEvent} e */
function onKeyDown(e) {
  if (e.isComposing) return
  const el = e.target
  if (!(el instanceof HTMLElement) || inManagedEditor(el)) return
  const kind = editableKind(el)
  if (!kind) return
  const key = e.key.toLowerCase()

  // Undo / redo - Ctrl/Cmd without Alt.
  if ((e.ctrlKey || e.metaKey) && !e.altKey && (key === 'z' || key === 'y')) {
    e.stopPropagation() // shield from the bubble-phase global hotkey layer
    if (kind === 'ce') return // let native contenteditable undo run
    e.preventDefault()
    if (key === 'y' || e.shiftKey) redo(el)
    else undo(el)
    return
  }

  // Word / line deletion.
  if (key === 'backspace' || key === 'delete') {
    const forward = key === 'delete'
    /** @type {'word' | 'line' | 'all' | null} */
    let mode = null
    if (IS_MAC) {
      if (e.metaKey && e.shiftKey) mode = 'all'
      else if (e.metaKey && !e.altKey) mode = 'line'
      else if (e.altKey && !e.metaKey) mode = 'word'
    } else {
      if (e.ctrlKey && e.shiftKey) mode = 'all'
      else if (e.ctrlKey && !e.altKey) mode = 'word'
      // Alt+Backspace deletes a word here too, not only on macOS. Muscle memory
      // is the smaller reason; the real one is that the app binds Alt+Backspace
      // globally to "discard staged changes", and an unclaimed chord reaches the
      // hotkey layer - so pressing it while typing threw the row away instead of
      // deleting a word. Claiming it is what stops the field being overruled.
      else if (e.altKey && !e.ctrlKey && !e.metaKey) mode = 'word'
    }
    if (!mode) return
    e.stopPropagation() // shield from the bubble-phase global hotkey layer
    if (kind === 'ce') return // native handles contenteditable deletion
    e.preventDefault()
    deleteBy(el, mode, forward)
  }
}

/** @param {Event} e */
function onInput(e) {
  const el = e.target
  if (!(el instanceof HTMLElement) || inManagedEditor(el)) return
  if (editableKind(el) !== 'field') return
  record(/** @type {HTMLInputElement | HTMLTextAreaElement} */ (el))
}

/**
 * Forget an element's undo history.
 *
 * The stack is keyed by ELEMENT, which is right for a field that holds one
 * value for its lifetime and wrong for one that is re-pointed at a new value
 * while staying mounted - the cell editor dock follows the grid's cursor, so
 * without this an undo in row 12 could restore row 8's text.
 * @param {HTMLInputElement | HTMLTextAreaElement | null} el
 */
export function resetInputHistory(el) {
  if (!el) return
  histories.set(el, { stack: [el.value], idx: 0, t: 0, suppress: false })
}

/** Install global listeners. Returns a cleanup function. */
export function installInputShortcuts() {
  document.addEventListener('keydown', onKeyDown, true)
  document.addEventListener('input', onInput, true)
  return () => {
    document.removeEventListener('keydown', onKeyDown, true)
    document.removeEventListener('input', onInput, true)
  }
}
