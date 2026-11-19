/**
 * Shift + wheel scrolls sideways, everywhere.
 *
 * Whether a webview turns a vertical wheel with Shift held into horizontal
 * scrolling depends on the OS, the webview and the mouse, so a wide result,
 * the related-rows panel or a settings table could only be scrolled sideways by
 * dragging its scrollbar. This does the conversion when nothing else did: the
 * nearest element under the pointer that can scroll sideways moves.
 *
 * The listener is non-passive (it has to preventDefault), and a non-passive
 * wheel listener puts the main thread in front of every wheel tick. So it is
 * attached only while Shift is held, the way the grid gates its own.
 */

/** A "line" of wheel delta (deltaMode 1). */
const LINE_PX = 16

/**
 * The element to move: the nearest ancestor of `target` that overflows
 * sideways and lets the user scroll it (overflow-x auto or scroll).
 * @param {EventTarget | null} target
 * @returns {HTMLElement | null}
 */
export function horizontalScroller(target) {
  let el = target instanceof Element ? target : null
  while (el && el !== document.documentElement) {
    if (el instanceof HTMLElement && el.scrollWidth > el.clientWidth + 1) {
      const ox = getComputedStyle(el).overflowX
      if (ox === 'auto' || ox === 'scroll') return el
    }
    el = el.parentElement
  }
  return null
}

/** @param {WheelEvent} e */
function onWheel(e) {
  // Something already handled it (the data grid has its own Shift handling),
  // or the device already sent sideways movement.
  if (e.defaultPrevented || !e.shiftKey || e.ctrlKey || e.metaKey || e.deltaX !== 0 || e.deltaY === 0) return
  const el = horizontalScroller(e.target)
  if (!el) return
  const scale = e.deltaMode === 1 ? LINE_PX : e.deltaMode === 2 ? el.clientWidth : 1
  e.preventDefault()
  el.scrollLeft += e.deltaY * scale
}

let attached = false
function attach() {
  if (attached) return
  attached = true
  window.addEventListener('wheel', onWheel, { passive: false })
}
function detach() {
  if (!attached) return
  attached = false
  window.removeEventListener('wheel', onWheel)
}

/** Install once at startup. Returns the uninstaller. */
export function installShiftWheel() {
  /** @param {KeyboardEvent} e */
  const onKey = (e) => (e.shiftKey ? attach() : detach())
  window.addEventListener('keydown', onKey, true)
  window.addEventListener('keyup', onKey, true)
  window.addEventListener('blur', detach)
  return () => {
    detach()
    window.removeEventListener('keydown', onKey, true)
    window.removeEventListener('keyup', onKey, true)
    window.removeEventListener('blur', detach)
  }
}
