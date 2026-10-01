/**
 * Dev-only frame timing, so smoothness is measured rather than guessed. On
 * only when the dev server starts with VITE_STROKE_PERF=1, and absent from
 * production builds (main.js imports it behind import.meta.env.DEV).
 *
 * It samples only while something is happening: a wheel, key or pointer event
 * keeps a requestAnimationFrame loop going for a second after it, recording the
 * gap between frames. A loop that never stopped would itself keep the
 * compositor busy and skew what it measures. Every two seconds of activity a
 * summary goes to the dev log through `perf_log`.
 */
import { invoke } from '@tauri-apps/api/core'

/** A frame later than this many budgets counts as dropped. */
const DROP_FACTOR = 1.5

export function installPerfProbe() {
  /** @type {number[]} */
  let frames = []
  let last = 0
  let until = 0
  let sampling = false
  let budget = 1000 / 60

  // The display's frame budget, from the first quiet frames.
  let probe = 0
  const calibrate = (/** @type {number} */ t) => {
    if (probe++ < 30) {
      if (last) frames.push(t - last)
      last = t
      requestAnimationFrame(calibrate)
      return
    }
    const s = frames.sort((a, b) => a - b)
    budget = s[Math.floor(s.length / 2)] || budget
    frames = []
    last = 0
    void invoke('perf_log', { line: `budget ${budget.toFixed(1)}ms (${(1000 / budget).toFixed(0)}Hz)` }).catch(() => {})
  }
  requestAnimationFrame(calibrate)

  const tick = (/** @type {number} */ t) => {
    if (last) frames.push(t - last)
    last = t
    if (performance.now() < until) requestAnimationFrame(tick)
    else { sampling = false; last = 0 }
  }
  const poke = () => {
    until = performance.now() + 1000
    if (!sampling) { sampling = true; requestAnimationFrame(tick) }
  }
  for (const type of ['wheel', 'keydown', 'pointerdown', 'pointermove']) {
    addEventListener(type, poke, { capture: true, passive: true })
  }

  setInterval(() => {
    if (frames.length < 20) return
    const s = frames.sort((a, b) => a - b)
    frames = []
    const q = (/** @type {number} */ p) => s[Math.min(s.length - 1, Math.floor(p * s.length))].toFixed(1)
    const dropped = s.filter((x) => x > budget * DROP_FACTOR).length
    const line = `frames ${s.length}: p50 ${q(0.5)} p95 ${q(0.95)} max ${s[s.length - 1].toFixed(1)}ms, ` +
      `${dropped} dropped (${((dropped / s.length) * 100).toFixed(0)}%)`
    void invoke('perf_log', { line }).catch(() => {})
  }, 2000)
}
