/**
 * The app's own eased scrolling, used by the grid and the sidebar unless the user
 * turns native scrolling on (Settings → Appearance).
 *
 * Wheel ticks are accumulated into a target offset and the element is walked
 * toward it one animation frame at a time, so a notch glides instead of jumping.
 * Native scrolling is the escape hatch: an OS that already does momentum well
 * (macOS trackpads), or an input method whose behaviour we have no business
 * overriding, is better left alone.
 *
 * Cost of admission, and why this is opt-out rather than opt-in everywhere:
 * animating the offset ourselves means a NON-PASSIVE wheel listener, which puts
 * the main thread in front of every wheel tick. The canvas grid is sensitive to
 * exactly that (see DataTable's wheel effect), so the listener is attached only
 * while eased scrolling is actually enabled, and the grid keeps repainting off
 * `scroll` events as before - this module only moves scrollTop.
 */

/** Fraction of the remaining distance covered per 60Hz frame. */
const EASE = 0.22
/** The frame the EASE above is quoted against, in ms. */
const EASE_REF_MS = 1000 / 60
/** Cap on a single frame's step, so a stalled tab doesn't teleport on resume. */
const MAX_FRAME_MS = 64
/** Floor on a frame's step. A clock too coarse to resolve the gap between two
 *  frames would otherwise report 0ms, and an ease of zero never converges. */
const MIN_FRAME_MS = 4
/** Below this many pixels the animation stops and snaps. */
const EPSILON = 0.5
/** A "line" of wheel delta, for deltaMode 1 (Firefox / some Linux mice). */
const LINE_PX = 16
/** Ceiling on the queued distance, so spinning the wheel can't wind up seconds of travel. */
const MAX_PENDING_SCREENS = 3

/**
 * Wheel delta in CSS pixels. Wheel events arrive in pixels, lines or pages
 * depending on the browser and device; a scroller that assumes pixels moves by a
 * few pixels per notch on the ones that don't.
 * @param {WheelEvent} e @param {number} viewport size of the scrollport along this axis
 * @returns {{ dx: number, dy: number }}
 */
export function wheelPixels(e, viewport) {
  const scale = e.deltaMode === 1 ? LINE_PX : e.deltaMode === 2 ? Math.max(1, viewport) : 1
  return { dx: e.deltaX * scale, dy: e.deltaY * scale }
}

/**
 * Eased scroller bound to one element.
 * @param {HTMLElement} el
 * @returns {{
 *   push: (dx: number, dy: number) => void,
 *   sync: () => void,
 *   stop: () => void,
 *   animating: () => boolean,
 * }}
 */
export function createSmoothScroll(el) {
  // The animation keeps its OWN float position rather than reading scrollTop back
  // each frame. An element quantizes the offset it was given, so a step small
  // enough to round away (a trackpad tick, or the tail of any ease) would leave
  // the measured distance unchanged: the loop then runs forever without moving
  // and scrolling appears dead. Owning the float means every frame makes progress
  // and the animation always converges.
  // Not read from the element here: this runs while the grid mounts, and
  // reading scrollTop then forced a layout in the middle of that render. The
  // first push() adopts the real position (sync) before moving.
  let targetTop = 0
  let targetLeft = 0
  let curTop = 0
  let curLeft = 0
  let raf = 0
  /** Timestamp of the last animated frame, 0 when the animation is at rest. */
  let _lastFrame = 0

  const maxTop = () => Math.max(0, el.scrollHeight - el.clientHeight)
  const maxLeft = () => Math.max(0, el.scrollWidth - el.clientWidth)
  const clamp = (/** @type {number} */ v, /** @type {number} */ max) => (v < 0 ? 0 : v > max ? max : v)
  /** Someone who asked for less motion gets none: jump, don't ease.
   *  The query is resolved once and kept live by the MediaQueryList itself -
   *  calling matchMedia() per wheel tick cost ~5us of style resolution each time,
   *  on the one path that must not touch the style system. */
  const motionQuery =
    typeof matchMedia === 'function' ? matchMedia('(prefers-reduced-motion: reduce)') : null
  const reduceMotion = () => motionQuery !== null && motionQuery.matches

  /** Adopt the element's current position - after a programmatic jump, a drag of
   *  the scrollbar, or a keyboard scroll, our position is stale and would yank the
   *  view back to where the wheel had last aimed it. */
  function sync() {
    targetTop = el.scrollTop
    targetLeft = el.scrollLeft
    curTop = targetTop
    curLeft = targetLeft
  }

  function stop() {
    if (raf) cancelAnimationFrame(raf)
    raf = 0
    _lastFrame = 0
  }

  function frame() {
    // Clock read here rather than from rAF's timestamp argument: a scheduler that
    // calls the callback with no argument (jsdom, a test stub, a polyfill) would
    // otherwise make `dt` NaN and the animation would never converge.
    const now = typeof performance !== 'undefined' ? performance.now() : Date.now()
    // Ease by ELAPSED TIME, not per frame. A fixed fraction per frame means the
    // animation converges twice as fast on a 120Hz display as on a 60Hz one, so
    // the same wheel notch travelled a different distance per unit time - and on
    // ProMotion, where most of this app runs, it also halved the ease the feel
    // was tuned for. Normalising against a 60Hz frame makes the curve identical
    // at any refresh rate.
    const dt =
      _lastFrame === 0
        ? EASE_REF_MS
        : Math.min(MAX_FRAME_MS, Math.max(MIN_FRAME_MS, now - _lastFrame))
    _lastFrame = now
    const ease = 1 - Math.pow(1 - EASE, dt / EASE_REF_MS)
    const dTop = targetTop - curTop
    const dLeft = targetLeft - curLeft
    if (Math.abs(dTop) < EPSILON && Math.abs(dLeft) < EPSILON) {
      curTop = targetTop
      curLeft = targetLeft
      // Land on a whole pixel: the grid draws text at integer offsets, and a
      // fractional one makes WebKit re-antialias every glyph.
      el.scrollTop = Math.round(curTop)
      el.scrollLeft = Math.round(curLeft)
      raf = 0
      _lastFrame = 0
      return
    }
    // Re-clamp as we go: content can shrink under us (a filter, a smaller page).
    curTop = clamp(curTop + dTop * ease, maxTop())
    curLeft = clamp(curLeft + dLeft * ease, maxLeft())
    // Written FRACTIONAL on purpose, and rounded only at the landing below.
    //
    // Rounding here instead looks like the obvious fix for the canvas drawing text
    // at integer offsets, and it is not. The ease moves curTop by a shrinking
    // fraction each frame, so the tail of every scroll advances by less than a
    // pixel per frame - at 120Hz, where each frame carries about half the delta it
    // did at 60. Rounding the write turns that tail into 0,0,1,0,1 and the whole
    // gesture ends in visible steps. The sub-pixel offset this leaves against the
    // canvas is under half a pixel; the stepping it would cost is not.
    el.scrollTop = curTop
    el.scrollLeft = curLeft
    raf = requestAnimationFrame(frame)
  }

  /** Queue `dx`/`dy` pixels of travel and keep the animation running. */
  function push(dx, dy) {
    // Anything that moved the element without going through us (keyboard,
    // scrollbar drag, scrollIntoView) has to be adopted before adding to it.
    if (!raf) sync()
    const capY = Math.max(1, el.clientHeight) * MAX_PENDING_SCREENS
    const capX = Math.max(1, el.clientWidth) * MAX_PENDING_SCREENS
    if (dy) targetTop = clamp(Math.min(curTop + capY, Math.max(curTop - capY, targetTop + dy)), maxTop())
    if (dx) targetLeft = clamp(Math.min(curLeft + capX, Math.max(curLeft - capX, targetLeft + dx)), maxLeft())
    if (reduceMotion()) {
      stop()
      curTop = targetTop
      curLeft = targetLeft
      el.scrollTop = Math.round(curTop)
      el.scrollLeft = Math.round(curLeft)
      return
    }
    if (!raf) raf = requestAnimationFrame(frame)
  }

  return { push, sync, stop, animating: () => raf !== 0 }
}

/**
 * Svelte action: eased wheel scrolling for a plain overflow container (the
 * sidebar and the other scrolling panels). The grid drives `createSmoothScroll`
 * itself, because its wheel handling also owns ctrl-zoom and shift-to-pan.
 *
 * `enabled` is reactive - pass `!$appNativeScroll`. When false the listener is
 * removed entirely, leaving the OS in charge with nothing of ours in the path.
 *
 * @param {HTMLElement} node
 * @param {{ enabled?: boolean, axis?: 'y' | 'x' | 'both' }} [params]
 */
export function smoothScroll(node, params = {}) {
  let enabled = params.enabled !== false
  let axis = params.axis ?? 'y'
  let scroller = createSmoothScroll(node)

  /** @param {WheelEvent} e */
  function onWheel(e) {
    // Ctrl+wheel is a zoom gesture (app zoom / trackpad pinch), never a scroll.
    if (e.ctrlKey || e.metaKey) return
    // Let a nested scroller that can actually move have the event.
    const inner = e.target instanceof Element ? e.target.closest('[data-native-scroll]') : null
    if (inner && inner !== node) return
    const { dx, dy } = wheelPixels(e, node.clientHeight)
    // Shift turns a vertical wheel sideways on either axis setting. On Windows
    // and Linux the delta stays on Y with Shift held, so `both` scrolled down.
    const wantX = axis === 'x' || (axis === 'both' && (e.shiftKey || Math.abs(dx) > Math.abs(dy))) || (axis === 'y' && e.shiftKey)
    const canScrollY = node.scrollHeight > node.clientHeight
    const canScrollX = node.scrollWidth > node.clientWidth
    if (wantX) {
      if (!canScrollX) return
      // Shift+wheel arrives as vertical delta; on the x axis it IS the movement.
      e.preventDefault()
      scroller.push(dx || dy, 0)
      return
    }
    if (!canScrollY) return
    e.preventDefault()
    scroller.push(0, dy)
  }

  // A press interrupts coasting: grabbing the scrollbar, or clicking something
  // while the tail of an ease is still running, should take effect now rather than
  // fight an animation that's still writing the offset.
  const onInterrupt = () => { scroller.stop(); scroller.sync() }

  function attach() {
    node.addEventListener('wheel', onWheel, { passive: false })
    node.addEventListener('pointerdown', onInterrupt, { passive: true })
  }
  function detach() {
    node.removeEventListener('wheel', onWheel)
    node.removeEventListener('pointerdown', onInterrupt)
    scroller.stop()
  }
  // Any scroll we didn't cause invalidates the target we were easing toward.
  /** @param {Event} _e */
  const onScroll = (_e) => { if (!scroller.animating()) scroller.sync() }
  node.addEventListener('scroll', onScroll, { passive: true })
  if (enabled) attach()

  return {
    /** @param {{ enabled?: boolean, axis?: 'y' | 'x' | 'both' }} next */
    update(next = {}) {
      const nextEnabled = next.enabled !== false
      axis = next.axis ?? axis
      if (nextEnabled === enabled) return
      enabled = nextEnabled
      if (enabled) { scroller.sync(); attach() } else detach()
    },
    destroy() {
      detach()
      node.removeEventListener('scroll', onScroll)
    },
  }
}
