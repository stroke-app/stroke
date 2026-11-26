// `/` focuses the search field in view, as on GitHub, YouTube and in most
// developer tools. Fields opt in with `data-slash-focus` (a number: the higher
// one wins when several are visible, so the page's search beats the sidebar's).
// Inside an open dialog only that dialog's fields count: the page behind it
// isn't reachable anyway.

import { findSearchInput } from './focus-search.js'

/** The sidebar's filter ranks 10: it has its own key (Mod+Shift+F). */
const SIDEBAR_RANK = 10

/** @param {Element | null} el */
export function isEditable(el) {
  if (!el || !(el instanceof HTMLElement)) return false
  if (el.isContentEditable) return true
  const tag = el.tagName
  if (tag === 'TEXTAREA' || tag === 'SELECT') return true
  if (tag !== 'INPUT') return false
  const type = /** @type {HTMLInputElement} */ (el).type
  return !['checkbox', 'radio', 'button', 'submit', 'reset', 'range', 'color', 'file'].includes(type)
}

/** @param {HTMLElement} el */
function visible(el) {
  if (/** @type {HTMLInputElement} */ (el).disabled) return false
  if (!el.getClientRects().length) return false
  const s = getComputedStyle(el)
  return s.visibility !== 'hidden' && s.display !== 'none'
}

/**
 * The field `/` should focus, or null.
 * @param {ParentNode} [root]
 * @returns {HTMLElement | null}
 */
export function slashTarget(root = document) {
  const dialogs = [...root.querySelectorAll('[role="dialog"], [role="alertdialog"]')].filter(
    (d) => d instanceof HTMLElement && visible(d),
  )
  const scope = /** @type {ParentNode} */ (dialogs.at(-1) ?? root)
  /** @type {HTMLElement | null} */
  let best = null
  let bestRank = -Infinity
  for (const el of scope.querySelectorAll('[data-slash-focus]')) {
    if (!(el instanceof HTMLElement) || !visible(el)) continue
    const rank = Number(el.dataset.slashFocus) || 0
    if (rank > bestRank) {
      best = el
      bestRank = rank
    }
  }
  return best
}

/**
 * The field `/` focuses: the tab or dialog in front first, and the sidebar
 * filter only when nothing else on screen is a search box. Fields that opted
 * in with a rank win; any other search-looking field in the page beats the
 * sidebar.
 * @returns {HTMLElement | null}
 */
export function slashFocusTarget() {
  const ranked = slashTarget()
  if (ranked && (Number(ranked.dataset.slashFocus) || 0) > SIDEBAR_RANK) return ranked
  // The page's own fields only: the sidebar's other searches (its connection
  // list) are not the page, and on a page with no search field one of them
  // took `/` from the table filter.
  const page = findSearchInput(document, (el) =>
    ((Number(el.dataset.slashFocus) || 0) <= SIDEBAR_RANK && el.hasAttribute('data-slash-focus'))
    || !!el.closest('[data-studio-region="sidebar"]'))
  return page ?? ranked
}

/**
 * Install the `/` handler; returns its remover.
 * @param {Window} [win]
 */
export function installSlashFocus(win = window) {
  /** @param {KeyboardEvent} e */
  function onKey(e) {
    if (e.key !== '/' || e.metaKey || e.ctrlKey || e.altKey || e.isComposing) return
    if (e.defaultPrevented || isEditable(document.activeElement)) return
    // Monaco and other editors take keys on a hidden textarea; that is caught
    // above. Listening in the capture phase, because focused chrome (the
    // titlebar, the grid) stops keys from bubbling. A page's own `/` (the AI
    // chat focuses its box) runs later in the same event, so the field is
    // focused after the event, and only if nothing else took focus.
    const target = slashFocusTarget()
    if (!target) return
    e.preventDefault()
    setTimeout(() => {
      if (isEditable(document.activeElement)) return
      target.focus()
      if (target instanceof HTMLInputElement) target.select()
    }, 0)
  }
  win.addEventListener('keydown', onKey, true)
  return () => win.removeEventListener('keydown', onKey, true)
}
