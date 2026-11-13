// Dev-only interaction timing, alongside the frame probe (perf-probe.js) and
// switched on the same way: a dev server started with VITE_STROKE_PERF=1.
// Records backend calls slower than SLOW_IPC_MS and interactions timed from the
// click to the frame after the UI settled, batched to the dev log through
// `perf_log` once a second. Off (and free) otherwise, production included.
import { invoke } from '@tauri-apps/api/core'

const ON = import.meta.env.DEV && import.meta.env.VITE_STROKE_PERF === '1'
const SLOW_IPC_MS = 30

/** @type {string[]} */
let buffer = []
let timer = /** @type {ReturnType<typeof setTimeout> | null} */ (null)

/** @param {string} line */
function push(line) {
  buffer.push(line)
  if (!timer) {
    timer = setTimeout(() => {
      const line = buffer.join(' | ')
      buffer = []
      timer = null
      invoke('perf_log', { line }).catch(() => {})
    }, 1000)
  }
}

/** Record one backend call. @param {string} command @param {number} ms @param {boolean} ok */
export function traceIpc(command, ms, ok) {
  if (!ON || command === 'perf_log' || ms < SLOW_IPC_MS) return
  push(`ipc ${command} ${Math.round(ms)}ms${ok ? '' : ' (failed)'}`)
}

/**
 * A page of rows that just came back: how big it was, how much of the round
 * trip the server spent on it, and what landing it cost the main thread up to
 * the frame it painted in. Lines up with `long frame` lines, so a stall can be
 * pinned on the payload (big `~MB`, long `land`) or ruled out of it. Only pages
 * over a few hundred rows or slow ones are logged.
 * @param {string} command @param {any} result the command's reply ({ rows, queryMs })
 * @param {number} ms the whole round trip, decode included
 */
export function traceRows(command, result, ms) {
  if (!ON) return
  const rows = Array.isArray(result?.rows) ? result.rows : null
  if (!rows || (rows.length < 500 && ms < SLOW_IPC_MS)) return
  // Sampled, like row-window.js measures: stringifying a big page to weigh it
  // would cost what is being measured.
  // Golden-ratio positions rather than an even stride, which lines up with any
  // periodic row pattern and weighs only the heavy (or only the light) rows.
  let bytes = 0
  let n = 0
  const samples = Math.min(16, rows.length)
  for (let k = 0; k < samples; k++) {
    const i = Math.floor(((k * 0.6180339887) % 1) * rows.length)
    try { bytes += JSON.stringify(rows[i])?.length ?? 0; n++ } catch { /* unmeasurable row */ }
  }
  const mb = n ? (bytes / n) * rows.length / 1e6 : 0
  const server = Number(result?.queryMs ?? result?.query_ms ?? 0)
  const t0 = performance.now()
  requestAnimationFrame(() => requestAnimationFrame(() => {
    push(`rows ${command} ${rows.length} rows ~${mb.toFixed(1)}MB, ipc ${Math.round(ms)}ms (server ${server}ms), land ${Math.round(performance.now() - t0)}ms`)
  }))
}

/**
 * Time an interaction to the frame it paints in: call the returned function
 * when its work is done.
 * @param {string} label
 */
export function traceInteraction(label) {
  if (!ON) return () => {}
  const t0 = performance.now()
  return () => {
    const work = performance.now() - t0
    // Two frames: the one that applies the DOM changes, and the one it paints in.
    requestAnimationFrame(() => requestAnimationFrame(() => {
      push(`ui ${label} ${Math.round(performance.now() - t0)}ms (work ${Math.round(work)}ms)`)
    }))
  }
}
