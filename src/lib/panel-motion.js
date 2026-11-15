/**
 * Open and close a docked panel the way a native split view does: the panel's
 * edge moves, and what is inside keeps its final size and is clipped, so it
 * never reflows (or, for the terminal, re-measures and repaints) frame by frame.
 *
 * The caller keeps the panel in the layout for the whole animation and hides
 * it once `closePanel` resolves. Motion follows the app's Motion setting and,
 * on "System", the OS's reduced-motion preference: then it is instant.
 */

/** macOS's sheet and sidebar curve: quick out of the gate, a long soft landing. */
export const PANEL_EASE = 'cubic-bezier(0.32, 0.72, 0, 1)'
export const PANEL_MS = 220

/** @returns {boolean} */
export function panelMotionOff() {
  if (typeof document === 'undefined') return true
  // Settings → Appearance → Animate panels, off.
  if (document.documentElement.getAttribute('data-panel-motion') === 'off') return true
  const pref = document.documentElement.getAttribute('data-motion')
  if (pref === 'full') return false
  if (pref === 'reduced') return true
  try { return window.matchMedia('(prefers-reduced-motion: reduce)').matches } catch { return false }
}

/** @type {WeakMap<HTMLElement, Animation>} */
const running = new WeakMap()

/**
 * @param {HTMLElement} el
 * @param {'width' | 'height'} prop
 * @param {number} from @param {number} to
 * @returns {Promise<void>}
 */
function run(el, prop, from, to) {
  // Picking up mid-flight: start from where the edge is now, not from the end.
  const prev = running.get(el)
  if (prev) {
    from = prop === 'width' ? el.getBoundingClientRect().width : el.getBoundingClientRect().height
    prev.cancel()
  }
  if (panelMotionOff() || typeof el.animate !== 'function' || from === to) return Promise.resolve()
  const overflow = el.style.overflow
  el.style.overflow = 'hidden'
  const anim = el.animate([{ [prop]: `${from}px` }, { [prop]: `${to}px` }], { duration: PANEL_MS, easing: PANEL_EASE })
  running.set(el, anim)
  return new Promise((resolve) => {
    const done = () => {
      if (running.get(el) === anim) {
        running.delete(el)
        el.style.overflow = overflow
      }
      resolve()
    }
    anim.onfinish = done
    anim.oncancel = done
  })
}

/**
 * Animate a panel open from 0 to its laid-out size. Call once it is visible.
 * @param {HTMLElement | null | undefined} el @param {'width' | 'height'} prop
 */
export function openPanel(el, prop) {
  if (!el) return Promise.resolve()
  const size = prop === 'width' ? el.offsetWidth : el.offsetHeight
  return run(el, prop, 0, size)
}

/**
 * Animate a panel closed from its size to 0. Hide it when this resolves.
 * @param {HTMLElement | null | undefined} el @param {'width' | 'height'} prop
 */
export function closePanel(el, prop) {
  if (!el) return Promise.resolve()
  const size = prop === 'width' ? el.offsetWidth : el.offsetHeight
  return run(el, prop, size, 0)
}

/**
 * Slide a panel in or out from the window edge it is docked to, with a
 * transform only: the compositor moves it, nothing is laid out per frame. The
 * caller lays the panel over the content while it moves (absolutely
 * positioned), so the page beside it changes size once, not on every frame.
 * @param {HTMLElement | null | undefined} el
 * @param {'bottom' | 'right'} edge
 * @param {boolean} open
 * @returns {Promise<void>}
 */
export function slidePanel(el, edge, open) {
  if (!el || panelMotionOff() || typeof el.animate !== 'function') return Promise.resolve()
  const prev = running.get(el)
  prev?.cancel()
  const away = edge === 'bottom' ? 'translateY(100%)' : 'translateX(100%)'
  const frames = open ? [{ transform: away }, { transform: 'none' }] : [{ transform: 'none' }, { transform: away }]
  const anim = el.animate(frames, { duration: PANEL_MS, easing: PANEL_EASE, fill: 'forwards' })
  running.set(el, anim)
  return new Promise((resolve) => {
    const done = () => {
      if (running.get(el) === anim) running.delete(el)
      // Leave no transform behind once it has landed open.
      if (open) anim.cancel()
      resolve()
    }
    anim.onfinish = done
    anim.oncancel = () => resolve()
  })
}

