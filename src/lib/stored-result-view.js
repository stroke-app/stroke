/**
 * The window's side of a console result kept in the backend's result store
 * (result_store.rs): a sparse `rows` array as long as the result that holds
 * only the windows near the viewport, so a 5M-row result costs the window a
 * few thousand rows instead of all of them. It follows table browsing's window
 * rules (row-window.js): windows sized by bytes, the viewport fetched first, a
 * little read-ahead in the direction of travel, far windows evicted.
 *
 * Unlike table browsing the result can still be growing: `grow` is called as
 * rows land in the store, and a window that was only partly readable when it
 * was fetched is fetched again once it fills.
 *
 * No Svelte here. The owner keeps `rows` in a raw (non-proxied) state, assigns
 * it when `onrows` reports a new array (the length changed), and bumps the
 * grid's `dataVersion` on `onchange` (rows spliced in place).
 */
import { resultWindow, resultDrop } from '$lib/api.js'
import { pickWindowRows } from '$lib/row-window.js'

const MAX_INFLIGHT = 2
const PREFETCH = 2
/** Rows kept on each side of the viewport: well past a flick, and a 5M-row
 *  result still costs the window a few tens of thousands of rows at most. */
const KEEP_ROWS = 20_000

export class StoredResultView {
  /**
   * @param {{
   *   id: string,
   *   onrows: (rows: any[]) => void,
   *   onchange: () => void,
   *   onstatus?: (status: { slow: boolean, failed: boolean }) => void,
   * }} opts
   */
  constructor({ id, onrows, onchange, onstatus }) {
    this.id = id
    this.onrows = onrows
    this.onchange = onchange
    this.onstatus = onstatus ?? (() => {})
    /** @type {any[]} */
    this.rows = []
    this.count = 0
    this.windowRows = 1000
    /** Windows fully resident. @type {Set<number>} */
    this.loaded = new Set()
    /** @type {Set<number>} */
    this.fetching = new Set()
    /** @type {number[]} */
    this.queue = []
    this.seq = 0
    this.failed = false
    this.lastRange = /** @type {[number, number]} */ ([0, 0])
    this.dir = 1
    this.disposed = false
  }

  /**
   * The first message of the run: its rows paint at once, and their size sets
   * the window size for the rest.
   * @param {any[][]} firstRows @param {number} count
   */
  begin(firstRows, count) {
    // Also a restart: a retried query streams into the same stored result again.
    this.seq++
    this.loaded.clear()
    this.fetching.clear()
    this.queue = []
    let bytes = 0
    const step = Math.max(1, Math.floor(firstRows.length / 4))
    let sampled = 0
    for (let i = 0; i < firstRows.length && sampled < 4; i += step, sampled++) {
      try { bytes += JSON.stringify(firstRows[i]).length } catch { bytes += 256 }
    }
    this.windowRows = pickWindowRows(sampled ? bytes / sampled : 0)
    this.count = count
    this.rows = new Array(count)
    for (let i = 0; i < firstRows.length && i < count; i++) this.rows[i] = firstRows[i]
    // Usually a partial window: it shows now and is fetched whole right after.
    if (firstRows.length >= this.windowRows) this.loaded.add(0)
    this.onrows(this.rows)
  }

  /** More rows are readable in the store. @param {number} count */
  grow(count) {
    if (this.disposed || count <= this.count) return
    // A new array: the grid reads `rows.length` reactively and the identity is
    // what tells it. Only resident rows are copied, never the whole length.
    // In-flight fetches write into whatever `this.rows` is when they land, so
    // only what is resident needs carrying: whole windows, and the old tail.
    const next = new Array(count)
    for (const w of this.loaded) this.copyWindow(this.rows, next, w)
    if (this.count > 0) this.copyWindow(this.rows, next, Math.floor((this.count - 1) / this.windowRows))
    this.rows = next
    this.count = count
    this.onrows(this.rows)
    // The rows on screen may have been past the old end.
    this.visible(this.lastRange[0], this.lastRange[1])
  }

  /** @param {any[]} from @param {any[]} to @param {number} w */
  copyWindow(from, to, w) {
    const start = w * this.windowRows
    const end = Math.min(start + this.windowRows, from.length, to.length)
    for (let i = start; i < end; i++) if (from[i] !== undefined) to[i] = from[i]
  }

  /**
   * The grid's visible row range: fetch what is on screen, then ahead in the
   * direction of travel; evict what is far away.
   * @param {number} start @param {number} end
   */
  visible(start, end) {
    if (this.disposed || this.count === 0) return
    const [prevStart] = this.lastRange
    if (start !== prevStart) this.dir = start > prevStart ? 1 : -1
    this.lastRange = [start, end]
    const n = this.windowRows
    const firstW = Math.floor(start / n)
    const lastW = Math.floor(Math.max(start, end) / n)
    /** @type {number[]} */
    const want = []
    for (let w = firstW; w <= lastW; w++) want.push(w)
    for (let i = 1; i <= PREFETCH; i++) want.push(this.dir >= 0 ? lastW + i : firstW - i)
    this.queue = want.filter((w) => w >= 0 && w * n < this.count && !this.loaded.has(w) && !this.fetching.has(w))
    this.pump()
    this.evict(firstW, lastW)
  }

  pump() {
    while (!this.disposed && this.fetching.size < MAX_INFLIGHT && this.queue.length) {
      const w = /** @type {number} */ (this.queue.shift())
      if (!this.loaded.has(w) && !this.fetching.has(w)) void this.fetch(w)
    }
  }

  /** @param {number} w */
  async fetch(w) {
    const seq = this.seq
    const start = w * this.windowRows
    const count = Math.min(this.windowRows, this.count - start)
    if (count <= 0) return
    this.fetching.add(w)
    try {
      const rows = await resultWindow(this.id, start, count)
      if (this.disposed || seq !== this.seq) return
      for (let i = 0; i < rows.length; i++) this.rows[start + i] = rows[i]
      // Only a whole window is done. The tail of a result still streaming is
      // fetched again once more rows land (grow calls visible).
      if (count === this.windowRows && rows.length === count) this.loaded.add(w)
      if (this.failed) { this.failed = false; this.onstatus({ slow: false, failed: false }) }
      this.onchange()
    } catch {
      if (this.disposed || seq !== this.seq) return
      this.failed = true
      this.onstatus({ slow: false, failed: true })
    } finally {
      this.fetching.delete(w)
      if (!this.disposed && seq === this.seq) this.pump()
    }
  }

  /** @param {number} firstW @param {number} lastW */
  evict(firstW, lastW) {
    const keep = Math.max(2, Math.ceil(KEEP_ROWS / this.windowRows))
    let evicted = false
    for (const w of this.loaded) {
      if (w >= firstW - keep && w <= lastW + keep) continue
      const start = w * this.windowRows
      const end = Math.min(start + this.windowRows, this.rows.length)
      for (let i = start; i < end; i++) this.rows[i] = undefined
      this.loaded.delete(w)
      evicted = true
    }
    if (evicted) this.onchange()
  }

  /** "Retry" in the grid's loading pill. */
  retry() {
    this.failed = false
    this.onstatus({ slow: false, failed: false })
    this.visible(this.lastRange[0], this.lastRange[1])
  }

  /**
   * The store's order changed (a sort): every resident row is now in the wrong
   * place, so they all go and the viewport is fetched again.
   */
  reorder() {
    this.seq++
    this.loaded.clear()
    this.fetching.clear()
    this.queue = []
    this.rows = new Array(this.count)
    this.onrows(this.rows)
    this.visible(this.lastRange[0], this.lastRange[1])
  }

  /** Every row, window by window, for export and copy. @param {(n: number) => void} [onprogress] */
  async readAll(onprogress) {
    /** @type {any[][]} */
    const out = []
    const step = Math.max(this.windowRows, 20_000)
    for (let start = 0; start < this.count; start += step) {
      const rows = await resultWindow(this.id, start, Math.min(step, this.count - start))
      for (const r of rows) out.push(r)
      onprogress?.(out.length)
    }
    return out
  }

  /** The tab ran again or closed: forget the rows here and in the store. */
  dispose() {
    if (this.disposed) return
    this.disposed = true
    this.seq++
    this.rows = []
    this.loaded.clear()
    this.queue = []
    resultDrop(this.id)
  }
}
