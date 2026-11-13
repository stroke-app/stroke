// Run non-urgent work when the user is not interacting.
//
// WKWebView (the engine the app ships on, macOS) has no requestIdleCallback, so
// the usual `setTimeout` fallback fires in the middle of a click or a scroll.
// This one waits for a short quiet spell after the last input instead, and
// still runs by `timeout` at the latest.

const QUIET_MS = 250

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
  const check = () => {
    const now = performance.now()
    const quietFor = now - lastInput
    if (quietFor >= QUIET_MS || now - start >= timeout) { fn(); return }
    timer = setTimeout(check, Math.min(QUIET_MS - quietFor, timeout - (now - start)) + 1)
  }
  timer = setTimeout(check, 50)
  return () => clearTimeout(timer)
}
