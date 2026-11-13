/**
 * ⌘A / Ctrl+A selects the text of the field you are typing in, everywhere.
 *
 * On macOS the menu bar owns ⌘A (mac_chrome.rs: the stock item swallowed it
 * before the SQL editor and the grid saw it), so a text field only got its
 * select-all back through a round trip: keydown → no handler took it → native
 * menu → IPC event → `menuActions.selectAll`. Anything on the way that called
 * preventDefault, or a field that was not a plain <input>, left ⌘A doing
 * nothing. Here the field is handled on the keydown itself, before any other
 * listener, and the menu never fires for it.
 *
 * Left alone: CodeMirror (its keymap has its own select-all, scoped to the
 * document) and xterm (the terminal page selects the typed line).
 */

const TEXT_INPUT = new Set(['text', 'search', 'url', 'tel', 'email', 'password', 'number', ''])

/** @param {EventTarget | null} t */
function selectIn(t) {
  if (t instanceof HTMLTextAreaElement) {
    if (t.classList.contains('xterm-helper-textarea')) return false
    t.select()
    return true
  }
  if (t instanceof HTMLInputElement) {
    if (!TEXT_INPUT.has(t.type)) return false
    t.select()
    return true
  }
  if (t instanceof HTMLElement && t.isContentEditable) {
    if (t.closest('.cm-editor')) return false
    const host = /** @type {HTMLElement} */ (t.closest('[contenteditable]:not([contenteditable="false"])') ?? t)
    const range = document.createRange()
    range.selectNodeContents(host)
    const sel = window.getSelection()
    sel?.removeAllRanges()
    sel?.addRange(range)
    return true
  }
  return false
}

/** Install once at startup. */
export function installSelectAll() {
  window.addEventListener('keydown', (e) => {
    if (!(e.metaKey || e.ctrlKey) || e.altKey || e.shiftKey || e.key.toLowerCase() !== 'a') return
    if (e.isComposing) return
    // DEBUG-SELECTALL (temporary)
    try {
      const t = /** @type {any} */ (e.target)
      const cs = t instanceof Element ? getComputedStyle(t) : null
      void import('@tauri-apps/api/core').then(({ invoke }) => invoke('perf_log', { line: `DEBUG-SELECTALL target=${t?.tagName}.${t?.type ?? ''} us=${cs?.webkitUserSelect} prevented=${e.defaultPrevented}` })).catch(() => {})
    } catch {}
    if (!selectIn(e.target)) return
    e.preventDefault()
    // The grid's and the shell's ⌘A (select all rows) must not also run.
    e.stopImmediatePropagation()
  }, { capture: true })
}

/** For the menu's Select All when the key did not come through the page. */
export function selectAllInFocus() {
  return selectIn(document.activeElement)
}
