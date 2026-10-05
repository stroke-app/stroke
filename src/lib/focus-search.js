/**
 * "/" focuses the search box for whatever you are looking at.
 *
 * The app has upwards of thirty search and filter fields - the sidebar's table
 * filter, the settings search, the log filter, the connection rail, the object
 * browser, each dialog's own. Binding a shortcut per field would be thirty
 * bindings to keep in step with thirty components, and would still miss the
 * next one somebody adds.
 *
 * So the key resolves its target instead: it looks at what is actually on
 * screen and picks the field that is most plausibly *the* search box there. A
 * field added tomorrow is found without anyone remembering to register it.
 *
 * The scoring is separated from the DOM so it can be tested - the ranking is
 * the part with judgement in it, and the part that will need tuning.
 */

/** Placeholders and labels that read as "this is the search box". */
const SEARCH_WORDS = /\b(search|filter|find)\b/i

/**
 * @typedef {object} SearchCandidate
 * @property {boolean} marked      carries an explicit `data-search-input`
 * @property {string} type         the input's `type`
 * @property {string} placeholder
 * @property {string} label        aria-label, or the accessible name
 * @property {number} docIndex     position in document order, for tie-breaking
 */

/**
 * How strongly a field claims to be the search box here. Higher wins; a score
 * of 0 means "not a search box" and is never focused.
 *
 * Ordering is deliberate. An explicit marker beats a guess, because a component
 * that has opted in knows better than this heuristic does. `type="search"` beats
 * wording, because it is a declaration rather than a coincidence of language.
 * Wording comes last and is still worth having: it covers every field written
 * before this existed.
 *
 * @param {SearchCandidate} c
 * @returns {number}
 */
export function scoreSearchInput(c) {
  if (c.marked) return 100
  if (c.type === 'search') return 80
  if (SEARCH_WORDS.test(c.placeholder)) return 60
  if (SEARCH_WORDS.test(c.label)) return 55
  return 0
}

/**
 * Pick the best candidate, or null when none of them is a search box.
 *
 * Ties go to the earliest in document order. Toolbars sit above the content
 * they filter, so the first match down the page is the one belonging to the
 * surface in front of you rather than one further in.
 *
 * @param {SearchCandidate[]} candidates
 * @returns {SearchCandidate | null}
 */
export function pickSearchInput(candidates) {
  let best = /** @type {SearchCandidate | null} */ (null)
  let bestScore = 0
  for (const c of candidates) {
    const score = scoreSearchInput(c)
    if (score > bestScore || (score === bestScore && score > 0 && best && c.docIndex < best.docIndex)) {
      best = c
      bestScore = score
    }
  }
  return bestScore > 0 ? best : null
}

/**
 * Is the user typing? "/" and "?" are ordinary characters in a text field, so
 * neither may be stolen while one has focus.
 *
 * CodeMirror is checked by container rather than by tag: its content is a
 * contenteditable, and it routes keys through elements that are not text
 * fields at all.
 *
 * @param {EventTarget | null} target
 * @returns {boolean}
 */
export function isTypingTarget(target) {
  if (!(target instanceof HTMLElement)) return false
  if (target.closest('.cm-editor')) return true
  if (target.isContentEditable) return true
  const tag = target.tagName
  return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT'
}

/**
 * The search box for what is currently on screen, if there is one.
 *
 * Scoped to the topmost dialog when one is open: a dialog owns the screen, and
 * focusing a field on the page behind it would type into something the user
 * cannot see.
 *
 * @param {Document | HTMLElement} [root]
 * @returns {HTMLInputElement | null}
 */
export function findSearchInput(root = document) {
  const doc = root instanceof Document ? root : root.ownerDocument
  if (!doc) return null

  const dialogs = [...doc.querySelectorAll('[data-slot="dialog-content"], [role="dialog"]')]
    .filter((d) => d instanceof HTMLElement && d.offsetParent !== null)
  const scope = dialogs.length ? /** @type {HTMLElement} */ (dialogs[dialogs.length - 1]) : root

  /** @type {SearchCandidate[]} */
  const candidates = []
  /** @type {HTMLInputElement[]} */
  const nodes = []

  const all = /** @type {HTMLElement} */ (/** @type {unknown} */ (scope)).querySelectorAll?.('input') ?? []
  let i = 0
  for (const el of all) {
    if (!(el instanceof HTMLInputElement)) continue
    if (el.disabled || el.readOnly) continue
    // offsetParent is null for display:none and for anything inside it, which is
    // how the app hides inactive tabs - exactly the fields that must not win.
    if (el.offsetParent === null) continue
    if (el.getClientRects().length === 0) continue
    candidates.push({
      marked: el.hasAttribute('data-search-input'),
      type: (el.type || 'text').toLowerCase(),
      placeholder: el.placeholder || '',
      label: el.getAttribute('aria-label') || '',
      docIndex: i,
    })
    nodes.push(el)
    i++
  }

  const best = pickSearchInput(candidates)
  return best ? (nodes[best.docIndex] ?? null) : null
}
