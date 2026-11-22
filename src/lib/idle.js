// Run non-urgent work when the user is not interacting.
//
// WKWebView (the engine the app ships on, macOS) has no requestIdleCallback, so
// the usual `setTimeout` fallback fires in the middle of a click or a scroll.
// This one waits for a short quiet spell after the last input instead, and
// still runs by `timeout` at the latest.

const QUIET_MS = 250
/** A timer this late means the main thread was busy rather than idle. */
const BUSY_LAG_MS = 25

let lastInput = 0
let listening = false

function listen() {
  if (listening || typeof window === 'undefined') return
  listening = true
  const note = () => { lastInput = performance.now() }
  for (const type of ['pointerdown', 'keydown', 'wheel']) {
    window.addEventListener(type, note, { capture: true, passive: true })
  }
}

/**
 * Schedule `fn` for an idle moment. Returns a cancel function.
 * @param {() => void} fn
 * @param {{ timeout?: number }} [opts] latest delay, in ms
 */
export function whenIdle(fn, { timeout = 2000 } = {}) {
  if (typeof window === 'undefined') { fn(); return () => {} }
  if (typeof window.requestIdleCallback === 'function') {
    const id = window.requestIdleCallback(() => fn(), { timeout })
    return () => window.cancelIdleCallback(id)
  }
  listen()
  const start = performance.now()
  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let timer
  /** When the pending check was due; a check that runs late found the thread busy. */
  let dueAt = start + 50
  const schedule = (/** @type {number} */ ms) => { dueAt = performance.now() + ms; timer = setTimeout(check, ms) }
  const check = () => {
    const now = performance.now()
    if (now - start >= timeout) { fn(); return }
    // No input is not the same as idle. A connect renders a whole page with no
    // input at all, and the chunk warmer used to parse editors in the middle of
    // it. A timer that fires well after it was due means the thread was busy
    // until just now: look again shortly.
    const busy = now - dueAt > BUSY_LAG_MS
    const quietFor = now - lastInput
    if (!busy && quietFor >= QUIET_MS) { fn(); return }
    schedule(busy ? 50 : Math.min(QUIET_MS - quietFor, timeout - (now - start)) + 1)
  }
  schedule(50)
  return () => clearTimeout(timer)
}
