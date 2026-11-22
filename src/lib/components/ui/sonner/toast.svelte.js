/**
 * Self-contained toast store (replaces svelte-sonner).
 *
 * svelte-sonner tracks per-toast heights in a parallel array that diverges from
 * the toast list, which crashes / infinite-loops (`effect_update_depth_exceeded`)
 * when several toasts dismiss at once. This implementation keeps a single plain
 * reactive list and lets Svelte transitions handle enter/exit animation, so there
 * is no height bookkeeping and no way to form a read-write reactive cycle.
 *
 * API surface mirrors the parts of svelte-sonner the app uses:
 *   toast.success/error/info/warning/message/loading(title, { description?, duration?, progress? })
 *   toast.update(id, { type?, title?, description?, progress?, action?, duration? })
 *   toast.dismiss(id?)        - dismiss one, or all when id is omitted
 *   toast.getActiveToasts()   - current toasts (reactive read)
 */
import { errorDuration } from '$lib/toast-durations.js'

let counter = 0
const DEFAULT_DURATION = 4500

/**
 * @typedef {'success' | 'error' | 'info' | 'warning' | 'message' | 'loading'} ToastType
 * @typedef {{ label: string, onClick: () => void }} ToastAction
 * @typedef {{ id: number, type: ToastType, title: string, description: string, action?: ToastAction, code?: boolean, progress?: number | null, key?: string }} Toast
 * @typedef {{ description?: string, duration?: number, action?: ToastAction, code?: boolean, progress?: number | null, key?: string }} ToastOptions
 */

class ToastStore {
  /** @type {Toast[]} */
  toasts = $state([])

  /** @type {Map<number, { remaining: number, start: number, handle: ReturnType<typeof setTimeout> }>} */
  #timers = new Map()

  /**
   * @param {number} id
   * @param {number} duration
   */
  #schedule(id, duration) {
    if (!Number.isFinite(duration) || duration <= 0) return
    const handle = setTimeout(() => this.dismiss(id), duration)
    this.#timers.set(id, { remaining: duration, start: Date.now(), handle })
  }

  /**
   * @param {ToastType} type
   * @param {unknown} title
   * @param {ToastOptions} [opts]
   */
  #add(type, title, opts = {}) {
    // A loading toast stays until it is updated to its outcome.
    // An error stays long enough to read (errorDuration).
    const duration = opts.duration ?? (type === 'loading' ? Infinity : type === 'error' ? errorDuration(title, opts) : DEFAULT_DURATION)
    // The same message again while it is still up (a double-click, a key
    // held down) restarts its timer instead of stacking a copy under it.
    const text = String(title ?? '')
    // A keyed toast replaces the one up under the same key, whatever it said:
    // a failure that repeats (a connect retried) is one toast that updates, not
    // a pile, even when it carries an action.
    const keyed = opts.key ? this.toasts.find((t) => t.key === opts.key) : undefined
    if (keyed) {
      const old = this.#timers.get(keyed.id)
      if (old) { clearTimeout(old.handle); this.#timers.delete(keyed.id) }
      /** @type {Toast} */
      const next = { id: keyed.id, type, title: text, description: opts.description ?? '', key: opts.key }
      if (opts.action) next.action = opts.action
      if (opts.code) next.code = true
      this.toasts = this.toasts.map((t) => (t.id === keyed.id ? next : t))
      this.#schedule(keyed.id, duration)
      return keyed.id
    }
    const same = type !== 'loading' && !opts.action && opts.progress == null
      ? this.toasts.find((t) => t.type === type && t.title === text && (t.description ?? '') === (opts.description ?? '') && !t.action && t.progress == null)
      : undefined
    if (same) {
      const old = this.#timers.get(same.id)
      if (old) { clearTimeout(old.handle); this.#timers.delete(same.id) }
      this.#schedule(same.id, duration)
      return same.id
    }
    const id = ++counter
    /** @type {Toast} */
    const entry = { id, type, title: String(title ?? ''), description: opts.description ?? '' }
    if (opts.action) entry.action = opts.action
    if (opts.code) entry.code = true
    if (opts.progress != null) entry.progress = opts.progress
    if (opts.key) entry.key = opts.key
    // newest first; immutable reassignment keeps the read-side reactive
    this.toasts = [entry, ...this.toasts]
    this.#schedule(id, duration)
    return id
  }

  /** @param {unknown} title @param {ToastOptions} [opts] */
  success = (title, opts) => this.#add('success', title, opts)
  /** @param {unknown} title @param {ToastOptions} [opts] */
  error = (title, opts) => this.#add('error', title, opts)
  /** @param {unknown} title @param {ToastOptions} [opts] */
  info = (title, opts) => this.#add('info', title, opts)
  /** @param {unknown} title @param {ToastOptions} [opts] */
  warning = (title, opts) => this.#add('warning', title, opts)
  /** @param {unknown} title @param {ToastOptions} [opts] */
  message = (title, opts) => this.#add('message', title, opts)
  /** Stays until updated: a spinner, and a bar when `progress` (0-1) is set. @param {unknown} title @param {ToastOptions} [opts] */
  loading = (title, opts) => this.#add('loading', title, opts)

  /**
   * Change a toast in place (a pull's progress, then its outcome). Leaving
   * `loading` starts the usual dismiss timer unless `duration` says otherwise.
   * @param {number} id
   * @param {Partial<Omit<Toast, 'id'>> & { duration?: number }} patch
   */
  update = (id, patch) => {
    const i = this.toasts.findIndex((t) => t.id === id)
    if (i < 0) return false
    const prev = this.toasts[i]
    const { duration, ...rest } = patch
    const next = { ...prev, ...rest, title: rest.title != null ? String(rest.title) : prev.title }
    this.toasts = this.toasts.map((t, k) => (k === i ? next : t))
    if (prev.type === 'loading' && next.type !== 'loading') {
      const old = this.#timers.get(id)
      if (old) { clearTimeout(old.handle); this.#timers.delete(id) }
      this.#schedule(id, duration ?? (next.type === 'error' ? errorDuration(next.title, next) : DEFAULT_DURATION))
    }
    return true
  }

  /** @param {number} [id] */
  dismiss = (id) => {
    if (id === undefined) {
      for (const t of this.#timers.values()) clearTimeout(t.handle)
      this.#timers.clear()
      this.toasts = []
      return
    }
    const t = this.#timers.get(id)
    if (t) {
      clearTimeout(t.handle)
      this.#timers.delete(id)
    }
    this.toasts = this.toasts.filter((x) => x.id !== id)
  }

  getActiveToasts = () => this.toasts

  /** Pause every auto-dismiss timer (e.g. while the pointer is over the stack). */
  pauseAll = () => {
    for (const t of this.#timers.values()) {
      clearTimeout(t.handle)
      t.remaining -= Date.now() - t.start
    }
  }

  /** Resume previously-paused auto-dismiss timers. */
  resumeAll = () => {
    for (const [id, t] of this.#timers) {
      t.start = Date.now()
      t.handle = setTimeout(() => this.dismiss(id), Math.max(0, t.remaining))
    }
  }
}

export const toast = new ToastStore()
